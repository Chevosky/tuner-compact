//! Instrument-first picker. Browsing never mutates the active tuning.
//! All choices retain the original registry indices used by saved preferences.

use crate::i18n::Language;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke};
use pitch_core::{get_note_display, Tuning};

const TEXT: Color32 = Color32::from_rgb(218, 225, 221);
const MUTED: Color32 = Color32::from_rgb(148, 161, 154);
const GREEN: Color32 = Color32::from_rgb(146, 232, 141);
const PAD: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Family {
    #[default]
    Guitar,
    Bass,
    Ukulele,
    Charango,
    Ronroco,
    Mandolin,
    Banjo,
    Violin,
    Viola,
    Cello,
    Chromatic,
    Other,
}

const FAMILIES: [Family; 12] = [
    Family::Guitar, Family::Bass, Family::Ukulele, Family::Charango, Family::Ronroco,
    Family::Mandolin, Family::Banjo, Family::Violin, Family::Viola, Family::Cello,
    Family::Chromatic, Family::Other,
];

impl Family {
    fn title(self, language: Language) -> &'static str {
        let (es, en) = match self {
            Self::Guitar => ("Guitarra", "Guitar"),
            Self::Bass => ("Bajo", "Bass"),
            Self::Ukulele => ("Ukelele", "Ukulele"),
            Self::Charango => ("Charango", "Charango"),
            Self::Ronroco => ("Ronroco", "Ronroco"),
            Self::Mandolin => ("Mandolina", "Mandolin"),
            Self::Banjo => ("Banjo", "Banjo"),
            Self::Violin => ("Violín", "Violin"),
            Self::Viola => ("Viola", "Viola"),
            Self::Cello => ("Violonchelo", "Cello"),
            Self::Chromatic => ("Cromático", "Chromatic"),
            Self::Other => ("Otros", "Other"),
        };
        language.text(es, en)
    }
}

fn family(tuning: &Tuning) -> Family {
    let name = tuning.name;
    if tuning.strings.is_empty() { return Family::Chromatic; }
    for (prefix, family) in [
        ("Charango ", Family::Charango), ("Ronroco ", Family::Ronroco),
        ("Baritone Ukulele", Family::Ukulele), ("Ukulele ", Family::Ukulele),
        ("Bass ", Family::Bass), ("Mandolin ", Family::Mandolin),
        ("Banjo ", Family::Banjo), ("Violin ", Family::Violin),
        ("Viola ", Family::Viola), ("Cello ", Family::Cello),
    ] {
        if name.starts_with(prefix) { return family; }
    }
    if ["Standard ", "Drop ", "Open ", "DADGAD", "Half Step ", "Full Step ", "Nashville ", "7-string ", "12-string ", "Baritone "]
        .iter().any(|prefix| name.starts_with(*prefix)) {
        Family::Guitar
    } else { Family::Other }
}

#[derive(Default)]
pub(crate) struct PresetSelector {
    search: String,
    browsing: Family,
    last_query: String,
    keyboard_choice: Option<usize>,
}

impl PresetSelector {
    fn open(&mut self, tunings: &[Tuning], selected: usize) {
        self.search.clear();
        self.last_query.clear();
        self.keyboard_choice = None;
        self.browsing = tunings.get(selected).map(family).unwrap_or_default();
    }

    fn browse(&mut self, instrument: Family) {
        self.browsing = instrument;
        self.search.clear();
        self.last_query.clear();
        self.keyboard_choice = None;
    }

    fn indices(&self, tunings: &[Tuning]) -> Vec<usize> {
        let query = normalize(self.search.trim());
        tunings.iter().enumerate().filter_map(|(index, tuning)| {
            (if query.is_empty() { family(tuning) == self.browsing } else { matches_query(tuning, &query) })
                .then_some(index)
        }).collect()
    }

    pub(crate) fn show(
        &mut self, ui: &mut egui::Ui, id: &'static str, tunings: &[Tuning],
        selected: &mut usize, max_width: f32, language: Language,
    ) {
        let popup_id = ui.make_persistent_id(id);
        let was_open = ui.memory(|memory| memory.is_popup_open(popup_id));
        let name = tunings.get(*selected).map(|tuning| label(tuning, language))
            .unwrap_or(language.text("Elegir afinación", "Choose a tuning"));
        let response = super::preset_button(ui, name, max_width, was_open, language);
        let opened = response.clicked() && !was_open;
        if response.clicked() {
            ui.memory_mut(|memory| memory.toggle_popup(popup_id));
            if opened { self.open(tunings, *selected); }
        }
        if !ui.memory(|memory| memory.is_popup_open(popup_id)) { return; }

        let bounds = popup_bounds(ui.ctx().screen_rect(), response.rect);
        let mut choice = None;
        let popup = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(bounds.min)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 26, 23))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(54, 66, 59)))
                    .rounding(8.0)
                    .inner_margin(PAD)
                    .show(ui, |ui| {
                        let width = (bounds.width() - 2.0 * PAD - 2.0).max(1.0);
                        let height = (bounds.height() - 2.0 * PAD - 2.0).max(1.0);
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                        let content_top = ui.cursor().top();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(language.text("Afinación", "Tuning")).size(14.0).color(TEXT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{} {}", tunings.len(), language.text("afinaciones", "tunings"))).size(11.0).color(MUTED));
                            });
                        });
                        let search = ui.add_sized([width, 28.0],
                            egui::TextEdit::singleline(&mut self.search)
                                .id(popup_id.with("search"))
                                .font(FontId::proportional(12.0))
                                .hint_text(language.text("Buscar en todos los instrumentos…", "Search all instruments…"))
                                .margin(egui::vec2(8.0, 5.0)));
                        if opened { search.request_focus(); }
                        let query = normalize(self.search.trim());
                        let query_changed = query != self.last_query;
                        if query_changed { self.keyboard_choice = None; }
                        self.last_query = query;
                        let body_height = (content_top + height - ui.cursor().top()).max(1.0);
                        let (body, _) = ui.allocate_exact_size(egui::vec2(width, body_height), Sense::hover());
                        let side_width = (width * 0.28).clamp(104.0, 128.0);
                        let left = Rect::from_min_max(body.min, egui::pos2(body.left() + side_width, body.bottom()));
                        let right = Rect::from_min_max(egui::pos2(left.right() + 16.0, body.top()), body.max);
                        ui.painter().line_segment(
                            [egui::pos2(left.right() + 7.0, body.top()), egui::pos2(left.right() + 7.0, body.bottom())],
                            Stroke::new(1.0, Color32::from_rgb(43, 53, 47)),
                        );
                        let mut changed_family = false;
                        let mut sidebar = ui.child_ui_with_id_source(left, egui::Layout::top_down(egui::Align::Min), "families");
                        sidebar.set_clip_rect(ui.clip_rect().intersect(left));
                        sidebar.spacing_mut().item_spacing.y = 2.0;
                        egui::ScrollArea::vertical().id_source(popup_id.with("families-scroll"))
                            .max_height(body_height).auto_shrink([false, false])
                            .show(&mut sidebar, |ui| {
                                for instrument in FAMILIES {
                                    if !tunings.iter().any(|tuning| family(tuning) == instrument) { continue; }
                                    let row = family_row(ui, instrument, self.browsing == instrument && self.search.trim().is_empty(), language);
                                    if row.clicked() {
                                        self.browse(instrument);
                                        changed_family = true;
                                    }
                                    if opened && instrument == self.browsing { row.scroll_to_me(Some(egui::Align::Center)); }
                                }
                            });
                        let searching = !self.search.trim().is_empty();
                        let indices = self.indices(tunings);
                        if self.keyboard_choice.is_some_and(|index| !indices.contains(&index)) { self.keyboard_choice = None; }
                        let mut keyboard_moved = false;
                        if search.has_focus() || search.lost_focus() {
                            for (key, forward) in [(egui::Key::ArrowDown, true), (egui::Key::ArrowUp, false)] {
                                if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key)) {
                                    self.keyboard_choice = step_choice(&indices, self.keyboard_choice, forward);
                                    keyboard_moved = true;
                                    search.request_focus();
                                }
                            }
                            if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                                choice = self.keyboard_choice.or_else(|| (indices.len() == 1).then(|| indices[0]));
                            }
                        }
                        let mut list = ui.child_ui_with_id_source(right, egui::Layout::top_down(egui::Align::Min), "tuning-list");
                        list.set_clip_rect(ui.clip_rect().intersect(right));
                        list.spacing_mut().item_spacing.y = 3.0;
                        let mut scroll = egui::ScrollArea::vertical().id_source(popup_id.with("tunings-scroll"))
                            .max_height(body_height).auto_shrink([false, false]);
                        if changed_family || query_changed { scroll = scroll.vertical_scroll_offset(0.0); }
                        scroll.show(&mut list, |ui| {
                            if indices.is_empty() {
                                ui.label(egui::RichText::new(language.text("No hay afinaciones que coincidan.", "No matching tunings.")).size(12.0).color(MUTED));
                            }
                            for index in indices {
                                let keyboard = self.keyboard_choice == Some(index);
                                let row = tuning_row(ui, &tunings[index], index == *selected, keyboard, searching, language);
                                if row.clicked() { choice = Some(index); }
                                if (opened && index == *selected) || (keyboard && keyboard_moved) {
                                    row.scroll_to_me(Some(egui::Align::Center));
                                }
                            }
                        });
                        if choice.is_some() || ui.input(|input| input.key_pressed(egui::Key::Escape)) { search.surrender_focus(); }
                    });
            });
        if let Some(index) = choice.filter(|index| *index < tunings.len()) {
            *selected = index;
            ui.memory_mut(|memory| { memory.close_popup(); memory.surrender_focus(popup_id.with("search")); });
        } else if ui.input(|input| input.key_pressed(egui::Key::Escape)) || (!response.clicked() && popup.response.clicked_elsewhere()) {
            ui.memory_mut(|memory| { memory.close_popup(); memory.surrender_focus(popup_id.with("search")); });
        }
    }
}

// Bounds include the frame. The compact 520x290 viewport is a supported layout,
// not a scaled-down version of an oversized mock-up.
fn popup_bounds(screen: Rect, anchor: Rect) -> Rect {
    let edge = 8.0;
    let gap = 4.0;
    let width = 480.0_f32.min((screen.width() - 2.0 * edge).max(1.0));
    let above = (anchor.top() - screen.top() - edge - gap).max(0.0);
    let below = (screen.bottom() - anchor.bottom() - edge - gap).max(0.0);
    let (height, top) = if above.max(below) >= 150.0 {
        let height = above.max(below).min(350.0);
        (height, if above > below { anchor.top() - gap - height } else { anchor.bottom() + gap })
    } else {
        let height = (screen.height() - edge * 2.0).clamp(1.0, 350.0);
        (height, screen.top() + edge)
    };
    let left = anchor.left().clamp(screen.left() + edge, (screen.right() - edge - width).max(screen.left() + edge));
    Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height))
}

fn family_row(ui: &mut egui::Ui, instrument: Family, browsing: bool, language: Language) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 25.0), Sense::click());
    if browsing || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4.0, if browsing { Color32::from_rgb(49, 57, 53) } else { Color32::from_rgb(34, 42, 38) });
    }
    let title = instrument.title(language);
    ui.painter().text(rect.left_center() + egui::vec2(7.0, 0.0), Align2::LEFT_CENTER, title, FontId::proportional(12.0), if browsing { TEXT } else { MUTED });
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, browsing, format!("{}: {title}", language.text("Instrumento", "Instrument"))));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn text_galley(ui: &egui::Ui, text: String, size: f32, color: Color32, width: f32, rows: usize) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text, FontId::proportional(size), color, width.max(1.0));
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = true;
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn tuning_row(ui: &mut egui::Ui, tuning: &Tuning, active: bool, keyboard: bool, searching: bool, language: Language) -> Response {
    let width = ui.available_width();
    let title = short_title(tuning, language);
    let title = if searching { format!("{} · {title}", family(tuning).title(language)) } else { title };
    let notes = notes(tuning, language);
    let title_galley = text_galley(ui, title, 13.0, if active { GREEN } else { TEXT }, width - 32.0, 1);
    let notes_galley = text_galley(ui, notes.clone(), 11.0, MUTED, width - 32.0, 2);
    let height = (title_galley.size().y + notes_galley.size().y + 12.0).max(43.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    if response.hovered() || keyboard || response.has_focus() || active {
        ui.painter().rect_filled(rect, 5.0, if response.hovered() || keyboard || response.has_focus() {
            Color32::from_rgb(39, 48, 43)
        } else { Color32::from_rgb(25, 38, 29) });
    }
    let origin = rect.min + egui::vec2(8.0, 5.0);
    let subtitle_y = origin.y + title_galley.size().y + 2.0;
    ui.painter().galley(origin, title_galley, TEXT);
    ui.painter().galley(egui::pos2(origin.x, subtitle_y), notes_galley, MUTED);
    if active {
        let center = Pos2::new(rect.right() - 12.0, rect.top() + 14.0);
        ui.painter().add(egui::Shape::line(vec![center + egui::vec2(-4.0, 0.0), center + egui::vec2(-1.0, 3.0), center + egui::vec2(5.0, -4.0)], Stroke::new(1.6, GREEN)));
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, active, format!("{} · {notes}", label(tuning, language))));
    // No hover tooltip: notes stay in the row instead of obscuring adjacent rows.
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn notes(tuning: &Tuning, language: Language) -> String {
    if tuning.strings.is_empty() { return language.text("Todas las notas · cualquier instrumento", "All notes · any instrument").to_owned(); }
    if matches!(family(tuning), Family::Charango | Family::Ronroco) || tuning.name.starts_with("12-string ") {
        return tuning.strings.chunks(2).map(|pair| {
            let first = get_note_display(&pair[0]);
            match pair.get(1).map(get_note_display) {
                Some(second) if second != first => format!("{first}/{second}"),
                _ => first,
            }
        }).collect::<Vec<_>>().join("  ·  ");
    }
    tuning.strings.iter().map(get_note_display).collect::<Vec<_>>().join("  ")
}

fn short_title(tuning: &Tuning, language: Language) -> String {
    let base = label(tuning, language).split(" (").next().unwrap_or(tuning.name);
    let base = base.strip_prefix("Charango ").or_else(|| base.strip_prefix("Ronroco ")).unwrap_or(base);
    match base {
        "estándar" | "Standard" => language.text("Estándar", "Standard").to_owned(),
        "argentino" | "Argentine" => language.text("Argentino", "Argentine").to_owned(),
        _ => base.to_owned(),
    }
}

fn step_choice(indices: &[usize], current: Option<usize>, forward: bool) -> Option<usize> {
    if indices.is_empty() { return None; }
    let index = current.and_then(|value| indices.iter().position(|index| *index == value));
    Some(indices[match index {
        Some(index) if forward => (index + 1).min(indices.len() - 1),
        Some(index) => index.saturating_sub(1),
        None if forward => 0,
        None => indices.len() - 1,
    }])
}

fn label(tuning: &Tuning, language: Language) -> &str {
    if tuning.strings.is_empty() { return language.text("Cromático · todos los instrumentos", "Chromatic · all instruments"); }
    if language == Language::English { return tuning.name; }
    match tuning.name {
        "Standard (EADGBE)" => "Estándar (EADGBE)",
        "Drop D (DADGBE)" => "Drop D (DADGBE)",
        "DADGAD" => "DADGAD",
        "Open G (DGDGBD)" => "Abierta G (DGDGBD)",
        "Open D (DADF#AD)" => "Abierta D (DADF#AD)",
        "Drop C (CGCFAD)" => "Drop C (CGCFAD)",
        "Half Step Down (D# G# C# F# A# D#)" => "Medio tono abajo (D# G# C# F# A# D#)",
        "Open E (EBEG#BE)" => "Abierta E (EBEG#BE)",
        "Drop B (BF#BEG#C#)" => "Drop B (BF#BEG#C#)",
        "Open C (CGCGCE)" => "Abierta C (CGCGCE)",
        "Open A (EAC#EAE)" => "Abierta A (EAC#EAE)",
        "Full Step Down (DGCFAD)" => "Un tono abajo (DGCFAD)",
        "Open Gm (DGDGA#D)" => "Abierta Gm (DGDGA#D)",
        "Nashville High Strung (EADGBE)" => "Nashville · cuerdas agudas (EADGBE)",
        "7-string Standard (BEADGBE)" => "7 cuerdas · estándar (BEADGBE)",
        "7-string Drop A (AEADGBE)" => "7 cuerdas · Drop A (AEADGBE)",
        "Baritone Standard (BEADF#B)" => "Barítono · estándar (BEADF#B)",
        "Baritone Drop A (AEADF#B)" => "Barítono · Drop A (AEADF#B)",
        "12-string Standard (E/e A/a D/d G/g B/B E/E)" => "12 cuerdas · estándar (E/e A/a D/d G/g B/B E/E)",
        "Bass Standard (EADG)" => "Bajo · estándar (EADG)",
        "Bass Drop D (DADG)" => "Bajo · Drop D (DADG)",
        "Bass Half Step Down (D#G#C#F#)" => "Bajo · medio tono abajo (D#G#C#F#)",
        "Bass 5-string (BEADG)" => "Bajo de 5 cuerdas (BEADG)",
        "Bass 6-string (BEADGC)" => "Bajo de 6 cuerdas (BEADGC)",
        "Ukulele Standard (GCEA)" => "Ukelele estándar (GCEA)",
        "Ukulele Low G (GCEA)" => "Ukelele · G grave (GCEA)",
        "Baritone Ukulele (DGBE)" => "Ukelele barítono (DGBE)",
        "Mandolin Standard (GDAE)" => "Mandolina estándar (GDAE)",
        "Banjo Open G (gDGBD)" => "Banjo · abierta G (gDGBD)",
        "Violin Standard (GDAE)" => "Violín estándar (GDAE)",
        "Viola Standard (CGDA)" => "Viola estándar (CGDA)",
        "Cello Standard (CGDA)" => "Violonchelo estándar (CGDA)",
        "Charango Standard (GCEAE)" => "Charango estándar (GCEAE)",
        "Ronroco Argentine (DGBEB)" => "Ronroco argentino (DGBEB)",
        _ => tuning.name,
    }
}

fn matches_query(tuning: &Tuning, query: &str) -> bool {
    let instrument = family(tuning);
    let aliases = match instrument {
        Family::Guitar => "guitar guitarra cuerdas strings",
        Family::Ronroco => "ronroco charangon charangón argentino argentine argentina",
        Family::Charango => "charango andino andean",
        Family::Chromatic => "chromatic vocal voice voz todos los instrumentos all instruments",
        _ => "",
    };
    let haystack = normalize(&format!("{} {} {} {} {aliases}", tuning.name, label(tuning, Language::Spanish), instrument.title(Language::Spanish), instrument.title(Language::English)));
    query.split_whitespace().all(|word| haystack.contains(word))
}

fn normalize(text: &str) -> String {
    text.to_lowercase().chars().map(|character| match character {
        'á' | 'à' | 'ä' | 'â' => 'a', 'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i', 'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u', '♯' => '#', '♭' => 'b',
        character => character,
    }).collect()
}

#[cfg(test)]
#[path = "instrument_selector_tests.rs"]
mod tests;
