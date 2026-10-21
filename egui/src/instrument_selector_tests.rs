use super::*;

#[test]
fn every_preset_is_reachable_once_without_reindexing() {
    let tunings = pitch_core::get_tunings();
    let names: Vec<_> = tunings.iter().map(|tuning| tuning.name).collect();
    let mut picker = PresetSelector::default();
    let mut all = Vec::new();
    for instrument in FAMILIES {
        picker.browse(instrument);
        all.extend(picker.indices(&tunings));
    }
    all.sort_unstable();
    assert_eq!(all, (0..tunings.len()).collect::<Vec<_>>());
    assert_eq!(tunings.len(), 35);
    assert_eq!(tunings.iter().map(|tuning| tuning.name).collect::<Vec<_>>(), names);
    assert!(tunings.iter().all(|tuning| family(tuning) != Family::Other));
}

#[test]
fn guitar_and_ukulele_variants_share_their_family() {
    let tunings = pitch_core::get_tunings();
    for (prefix, expected) in [
        ("7-string ", Family::Guitar), ("12-string ", Family::Guitar),
        ("Baritone Standard", Family::Guitar), ("Baritone Drop", Family::Guitar),
        ("Baritone Ukulele", Family::Ukulele),
    ] {
        let matches: Vec<_> = tunings.iter().filter(|tuning| tuning.name.starts_with(prefix)).collect();
        assert!(!matches.is_empty());
        assert!(matches.iter().all(|tuning| family(tuning) == expected));
    }
}

#[test]
fn reopening_restores_the_active_instrument_not_the_last_browsed_family() {
    let tunings = pitch_core::get_tunings();
    let mut picker = PresetSelector::default();
    for (index, tuning) in tunings.iter().enumerate() {
        picker.browse(Family::Other);
        picker.search = "unrelated text".to_owned();
        picker.keyboard_choice = Some(0);
        picker.open(&tunings, index);
        assert_eq!(picker.browsing, family(tuning));
        assert!(picker.search.is_empty());
        assert!(picker.keyboard_choice.is_none());
        assert!(picker.indices(&tunings).contains(&index));
    }
}

#[test]
fn search_is_global_bilingual_accent_insensitive_and_preserves_variants() {
    let tunings = pitch_core::get_tunings();
    let mut picker = PresetSelector::default();
    for (query, expected) in [
        ("charango estándar", "Charango Standard (GCEAE)"),
        ("charangón argentino", "Ronroco Argentine (DGBEB)"),
        ("ronroco argentine", "Ronroco Argentine (DGBEB)"),
        ("violín standard", "Violin Standard (GDAE)"),
        ("guitarra 12 cuerdas", "12-string Standard (E/e A/a D/d G/g B/B E/E)"),
    ] {
        picker.browse(Family::Bass);
        picker.search = query.to_owned();
        let matches = picker.indices(&tunings);
        assert_eq!(matches.len(), 1, "query: {query}");
        assert_eq!(tunings[matches[0]].name, expected);
    }
    picker.search = "no-such-preset".to_owned();
    assert!(picker.indices(&tunings).is_empty());
    picker.search = "   ".to_owned();
    assert!(picker.indices(&tunings).iter().all(|index| family(&tunings[*index]) == Family::Bass));
}

#[test]
fn paired_courses_retain_the_real_octaves_in_the_visible_subtitle() {
    let tunings = pitch_core::get_tunings();
    for prefix in ["Charango ", "Ronroco "] {
        let tuning = tunings.iter().find(|tuning| tuning.name.starts_with(prefix)).unwrap();
        assert_eq!(tuning.strings.len(), 10);
        let summary = notes(tuning, Language::English);
        let courses: Vec<_> = summary.split("  ·  ").collect();
        assert_eq!(courses.len(), 5);
        for (course, pair) in courses.iter().zip(tuning.strings.chunks(2)) {
            for note in pair {
                assert!(course.contains(&get_note_display(note)), "missing octave in {course}");
            }
        }
        assert!(summary.contains('/'), "the split-octave course must be explicit");
        assert_eq!(summary, notes(tuning, Language::Spanish));
    }
}

#[test]
fn popup_fits_the_actual_compact_viewport_at_all_button_positions() {
    for size in [egui::vec2(520.0, 290.0), egui::vec2(680.0, 294.0), egui::vec2(980.0, 720.0)] {
        let screen = Rect::from_min_size(egui::pos2(13.0, 17.0), size);
        for x in [screen.left() + 8.0, screen.right() - 100.0] {
            for y in [screen.top() + 8.0, screen.center().y, screen.bottom() - 36.0] {
                let popup = popup_bounds(screen, Rect::from_min_size(egui::pos2(x, y), egui::vec2(90.0, 28.0)));
                assert!(screen.contains_rect(popup), "{screen:?} does not contain {popup:?}");
                assert!(popup.width() <= 480.0);
                assert!(popup.height() <= 350.0);
            }
        }
    }
}

#[test]
fn keyboard_navigation_never_returns_an_out_of_range_choice() {
    assert_eq!(step_choice(&[], None, true), None);
    assert_eq!(step_choice(&[3, 8, 34], None, true), Some(3));
    assert_eq!(step_choice(&[3, 8, 34], None, false), Some(34));
    assert_eq!(step_choice(&[3, 8, 34], Some(3), false), Some(3));
    assert_eq!(step_choice(&[3, 8, 34], Some(34), true), Some(34));
    assert_eq!(step_choice(&[3, 8, 34], Some(8), true), Some(34));
    assert_eq!(step_choice(&[3, 8, 34], Some(999), true), Some(3));
}

struct Harness {
    context: egui::Context,
    picker: PresetSelector,
    tunings: Vec<Tuning>,
    selected: usize,
    frame: u64,
    size: egui::Vec2,
    language: Language,
}

impl Harness {
    fn new(language: Language, size: egui::Vec2) -> Self {
        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut(|style| style.animation_time = 0.0);
        Self { context, picker: PresetSelector::default(), tunings: pitch_core::get_tunings(), selected: 0, frame: 0, size, language }
    }

    fn draw(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.frame as f64 / 30.0), events, ..Default::default()
        };
        let context = self.context.clone();
        let output = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.add_space((self.size.y - 68.0).max(0.0));
                self.picker.show(ui, "picker-regression", &self.tunings, &mut self.selected, 300.0, self.language);
            });
        });
        self.frame += 1;
        output
    }

    fn click(&mut self, pos: Pos2) -> egui::FullOutput {
        self.draw(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.draw(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
        }
        self.draw(Vec::new());
        self.draw(Vec::new())
    }

    fn key(&mut self, key: egui::Key) {
        for pressed in [true, false] {
            self.draw(vec![egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }]);
        }
        self.draw(Vec::new());
    }

    fn open(&mut self) -> egui::FullOutput {
        self.draw(Vec::new());
        self.draw(Vec::new());
        let output = self.draw(Vec::new());
        let prefix = self.language.text("Afinación:", "Tuning:");
        self.click(widget_center(&output, prefix))
    }
}

fn widget_center(output: &egui::FullOutput, prefix: &str) -> Pos2 {
    let nodes = &output.platform_output.accesskit_update.as_ref().expect("accessibility enabled").nodes;
    let node = nodes.iter().map(|(_, node)| node)
        .find(|node| node.name().unwrap_or("").starts_with(prefix))
        .unwrap_or_else(|| panic!("missing widget {prefix}: {:?}", nodes.iter().map(|(_, node)| node.name()).collect::<Vec<_>>()));
    let bounds = node.bounds().expect("widget bounds");
    egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32)
}

fn search_center(output: &egui::FullOutput) -> Pos2 {
    let node = output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter().map(|(_, node)| node)
        .find(|node| node.role() == egui::accesskit::Role::TextInput).expect("search input");
    let bounds = node.bounds().unwrap();
    egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32)
}

#[test]
fn clicking_an_instrument_only_browses_then_a_tuning_commits_and_closes() {
    for language in [Language::Spanish, Language::English] {
        let mut h = Harness::new(language, egui::vec2(520.0, 290.0));
        let output = h.open();
        assert!(h.context.memory(|memory| memory.any_popup_open()));
        let instrument = language.text("Instrumento: Charango", "Instrument: Charango");
        let output = h.click(widget_center(&output, instrument));
        assert_eq!(h.selected, 0, "browsing must not retune the engine");
        assert_eq!(h.picker.browsing, Family::Charango);
        assert!(h.context.memory(|memory| memory.any_popup_open()));
        h.click(widget_center(&output, language.text("Charango estándar", "Charango Standard")));
        assert_eq!(family(&h.tunings[h.selected]), Family::Charango);
        assert!(!h.context.memory(|memory| memory.any_popup_open()));
    }
}

#[test]
fn escape_and_outside_click_cancel_without_changing_the_active_tuning() {
    for outside in [false, true] {
        let mut h = Harness::new(Language::Spanish, egui::vec2(680.0, 294.0));
        let output = h.open();
        h.click(widget_center(&output, "Instrumento: Ronroco"));
        assert_eq!(h.selected, 0);
        if outside { h.click(egui::pos2(675.0, 280.0)); } else { h.key(egui::Key::Escape); }
        assert_eq!(h.selected, 0);
        assert!(!h.context.memory(|memory| memory.any_popup_open()));
        h.open();
        assert_eq!(h.picker.browsing, Family::Guitar);
    }
}

#[test]
fn enter_selects_the_unique_global_result_after_text_edit_loses_focus() {
    let mut h = Harness::new(Language::Spanish, egui::vec2(520.0, 290.0));
    let output = h.open();
    h.click(search_center(&output));
    h.draw(vec![egui::Event::Text("violín".to_owned())]);
    for _ in 0..3 { h.draw(Vec::new()); }
    assert!(h.context.wants_keyboard_input());
    h.key(egui::Key::Enter);
    assert_eq!(family(&h.tunings[h.selected]), Family::Violin);
    assert!(!h.context.memory(|memory| memory.any_popup_open()));
    assert!(!h.context.wants_keyboard_input());
}

#[test]
fn language_change_does_not_reset_the_browsed_family_or_active_preset() {
    let mut h = Harness::new(Language::Spanish, egui::vec2(680.0, 294.0));
    let output = h.open();
    h.click(widget_center(&output, "Instrumento: Charango"));
    h.language = Language::English;
    for _ in 0..3 { h.draw(Vec::new()); }
    assert_eq!(h.picker.browsing, Family::Charango);
    assert_eq!(h.selected, 0);
    assert!(h.context.memory(|memory| memory.any_popup_open()));
}
