use crate::state::{apply_recovery_code, SharedTunerState};
use eframe::egui;
use pitch_core::TunerEngine;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use audio_input::{input_device_names, InputConfig, RecoveryPolicy, SupervisedInputStream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InputStatus {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AudioEvent {
    InputStarted,
    InputStopped,
    InputFailed(String),
    ToneFailed(String),
    DeviceRefreshFailed(String),
}

/// The UI owns only channels and cancellation tokens. CoreAudio calls,
/// stream destruction and thread joins all run on the audio owner thread.
/// In particular, neither a permission dialog nor a slow device shutdown
/// can block an egui event handler.
#[derive(Default)]
pub(crate) struct AudioManager {
    pub(crate) input_devices: Vec<String>,
    pub(crate) selected_input_device: Option<String>,
    commands: Option<mpsc::Sender<Command>>,
    events: Option<mpsc::Receiver<WorkerEvent>>,
    control: Arc<WorkerControl>,
    input_cancelled: Option<Arc<AtomicBool>>,
    tone_cancelled: Option<Arc<AtomicBool>>,
    input_status: InputStatus,
    refresh_pending: bool,
    tone_pending: bool,
    tone_playing: bool,
    pending_events: Vec<AudioEvent>,
}

#[derive(Default)]
struct WorkerControl {
    input_generation: AtomicU64,
    tone_generation: AtomicU64,
    shutdown: AtomicBool,
}

struct InputRequest {
    generation: u64,
    cancelled: Arc<AtomicBool>,
    device_name: Option<String>,
    state: SharedTunerState,
    engine: Arc<Mutex<TunerEngine>>,
    context: egui::Context,
}

enum Command {
    RefreshDevices,
    StartInput(InputRequest),
    StopInput(u64),
    PlayTone {
        generation: u64,
        frequency: f32,
        cancelled: Arc<AtomicBool>,
    },
    StopTone(u64),
    Shutdown,
}

#[derive(Debug)]
enum WorkerEvent {
    Devices(Result<Vec<String>, String>),
    InputStarted(u64),
    InputStopped(u64),
    InputFailed(u64, String),
    ToneStarted(u64),
    ToneStopped(u64),
    ToneFailed(u64, String),
}

impl AudioManager {
    fn ensure_worker(&mut self) -> Result<(), String> {
        if self.commands.is_none() {
            self.spawn_worker(|events| NativeBackend { events })?;
        }
        Ok(())
    }

    fn spawn_worker<B, F>(&mut self, factory: F) -> Result<(), String>
    where
        B: AudioBackend + 'static,
        F: FnOnce(mpsc::Sender<WorkerEvent>) -> B + Send + 'static,
    {
        let (commands_tx, commands_rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let control = self.control.clone();
        // Construct the backend inside this thread: a cpal::Stream is not
        // Send on all supported platforms and never crosses the boundary.
        thread::Builder::new()
            .name("tuner-audio-owner".into())
            .spawn(move || {
                let backend = factory(events_tx.clone());
                run_worker(backend, commands_rx, events_tx, control);
            })
            .map_err(|error| format!("Could not start audio worker: {error}"))?;
        self.commands = Some(commands_tx);
        self.events = Some(events_rx);
        Ok(())
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .as_ref()
            .ok_or_else(|| "Audio worker is unavailable".to_string())?
            .send(command)
            .map_err(|_| "Audio worker stopped unexpectedly".to_string())
    }

    pub(crate) fn refresh(&mut self) {
        if self.refresh_pending {
            return;
        }
        match self
            .ensure_worker()
            .and_then(|()| self.send(Command::RefreshDevices))
        {
            Ok(()) => self.refresh_pending = true,
            Err(error) => self
                .pending_events
                .push(AudioEvent::DeviceRefreshFailed(error)),
        }
    }

    /// Schedules startup. InputStarted/InputFailed from poll reports the
    /// actual result; Ok here means that the request was queued.
    pub(crate) fn start_input(
        &mut self,
        state: SharedTunerState,
        engine: Arc<Mutex<TunerEngine>>,
        context: egui::Context,
    ) -> Result<(), String> {
        self.ensure_worker()?;
        self.cancel_input();
        let generation = self.control.input_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.input_cancelled = Some(cancelled.clone());
        self.input_status = InputStatus::Starting;
        if let Err(error) = self.send(Command::StartInput(InputRequest {
            generation,
            cancelled,
            device_name: self.selected_input_device.clone(),
            state,
            engine,
            context,
        })) {
            self.cancel_input();
            self.input_status = InputStatus::Stopped;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn stop_input(&mut self) {
        // This flag is observed by the input supervisor and frame callback
        // even while the owner is inside a native open/close call.
        self.cancel_input();
        let generation = self.control.input_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if self.commands.is_some() && self.send(Command::StopInput(generation)).is_ok() {
            self.input_status = InputStatus::Stopping;
        } else {
            self.input_status = InputStatus::Stopped;
        }
    }

    fn cancel_input(&mut self) {
        if let Some(cancelled) = self.input_cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
    }

    pub(crate) fn input_status(&self) -> InputStatus {
        self.input_status
    }

    pub(crate) fn play_tone(&mut self, frequency: f32) -> Result<(), String> {
        self.ensure_worker()?;
        self.cancel_tone();
        let generation = self.control.tone_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.tone_cancelled = Some(cancelled.clone());
        if let Err(error) = self.send(Command::PlayTone {
            generation,
            frequency,
            cancelled,
        }) {
            self.cancel_tone();
            self.tone_playing = false;
            self.tone_pending = false;
            return Err(error);
        }
        // Reflect the requested state so a second click can cancel startup.
        self.tone_playing = true;
        self.tone_pending = true;
        Ok(())
    }

    pub(crate) fn stop_tone(&mut self) {
        // Silence does not wait for the owner thread. It may currently be
        // waiting for a microphone permission dialog or another native call.
        self.cancel_tone();
        let generation = self.control.tone_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.tone_playing = false;
        self.tone_pending =
            self.commands.is_some() && self.send(Command::StopTone(generation)).is_ok();
    }

    fn cancel_tone(&mut self) {
        if let Some(cancelled) = self.tone_cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
    }

    pub(crate) fn is_tone_playing(&self) -> bool {
        self.tone_playing
    }

    /// Drains only ready messages. Call once per UI update, including when
    /// stopped; short repaint polling ends as soon as pending work finishes.
    pub(crate) fn poll(&mut self, context: &egui::Context) -> Vec<AudioEvent> {
        let mut received = Vec::new();
        let mut disconnected = false;
        if let Some(events) = &self.events {
            loop {
                match events.try_recv() {
                    Ok(event) => received.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut result = std::mem::take(&mut self.pending_events);
        let input_generation = self.control.input_generation.load(Ordering::Acquire);
        let tone_generation = self.control.tone_generation.load(Ordering::Acquire);
        for event in received {
            match event {
                WorkerEvent::Devices(devices) => {
                    self.refresh_pending = false;
                    match devices {
                        Ok(devices) => self.input_devices = devices,
                        Err(error) => result.push(AudioEvent::DeviceRefreshFailed(error)),
                    }
                }
                WorkerEvent::InputStarted(generation)
                    if generation == input_generation
                        && self.input_status == InputStatus::Starting
                        && self
                            .input_cancelled
                            .as_ref()
                            .is_some_and(|cancelled| !cancelled.load(Ordering::Acquire)) =>
                {
                    self.input_status = InputStatus::Running;
                    result.push(AudioEvent::InputStarted);
                }
                WorkerEvent::InputStopped(generation) if generation == input_generation => {
                    self.input_status = InputStatus::Stopped;
                    result.push(AudioEvent::InputStopped);
                }
                WorkerEvent::InputFailed(generation, error) if generation == input_generation => {
                    self.cancel_input();
                    self.input_status = InputStatus::Stopped;
                    // Fatal runtime errors also release the failed capture.
                    let _ = self.send(Command::StopInput(generation));
                    result.push(AudioEvent::InputFailed(error));
                }
                WorkerEvent::ToneStarted(generation) if generation == tone_generation => {
                    self.tone_pending = false;
                    self.tone_playing = true;
                }
                WorkerEvent::ToneStopped(generation) if generation == tone_generation => {
                    self.tone_pending = false;
                    self.tone_playing = false;
                }
                WorkerEvent::ToneFailed(generation, error) if generation == tone_generation => {
                    self.cancel_tone();
                    self.tone_pending = false;
                    self.tone_playing = false;
                    result.push(AudioEvent::ToneFailed(error));
                }
                _ => {} // A cancelled/replaced request cannot revive the UI.
            }
        }
        if disconnected {
            self.commands = None;
            self.events = None;
            self.cancel_input();
            self.cancel_tone();
            let input_was_active = self.input_status != InputStatus::Stopped;
            let tone_was_active = self.tone_playing || self.tone_pending;
            let refresh_was_pending = self.refresh_pending;
            self.input_status = InputStatus::Stopped;
            self.refresh_pending = false;
            self.tone_pending = false;
            self.tone_playing = false;
            if input_was_active {
                result.push(AudioEvent::InputFailed(
                    "Audio worker stopped unexpectedly".into(),
                ));
            }
            if tone_was_active {
                result.push(AudioEvent::ToneFailed(
                    "Audio worker stopped unexpectedly".into(),
                ));
            }
            if refresh_was_pending {
                result.push(AudioEvent::DeviceRefreshFailed(
                    "Audio worker stopped unexpectedly".into(),
                ));
            }
        }
        if self.refresh_pending
            || self.tone_pending
            || matches!(
                self.input_status,
                InputStatus::Starting | InputStatus::Stopping
            )
        {
            context.request_repaint_after(Duration::from_millis(25));
        }
        result
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        self.cancel_input();
        self.cancel_tone();
        self.control.shutdown.store(true, Ordering::Release);
        // Never join on the event-loop thread. The owner performs teardown
        // once the native call returns; it skips any queued startup requests.
        let _ = self.send(Command::Shutdown);
    }
}

trait AudioBackend {
    type Input;
    type Output;
    fn devices(&mut self) -> Result<Vec<String>, String>;
    fn open_input(&mut self, request: InputRequest) -> Result<Self::Input, String>;
    fn stop_input(&mut self, input: Self::Input);
    fn open_tone(
        &mut self,
        frequency: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self::Output, String>;
    fn stop_tone(&mut self, output: Self::Output);
}

fn run_worker<B: AudioBackend>(
    mut backend: B,
    commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<WorkerEvent>,
    control: Arc<WorkerControl>,
) {
    let mut input = None;
    let mut output = None;
    while let Ok(command) = commands.recv() {
        if control.shutdown.load(Ordering::Acquire) {
            break;
        }
        match command {
            Command::RefreshDevices => {
                let _ = events.send(WorkerEvent::Devices(backend.devices()));
            }
            Command::StartInput(request) => {
                let generation = request.generation;
                if generation != control.input_generation.load(Ordering::Acquire)
                    || request.cancelled.load(Ordering::Acquire)
                {
                    continue;
                }
                if let Some(previous) = input.take() {
                    backend.stop_input(previous);
                }
                if control.shutdown.load(Ordering::Acquire)
                    || generation != control.input_generation.load(Ordering::Acquire)
                    || request.cancelled.load(Ordering::Acquire)
                {
                    continue;
                }
                let cancelled = request.cancelled.clone();
                match backend.open_input(request) {
                    Ok(opened) => {
                        if control.shutdown.load(Ordering::Acquire)
                            || generation != control.input_generation.load(Ordering::Acquire)
                            || cancelled.load(Ordering::Acquire)
                        {
                            backend.stop_input(opened);
                        } else {
                            input = Some(opened);
                            let _ = events.send(WorkerEvent::InputStarted(generation));
                        }
                    }
                    Err(error) => {
                        let _ = events.send(WorkerEvent::InputFailed(generation, error));
                    }
                }
            }
            Command::StopInput(generation) => {
                if generation == control.input_generation.load(Ordering::Acquire) {
                    if let Some(previous) = input.take() {
                        backend.stop_input(previous);
                    }
                    let _ = events.send(WorkerEvent::InputStopped(generation));
                }
            }
            Command::PlayTone {
                generation,
                frequency,
                cancelled,
            } => {
                if generation != control.tone_generation.load(Ordering::Acquire)
                    || cancelled.load(Ordering::Acquire)
                {
                    continue;
                }
                if let Some(previous) = output.take() {
                    backend.stop_tone(previous);
                }
                if control.shutdown.load(Ordering::Acquire)
                    || generation != control.tone_generation.load(Ordering::Acquire)
                    || cancelled.load(Ordering::Acquire)
                {
                    continue;
                }
                match backend.open_tone(frequency, cancelled.clone()) {
                    Ok(opened) => {
                        if control.shutdown.load(Ordering::Acquire)
                            || generation != control.tone_generation.load(Ordering::Acquire)
                            || cancelled.load(Ordering::Acquire)
                        {
                            backend.stop_tone(opened);
                        } else {
                            output = Some(opened);
                            let _ = events.send(WorkerEvent::ToneStarted(generation));
                        }
                    }
                    Err(error) => {
                        let _ = events.send(WorkerEvent::ToneFailed(generation, error));
                    }
                }
            }
            Command::StopTone(generation) => {
                if generation == control.tone_generation.load(Ordering::Acquire) {
                    if let Some(previous) = output.take() {
                        backend.stop_tone(previous);
                    }
                    let _ = events.send(WorkerEvent::ToneStopped(generation));
                }
            }
            Command::Shutdown => break,
        }
    }
    if let Some(input) = input {
        backend.stop_input(input);
    }
    if let Some(output) = output {
        backend.stop_tone(output);
    }
}

struct NativeBackend {
    events: mpsc::Sender<WorkerEvent>,
}

impl AudioBackend for NativeBackend {
    type Input = SupervisedInputStream;
    type Output = Stream;

    fn devices(&mut self) -> Result<Vec<String>, String> {
        Ok(input_device_names())
    }

    fn open_input(&mut self, request: InputRequest) -> Result<Self::Input, String> {
        let InputRequest {
            generation,
            cancelled,
            device_name,
            state,
            engine,
            context,
        } = request;
        let recovery_state = state.clone();
        let frame_context = context.clone();
        let recovery_context = context.clone();
        let frame_cancelled = cancelled.clone();
        let error_cancelled = cancelled.clone();
        let recovery_cancelled = cancelled.clone();
        let events = self.events.clone();
        SupervisedInputStream::open_cancellable(
            InputConfig {
                device_name,
                ..InputConfig::default()
            },
            RecoveryPolicy::default(),
            move |samples, sample_rate| {
                if frame_cancelled.load(Ordering::Acquire) {
                    return;
                }
                let frame = match engine.lock() {
                    Ok(mut engine) => {
                        if frame_cancelled.load(Ordering::Acquire) {
                            return;
                        }
                        engine.process(samples, sample_rate)
                    }
                    Err(_) => return,
                };
                if let Ok(mut state) = state.lock() {
                    if frame_cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    state.apply(frame, samples, sample_rate);
                }
                frame_context.request_repaint();
            },
            move |error| {
                if !error_cancelled.load(Ordering::Acquire) {
                    let _ = events.send(WorkerEvent::InputFailed(generation, error));
                    context.request_repaint();
                }
            },
            move |event| {
                if let Ok(mut state) = recovery_state.lock() {
                    if recovery_cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    apply_recovery_code(&mut state, event.code());
                }
                recovery_context.request_repaint();
            },
            cancelled,
        )
    }

    fn stop_input(&mut self, mut input: Self::Input) {
        input.stop();
    }

    fn open_tone(
        &mut self,
        frequency: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self::Output, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No output device available".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|error| format!("Could not read output config: {error}"))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let stream = match sample_format {
            SampleFormat::I8 => build_tone_stream::<i8>(&device, &config, frequency, cancelled),
            SampleFormat::I16 => build_tone_stream::<i16>(&device, &config, frequency, cancelled),
            SampleFormat::I32 => build_tone_stream::<i32>(&device, &config, frequency, cancelled),
            SampleFormat::I64 => build_tone_stream::<i64>(&device, &config, frequency, cancelled),
            SampleFormat::U8 => build_tone_stream::<u8>(&device, &config, frequency, cancelled),
            SampleFormat::U16 => build_tone_stream::<u16>(&device, &config, frequency, cancelled),
            SampleFormat::U32 => build_tone_stream::<u32>(&device, &config, frequency, cancelled),
            SampleFormat::U64 => build_tone_stream::<u64>(&device, &config, frequency, cancelled),
            SampleFormat::F32 => build_tone_stream::<f32>(&device, &config, frequency, cancelled),
            SampleFormat::F64 => build_tone_stream::<f64>(&device, &config, frequency, cancelled),
            format => Err(format!("Unsupported output sample format: {format}")),
        }?;
        stream
            .play()
            .map_err(|error| format!("Could not play reference tone: {error}"))?;
        Ok(stream)
    }

    fn stop_tone(&mut self, output: Self::Output) {
        // Explicitly stop native IO before releasing callbacks/resources.
        let _ = output.pause();
        drop(output);
    }
}

fn build_tone_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    frequency: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Stream, String>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = usize::from(config.channels.max(1));
    let mut phase = 0.0_f32;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                fill_tone_buffer(
                    data,
                    channels,
                    frequency,
                    sample_rate,
                    &mut phase,
                    &cancelled,
                );
            },
            |error| eprintln!("reference tone output error: {error}"),
            None,
        )
        .map_err(|error| format!("Could not create reference tone stream: {error}"))
}

fn fill_tone_buffer<T>(
    data: &mut [T],
    channels: usize,
    frequency: f32,
    sample_rate: f32,
    phase: &mut f32,
    cancelled: &AtomicBool,
) where
    T: Sample + SizedSample + FromSample<f32>,
{
    if cancelled.load(Ordering::Acquire) {
        // Converting floating-point zero also gives the correct equilibrium
        // for unsigned output formats (for example 128 for u8).
        data.fill(T::from_sample(0.0));
        return;
    }
    for frame in data.chunks_mut(channels) {
        let sample = (2.0 * std::f32::consts::PI * *phase).sin() * 0.18;
        for output in frame {
            *output = T::from_sample(sample);
        }
        *phase = (*phase + frequency / sample_rate).fract();
    }
}

#[cfg(test)]
#[path = "audio_lifecycle_tests.rs"]
mod lifecycle_tests;
