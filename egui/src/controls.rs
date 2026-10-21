//! Compact controls drawn with egui geometry and the application's own fonts.

use crate::audio::InputStatus;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke};
use pitch_core::{get_note_display, Tuning};

pub(super) fn microphone_button(ui: &mut egui::Ui, status: InputStatus, device: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 28.0), Sense::click());
    let color = match status {
        InputStatus::Running => Color32::from_rgb(112, 220, 122),
        InputStatus::Starting => Color32::from_rgb(224, 183, 107),
        InputStatus::Stopped | InputStatus::Stopping => Color32::from_rgb(231, 113, 113),
    };
    paint_icon_background(ui, rect, &response, color);
    let center = rect.center();
    let painter = ui.painter();
    let stroke = Stroke::new(1.65, color);
    painter.rect_stroke(
        Rect::from_min_max(
            center + egui::vec2(-5.0, -3.0),
            center + egui::vec2(5.0, 4.0),
        ),
        2.0,
        stroke,
    );
    for x in [-3.0, 3.0] {
        painter.line_segment(
            [center + egui::vec2(x, -7.0), center + egui::vec2(x, -3.0)],
            stroke,
        );
    }
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(0.0, 4.0),
            center + egui::vec2(0.0, 8.0),
            center + egui::vec2(4.0, 8.0),
        ],
        stroke,
    ));
    let (state, action) = match status {
        InputStatus::Running => ("Micrófono activo", "Clic para silenciar"),
        InputStatus::Starting => ("Iniciando micrófono…", "Clic para cancelar"),
        InputStatus::Stopping => (
            "Micrófono silenciado; cerrando entrada…",
            "Clic para volver a escuchar",
        ),
        InputStatus::Stopped => ("Micrófono en pausa", "Clic para escuchar"),
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, format!("{state}. {action}"))
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{state}\n{device}\n{action} · Espacio / M"))
}

pub(super) fn stop_tone_button(ui: &mut egui::Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::click());
    let color = Color32::from_rgb(132, 218, 249);
    paint_icon_background(ui, rect, &response, color);
    ui.painter().rect_filled(
        Rect::from_center_size(rect.center(), egui::vec2(8.0, 8.0)),
        1.5,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, "Detener tono de referencia")
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Tono de referencia activo\nClic para detener · R")
}

fn paint_icon_background(ui: &egui::Ui, rect: Rect, response: &Response, color: Color32) {
    let alpha = if response.is_pointer_button_down_on() {
        34
    } else if response.hovered() {
        22
    } else {
        10
    };
    ui.painter().rect_filled(
        rect,
        5.0,
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha),
    );
    if response.has_focus() {
        ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.0, color));
    }
}

#[derive(Default)]
pub(super) struct PresetSelector {
    search: String,
}

impl PresetSelector {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        id: &'static str,
        tunings: &[Tuning],
        selected: &mut usize,
        max_width: f32,
    ) {
        let popup_id = ui.make_persistent_id(id);
        let was_open = ui.memory(|memory| memory.is_popup_open(popup_id));
        let name = tunings
            .get(*selected)
            .map(label)
            .unwrap_or("Elegir afinación");
        let response = preset_button(ui, name, max_width, was_open);
        let opened = response.clicked() && !was_open;
        if response.clicked() {
            ui.memory_mut(|memory| memory.toggle_popup(popup_id));
            if opened {
                self.search.clear();
            }
        }
        if !ui.memory(|memory| memory.is_popup_open(popup_id)) {
            return;
        }

        let screen = ui.ctx().screen_rect();
        let above = response.rect.top() - screen.top();
        let below = screen.bottom() - response.rect.bottom();
        let opens_above = above > below;
        let position = if opens_above {
            response.rect.left_top()
        } else {
            response.rect.left_bottom()
        };
        let height = (above.max(below) - 20.0).clamp(130.0, 360.0);
        let width = (screen.width() - 40.0).clamp(230.0, 360.0);
        let current_group = tunings.get(*selected).map(group_index).unwrap_or(0);
        let mut choice = None;
        let popup =
            egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .pivot(if opens_above {
                    Align2::LEFT_BOTTOM
                } else {
                    Align2::LEFT_TOP
                })
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(Color32::from_rgb(21, 28, 24))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(51, 65, 56)))
                        .rounding(7.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_width(width);
                            ui.spacing_mut().item_spacing = egui::vec2(6.0, 5.0);
                            ui.spacing_mut().button_padding = egui::vec2(7.0, 4.0);
                            ui.spacing_mut().interact_size.y = 25.0;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Afinación").strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.weak(format!("{} presets", tunings.len()));
                                    },
                                );
                            });
                            let search = ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .id(popup_id.with("search"))
                                    .hint_text("Buscar afinación o instrumento…")
                                    .desired_width(f32::INFINITY)
                                    .margin(egui::vec2(7.0, 5.0)),
                            );
                            if opened {
                                search.request_focus();
                            }
                            let query = normalize(self.search.trim());
                            let matches: Vec<usize> = tunings
                                .iter()
                                .enumerate()
                                .filter_map(|(index, tuning)| {
                                    matches_query(tuning, &query).then_some(index)
                                })
                                .collect();
                            egui::ScrollArea::vertical()
                                .id_source(popup_id.with("presets"))
                                .max_height((height - 65.0).max(65.0))
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if matches.is_empty() {
                                        ui.weak("No hay afinaciones que coincidan.");
                                    }
                                    for (group, (title, _)) in GROUPS.iter().enumerate() {
                                        let indices: Vec<usize> = matches
                                            .iter()
                                            .copied()
                                            .filter(|index| group_index(&tunings[*index]) == group)
                                            .collect();
                                        if indices.is_empty() {
                                            continue;
                                        }
                                        if query.is_empty() {
                                            egui::CollapsingHeader::new(format!(
                                                "{title} · {}",
                                                indices.len()
                                            ))
                                            .id_source(popup_id.with(group))
                                            .default_open(group == current_group)
                                            .open(opened.then_some(group == current_group))
                                            .show_unindented(ui, |ui| {
                                                preset_rows(
                                                    ui,
                                                    tunings,
                                                    &indices,
                                                    *selected,
                                                    &mut choice,
                                                    opened,
                                                );
                                            });
                                        } else {
                                            ui.label(
                                                egui::RichText::new(*title)
                                                    .small()
                                                    .color(Color32::from_rgb(135, 167, 144)),
                                            );
                                            preset_rows(
                                                ui,
                                                tunings,
                                                &indices,
                                                *selected,
                                                &mut choice,
                                                false,
                                            );
                                        }
                                    }
                                });
                            if (search.has_focus() || search.lost_focus())
                                && matches.len() == 1
                                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                            {
                                choice = matches.first().copied();
                            }
                            if choice.is_some()
                                || ui.input(|input| input.key_pressed(egui::Key::Escape))
                            {
                                search.surrender_focus();
                            }
                        });
                });
        if let Some(index) = choice {
            // Selection always refers to the original vector and persisted index.
            *selected = index;
            ui.memory_mut(|memory| memory.close_popup());
        } else if ui.input(|input| input.key_pressed(egui::Key::Escape))
            || (!response.clicked() && popup.response.clicked_elsewhere())
        {
            ui.memory_mut(|memory| {
                memory.close_popup();
                memory.surrender_focus(popup_id.with("search"));
            });
        }
    }
}

fn preset_button(ui: &mut egui::Ui, name: &str, max_width: f32, open: bool) -> Response {
    let font = FontId::proportional(13.0);
    let color = Color32::from_rgb(181, 199, 185);
    let text_width = ui
        .painter()
        .layout_no_wrap(name.to_owned(), font.clone(), color)
        .size()
        .x;
    let width = (text_width.ceil() + 32.0).min(max_width.max(80.0));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::click());
    if open || response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, 5.0, Color32::from_rgb(33, 44, 36));
    }
    let text = super::fit_label(ui, name, width - 31.0);
    ui.painter().text(
        rect.left_center() + egui::vec2(7.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
    let center = Pos2::new(rect.right() - 11.0, rect.center().y);
    let direction = if open { -1.0 } else { 1.0 };
    ui.painter().add(egui::Shape::line(
        vec![
            center + egui::vec2(-3.0, -1.5 * direction),
            center + egui::vec2(0.0, 1.5 * direction),
            center + egui::vec2(3.0, -1.5 * direction),
        ],
        Stroke::new(1.2, Color32::from_rgb(106, 139, 115)),
    ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            format!("Afinación: {name}. Elegir otra afinación"),
        )
    });
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if open {
        response
    } else {
        response.on_hover_text(format!(
            "{name}\nElegir afinación · buscar por nombre o instrumento"
        ))
    }
}

fn preset_rows(
    ui: &mut egui::Ui,
    tunings: &[Tuning],
    indices: &[usize],
    selected: usize,
    choice: &mut Option<usize>,
    reveal_selected: bool,
) {
    for &index in indices {
        let tuning = &tunings[index];
        let text = super::fit_label(ui, label(tuning), ui.available_width() - 18.0);
        let response = ui
            .add_sized(
                [ui.available_width(), 25.0],
                egui::SelectableLabel::new(index == selected, text),
            )
            .on_hover_text(format!(
                "{}\n{}",
                label(tuning),
                tuning
                    .strings
                    .iter()
                    .map(get_note_display)
                    .collect::<Vec<_>>()
                    .join("  ")
            ));
        if response.clicked() {
            *choice = Some(index);
        }
        if reveal_selected && index == selected {
            response.scroll_to_me(Some(egui::Align::Center));
        }
    }
}

fn label(tuning: &Tuning) -> &str {
    if tuning.strings.is_empty() {
        "Cromático · todos los instrumentos"
    } else {
        tuning.name
    }
}

// The native Tuning type exposes its name and notes, but not registry instrument
// metadata. These presentation groups never reorder or recreate its presets.
const GROUPS: [(&str, &str); 14] = [
    ("Guitarra", "guitar 6 cuerdas"),
    ("Guitarra de 7 cuerdas", "guitar 7 string"),
    ("Guitarra barítono", "baritone guitar"),
    ("Guitarra de 12 cuerdas", "guitar 12 string"),
    ("Bajo", "bass"),
    ("Ukelele", "ukulele"),
    ("Ukelele barítono", "baritone ukulele"),
    ("Mandolina", "mandolin"),
    ("Banjo", "banjo"),
    ("Violín", "violin"),
    ("Viola", "viola"),
    ("Violonchelo", "cello"),
    ("Cromático y voz", "chromatic vocal todos los instrumentos"),
    ("Otros instrumentos", "other"),
];

fn group_index(tuning: &Tuning) -> usize {
    let name = tuning.name;
    if tuning.strings.is_empty() {
        12
    } else if name.starts_with("7-string ") {
        1
    } else if name.starts_with("12-string ") {
        3
    } else if name.starts_with("Baritone Ukulele") {
        6
    } else if name.starts_with("Baritone ") {
        2
    } else if name.starts_with("Bass ") {
        4
    } else if name.starts_with("Ukulele ") {
        5
    } else if name.starts_with("Mandolin ") {
        7
    } else if name.starts_with("Banjo ") {
        8
    } else if name.starts_with("Violin ") {
        9
    } else if name.starts_with("Viola ") {
        10
    } else if name.starts_with("Cello ") {
        11
    } else if [
        "Standard ",
        "Drop ",
        "Open ",
        "DADGAD",
        "Half Step ",
        "Full Step ",
        "Nashville ",
    ]
    .iter()
    .any(|prefix| name.starts_with(*prefix))
    {
        0
    } else {
        13
    }
}

fn matches_query(tuning: &Tuning, query: &str) -> bool {
    let group = GROUPS[group_index(tuning)];
    let haystack = normalize(&format!("{} {} {}", tuning.name, group.0, group.1));
    query.split_whitespace().all(|word| haystack.contains(word))
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|character| match character {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            '♯' => '#',
            '♭' => 'b',
            character => character,
        })
        .collect()
}

#[cfg(test)]
mod selector_regression_tests {
    use super::*;

    fn widget_center(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        name_prefix: &str,
    ) -> egui::Pos2 {
        let bounds = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accessibility tree is enabled")
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.role() == role && node.name().unwrap_or("").starts_with(name_prefix))
            .expect("the requested widget is present")
            .bounds()
            .expect("the widget has visible bounds");
        egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        )
    }

    fn click(
        draw: &mut impl FnMut(Vec<egui::Event>) -> egui::FullOutput,
        pos: egui::Pos2,
    ) -> egui::FullOutput {
        draw(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            draw(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        draw(Vec::new());
        draw(Vec::new())
    }

    #[test]
    fn enter_selects_unique_preset_after_text_edit_loses_focus() {
        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut(|style| style.animation_time = 0.0);
        let tunings = pitch_core::get_tunings();
        let expected = tunings
            .iter()
            .position(|tuning| tuning.name.starts_with("Violin "))
            .expect("the violin preset is present");
        let mut selector = PresetSelector::default();
        let mut selected = 0;
        let mut frame = 0;
        let mut draw = |events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(520.0, 290.0),
                )),
                time: Some(frame as f64 / 30.0),
                events,
                ..Default::default()
            };
            let output = context.run(input, |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    selector.show(ui, "enter-regression", &tunings, &mut selected, 300.0);
                });
            });
            frame += 1;
            output
        };

        // Exercise the real opening path instead of forcing popup/focus memory
        // before the popup's initial sizing pass.
        draw(Vec::new());
        draw(Vec::new());
        let output = draw(Vec::new());
        let trigger = widget_center(&output, egui::accesskit::Role::Button, "Afinación:");
        let output = click(&mut draw, trigger);
        assert!(context.memory(|memory| memory.any_popup_open()));
        assert!(context.wants_keyboard_input());

        let search = widget_center(&output, egui::accesskit::Role::TextInput, "");
        click(&mut draw, search);
        assert!(context.memory(|memory| memory.any_popup_open()));
        draw(vec![egui::Event::Text("violín".to_owned())]);
        for _ in 0..3 {
            draw(Vec::new());
        }
        assert!(context.wants_keyboard_input());
        for pressed in [true, false] {
            draw(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        draw(Vec::new());
        draw(Vec::new());
        drop(draw);

        assert_eq!(selected, expected);
        assert!(!context.memory(|memory| memory.any_popup_open()));
        assert!(!context.wants_keyboard_input());
    }
}
