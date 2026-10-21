use crate::audio::AudioManager;
use crate::diagnostics::diagnostic_hint;
use crate::state::{SharedTunerState, TunerViewState};
use crate::tuner_panel::{self, PanelReading};
use crate::visualization::VisualizationHistory;
use eframe::egui;
use pitch_core::{
    format_freq, get_note_display, get_tunings, AnalysisWindowSet, EngineConfig, TunerEngine,
    Tuning,
};
use std::sync::{Arc, Mutex};

pub(crate) struct App {
    a4: f32,
    audio: AudioManager,
    engine: Arc<Mutex<TunerEngine>>,
    listening: bool,
    show_spectrum: bool,
    settings_open: bool,
    analysis_open: bool,
    compact_layout_pending: bool,
    state: SharedTunerState,
    tuning_index: usize,
    tunings: Vec<Tuning>,
    visualization: VisualizationHistory,
}

impl Default for App {
    fn default() -> Self {
        let tunings = get_tunings();
        Self {
            a4: 440.0,
            audio: AudioManager::default(),
            engine: Arc::new(Mutex::new(TunerEngine::with_config(EngineConfig {
                // Canonical dual-lane layout (fast 2048 + full-frame 8192),
                // same as every other shipped host.
                analysis_windows: AnalysisWindowSet::standard(),
                spectrum_bins: 0,
                tuning: tunings.first().cloned(),
                ..EngineConfig::default()
            }))),
            listening: false,
            show_spectrum: false,
            settings_open: false,
            analysis_open: false,
            compact_layout_pending: true,
            state: Arc::new(Mutex::new(TunerViewState::default())),
            tuning_index: 0,
            tunings,
            visualization: VisualizationHistory::default(),
        }
    }
}

impl App {
    pub(crate) fn configure_style(context: &egui::Context) {
        let mut style = (*context.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = egui::Color32::from_rgb(17, 21, 20);
        style.visuals.window_fill = egui::Color32::from_rgb(22, 27, 25);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(9, 13, 11);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(42, 78, 54);
        style.visuals.selection.stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(146, 232, 141));
        style.visuals.hyperlink_color = egui::Color32::from_rgb(132, 218, 249);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(36, 43, 39);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(49, 63, 53);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(58, 82, 62);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(5.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(5.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(5.0);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
        context.set_style(style);
    }

    pub(crate) fn restore(&mut self, storage: Option<&dyn eframe::Storage>) {
        let Some(storage) = storage else { return };
        // Migrate only the old oversized window. Subsequent launches retain
        // the user's own window geometry through eframe's existing storage.
        self.compact_layout_pending =
            storage.get_string("compact_layout_version").as_deref() != Some("1");
        self.a4 = parse_or(storage, "a4", self.a4).clamp(420.0, 460.0);
        self.tuning_index = parse_or(storage, "tuning_index", parse_or(storage, "t_idx", 0_usize))
            .min(self.tunings.len().saturating_sub(1));
        self.show_spectrum = parse_bool(storage, "show_spectrum") || parse_bool(storage, "spec");
        self.visualization.show_spectrogram = parse_bool(storage, "show_spectrogram");
        self.audio.selected_input_device = storage
            .get_string("input_device")
            .filter(|name| !name.is_empty());
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_a4(self.a4);
            if let Some(tuning) = self.tunings.get(self.tuning_index).cloned() {
                engine.set_tuning(tuning);
            }
            engine.set_spectrum_enabled(self.show_spectrum || self.visualization.show_spectrogram);
        }
    }

    pub(crate) fn refresh_devices(&mut self) {
        self.audio.refresh();
    }

    fn toggle_mic(&mut self, context: &egui::Context) {
        if self.listening {
            self.audio.stop_input();
            self.listening = false;
            self.visualization.clear();
            if let Ok(mut engine) = self.engine.lock() {
                engine.reset();
            }
            if let Ok(mut state) = self.state.lock() {
                state.clear_detection();
            }
            return;
        }

        if self.audio.input_devices.is_empty() {
            self.audio.refresh();
        }
        match self
            .audio
            .start_input(self.state.clone(), self.engine.clone(), context.clone())
        {
            Ok(()) => self.listening = true,
            Err(error) => {
                if let Ok(mut state) = self.state.lock() {
                    state.error = Some(error);
                }
            }
        }
    }

    fn restart_mic(&mut self, context: &egui::Context) {
        if !self.listening {
            return;
        }
        self.audio.stop_input();
        self.listening = false;
        self.toggle_mic(context);
    }

    fn toggle_reference_tone(&mut self) {
        if self.audio_is_playing_tone() {
            self.audio.stop_tone();
            return;
        }
        let Some(frequency) = self
            .tunings
            .get(self.tuning_index)
            .and_then(|tuning| tuning.strings.first())
            .map(|note| note.frequency)
        else {
            return;
        };
        self.play_tone(frequency);
    }

    fn play_random_string(&mut self) {
        let Some(strings) = self
            .tunings
            .get(self.tuning_index)
            .map(|tuning| tuning.strings.as_slice())
            .filter(|strings| !strings.is_empty())
        else {
            return;
        };
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        self.play_tone(strings[nanos as usize % strings.len()].frequency);
    }

    fn play_tone(&mut self, frequency: f32) {
        if let Err(error) = self.audio.play_tone(frequency) {
            if let Ok(mut state) = self.state.lock() {
                state.error = Some(error);
            }
        }
    }

    fn audio_is_playing_tone(&self) -> bool {
        self.audio.is_tone_playing()
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        if context.wants_keyboard_input() {
            return;
        }
        if context
            .input(|input| input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::M))
        {
            self.toggle_mic(context);
        }
        if context.input(|input| input.key_pressed(egui::Key::R)) {
            self.toggle_reference_tone();
        }
    }

    fn draw_errors(&self, ui: &mut egui::Ui, state: &TunerViewState) {
        if let Some(error) = &state.error {
            ui.colored_label(egui::Color32::from_rgb(248, 113, 113), error);
        }
    }

    fn draw_diagnostics(&self, ui: &mut egui::Ui, state: &TunerViewState) {
        self.draw_errors(ui, state);
        for code in state.diagnostics.iter().chain(&state.backend_diagnostics) {
            ui.colored_label(
                egui::Color32::from_rgb(234, 179, 8),
                format!("⚠ {} — {}", code, diagnostic_hint(code)),
            );
        }
    }

    fn tuning_picker(&mut self, ui: &mut egui::Ui, id: &'static str, width: f32) {
        let previous_tuning = self.tuning_index;
        let name = self
            .tunings
            .get(self.tuning_index)
            .map(tuning_label)
            .unwrap_or("—");
        egui::ComboBox::from_id_source(id)
            .width(width)
            .selected_text(fit_label(ui, name, width - 42.0))
            .show_ui(ui, |ui| {
                for (index, tuning) in self.tunings.iter().enumerate() {
                    ui.selectable_value(&mut self.tuning_index, index, tuning_label(tuning));
                }
            })
            .response
            .on_hover_text(name);
        if self.tuning_index != previous_tuning {
            if let (Some(tuning), Ok(mut engine)) = (
                self.tunings.get(self.tuning_index).cloned(),
                self.engine.lock(),
            ) {
                engine.set_tuning(tuning);
            }
        }
    }

    fn draw_compact(&mut self, ui: &mut egui::Ui, state: &TunerViewState, context: &egui::Context) {
        let display_note = state.display_note();
        let target_hint = if self.listening && state.frequency.is_some() {
            state.target.as_ref().map(|target| {
                format!(
                    "OBJETIVO {} · DETECTADA {}",
                    get_note_display(target),
                    state.note.as_deref().unwrap_or("—")
                )
            })
        } else {
            None
        };
        tuner_panel::render(ui, PanelReading {
            frequency: state.frequency,
            cents: state.cents,
            note: display_note.as_deref(),
            in_tune: state.in_tune,
            level: state.level,
            listening: self.listening,
            a4: self.a4,
            target_hint: target_hint.as_deref(),
        }).on_hover_text("La nota central es la referencia del arco. Flecha izquierda: subir el tono; derecha: bajarlo. Verde: afinado. En Análisis podés ver también la nota detectada.");

        ui.add_space(3.0);
        ui.horizontal(|ui| {
            let label = if self.listening {
                "Pausar mic."
            } else {
                "Escuchar"
            };
            if ui
                .add_sized([88.0, 28.0], egui::Button::new(label))
                .on_hover_text("Iniciar o detener el micrófono · Espacio / M")
                .clicked()
            {
                self.toggle_mic(context);
            }
            let picker_width = (ui.available_width() - 172.0).clamp(140.0, 280.0);
            self.tuning_picker(ui, "compact_tuning", picker_width);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(self.analysis_open, "Análisis")
                    .clicked()
                {
                    self.analysis_open = !self.analysis_open;
                }
                if ui.selectable_label(self.settings_open, "Ajustes").clicked() {
                    self.settings_open = !self.settings_open;
                }
            });
        });

        ui.horizontal_wrapped(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(6.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(
                dot.center(),
                2.5,
                if self.listening {
                    egui::Color32::from_rgb(112, 213, 104)
                } else {
                    egui::Color32::from_gray(83)
                },
            );
            let device = self
                .audio
                .selected_input_device
                .as_deref()
                .unwrap_or("Entrada del sistema");
            ui.small(fit_label(ui, device, ui.available_width().min(200.0)))
                .on_hover_text(device);
            if self.audio_is_playing_tone() && ui.small_button("Detener tono · R").clicked() {
                self.audio.stop_tone();
            }
            if state.is_power {
                ui.small("Power chord");
            }
            let diagnostics_count = state.diagnostics.len() + state.backend_diagnostics.len();
            if diagnostics_count > 0
                && ui
                    .small_button(format!("Avisos: {diagnostics_count}"))
                    .clicked()
            {
                self.analysis_open = true;
            }
        });
        self.draw_errors(ui, state);
    }

    fn draw_controls(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.heading("Afinación");
        self.tuning_picker(ui, "settings_tuning", ui.available_width().min(360.0));

        ui.collapsing("Editar afinación actual", |ui| {
            let Some(tuning) = self.tunings.get_mut(self.tuning_index) else {
                return;
            };
            if tuning.strings.is_empty() {
                ui.small("El modo cromático sigue todas las notas; no tiene cuerdas fijas.");
            }
            let mut changed = false;
            for string in &mut tuning.strings {
                ui.horizontal(|ui| {
                    ui.label(get_note_display(string));
                    changed |= ui
                        .add(egui::Slider::new(&mut string.frequency, 20.0..=1_200.0).text("Hz"))
                        .changed();
                });
            }
            if changed {
                if let Ok(mut engine) = self.engine.lock() {
                    engine.set_tuning(tuning.clone());
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Referencia A4");
            if ui
                .add(egui::Slider::new(&mut self.a4, 420.0..=460.0).text("Hz"))
                .changed()
            {
                if let Ok(mut engine) = self.engine.lock() {
                    engine.set_a4(self.a4);
                }
            }
        });
        ui.add_space(8.0);
        ui.heading("Entrada de audio");
        ui.horizontal(|ui| {
            let previous = self.audio.selected_input_device.clone();
            let device = self
                .audio
                .selected_input_device
                .as_deref()
                .unwrap_or("Predeterminada del sistema");
            let width = (ui.available_width() - 100.0).max(160.0);
            egui::ComboBox::from_id_source("input_device")
                .width(width)
                .selected_text(fit_label(ui, device, width - 42.0))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.audio.selected_input_device,
                        None,
                        "Predeterminada del sistema",
                    );
                    for name in &self.audio.input_devices {
                        ui.selectable_value(
                            &mut self.audio.selected_input_device,
                            Some(name.clone()),
                            name,
                        );
                    }
                });
            if ui.button("Actualizar").clicked() {
                self.audio.refresh();
            }
            if self.audio.selected_input_device != previous {
                self.restart_mic(context);
            }
        });
        if ui
            .button(if self.listening {
                "Detener micrófono"
            } else {
                "Iniciar micrófono"
            })
            .clicked()
        {
            self.toggle_mic(context);
        }

        ui.add_space(8.0);
        ui.heading("Tonos y entrenamiento auditivo");
        let has_strings = self
            .tunings
            .get(self.tuning_index)
            .is_some_and(|tuning| !tuning.strings.is_empty());
        ui.horizontal_wrapped(|ui| {
            let playing = self.audio_is_playing_tone();
            if ui
                .add_enabled(
                    has_strings || playing,
                    egui::Button::new(if playing {
                        "Detener tono"
                    } else {
                        "Tono de referencia"
                    }),
                )
                .on_hover_text("Reproduce la primera cuerda de la afinación. Atajo: R.")
                .clicked()
            {
                self.toggle_reference_tone();
            }
            if ui
                .add_enabled(has_strings, egui::Button::new("Nota aleatoria"))
                .on_hover_text(
                    "Entrenamiento auditivo: reproduce una cuerda al azar de la afinación actual.",
                )
                .clicked()
            {
                self.play_random_string();
            }
        });
        if !has_strings {
            ui.small("Elegí una afinación con cuerdas para usar los tonos de referencia y el entrenamiento.");
        }
        ui.add_space(8.0);
        ui.separator();
        ui.small("Espacio / M: micrófono     ·     R: tono de referencia");
        ui.small("Los gráficos, el espectro y los diagnósticos están en Análisis.");
    }

    fn draw_analysis(&mut self, ui: &mut egui::Ui, state: &TunerViewState) {
        ui.heading("Lectura de afinación");
        egui::Grid::new("reading_details")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                ui.label("Nota detectada");
                ui.monospace(if state.frequency.is_some() {
                    state.note.as_deref().unwrap_or("—")
                } else {
                    "—"
                });
                ui.end_row();
                ui.label("Referencia del indicador");
                ui.monospace(state.display_note().unwrap_or_else(|| "—".to_string()));
                ui.end_row();
                ui.label("Frecuencia");
                ui.monospace(
                    state
                        .frequency
                        .map(|frequency| format!("{} Hz", format_freq(frequency)))
                        .unwrap_or_else(|| "—".to_string()),
                );
                ui.end_row();
                ui.label("Desviación");
                ui.monospace(if state.frequency.is_some() {
                    format!("{:+.1} cents", state.cents)
                } else {
                    "—".to_string()
                });
                ui.end_row();
                ui.label("Confianza");
                ui.monospace(format!("{:.0}%", state.confidence * 100.0));
                ui.end_row();
                if state.is_power {
                    ui.label("Acorde");
                    ui.label("Power chord detectado");
                    ui.end_row();
                }
            });
        ui.add(
            egui::ProgressBar::new(state.level)
                .text("Nivel de entrada")
                .desired_width(ui.available_width().min(520.0)),
        );
        ui.add_space(5.0);
        ui.heading("Señal e historial");
        ui.small("Forma de onda · indicador lineal · historial de cents");
        self.visualization.draw_primary(ui, state);
        ui.add_space(8.0);

        let spectrum_was_enabled = self.show_spectrum || self.visualization.show_spectrogram;
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_spectrum, "Espectro");
            ui.checkbox(&mut self.visualization.show_spectrogram, "Espectrograma");
        });
        let spectrum_is_enabled = self.show_spectrum || self.visualization.show_spectrogram;
        if spectrum_is_enabled != spectrum_was_enabled {
            if let Ok(mut engine) = self.engine.lock() {
                engine.set_spectrum_enabled(spectrum_is_enabled);
            }
            if !self.visualization.show_spectrogram {
                self.visualization.clear_spectrogram();
            }
        }
        if self.show_spectrum {
            self.visualization
                .draw_spectrum(ui, state, state.sample_rate.max(1.0));
        }
        if self.visualization.show_spectrogram {
            self.visualization.draw_spectrogram(ui);
        }
        if !self.listening {
            ui.small("Iniciá el micrófono para ver la señal y los gráficos en vivo.");
        }
        ui.add_space(8.0);
        ui.separator();
        ui.heading("Diagnósticos");
        self.draw_diagnostics(ui, state);
        if state.error.is_none()
            && state.diagnostics.is_empty()
            && state.backend_diagnostics.is_empty()
        {
            ui.small("Sin avisos.");
        }
    }

    fn show_settings(&mut self, context: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        context.show_viewport_immediate(
            egui::ViewportId::from_hash_of("tuner_settings"),
            egui::ViewportBuilder::default()
                .with_title("Tuner · Ajustes")
                .with_inner_size([480.0, 560.0])
                .with_min_inner_size([380.0, 300.0]),
            |context, class| {
                if class == egui::ViewportClass::Embedded {
                    egui::Window::new("Ajustes")
                        .open(&mut open)
                        .default_width(440.0)
                        .vscroll(true)
                        .show(context, |ui| self.draw_controls(ui, context));
                } else {
                    self.handle_shortcuts(context);
                    if context.input(|input| input.viewport().close_requested()) {
                        open = false;
                    }
                    egui::CentralPanel::default()
                        .frame(egui::Frame::central_panel(&context.style()).inner_margin(16.0))
                        .show(context, |ui| {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.draw_controls(ui, context));
                        });
                }
            },
        );
        self.settings_open = open;
    }

    fn show_analysis(&mut self, context: &egui::Context, state: &TunerViewState) {
        if !self.analysis_open {
            return;
        }
        let mut open = true;
        context.show_viewport_immediate(
            egui::ViewportId::from_hash_of("tuner_analysis"),
            egui::ViewportBuilder::default()
                .with_title("Tuner · Análisis")
                .with_inner_size([620.0, 620.0])
                .with_min_inner_size([420.0, 320.0]),
            |context, class| {
                if class == egui::ViewportClass::Embedded {
                    egui::Window::new("Análisis")
                        .open(&mut open)
                        .default_width(560.0)
                        .vscroll(true)
                        .show(context, |ui| self.draw_analysis(ui, state));
                } else {
                    self.handle_shortcuts(context);
                    if context.input(|input| input.viewport().close_requested()) {
                        open = false;
                    }
                    egui::CentralPanel::default()
                        .frame(egui::Frame::central_panel(&context.style()).inner_margin(16.0))
                        .show(context, |ui| {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.draw_analysis(ui, state));
                        });
                }
            },
        );
        self.analysis_open = open;
    }
}

impl eframe::App for App {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if self.compact_layout_pending {
            context.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(680.0, 340.0)));
            self.compact_layout_pending = false;
        }
        self.handle_shortcuts(context);
        let state = self
            .state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default();
        self.visualization.capture(&state, self.listening);

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&context.style()).inner_margin(12.0))
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.draw_compact(ui, &state, context);
                    });
            });
        self.show_settings(context);
        self.show_analysis(context, &state);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("compact_layout_version", "1".to_owned());
        storage.set_string("a4", self.a4.to_string());
        storage.set_string("tuning_index", self.tuning_index.to_string());
        storage.set_string("show_spectrum", self.show_spectrum.to_string());
        storage.set_string(
            "input_device",
            self.audio.selected_input_device.clone().unwrap_or_default(),
        );
        storage.set_string(
            "show_spectrogram",
            self.visualization.show_spectrogram.to_string(),
        );
    }
}

fn tuning_label(tuning: &Tuning) -> &str {
    if tuning.strings.is_empty() {
        "Cromático · todos los instrumentos"
    } else {
        tuning.name
    }
}

fn fit_label(ui: &egui::Ui, text: &str, max_width: f32) -> String {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = ui.visuals().text_color();
    if ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), color)
        .size()
        .x
        <= max_width
    {
        return text.to_owned();
    }
    let mut shortened = text.to_owned();
    while !shortened.is_empty() {
        shortened.pop();
        let candidate = format!("{shortened}…");
        if ui
            .painter()
            .layout_no_wrap(candidate.clone(), font.clone(), color)
            .size()
            .x
            <= max_width
        {
            return candidate;
        }
    }
    "…".to_owned()
}

fn parse_or<T>(storage: &dyn eframe::Storage, key: &str, fallback: T) -> T
where
    T: std::str::FromStr,
{
    storage
        .get_string(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn parse_bool(storage: &dyn eframe::Storage, key: &str) -> bool {
    storage.get_string(key).as_deref() == Some("true")
}
