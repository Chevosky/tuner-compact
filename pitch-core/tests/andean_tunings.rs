use pitch_core::{
    get_cents, get_note_display, get_tunings, EngineConfig, FrameResolver, TunerEngine, Tuning,
};

const CHARANGO: &str = "Charango Standard (GCEAE)";
const RONROCO: &str = "Ronroco Argentine (DGBEB)";

fn tuning_named(name: &str) -> Tuning {
    get_tunings()
        .into_iter()
        .find(|tuning| tuning.name == name)
        .unwrap_or_else(|| panic!("missing tuning {name}"))
}

#[test]
fn andean_presets_keep_ten_strings_and_their_course_octaves() {
    // Source: Aquila's charango and Argentine ronroco/charangon string sets:
    // https://aquilacorde.com/en/negozio/modern-instrument-string-sets/south-america-instruments/nylgut-spanish-instruments/nylgut-charango/
    // https://aquilacorde.com/en/negozio/modern-instrument-string-sets/south-america-instruments/nylgut-spanish-instruments/nylgut-ronroco/
    // Five double courses, from the fifth to the first. The middle course
    // contains both octaves; these reentrant tunings must not be pitch-sorted.
    let cases = [
        (
            CHARANGO,
            ["G4", "G4", "C5", "C5", "E4", "E5", "A4", "A4", "E5", "E5"],
        ),
        (
            RONROCO,
            ["D4", "D4", "G4", "G4", "B3", "B4", "E4", "E4", "B4", "B4"],
        ),
    ];

    for (name, expected) in cases {
        let tuning = tuning_named(name);
        let actual = tuning
            .strings
            .iter()
            .map(get_note_display)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{name} course layout");

        for (course_index, course) in tuning.strings.chunks_exact(2).enumerate() {
            let interval = get_cents(course[1].frequency, course[0].frequency);
            let expected_interval = if course_index == 2 { 1_200.0 } else { 0.0 };
            assert!(
                (interval - expected_interval).abs() < 0.01,
                "{name}, course {}: {interval} cents",
                5 - course_index
            );
        }
    }
}

#[test]
fn andean_presets_resolve_each_string_after_a4_changes() {
    for name in [CHARANGO, RONROCO] {
        let tuning = tuning_named(name);
        let mut resolver = FrameResolver::new(440.0, tuning.clone(), None);

        for a4 in [432.0, 440.0, 442.0] {
            resolver.set_a4(a4);
            for string in &tuning.strings {
                let target_frequency = string.frequency * a4 / 440.0;
                for offset in [-12.0_f32, 12.0] {
                    let played = target_frequency * 2.0_f32.powf(offset / 1_200.0);
                    let frame = resolver.resolve(Some(played));
                    let target = frame.target.expect("preset should select a string");
                    let label = get_note_display(string);

                    assert_eq!(get_note_display(&target), label, "{name}, A4={a4}");
                    assert_eq!(frame.note, label, "{name}, A4={a4}");
                    assert!((target.frequency - target_frequency).abs() < 0.001);
                    assert!((frame.cents - offset).abs() < 0.01);
                    assert!(!frame.in_tune);
                }

                let frame = resolver.resolve(Some(target_frequency));
                assert!(
                    frame.in_tune,
                    "{name}, {} at A4={a4}",
                    get_note_display(string)
                );
                assert!(frame.cents.abs() < 0.01);
            }
        }
    }
}

#[test]
fn andean_middle_course_octaves_survive_detection_and_resolution() {
    let sample_rate = 48_000.0;
    for name in [CHARANGO, RONROCO] {
        let tuning = tuning_named(name);
        for a4 in [440.0, 442.0] {
            for (string_index, detune_cents) in [(4, -9.0_f32), (5, 9.0)] {
                let expected = &tuning.strings[string_index];
                let reference = expected.frequency * a4 / 440.0;
                let frequency = reference * 2.0_f32.powf(detune_cents / 1_200.0);
                // A single plucked string with three harmonics. Both strings
                // of an octave course are tuned separately, not as a chord.
                let buffer = (0..4_096)
                    .map(|index| {
                        let phase = std::f32::consts::TAU * frequency * index as f32 / sample_rate;
                        0.5 * phase.sin() + 0.2 * (2.0 * phase).sin() + 0.1 * (3.0 * phase).sin()
                    })
                    .collect::<Vec<_>>();
                let mut engine = TunerEngine::with_config(EngineConfig {
                    a4,
                    tuning: Some(tuning.clone()),
                    spectrum_bins: 0,
                    ..EngineConfig::default()
                });
                let mut frame = engine.process(&buffer, sample_rate);
                for _ in 1..6 {
                    frame = engine.process(&buffer, sample_rate);
                }

                let measured = frame.freq.expect("single string should produce a reading");
                let label = get_note_display(expected);
                assert!(
                    get_cents(measured, frequency).abs() < 3.0,
                    "{name}, {label}, A4={a4}: expected {frequency} Hz, got {measured} Hz"
                );
                assert_eq!(frame.note, label);
                let target = frame.target.expect("preset target");
                assert_eq!(get_note_display(&target), label);
                assert!((target.frequency - reference).abs() < 0.001);
                assert!((frame.cents - detune_cents).abs() < 3.0);
                assert!(!frame.in_tune);
            }
        }
    }
}
