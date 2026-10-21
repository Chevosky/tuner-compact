use crate::i18n::Language;
use crate::state::TunerViewState;
use eframe::egui;
use std::collections::VecDeque;
#[cfg(test)]
use std::sync::Arc;

const CENTS_HISTORY_LIMIT: usize = 300;
const SPECTROGRAM_HISTORY_LIMIT: usize = 150;
const SPECTROGRAM_FREQUENCY_BINS: usize = 80;

struct SpectrogramTexture {
    image: egui::ColorImage,
    texture: Option<egui::TextureHandle>,
    write_index: usize,
    len: usize,
    dirty_columns: [bool; SPECTROGRAM_HISTORY_LIMIT],
}

impl Default for SpectrogramTexture {
    fn default() -> Self {
        Self {
            image: egui::ColorImage::new(
                [SPECTROGRAM_HISTORY_LIMIT, SPECTROGRAM_FREQUENCY_BINS],
                egui::Color32::from_gray(15),
            ),
            texture: None,
            write_index: 0,
            len: 0,
            dirty_columns: [false; SPECTROGRAM_HISTORY_LIMIT],
        }
    }
}

impl SpectrogramTexture {
    fn push(&mut self, spectrum: &[f32]) {
        let column = self.write_index;
        for bin in 0..SPECTROGRAM_FREQUENCY_BINS {
            let row = SPECTROGRAM_FREQUENCY_BINS - 1 - bin;
            self.image.pixels[row * SPECTROGRAM_HISTORY_LIMIT + column] =
                spectrogram_color(spectrum.get(bin).copied().unwrap_or(0.0));
        }
        self.dirty_columns[column] = true;
        self.write_index = (self.write_index + 1) % SPECTROGRAM_HISTORY_LIMIT;
        self.len = (self.len + 1).min(SPECTROGRAM_HISTORY_LIMIT);
    }

    fn clear(&mut self) {
        self.image.pixels.fill(egui::Color32::from_gray(15));
        self.texture = None;
        self.write_index = 0;
        self.len = 0;
        self.dirty_columns.fill(false);
    }

    fn dirty_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut column = 0;
        while column < SPECTROGRAM_HISTORY_LIMIT {
            if !self.dirty_columns[column] {
                column += 1;
                continue;
            }
            let start = column;
            while column < SPECTROGRAM_HISTORY_LIMIT && self.dirty_columns[column] {
                column += 1;
            }
            ranges.push((start, column));
        }
        ranges
    }

    fn upload(&mut self, context: &egui::Context) -> egui::TextureId {
        if self.texture.is_none() {
            self.texture = Some(context.load_texture(
                "spectrogram",
                self.image.clone(),
                egui::TextureOptions::NEAREST,
            ));
            self.dirty_columns.fill(false);
        }
        for (start, end) in self.dirty_ranges() {
            let width = end - start;
            let mut pixels = Vec::with_capacity(width * SPECTROGRAM_FREQUENCY_BINS);
            for row in 0..SPECTROGRAM_FREQUENCY_BINS {
                let row_start = row * SPECTROGRAM_HISTORY_LIMIT + start;
                pixels.extend_from_slice(&self.image.pixels[row_start..row_start + width]);
            }
            let image = egui::ColorImage {
                size: [width, SPECTROGRAM_FREQUENCY_BINS],
                pixels,
            };
            if let Some(texture) = &mut self.texture {
                texture.set_partial([start, 0], image, egui::TextureOptions::NEAREST);
            }
            self.dirty_columns[start..end].fill(false);
        }
        self.texture
            .as_ref()
            .expect("spectrogram texture is initialized before drawing")
            .id()
    }
}

#[derive(Default)]
pub(crate) struct VisualizationHistory {
    cents: VecDeque<f32>,
    last_frame_id: u64,
    spectrogram: SpectrogramTexture,
    pub(crate) show_spectrogram: bool,
}

impl VisualizationHistory {
    pub(crate) fn capture(&mut self, state: &TunerViewState, listening: bool) {
        if !listening || state.frame_id == self.last_frame_id {
            return;
        }
        self.last_frame_id = state.frame_id;
        push_bounded(&mut self.cents, state.cents, CENTS_HISTORY_LIMIT);
        if self.show_spectrogram && !state.spectrum.is_empty() {
            self.spectrogram.push(&state.spectrum);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.cents.clear();
        self.spectrogram.clear();
    }

    pub(crate) fn clear_spectrogram(&mut self) {
        self.spectrogram.clear();
    }

    pub(crate) fn draw_primary(&self, ui: &mut egui::Ui, state: &TunerViewState) {
        draw_waveform(ui, &state.waveform);
        draw_cents_gauge(ui, state.cents);
        draw_cents_history(ui, &self.cents);
    }

    pub(crate) fn draw_spectrum(
        &self,
        ui: &mut egui::Ui,
        state: &TunerViewState,
        sample_rate: f32,
        language: Language,
    ) {
        draw_spectrum(ui, &state.spectrum, state.frequency, sample_rate, language);
    }

    pub(crate) fn draw_spectrogram(&mut self, ui: &mut egui::Ui, language: Language) {
        draw_spectrogram(ui, &mut self.spectrogram, language);
    }
}

fn push_bounded<T>(history: &mut VecDeque<T>, value: T, limit: usize) {
    if history.len() == limit {
        history.pop_front();
    }
    history.push_back(value);
}

fn draw_waveform(ui: &mut egui::Ui, samples: &[f32]) {
    if samples.is_empty() {
        return;
    }
    ui.add_space(4.0);
    let size = egui::vec2(ui.available_width().min(520.0), 36.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));

    let point_count = (rect.width() as usize).clamp(2, samples.len());
    let points = (0..point_count)
        .map(|index| {
            let sample_index = index * (samples.len() - 1) / (point_count - 1);
            let x = egui::lerp(rect.x_range(), index as f32 / (point_count - 1) as f32);
            let y = (rect.center().y - samples[sample_index] * rect.height() * 0.45)
                .clamp(rect.min.y, rect.max.y);
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 210, 160)),
    ));
}

fn draw_cents_gauge(ui: &mut egui::Ui, cents: f32) {
    let size = egui::vec2(ui.available_width().min(520.0), 18.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(48));
    painter.vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, egui::Color32::from_gray(130)),
    );
    let x = (rect.center().x + cents / 50.0 * rect.width() * 0.5).clamp(rect.min.x, rect.max.x);
    let color = if cents.abs() < 5.0 {
        egui::Color32::from_rgb(52, 211, 153)
    } else {
        egui::Color32::from_rgb(251, 191, 36)
    };
    painter.circle_filled(egui::pos2(x, rect.center().y), 7.0, color);
}

fn draw_cents_history(ui: &mut egui::Ui, history: &VecDeque<f32>) {
    if history.len() < 2 {
        return;
    }
    ui.add_space(4.0);
    let size = egui::vec2(ui.available_width().min(520.0), 44.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));
    painter.hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, egui::Color32::from_gray(65)),
    );
    let denominator = (history.len() - 1) as f32;
    let points = history
        .iter()
        .enumerate()
        .map(|(index, cents)| {
            let x = egui::lerp(rect.x_range(), index as f32 / denominator);
            let y = (rect.center().y - cents / 50.0 * rect.height() * 0.5)
                .clamp(rect.min.y, rect.max.y);
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.25, egui::Color32::from_rgb(110, 231, 183)),
    ));
}

fn draw_spectrum(
    ui: &mut egui::Ui,
    spectrum: &[f32],
    frequency: Option<f32>,
    sample_rate: f32,
    language: Language,
) {
    if spectrum.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(language.text("Espectro", "Spectrum"));
    let bin_count = spectrum.len().min(200);
    let size = egui::vec2(ui.available_width().min(600.0), 84.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));
    let bar_width = rect.width() / bin_count as f32;
    for (index, magnitude) in spectrum.iter().take(bin_count).enumerate() {
        let height = magnitude * rect.height();
        let left = rect.min.x + index as f32 * bar_width;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left, rect.max.y - height),
                egui::pos2(left + (bar_width - 1.0).max(1.0), rect.max.y),
            ),
            0.0,
            egui::Color32::from_rgb(80, 200, 120),
        );
    }

    if let Some(frequency) = frequency {
        let fft_size = spectrum.len() * 4;
        for harmonic in 2..=5 {
            let bin = ((frequency * harmonic as f32 / (sample_rate / fft_size as f32)) as usize)
                .min(bin_count - 1);
            let x = rect.min.x + bin as f32 * bar_width;
            painter.vline(
                x,
                rect.y_range(),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 220, 80)),
            );
        }
    }
}

fn spectrogram_color(value: f32) -> egui::Color32 {
    egui::Color32::from_rgb(
        (value * 255.0).clamp(0.0, 255.0) as u8,
        (value.sqrt() * 210.0).clamp(0.0, 255.0) as u8,
        (40.0 + value * 50.0).clamp(0.0, 255.0) as u8,
    )
}

fn draw_spectrogram(ui: &mut egui::Ui, spectrogram: &mut SpectrogramTexture, language: Language) {
    if spectrogram.len == 0 {
        return;
    }
    ui.add_space(4.0);
    ui.label(language.text("Espectrograma", "Spectrogram"));
    let size = egui::vec2(ui.available_width().min(520.0), 72.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let texture_id = spectrogram.upload(ui.ctx());
    let mut mesh = egui::Mesh::with_texture(texture_id);
    let capacity = SPECTROGRAM_HISTORY_LIMIT as f32;

    if spectrogram.len < SPECTROGRAM_HISTORY_LIMIT || spectrogram.write_index == 0 {
        let visible = spectrogram.len as f32 / capacity;
        mesh.add_rect_with_uv(
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(visible, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        let tail_fraction = (SPECTROGRAM_HISTORY_LIMIT - spectrogram.write_index) as f32 / capacity;
        let split_x = egui::lerp(rect.x_range(), tail_fraction);
        mesh.add_rect_with_uv(
            egui::Rect::from_min_max(rect.min, egui::pos2(split_x, rect.max.y)),
            egui::Rect::from_min_max(
                egui::pos2(spectrogram.write_index as f32 / capacity, 0.0),
                egui::pos2(1.0, 1.0),
            ),
            egui::Color32::WHITE,
        );
        mesh.add_rect_with_uv(
            egui::Rect::from_min_max(egui::pos2(split_x, rect.min.y), rect.max),
            egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(spectrogram.write_index as f32 / capacity, 1.0),
            ),
            egui::Color32::WHITE,
        );
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(frame_id: u64, cents: f32) -> TunerViewState {
        TunerViewState {
            cents,
            frame_id,
            spectrum: Arc::from([0.25, 0.5, 0.75]),
            ..TunerViewState::default()
        }
    }

    #[test]
    fn capture_ignores_duplicate_frames_and_disabled_input() {
        let mut history = VisualizationHistory {
            show_spectrogram: true,
            ..VisualizationHistory::default()
        };
        let frame = state(1, 3.0);

        history.capture(&frame, false);
        assert!(history.cents.is_empty());
        history.capture(&frame, true);
        history.capture(&frame, true);

        assert_eq!(history.cents.len(), 1);
        assert_eq!(history.spectrogram.len, 1);
    }

    #[test]
    fn capture_keeps_histories_bounded() {
        let mut history = VisualizationHistory {
            show_spectrogram: true,
            ..VisualizationHistory::default()
        };
        for frame_id in 1..=(CENTS_HISTORY_LIMIT as u64 + 25) {
            history.capture(&state(frame_id, frame_id as f32), true);
        }

        assert_eq!(history.cents.len(), CENTS_HISTORY_LIMIT);
        assert_eq!(history.spectrogram.len, SPECTROGRAM_HISTORY_LIMIT);
        assert_eq!(history.cents.front(), Some(&26.0));
    }

    #[test]
    fn clear_resets_both_histories_and_spectrogram_can_reset_independently() {
        let mut history = VisualizationHistory {
            show_spectrogram: true,
            ..VisualizationHistory::default()
        };
        history.capture(&state(1, 1.0), true);
        history.clear_spectrogram();
        assert_eq!(history.cents.len(), 1);
        assert_eq!(history.spectrogram.len, 0);

        history.capture(&state(2, 2.0), true);
        history.clear();
        assert!(history.cents.is_empty());
        assert_eq!(history.spectrogram.len, 0);
    }

    #[test]
    fn spectrogram_column_preserves_palette_and_frequency_direction() {
        let mut spectrogram = SpectrogramTexture::default();
        spectrogram.push(&[1.0]);

        assert_eq!(
            spectrogram.image.pixels[(SPECTROGRAM_FREQUENCY_BINS - 1) * SPECTROGRAM_HISTORY_LIMIT],
            spectrogram_color(1.0)
        );
        assert_eq!(spectrogram.image.pixels[0], spectrogram_color(0.0));
        assert_eq!(spectrogram.dirty_ranges(), vec![(0, 1)]);
    }

    #[test]
    fn spectrogram_rolls_over_and_tracks_all_dirty_ranges() {
        let mut spectrogram = SpectrogramTexture {
            write_index: SPECTROGRAM_HISTORY_LIMIT - 1,
            len: SPECTROGRAM_HISTORY_LIMIT,
            ..SpectrogramTexture::default()
        };
        spectrogram.push(&[0.25]);
        spectrogram.push(&[0.5]);
        spectrogram.push(&[1.0]);

        assert_eq!(spectrogram.len, SPECTROGRAM_HISTORY_LIMIT);
        assert_eq!(spectrogram.write_index, 2);
        assert_eq!(
            spectrogram.dirty_ranges(),
            vec![
                (0, 2),
                (SPECTROGRAM_HISTORY_LIMIT - 1, SPECTROGRAM_HISTORY_LIMIT)
            ]
        );
        assert_eq!(
            spectrogram.image.pixels
                [(SPECTROGRAM_FREQUENCY_BINS - 1) * SPECTROGRAM_HISTORY_LIMIT + 1],
            spectrogram_color(1.0)
        );
    }
}
