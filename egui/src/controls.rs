//! Compact controls drawn with egui geometry and the application's own fonts.

use crate::audio::InputStatus;
use crate::i18n::Language;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke};

#[path = "instrument_selector.rs"]
mod instrument_selector;
pub(super) use instrument_selector::PresetSelector;

pub(super) fn microphone_button(
    ui: &mut egui::Ui,
    status: InputStatus,
    device: &str,
    language: Language,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 28.0), Sense::click());
    let color = match status {
        InputStatus::Running => Color32::from_rgb(112, 220, 122),
        InputStatus::Starting => Color32::from_rgb(224, 183, 107),
        InputStatus::Stopped | InputStatus::Stopping => Color32::from_rgb(231, 113, 113),
    };
    paint_icon_background(ui, rect, &response, color);
    let center = rect.center();
    let painter = ui.painter();
    let stroke = Stroke::new(1.4, color);
    // A quarter-inch instrument jack: one metal shaft, a separated tip,
    // a wider grip and a cable. Local coordinates follow its diagonal axis.
    let point = |along: f32, across: f32| {
        center + egui::vec2(along + across, across - along) * std::f32::consts::FRAC_1_SQRT_2
    };
    painter.add(egui::Shape::convex_polygon(
        vec![
            point(0.0, -2.8),
            point(-5.5, -2.8),
            point(-7.0, -1.8),
            point(-7.0, 1.8),
            point(-5.5, 2.8),
            point(0.0, 2.8),
        ],
        color.gamma_multiply(0.12),
        stroke,
    ));
    for along in [-4.5, -2.5] {
        painter.line_segment(
            [point(along, -2.3), point(along, 2.3)],
            Stroke::new(1.0, color.gamma_multiply(0.65)),
        );
    }
    painter.add(egui::Shape::convex_polygon(
        vec![
            point(0.5, -1.5),
            point(4.3, -1.5),
            point(4.3, 1.5),
            point(0.5, 1.5),
        ],
        color,
        Stroke::NONE,
    ));
    // The dark gap is the insulating band of a mono (TS) guitar plug.
    painter.add(egui::Shape::convex_polygon(
        vec![
            point(5.6, -1.5),
            point(8.4, -1.5),
            point(9.3, -1.0),
            point(10.0, 0.0),
            point(9.3, 1.0),
            point(8.4, 1.5),
            point(5.6, 1.5),
        ],
        color,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::line(
        vec![
            point(-7.0, 0.0),
            point(-9.5, 0.0),
            point(-10.0, 2.4),
            point(-12.0, 2.4),
        ],
        stroke,
    ));
    let (state, action) = match status {
        InputStatus::Running => (
            language.text("Micrófono activo", "Microphone active"),
            language.text("Clic para silenciar", "Click to mute"),
        ),
        InputStatus::Starting => (
            language.text("Iniciando micrófono…", "Starting microphone…"),
            language.text("Clic para cancelar", "Click to cancel"),
        ),
        InputStatus::Stopping => (
            language.text("Micrófono silenciado; cerrando entrada…", "Microphone muted; closing input…"),
            language.text("Clic para volver a escuchar", "Click to listen again"),
        ),
        InputStatus::Stopped => (
            language.text("Micrófono en pausa", "Microphone paused"),
            language.text("Clic para escuchar", "Click to listen"),
        ),
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, format!("{state}. {action}"))
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{state}\n{device}\n{action} · {} / M", language.text("Espacio", "Space")))
}

pub(super) fn stop_tone_button(ui: &mut egui::Ui, language: Language) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::click());
    let color = Color32::from_rgb(132, 218, 249);
    paint_icon_background(ui, rect, &response, color);
    ui.painter().rect_filled(
        Rect::from_center_size(rect.center(), egui::vec2(8.0, 8.0)),
        1.5,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            language.text("Detener tono de referencia", "Stop reference tone"),
        )
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(language.text(
        "Tono de referencia activo\nClic para detener · R",
        "Reference tone playing\nClick to stop · R",
    ))
}

fn paint_icon_background(ui: &egui::Ui, rect: Rect, response: &Response, color: Color32) {
    let alpha = if response.is_pointer_button_down_on() { 34 } else if response.hovered() { 22 } else { 10 };
    ui.painter().rect_filled(rect, 5.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha));
    if response.has_focus() {
        ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.0, color));
    }
}

fn preset_button(ui: &mut egui::Ui, name: &str, max_width: f32, open: bool, language: Language) -> Response {
    let font = FontId::proportional(13.0);
    let color = Color32::from_rgb(181, 199, 185);
    let text_width = ui.painter().layout_no_wrap(name.to_owned(), font.clone(), color).size().x;
    let width = (text_width.ceil() + 32.0).min(max_width.max(80.0));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::click());
    if open || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 5.0, Color32::from_rgb(33, 44, 36));
    }
    let text = super::fit_label(ui, name, width - 31.0);
    ui.painter().text(rect.left_center() + egui::vec2(7.0, 0.0), Align2::LEFT_CENTER, text, font, color);
    let center = Pos2::new(rect.right() - 11.0, rect.center().y);
    let direction = if open { -1.0 } else { 1.0 };
    ui.painter().add(egui::Shape::line(
        vec![center + egui::vec2(-3.0, -1.5 * direction), center + egui::vec2(0.0, 1.5 * direction), center + egui::vec2(3.0, -1.5 * direction)],
        Stroke::new(1.2, Color32::from_rgb(106, 139, 115)),
    ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, format!("{}: {name}. {}", language.text("Afinación", "Tuning"), language.text("Elegir otra afinación", "Choose another tuning")))
    });
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if open { response } else {
        response.on_hover_text(format!("{name}\n{}", language.text("Elegir afinación · buscar por nombre o instrumento", "Choose a tuning · search by name or instrument")))
    }
}
