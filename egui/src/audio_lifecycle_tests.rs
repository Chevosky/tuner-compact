use super::*;
use crate::state::TunerViewState;
use std::time::Instant;

// These gates deliberately model native calls that have not returned. A UI
// operation must finish while the gate remains shut, rather than relying on
// a particularly fast fake audio device.
const TEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum Audit {
    Opening(u64, Option<String>, Arc<AtomicBool>),
    Stopping(u64),
    Stopped(u64),
    Enumerating,
    ToneOpened(Arc<AtomicBool>),
    ToneStopping,
    ToneStopped,
    Exited,
}

#[derive(Default)]
struct Gates {
    open: Option<mpsc::Receiver<()>>,
    stop: Option<mpsc::Receiver<()>>,
    enumerate: Option<mpsc::Receiver<()>>,
    tone_stop: Option<mpsc::Receiver<()>>,
    fail_first_open: bool,
}

struct FakeBackend {
    audit: mpsc::Sender<Audit>,
    gates: Gates,
}

impl AudioBackend for FakeBackend {
    type Input = InputRequest;
    type Output = ();

    fn devices(&mut self) -> Result<Vec<String>, String> {
        let _ = self.audit.send(Audit::Enumerating);
        if let Some(gate) = self.gates.enumerate.take() {
            let _ = gate.recv();
        }
        Ok(vec!["Test microphone".into()])
    }

    fn open_input(&mut self, request: InputRequest) -> Result<Self::Input, String> {
        let _ = self.audit.send(Audit::Opening(
            request.generation,
            request.device_name.clone(),
            request.cancelled.clone(),
        ));
        if let Some(gate) = self.gates.open.take() {
            let _ = gate.recv();
        }
        if std::mem::take(&mut self.gates.fail_first_open) {
            Err("permission denied by test device".into())
        } else {
            Ok(request)
        }
    }

    fn stop_input(&mut self, input: Self::Input) {
        let _ = self.audit.send(Audit::Stopping(input.generation));
        if let Some(gate) = self.gates.stop.take() {
            let _ = gate.recv();
        }
        let _ = self.audit.send(Audit::Stopped(input.generation));
    }

    fn open_tone(
        &mut self,
        _frequency: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self::Output, String> {
        let _ = self.audit.send(Audit::ToneOpened(cancelled));
        Ok(())
    }

    fn stop_tone(&mut self, _output: Self::Output) {
        let _ = self.audit.send(Audit::ToneStopping);
        if let Some(gate) = self.gates.tone_stop.take() {
            let _ = gate.recv();
        }
        let _ = self.audit.send(Audit::ToneStopped);
    }
}

impl Drop for FakeBackend {
    fn drop(&mut self) {
        let _ = self.audit.send(Audit::Exited);
    }
}

fn manager_with(gates: Gates) -> (AudioManager, mpsc::Receiver<Audit>) {
    let (audit, observed) = mpsc::channel();
    let mut manager = AudioManager::default();
    manager
        .spawn_worker(move |_| FakeBackend { audit, gates })
        .unwrap();
    (manager, observed)
}

fn start(manager: &mut AudioManager) {
    manager
        .start_input(
            Arc::new(Mutex::new(TunerViewState::default())),
            Arc::new(Mutex::new(TunerEngine::new(440.0))),
            egui::Context::default(),
        )
        .unwrap();
}

fn next(audit: &mpsc::Receiver<Audit>) -> Audit {
    audit.recv_timeout(TEST_TIMEOUT).expect("worker progress")
}

fn nonblocking<T, F>(operation: F, release_if_blocked: &mpsc::Sender<()>) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (done, result) = mpsc::channel();
    let ui = thread::spawn(move || {
        let _ = done.send(operation());
    });
    let outcome = result.recv_timeout(TEST_TIMEOUT);
    if outcome.is_err() {
        // Release the fake native call even when the assertion fails, so a
        // regression does not strand a test thread or hang the whole suite.
        let _ = release_if_blocked.send(());
    }
    ui.join().unwrap();
    outcome.expect("UI operation waited for a blocked native call")
}

fn until_status(manager: &mut AudioManager, expected: InputStatus) -> Vec<AudioEvent> {
    let deadline = Instant::now() + TEST_TIMEOUT;
    let mut events = Vec::new();
    loop {
        events.extend(manager.poll(&egui::Context::default()));
        if manager.input_status() == expected {
            return events;
        }
        assert!(Instant::now() < deadline, "missing status {expected:?}");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn permission_wait_can_be_cancelled_and_ui_dropped_without_joining() {
    let (release, gate) = mpsc::channel();
    let (mut manager, audit) = manager_with(Gates {
        open: Some(gate),
        ..Gates::default()
    });
    manager = nonblocking(
        move || {
            start(&mut manager);
            manager
        },
        &release,
    );
    let cancelled = match next(&audit) {
        Audit::Opening(_, _, cancelled) => cancelled,
        event => panic!("unexpected {event:?}"),
    };
    manager = nonblocking(
        move || {
            manager.stop_input();
            manager
        },
        &release,
    );
    assert!(cancelled.load(Ordering::Acquire));
    assert_eq!(manager.input_status(), InputStatus::Stopping);
    nonblocking(move || drop(manager), &release);
    release.send(()).unwrap();
    assert!(matches!(next(&audit), Audit::Stopping(1)));
    assert!(matches!(next(&audit), Audit::Stopped(1)));
    assert!(matches!(next(&audit), Audit::Exited));
}

#[test]
fn stopping_does_not_block_the_ui_while_native_teardown_waits() {
    let (release, gate) = mpsc::channel();
    let (mut manager, audit) = manager_with(Gates {
        stop: Some(gate),
        ..Gates::default()
    });
    start(&mut manager);
    assert!(matches!(next(&audit), Audit::Opening(1, _, _)));
    until_status(&mut manager, InputStatus::Running);
    manager = nonblocking(
        move || {
            manager.stop_input();
            manager
        },
        &release,
    );
    assert!(matches!(next(&audit), Audit::Stopping(1)));
    manager = nonblocking(
        move || {
            assert!(manager.poll(&egui::Context::default()).is_empty());
            assert_eq!(manager.input_status(), InputStatus::Stopping);
            manager
        },
        &release,
    );
    release.send(()).unwrap();
    assert!(matches!(next(&audit), Audit::Stopped(1)));
    assert!(until_status(&mut manager, InputStatus::Stopped).contains(&AudioEvent::InputStopped));
    drop(manager);
    assert!(matches!(next(&audit), Audit::Exited));
}

#[test]
fn rapid_device_changes_close_cancelled_open_and_skip_obsolete_requests() {
    let (release, gate) = mpsc::channel();
    let (mut manager, audit) = manager_with(Gates {
        open: Some(gate),
        ..Gates::default()
    });
    manager.selected_input_device = Some("First".into());
    start(&mut manager);
    assert!(matches!(next(&audit), Audit::Opening(1, Some(name), _) if name == "First"));
    manager.selected_input_device = Some("Obsolete".into());
    start(&mut manager);
    manager.selected_input_device = Some("Latest".into());
    start(&mut manager);
    release.send(()).unwrap();
    assert!(matches!(next(&audit), Audit::Stopping(1)));
    assert!(matches!(next(&audit), Audit::Stopped(1)));
    assert!(matches!(next(&audit), Audit::Opening(3, Some(name), _) if name == "Latest"));
    let events = until_status(&mut manager, InputStatus::Running);
    assert_eq!(events, vec![AudioEvent::InputStarted]);
    drop(manager);
    assert!(matches!(next(&audit), Audit::Stopping(3)));
    assert!(matches!(next(&audit), Audit::Stopped(3)));
    assert!(matches!(next(&audit), Audit::Exited));
}

#[test]
fn failed_start_is_reported_and_a_later_request_can_succeed() {
    let (mut manager, audit) = manager_with(Gates {
        fail_first_open: true,
        ..Gates::default()
    });
    start(&mut manager);
    assert!(matches!(next(&audit), Audit::Opening(1, _, _)));
    let events = until_status(&mut manager, InputStatus::Stopped);
    assert!(events.contains(&AudioEvent::InputFailed(
        "permission denied by test device".into()
    )));
    start(&mut manager);
    assert!(matches!(next(&audit), Audit::Opening(2, _, _)));
    until_status(&mut manager, InputStatus::Running);
    drop(manager);
    assert!(matches!(next(&audit), Audit::Stopping(2)));
    assert!(matches!(next(&audit), Audit::Stopped(2)));
    assert!(matches!(next(&audit), Audit::Exited));
}

#[test]
fn stale_events_and_a_late_start_cannot_revive_a_failed_session() {
    let (events, receiver) = mpsc::channel();
    let mut manager = AudioManager::default();
    manager.events = Some(receiver);
    manager.control.input_generation.store(7, Ordering::Release);
    manager.input_cancelled = Some(Arc::new(AtomicBool::new(false)));
    manager.input_status = InputStatus::Starting;
    events.send(WorkerEvent::InputStarted(6)).unwrap();
    events
        .send(WorkerEvent::InputFailed(6, "obsolete error".into()))
        .unwrap();
    assert!(manager.poll(&egui::Context::default()).is_empty());
    assert_eq!(manager.input_status(), InputStatus::Starting);
    // An error callback can race the owner's publication of InputStarted.
    events
        .send(WorkerEvent::InputFailed(7, "current failure".into()))
        .unwrap();
    events.send(WorkerEvent::InputStarted(7)).unwrap();
    assert_eq!(
        manager.poll(&egui::Context::default()),
        vec![AudioEvent::InputFailed("current failure".into())]
    );
    assert_eq!(manager.input_status(), InputStatus::Stopped);
}

#[test]
fn shutdown_discards_queued_device_queries_and_tone_start() {
    let (release, gate) = mpsc::channel();
    let (mut manager, audit) = manager_with(Gates {
        open: Some(gate),
        ..Gates::default()
    });
    start(&mut manager);
    assert!(matches!(next(&audit), Audit::Opening(1, _, _)));
    manager.refresh();
    manager.play_tone(110.0).unwrap();
    nonblocking(move || drop(manager), &release);
    release.send(()).unwrap();
    assert!(matches!(next(&audit), Audit::Stopping(1)));
    assert!(matches!(next(&audit), Audit::Stopped(1)));
    assert!(matches!(next(&audit), Audit::Exited));
}

#[test]
fn device_enumeration_and_reference_tone_stop_never_run_on_the_ui_thread() {
    let (release_devices, device_gate) = mpsc::channel();
    let (release_tone, tone_gate) = mpsc::channel();
    let (mut manager, audit) = manager_with(Gates {
        enumerate: Some(device_gate),
        tone_stop: Some(tone_gate),
        ..Gates::default()
    });
    manager = nonblocking(
        move || {
            manager.refresh();
            manager
        },
        &release_devices,
    );
    assert!(matches!(next(&audit), Audit::Enumerating));
    manager = nonblocking(
        move || {
            manager.poll(&egui::Context::default());
            manager
        },
        &release_devices,
    );
    release_devices.send(()).unwrap();
    manager.play_tone(110.0).unwrap();
    assert!(matches!(next(&audit), Audit::ToneOpened(_)));
    manager = nonblocking(
        move || {
            manager.stop_tone();
            manager
        },
        &release_tone,
    );
    assert!(!manager.is_tone_playing());
    assert!(matches!(next(&audit), Audit::ToneStopping));
    nonblocking(move || drop(manager), &release_tone);
    release_tone.send(()).unwrap();
    assert!(matches!(next(&audit), Audit::ToneStopped));
    assert!(matches!(next(&audit), Audit::Exited));
}

#[test]
fn stop_replace_and_drop_silence_tone_while_microphone_open_blocks_owner() {
    for action in ["stop", "replace", "drop"] {
        let (release, gate) = mpsc::channel();
        let (mut manager, audit) = manager_with(Gates {
            open: Some(gate),
            ..Gates::default()
        });
        manager.play_tone(110.0).unwrap();
        let tone_cancelled = match next(&audit) {
            Audit::ToneOpened(cancelled) => cancelled,
            event => panic!("unexpected {event:?}"),
        };
        let mut phase = 0.25;
        let mut float_samples = [0.0_f32; 64];
        fill_tone_buffer(
            &mut float_samples,
            2,
            110.0,
            48_000.0,
            &mut phase,
            &tone_cancelled,
        );
        assert!(float_samples.iter().any(|sample| *sample != 0.0));

        start(&mut manager);
        assert!(matches!(next(&audit), Audit::Opening(1, _, _)));
        let manager = nonblocking(
            move || match action {
                "stop" => {
                    manager.stop_tone();
                    Some(manager)
                }
                "replace" => {
                    manager.play_tone(220.0).unwrap();
                    Some(manager)
                }
                "drop" => {
                    drop(manager);
                    None
                }
                _ => unreachable!(),
            },
            &release,
        );
        assert!(tone_cancelled.load(Ordering::Acquire), "{action}");
        // The owner still cannot process StopTone or destroy the output.
        assert!(audit.try_recv().is_err(), "owner escaped the native gate");
        // Exercise the exact function used by the real output callback,
        // including unsigned equilibrium rather than integer zero.
        fill_tone_buffer(
            &mut float_samples,
            2,
            110.0,
            48_000.0,
            &mut phase,
            &tone_cancelled,
        );
        assert_eq!(
            float_samples,
            [<f32 as Sample>::EQUILIBRIUM; 64],
            "{action}"
        );
        let mut unsigned_samples = [0_u8; 64];
        fill_tone_buffer(
            &mut unsigned_samples,
            2,
            110.0,
            48_000.0,
            &mut phase,
            &tone_cancelled,
        );
        assert_eq!(
            unsigned_samples,
            [<u8 as Sample>::EQUILIBRIUM; 64],
            "{action}"
        );

        if let Some(manager) = manager {
            // Replacing while blocked also leaves a pending new tone; Drop
            // must cancel that request before the owner can open it.
            let pending = manager.tone_cancelled.clone();
            nonblocking(move || drop(manager), &release);
            if let Some(pending) = pending {
                assert!(pending.load(Ordering::Acquire));
            }
        }
        release.send(()).unwrap();
        assert!(matches!(next(&audit), Audit::Stopping(1)));
        assert!(matches!(next(&audit), Audit::Stopped(1)));
        assert!(matches!(next(&audit), Audit::ToneStopping));
        assert!(matches!(next(&audit), Audit::ToneStopped));
        assert!(matches!(next(&audit), Audit::Exited));
    }
}
