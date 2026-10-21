mod recovery;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub use recovery::{
    RecoveryEvent, RecoveryMachine, RecoveryPolicy, RecoveryStep, BACKEND_RECOVERY_ATTEMPTED,
    BACKEND_RECOVERY_FAILED, BACKEND_RECOVERY_SUCCEEDED, BACKEND_STREAM_LOST,
};

const AUDIO_CHUNK_CAPACITY: usize = 8_192;
const AUDIO_CHUNK_POOL_SIZE: usize = 4;
const DEFAULT_FRAME_INTERVAL: Duration = Duration::from_millis(33);
// Matches the web analysis window (8192 samples). At 48 kHz a 4096-sample
// window leaves YIN only ~2 comparison periods of bass E1 (41 Hz), which is
// not enough for a stable CMNDF on a decaying inharmonic string; 8192 gives
// the native path the same low-string headroom as the browser path.
const DEFAULT_WINDOW_SIZE: usize = 8_192;
const MAX_WINDOW_SIZE: usize = 8_192;
const PROCESSOR_POLL_INTERVAL: Duration = Duration::from_millis(20);
const INPUT_CANCELLED: &str = "Audio input start was cancelled";

/// Frame handler shared between successive captures of one supervised
/// session. Locked only on the processor thread, never on the cpal callback.
type SharedFrameHandler = Arc<Mutex<Box<dyn FnMut(&[f32], f32) + Send>>>;
type SharedErrorHandler = Arc<Mutex<Box<dyn FnMut(String) + Send>>>;
type SharedRecoveryHandler = Arc<Mutex<Box<dyn FnMut(RecoveryEvent) + Send>>>;

fn share_frame_handler<FrameHandler>(handler: FrameHandler) -> SharedFrameHandler
where
    FrameHandler: FnMut(&[f32], f32) + Send + 'static,
{
    Arc::new(Mutex::new(Box::new(handler)))
}

fn share_error_handler<ErrorHandler>(handler: ErrorHandler) -> SharedErrorHandler
where
    ErrorHandler: FnMut(String) + Send + 'static,
{
    Arc::new(Mutex::new(Box::new(handler)))
}

fn call_shared_error(handler: &SharedErrorHandler, error: String) {
    if let Ok(mut handler) = handler.lock() {
        handler(error);
    }
}

#[derive(Clone, Debug)]
pub struct InputConfig {
    pub device_name: Option<String>,
    pub frame_interval: Duration,
    pub window_size: usize,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            frame_interval: DEFAULT_FRAME_INTERVAL,
            window_size: DEFAULT_WINDOW_SIZE,
        }
    }
}

impl InputConfig {
    fn normalized(mut self) -> Self {
        self.window_size = self.window_size.clamp(64, MAX_WINDOW_SIZE);
        if self.frame_interval.is_zero() {
            self.frame_interval = DEFAULT_FRAME_INTERVAL;
        }
        self
    }
}

struct AudioChunk {
    length: usize,
    samples: Box<[f32; AUDIO_CHUNK_CAPACITY]>,
}

impl AudioChunk {
    fn new() -> Self {
        Self {
            length: 0,
            samples: Box::new([0.0; AUDIO_CHUNK_CAPACITY]),
        }
    }
}

struct SampleWindow {
    filled: usize,
    samples: Vec<f32>,
    write_cursor: usize,
}

impl SampleWindow {
    fn new(size: usize) -> Self {
        Self {
            filled: 0,
            samples: vec![0.0; size],
            write_cursor: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        for sample in samples {
            self.samples[self.write_cursor] = *sample;
            self.write_cursor = (self.write_cursor + 1) % self.samples.len();
            self.filled = (self.filled + 1).min(self.samples.len());
        }
    }

    fn copy_ordered(&self, output: &mut [f32]) -> bool {
        if self.filled < self.samples.len() || output.len() != self.samples.len() {
            return false;
        }
        let tail = self.samples.len() - self.write_cursor;
        output[..tail].copy_from_slice(&self.samples[self.write_cursor..]);
        output[tail..].copy_from_slice(&self.samples[..self.write_cursor]);
        true
    }
}

/// One opened capture: cpal stream plus the frame processor thread.
struct Capture {
    processor: Option<JoinHandle<()>>,
    stream: Option<Stream>,
    progress: Arc<AtomicU64>,
    processor_stop: Arc<AtomicBool>,
}

impl Capture {
    /// Open the device and build the stream. Some cpal backends, including
    /// CoreAudio 0.15, begin capture while building, before `play` is called.
    ///
    /// The cpal data callback only copies samples into the ring channel and
    /// bumps a relaxed atomic progress counter — no DSP, no allocation, no
    /// locks. The cpal error callback forwards into `errors` (and optionally
    /// directly to `forward_errors` for the legacy unsupervised mode).
    fn open(
        config: &InputConfig,
        on_frame: SharedFrameHandler,
        errors: mpsc::Sender<String>,
        forward_errors: Option<SharedErrorHandler>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err(INPUT_CANCELLED.to_string());
        }
        let host = cpal::default_host();
        let device = find_input_device(&host, config.device_name.as_deref())?;
        let supported = device
            .default_input_config()
            .map_err(|error| format!("Could not read microphone config: {error}"))?;
        let sample_format = supported.sample_format();
        let stream_config: StreamConfig = supported.into();
        let sample_rate = stream_config.sample_rate.0 as f32;
        let channels = usize::from(stream_config.channels.max(1));
        if cancelled.load(Ordering::Acquire) {
            return Err(INPUT_CANCELLED.to_string());
        }

        let (samples_tx, samples_rx) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        let (pool_tx, pool_rx) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        for _ in 0..AUDIO_CHUNK_POOL_SIZE {
            pool_tx
                .send(AudioChunk::new())
                .map_err(|_| "Could not initialize audio buffer pool")?;
        }

        let processor_pool = pool_tx.clone();
        let window_size = config.window_size;
        let frame_interval = config.frame_interval;
        let processor_stop = Arc::new(AtomicBool::new(false));
        let processor_cancelled = processor_stop.clone();
        let processor = thread::spawn(move || {
            process_frames(
                window_size,
                frame_interval,
                sample_rate,
                samples_rx,
                processor_pool,
                on_frame,
                processor_cancelled,
                cancelled,
            );
        });
        let progress = Arc::new(AtomicU64::new(0));
        // Own the processor before building the native stream, so a backend
        // error also cancels and joins it instead of detaching its handle.
        let mut capture = Self {
            processor: Some(processor),
            stream: None,
            progress: progress.clone(),
            processor_stop,
        };
        capture.stream = Some(build_input_stream(
            &device,
            &stream_config,
            sample_format,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress.clone(),
        )?);

        Ok(capture)
    }

    fn play(&self) -> Result<(), String> {
        self.stream
            .as_ref()
            .ok_or_else(|| "Audio input stream is closed".to_string())?
            .play()
            .map_err(|error| format!("Could not start microphone stream: {error}"))
    }

    fn stop(&mut self) {
        // Processor termination must not depend on the native callback
        // releasing its sender. In particular, a backend may retain that
        // callback while stopping or tearing down the device.
        self.processor_stop.store(true, Ordering::Release);
        if let Some(stream) = self.stream.take() {
            if let Err(error) = stream.pause() {
                eprintln!("Could not pause microphone stream during shutdown: {error}");
            }
            drop(stream);
        }
        if let Some(processor) = self.processor.take() {
            if processor.join().is_err() {
                eprintln!("Microphone frame processor panicked during capture");
            }
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct InputStream {
    capture: Capture,
}

impl InputStream {
    pub fn open<FrameHandler, ErrorHandler>(
        config: InputConfig,
        on_frame: FrameHandler,
        on_error: ErrorHandler,
    ) -> Result<Self, String>
    where
        FrameHandler: FnMut(&[f32], f32) + Send + 'static,
        ErrorHandler: FnMut(String) + Send + 'static,
    {
        let config = config.normalized();
        let (errors_tx, _errors_rx) = mpsc::channel();
        let capture = Capture::open(
            &config,
            share_frame_handler(on_frame),
            errors_tx,
            Some(share_error_handler(on_error)),
            Arc::new(AtomicBool::new(false)),
        )?;
        Ok(Self { capture })
    }

    pub fn play(&self) -> Result<(), String> {
        self.capture.play()
    }

    pub fn stop(&mut self) {
        self.capture.stop();
    }
}

/// Supervised input stream with automatic stream-loss recovery.
///
/// A supervisor thread owns the capture, watches the cpal error channel and a
/// no-data stall watchdog, and reopens the stream with backoff according to
/// `RecoveryPolicy`. Every transition is reported through `on_recovery` as a
/// typed `RecoveryEvent` with a stable diagnostic code; `on_error` fires only
/// when recovery is exhausted (fatal) — intermediate losses keep the session
/// alive without a restart.
pub struct SupervisedInputStream {
    control: Option<mpsc::Sender<SupervisorCommand>>,
    supervisor: Option<JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}

enum SupervisorCommand {
    Stop,
}

impl SupervisedInputStream {
    pub fn open<FrameHandler, ErrorHandler, RecoveryHandler>(
        config: InputConfig,
        policy: RecoveryPolicy,
        on_frame: FrameHandler,
        on_error: ErrorHandler,
        on_recovery: RecoveryHandler,
    ) -> Result<Self, String>
    where
        FrameHandler: FnMut(&[f32], f32) + Send + 'static,
        ErrorHandler: FnMut(String) + Send + 'static,
        RecoveryHandler: FnMut(RecoveryEvent) + Send + 'static,
    {
        Self::open_cancellable(
            config,
            policy,
            on_frame,
            on_error,
            on_recovery,
            Arc::new(AtomicBool::new(false)),
        )
    }

    /// Open a supervised input on a control worker, with a token that can be
    /// set from the UI while a microphone permission prompt is outstanding.
    ///
    /// There is deliberately no two-second startup deadline: permission is a
    /// human decision. This call waits for the native open/cleanup to finish,
    /// including after cancellation, so callers must keep it off the UI
    /// thread and serialize attempts. The token cannot interrupt a blocked
    /// OS call, but late completion is closed without publishing success.
    pub fn open_cancellable<FrameHandler, ErrorHandler, RecoveryHandler>(
        config: InputConfig,
        policy: RecoveryPolicy,
        on_frame: FrameHandler,
        on_error: ErrorHandler,
        on_recovery: RecoveryHandler,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, String>
    where
        FrameHandler: FnMut(&[f32], f32) + Send + 'static,
        ErrorHandler: FnMut(String) + Send + 'static,
        RecoveryHandler: FnMut(RecoveryEvent) + Send + 'static,
    {
        if cancelled.load(Ordering::Acquire) {
            return Err(INPUT_CANCELLED.to_string());
        }
        let config = config.normalized();
        let on_frame = share_frame_handler(on_frame);
        let on_error = share_error_handler(on_error);
        let on_recovery: SharedRecoveryHandler = Arc::new(Mutex::new(Box::new(on_recovery)));

        let (control_tx, control_rx) = mpsc::channel();
        let (errors_tx, errors_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();

        // The first capture is opened inside the supervisor thread: cpal
        // streams are not Send on every platform (e.g. macOS CoreAudio), so a
        // stream must never cross threads. Startup failures still surface
        // synchronously through `ready`.
        let supervisor = {
            let control_tx = control_tx.clone();
            let cancelled = cancelled.clone();
            thread::spawn(move || {
                supervise(
                    config,
                    policy,
                    SupervisorChannels {
                        commands: control_rx,
                        errors: errors_rx,
                        errors_tx,
                        command_tx: control_tx,
                    },
                    on_frame,
                    on_error,
                    on_recovery,
                    ready_tx,
                    cancelled,
                );
            })
        };

        match wait_for_startup(&ready_rx, &cancelled) {
            Ok(()) => Ok(Self {
                control: Some(control_tx),
                supervisor: Some(supervisor),
                cancelled,
            }),
            Err(error) => {
                cancelled.store(true, Ordering::Release);
                let _ = control_tx.send(SupervisorCommand::Stop);
                let _ = supervisor.join();
                Err(error)
            }
        }
    }

    /// Stop and release this capture. Native teardown and the thread join can
    /// block, so this belongs on the same control worker that opens streams.
    pub fn stop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(control) = self.control.take() {
            let _ = control.send(SupervisorCommand::Stop);
        }
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
    }
}

fn wait_for_startup(
    ready: &mpsc::Receiver<Result<(), String>>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let result = ready
        .recv()
        .map_err(|_| "Audio input supervisor ended before startup completed".to_string())?;
    if cancelled.load(Ordering::Acquire) {
        return Err(INPUT_CANCELLED.to_string());
    }
    result
}

impl Drop for SupervisedInputStream {
    fn drop(&mut self) {
        self.stop();
    }
}

struct SupervisorChannels {
    commands: mpsc::Receiver<SupervisorCommand>,
    errors: mpsc::Receiver<String>,
    errors_tx: mpsc::Sender<String>,
    command_tx: mpsc::Sender<SupervisorCommand>,
}

fn emit_recovery(handler: &SharedRecoveryHandler, event: RecoveryEvent) {
    if let Ok(mut handler) = handler.lock() {
        handler(event);
    }
}

fn wait_for_stop(
    commands: &mpsc::Receiver<SupervisorCommand>,
    cancelled: &AtomicBool,
    duration: Duration,
) -> bool {
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Acquire) {
            return true;
        }
        match commands.try_recv() {
            Ok(SupervisorCommand::Stop) | Err(mpsc::TryRecvError::Disconnected) => return true,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let remaining = duration.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return false;
        }
        match commands.recv_timeout(remaining.min(PROCESSOR_POLL_INTERVAL)) {
            Ok(SupervisorCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn supervise(
    config: InputConfig,
    policy: RecoveryPolicy,
    channels: SupervisorChannels,
    on_frame: SharedFrameHandler,
    on_error: SharedErrorHandler,
    on_recovery: SharedRecoveryHandler,
    ready: mpsc::Sender<Result<(), String>>,
    cancelled: Arc<AtomicBool>,
) {
    let mut capture = match Capture::open(
        &config,
        on_frame.clone(),
        channels.errors_tx.clone(),
        None,
        cancelled.clone(),
    ) {
        Ok(capture) => capture,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if cancelled.load(Ordering::Acquire) {
        capture.stop();
        let _ = ready.send(Err(INPUT_CANCELLED.to_string()));
        return;
    }
    if let Err(error) = capture.play() {
        let _ = ready.send(Err(error));
        return;
    }
    if cancelled.load(Ordering::Acquire) {
        capture.stop();
        let _ = ready.send(Err(INPUT_CANCELLED.to_string()));
        return;
    }
    if ready.send(Ok(())).is_err() {
        capture.stop();
        return;
    }

    let mut machine = RecoveryMachine::new(policy.max_attempts, policy.backoff.clone());
    let mut last_progress = capture.progress.load(Ordering::Relaxed);
    let mut last_progress_change = Instant::now();
    let mut failure_reported = false;

    'outer: loop {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        // Drain cpal stream errors first; each one means the stream is lost.
        let mut lost_reason: Option<String> = None;
        if wait_for_stop(&channels.commands, &cancelled, policy.tick) {
            break;
        }
        while let Ok(reason) = channels.errors.try_recv() {
            lost_reason = Some(reason);
        }

        if lost_reason.is_none() && !machine.is_exhausted() {
            // Stall watchdog: data must keep flowing while the stream plays.
            let progress = capture.progress.load(Ordering::Relaxed);
            if progress != last_progress {
                last_progress = progress;
                last_progress_change = Instant::now();
                machine.recovered();
                failure_reported = false;
            } else if last_progress_change.elapsed() > policy.stall_timeout {
                lost_reason =
                    Some("Input stream stalled — no audio data from the device".to_string());
            }
        }

        let Some(mut reason) = lost_reason else {
            continue;
        };
        if machine.is_exhausted() {
            continue;
        }
        emit_recovery(
            &on_recovery,
            RecoveryEvent::StreamLost {
                reason: reason.clone(),
            },
        );

        // Recovery loop: bounded reopen attempts with backoff.
        let mut step = machine.stream_lost();
        loop {
            if cancelled.load(Ordering::Acquire) {
                break 'outer;
            }
            match step {
                RecoveryStep::GiveUp => {
                    if !failure_reported {
                        failure_reported = true;
                        emit_recovery(
                            &on_recovery,
                            RecoveryEvent::Failed {
                                reason: reason.clone(),
                                attempts: machine.attempts(),
                            },
                        );
                        call_shared_error(
                            &on_error,
                            format!(
                                "Audio input stream could not be recovered after {} attempts: {reason}",
                                machine.attempts()
                            ),
                        );
                    }
                    break;
                }
                RecoveryStep::Retry { attempt, delay } => {
                    emit_recovery(
                        &on_recovery,
                        RecoveryEvent::Attempted {
                            attempt,
                            max_attempts: machine.max_attempts(),
                        },
                    );
                    // Interruptible backoff: a stop request aborts
                    // immediately; fresh cpal errors drain on the next tick.
                    if wait_for_stop(&channels.commands, &cancelled, delay) {
                        break 'outer;
                    }
                    capture.stop();
                    if cancelled.load(Ordering::Acquire) {
                        break 'outer;
                    }
                    match Capture::open(
                        &config,
                        on_frame.clone(),
                        channels.errors_tx.clone(),
                        None,
                        cancelled.clone(),
                    ) {
                        Ok(mut new_capture) => {
                            if cancelled.load(Ordering::Acquire) {
                                new_capture.stop();
                                break 'outer;
                            }
                            match new_capture.play() {
                                Ok(()) => {
                                    if cancelled.load(Ordering::Acquire) {
                                        new_capture.stop();
                                        break 'outer;
                                    }
                                    capture = new_capture;
                                    machine.attempt_succeeded();
                                    last_progress = capture.progress.load(Ordering::Relaxed);
                                    last_progress_change = Instant::now();
                                    emit_recovery(
                                        &on_recovery,
                                        RecoveryEvent::Succeeded { attempt },
                                    );
                                    continue 'outer;
                                }
                                Err(play_error) => {
                                    reason = play_error;
                                    step = machine.attempt_failed();
                                }
                            }
                        }
                        Err(open_error) => {
                            reason = open_error;
                            step = machine.attempt_failed();
                        }
                    }
                }
            }
        }
    }
    capture.stop();
    drop(channels.command_tx);
}

pub fn default_input_available() -> bool {
    cpal::default_host().default_input_device().is_some()
}

pub fn input_device_names() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

fn find_input_device(host: &cpal::Host, selected: Option<&str>) -> Result<cpal::Device, String> {
    if let Some(selected) = selected {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(device) =
                devices.find(|device| device.name().map(|name| name == selected).unwrap_or(false))
            {
                return Ok(device);
            }
        }
        return Err(format!("Input device is no longer available: {selected}"));
    }
    host.default_input_device()
        .ok_or_else(|| "No input microphone found".to_string())
}

#[allow(clippy::too_many_arguments)]
fn build_input_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    channels: usize,
    samples_tx: mpsc::SyncSender<AudioChunk>,
    pool_rx: mpsc::Receiver<AudioChunk>,
    pool_tx: mpsc::SyncSender<AudioChunk>,
    errors: mpsc::Sender<String>,
    forward_errors: Option<SharedErrorHandler>,
    progress: Arc<AtomicU64>,
) -> Result<Stream, String> {
    match sample_format {
        SampleFormat::I8 => build_typed_input_stream::<i8>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::I16 => build_typed_input_stream::<i16>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::I32 => build_typed_input_stream::<i32>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::I64 => build_typed_input_stream::<i64>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::U8 => build_typed_input_stream::<u8>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::U16 => build_typed_input_stream::<u16>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::U32 => build_typed_input_stream::<u32>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::U64 => build_typed_input_stream::<u64>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::F32 => build_typed_input_stream::<f32>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        SampleFormat::F64 => build_typed_input_stream::<f64>(
            device,
            config,
            channels,
            samples_tx,
            pool_rx,
            pool_tx,
            errors,
            forward_errors,
            progress,
        ),
        format => Err(format!("Unsupported microphone sample format: {format}")),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_typed_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    samples_tx: mpsc::SyncSender<AudioChunk>,
    pool_rx: mpsc::Receiver<AudioChunk>,
    pool_tx: mpsc::SyncSender<AudioChunk>,
    errors: mpsc::Sender<String>,
    forward_errors: Option<SharedErrorHandler>,
    progress: Arc<AtomicU64>,
) -> Result<Stream, String>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let Ok(mut chunk) = pool_rx.try_recv() else {
                    return;
                };
                chunk.length = 0;
                for frame in data.chunks(channels).take(AUDIO_CHUNK_CAPACITY) {
                    let sum = frame
                        .iter()
                        .map(|sample| f32::from_sample(*sample))
                        .sum::<f32>();
                    chunk.samples[chunk.length] = sum / frame.len() as f32;
                    chunk.length += 1;
                }
                progress.fetch_add(chunk.length as u64, Ordering::Relaxed);

                if let Err(error) = samples_tx.try_send(chunk) {
                    let chunk = match error {
                        mpsc::TrySendError::Full(chunk)
                        | mpsc::TrySendError::Disconnected(chunk) => chunk,
                    };
                    let _ = pool_tx.try_send(chunk);
                }
            },
            move |error| {
                let message = error.to_string();
                let _ = errors.send(message.clone());
                if let Some(forward) = &forward_errors {
                    call_shared_error(forward, message);
                }
            },
            None,
        )
        .map_err(|error| format!("Could not create microphone stream: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn process_frames(
    window_size: usize,
    frame_interval: Duration,
    sample_rate: f32,
    samples_rx: mpsc::Receiver<AudioChunk>,
    pool_tx: mpsc::SyncSender<AudioChunk>,
    on_frame: SharedFrameHandler,
    stopped: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
) {
    let mut ring = SampleWindow::new(window_size);
    let mut window = vec![0.0; window_size];
    let mut last_frame = Instant::now() - frame_interval;

    while !stopped.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
        let chunk = match samples_rx.recv_timeout(PROCESSOR_POLL_INTERVAL) {
            Ok(chunk) => chunk,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
            break;
        }
        ring.push(&chunk.samples[..chunk.length]);
        if last_frame.elapsed() >= frame_interval && ring.copy_ordered(&mut window) {
            last_frame = Instant::now();
            if let Ok(mut on_frame) = on_frame.lock() {
                if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
                    break;
                }
                on_frame(&window, sample_rate);
            }
        }
        // Returning a recycled buffer must not prevent cancellation when a
        // backend has stopped consuming its pool but still owns the receiver.
        let _ = pool_tx.try_send(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_window_preserves_chronological_order_after_wrap() {
        let mut ring = SampleWindow::new(4);
        ring.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut output = [0.0; 4];
        assert!(ring.copy_ordered(&mut output));
        assert_eq!(output, [2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn incomplete_window_is_not_emitted() {
        let mut ring = SampleWindow::new(4);
        ring.push(&[1.0, 2.0]);
        assert!(!ring.copy_ordered(&mut [0.0; 4]));
    }

    fn spawn_test_processor(
        samples: mpsc::Receiver<AudioChunk>,
        pool: mpsc::SyncSender<AudioChunk>,
        on_frame: SharedFrameHandler,
        stopped: Arc<AtomicBool>,
        cancelled: Arc<AtomicBool>,
    ) -> (JoinHandle<()>, mpsc::Receiver<()>) {
        let (finished_tx, finished_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            process_frames(
                64,
                Duration::from_millis(1),
                48_000.0,
                samples,
                pool,
                on_frame,
                stopped,
                cancelled,
            );
            let _ = finished_tx.send(());
        });
        (thread, finished_rx)
    }

    #[test]
    fn processor_stops_while_native_callback_still_owns_sender() {
        let (_callback_sender, samples) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        let (pool, _callback_pool) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        let stopped = Arc::new(AtomicBool::new(false));
        let (processor, finished) = spawn_test_processor(
            samples,
            pool,
            share_frame_handler(|_, _| panic!("empty input must not produce a frame")),
            stopped.clone(),
            Arc::new(AtomicBool::new(false)),
        );
        stopped.store(true, Ordering::Release);
        finished
            .recv_timeout(Duration::from_secs(1))
            .expect("stop must not wait for the backend to destroy its callback");
        processor.join().unwrap();
    }

    #[test]
    fn session_cancellation_discards_already_queued_audio() {
        let (samples_tx, samples) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        let (pool, _callback_pool) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        let mut chunk = AudioChunk::new();
        chunk.length = 64;
        samples_tx.send(chunk).unwrap();
        let (processor, finished) = spawn_test_processor(
            samples,
            pool,
            share_frame_handler(|_, _| panic!("cancelled session must not publish a frame")),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(true)),
        );
        finished
            .recv_timeout(Duration::from_secs(1))
            .expect("queued audio must not delay cancellation");
        processor.join().unwrap();
    }

    #[test]
    fn full_recycle_pool_does_not_block_processor_shutdown() {
        let (samples_tx, samples) = mpsc::sync_channel(AUDIO_CHUNK_POOL_SIZE);
        let (pool, _callback_pool) = mpsc::sync_channel(1);
        pool.send(AudioChunk::new()).unwrap();
        let mut chunk = AudioChunk::new();
        chunk.length = 64;
        chunk.samples[..64].fill(0.25);
        samples_tx.send(chunk).unwrap();

        let (frame_tx, frame_rx) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let (processor, finished) = spawn_test_processor(
            samples,
            pool,
            share_frame_handler(move |samples, sample_rate| {
                assert_eq!(samples, &[0.25; 64]);
                assert_eq!(sample_rate, 48_000.0);
                frame_tx.send(()).unwrap();
            }),
            stopped.clone(),
            Arc::new(AtomicBool::new(false)),
        );
        frame_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the active session should process its unchanged sample window");
        stopped.store(true, Ordering::Release);
        finished
            .recv_timeout(Duration::from_secs(1))
            .expect("returning a buffer to a retained full pool must not block stop");
        processor.join().unwrap();
    }

    #[test]
    fn permission_response_can_take_more_than_two_seconds() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let backend = thread::spawn(move || {
            thread::sleep(Duration::from_millis(2_100));
            ready_tx.send(Ok(())).unwrap();
        });
        assert_eq!(wait_for_startup(&ready_rx, &AtomicBool::new(false)), Ok(()));
        backend.join().unwrap();
    }

    #[test]
    fn cancelled_start_waits_for_native_completion_and_rejects_late_success() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            finished_tx
                .send(wait_for_startup(&ready_rx, &worker_cancelled))
                .unwrap();
        });
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            finished_rx.recv_timeout(Duration::from_millis(30)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        ready_tx.send(Ok(())).unwrap();
        assert_eq!(
            finished_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Err(INPUT_CANCELLED.to_string())
        );
        worker.join().unwrap();
    }

    #[test]
    fn pre_cancelled_start_returns_without_accessing_audio_backend() {
        let result = SupervisedInputStream::open_cancellable(
            InputConfig {
                device_name: Some("not a physical test device".to_string()),
                ..InputConfig::default()
            },
            RecoveryPolicy::default(),
            |_, _| panic!("cancelled start must not publish audio"),
            |_| panic!("cancelled start must not report a device failure"),
            |_| panic!("cancelled start must not attempt recovery"),
            Arc::new(AtomicBool::new(true)),
        );
        assert_eq!(result.err(), Some(INPUT_CANCELLED.to_string()));
    }

    #[test]
    fn cancellation_interrupts_recovery_backoff_without_a_control_message() {
        let (_control, commands) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (finished_tx, finished_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            finished_tx
                .send(wait_for_stop(
                    &commands,
                    &worker_cancelled,
                    Duration::from_secs(5),
                ))
                .unwrap();
        });
        cancelled.store(true, Ordering::Release);
        assert!(finished_rx.recv_timeout(Duration::from_secs(1)).unwrap());
        worker.join().unwrap();
    }
}
