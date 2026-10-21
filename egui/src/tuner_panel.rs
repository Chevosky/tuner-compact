//! Vector instrument face. This module only presents a resolved reading; the
//! audio engine remains the owner of pitch, target and in-tune state.

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Response, Stroke};

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 224.0;
const GREEN: Color32 = Color32::from_rgb(92, 255, 49);
const CYAN: Color32 = Color32::from_rgb(132, 218, 249);
const AMBER: Color32 = Color32::from_rgb(255, 192, 79);
const OFF: Color32 = Color32::from_rgb(27, 38, 29);

pub(crate) struct PanelReading<'a> {
    pub(crate) frequency: Option<f32>,
    pub(crate) cents: f32,
    pub(crate) note: Option<&'a str>,
    pub(crate) in_tune: bool,
    pub(crate) level: f32,
    pub(crate) listening: bool,
    pub(crate) a4: f32,
    pub(crate) target_hint: Option<&'a str>,
}

/// Allocate a compact, scalable instrument face, without images or textures.
pub(crate) fn render(ui: &mut egui::Ui, reading: PanelReading<'_>) -> Response {
    let width = ui.available_width().max(1.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, width * HEIGHT / WIDTH),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let face = Face {
        painter: &painter,
        origin: rect.min,
        scale: width / WIDTH,
    };
    face.background(rect);

    let frequency = reading
        .frequency
        .filter(|frequency| frequency.is_finite() && *frequency > 0.0)
        .filter(|_| reading.listening);
    let note = reading.note.and_then(parse_note);
    let has_reading = frequency.is_some() && note.is_some() && reading.cents.is_finite();
    // Silence and invalid data must never light the in-tune indicator.
    let in_tune = has_reading && reading.in_tune;
    let indicator = if in_tune { GREEN } else { AMBER };

    face.arc(has_reading.then_some(reading.cents), indicator);

    face.text(
        [30.0, 21.0],
        Align2::LEFT_CENTER,
        &frequency
            .map(|frequency| format!("{frequency:.2} Hz"))
            .unwrap_or_else(|| "— Hz".to_owned()),
        17.0,
        if frequency.is_some() { CYAN } else { muted() },
        true,
    );
    face.text(
        [610.0, 21.0],
        Align2::RIGHT_CENTER,
        &if has_reading {
            format!("{:+.1} cents", reading.cents)
        } else {
            "— cents".to_owned()
        },
        17.0,
        if has_reading { CYAN } else { muted() },
        true,
    );

    let note_color = if has_reading {
        if in_tune {
            GREEN
        } else {
            Color32::from_rgb(124, 222, 98)
        }
    } else {
        Color32::from_rgb(73, 92, 77)
    };
    if has_reading {
        if let Some(note) = note {
            face.segment_character([279.0, 73.0], 1.0, note.letter, note_color, true);
            face.accidental([361.0, 79.0], note.accidental, note_color);
            let octave = note.octave.to_string();
            let octave_scale = 0.34;
            let start_x = 361.0 - (octave.chars().count().saturating_sub(1) as f32 * 11.5);
            for (index, character) in octave.chars().enumerate() {
                face.segment_character(
                    [start_x + index as f32 * 23.0, 137.0],
                    octave_scale,
                    character,
                    note_color,
                    true,
                );
            }
        }
    } else {
        face.segment_character([279.0, 73.0], 1.0, '-', note_color, false);
        face.accidental([361.0, 79.0], Accidental::Natural, OFF);
    }

    // Direction means the adjustment to make: left arrow points up in pitch,
    // right arrow points down. Both illuminate only when the engine says tuned.
    face.arrow(
        [211.0, 121.0],
        true,
        has_reading && (in_tune || reading.cents < 0.0),
        indicator,
    );
    face.arrow(
        [429.0, 121.0],
        false,
        has_reading && (in_tune || reading.cents > 0.0),
        indicator,
    );

    face.input_level(reading.level, reading.listening);
    face.text(
        [534.0, 176.0],
        Align2::CENTER_CENTER,
        "A4",
        10.0,
        muted(),
        false,
    );
    face.text(
        [534.0, 194.0],
        Align2::CENTER_CENTER,
        &format!("{:.1} Hz", reading.a4),
        17.0,
        CYAN,
        true,
    );
    // A quiet wordmark rather than the third-party logo in the reference.
    for (index, character) in "TUNER".chars().enumerate() {
        face.text(
            [289.0 + index as f32 * 15.5, 190.0],
            Align2::CENTER_CENTER,
            &character.to_string(),
            16.0,
            Color32::from_rgb(100, 112, 109),
            true,
        );
    }
    let hint = reading.target_hint.unwrap_or(if reading.listening {
        "ESCUCHANDO"
    } else {
        "MICRÓFONO EN PAUSA"
    });
    let hint: String = hint.chars().take(34).collect();
    face.text(
        [320.0, 213.0],
        Align2::CENTER_CENTER,
        &hint,
        10.0,
        if in_tune {
            Color32::from_rgb(128, 179, 125)
        } else {
            muted()
        },
        false,
    );

    response
}

struct Face<'a> {
    painter: &'a Painter,
    origin: Pos2,
    scale: f32,
}

impl Face<'_> {
    fn point(&self, point: [f32; 2]) -> Pos2 {
        self.origin + egui::vec2(point[0], point[1]) * self.scale
    }

    fn text(
        &self,
        position: [f32; 2],
        alignment: Align2,
        text: &str,
        size: f32,
        color: Color32,
        monospace: bool,
    ) {
        let font = if monospace {
            FontId::monospace(size * self.scale)
        } else {
            FontId::proportional(size * self.scale)
        };
        self.painter
            .text(self.point(position), alignment, text, font, color);
    }

    fn line(&self, points: [[f32; 2]; 2], width: f32, color: Color32) {
        self.painter.line_segment(
            [self.point(points[0]), self.point(points[1])],
            Stroke::new(width * self.scale, color),
        );
    }

    fn background(&self, rect: Rect) {
        self.painter
            .rect_filled(rect, 7.0 * self.scale, Color32::from_rgb(5, 8, 8));

        // A shallow curved glass highlight, using a tiny vertex-color mesh.
        // There is no reflection outside the actual instrument window.
        let mut gloss = egui::epaint::Mesh::default();
        const STRIPS: u32 = 32;
        for index in 0..=STRIPS {
            let t = index as f32 / STRIPS as f32;
            let x = 3.0 + t * (WIDTH - 6.0);
            let curve = 84.0 + 37.0 * (1.0 - (t * 2.0 - 1.0).powi(2));
            gloss.colored_vertex(self.point([x, 3.0]), Color32::from_rgb(35, 40, 39));
            gloss.colored_vertex(self.point([x, curve]), Color32::from_rgb(15, 20, 18));
            if index > 0 {
                let previous = (index - 1) * 2;
                gloss.add_triangle(previous, previous + 2, previous + 1);
                gloss.add_triangle(previous + 1, previous + 2, previous + 3);
            }
        }
        self.painter.add(egui::Shape::mesh(gloss));
        self.painter.rect_stroke(
            rect.shrink(0.5 * self.scale),
            7.0 * self.scale,
            Stroke::new(self.scale, Color32::from_rgb(45, 52, 49)),
        );
        self.line(
            [[12.0, 222.0], [628.0, 222.0]],
            0.75,
            Color32::from_rgb(20, 28, 25),
        );
    }

    fn arc_point(cents: f32, radius: f32) -> [f32; 2] {
        let angle = (cents / 50.0 * 55.0).to_radians();
        [320.0 + angle.sin() * radius, 355.0 - angle.cos() * radius]
    }

    fn arc(&self, cents: Option<f32>, active_color: Color32) {
        let clamped = cents.map(|cents| cents.clamp(-50.0, 50.0));
        for tick in -50_i32..=50 {
            let center_zone = tick.abs() <= 4;
            let distance = clamped.map(|cents| (cents - tick as f32).abs());
            let base = if center_zone && cents.is_some() {
                Color32::from_rgb(45, 73, 42)
            } else {
                Color32::from_rgb(48, 58, 52)
            };
            let color = if let Some(distance) = distance.filter(|distance| *distance < 4.0) {
                mix(base, active_color, (1.0 - distance / 4.0) * 0.72)
            } else {
                base
            };
            let major = tick % 25 == 0;
            self.line(
                [
                    Self::arc_point(tick as f32, 341.0),
                    Self::arc_point(tick as f32, if major { 309.0 } else { 314.0 }),
                ],
                if major { 2.8 } else { 2.15 },
                color,
            );
        }

        for (value, position) in [
            ("−50", [25.0, 161.0]),
            ("−25", [155.0, 36.0]),
            ("0", [320.0, 61.0]),
            ("+25", [485.0, 36.0]),
            ("+50", [615.0, 161.0]),
        ] {
            self.text(position, Align2::CENTER_CENTER, value, 10.0, muted(), true);
        }

        if let Some(cents) = clamped {
            let points = [Self::arc_point(cents, 342.0), Self::arc_point(cents, 312.0)];
            self.line(points, 15.0, with_alpha(active_color, 14));
            self.line(points, 9.0, with_alpha(active_color, 38));
            self.line(points, 4.4, active_color);
            let angle = (cents / 50.0 * 55.0).to_radians();
            let base = Self::arc_point(cents, 312.0);
            let tangent = [angle.cos() * 5.0, angle.sin() * 5.0];
            self.painter.add(egui::Shape::convex_polygon(
                vec![
                    self.point([base[0] - tangent[0], base[1] - tangent[1]]),
                    self.point([base[0] + tangent[0], base[1] + tangent[1]]),
                    self.point(Self::arc_point(cents, 303.0)),
                ],
                active_color,
                Stroke::NONE,
            ));
        }
    }

    fn segment_character(
        &self,
        position: [f32; 2],
        size: f32,
        character: char,
        color: Color32,
        glow: bool,
    ) {
        // Segment order: top, upper-right, lower-right, bottom, lower-left,
        // upper-left, middle. B and D use their conventional LCD lowercases.
        const SEGMENTS: [[[f32; 2]; 6]; 7] = [
            [
                [10.0, 0.0],
                [50.0, 0.0],
                [56.0, 5.0],
                [50.0, 10.0],
                [10.0, 10.0],
                [4.0, 5.0],
            ],
            [
                [56.0, 8.0],
                [61.0, 13.0],
                [61.0, 43.0],
                [56.0, 48.0],
                [51.0, 43.0],
                [51.0, 13.0],
            ],
            [
                [56.0, 50.0],
                [61.0, 55.0],
                [61.0, 85.0],
                [56.0, 90.0],
                [51.0, 85.0],
                [51.0, 55.0],
            ],
            [
                [10.0, 86.0],
                [50.0, 86.0],
                [56.0, 91.0],
                [50.0, 96.0],
                [10.0, 96.0],
                [4.0, 91.0],
            ],
            [
                [4.0, 50.0],
                [9.0, 55.0],
                [9.0, 85.0],
                [4.0, 90.0],
                [-1.0, 85.0],
                [-1.0, 55.0],
            ],
            [
                [4.0, 8.0],
                [9.0, 13.0],
                [9.0, 43.0],
                [4.0, 48.0],
                [-1.0, 43.0],
                [-1.0, 13.0],
            ],
            [
                [10.0, 43.0],
                [50.0, 43.0],
                [56.0, 48.0],
                [50.0, 53.0],
                [10.0, 53.0],
                [4.0, 48.0],
            ],
        ];
        let mask = segment_mask(character);
        for (index, segment) in SEGMENTS.iter().enumerate() {
            let lit = mask & (1 << index) != 0;
            let points: Vec<_> = segment
                .iter()
                .map(|point| {
                    self.point([
                        position[0] + (point[0] + (96.0 - point[1]) * 0.075) * size,
                        position[1] + point[1] * size,
                    ])
                })
                .collect();
            if lit && glow {
                self.painter.add(egui::Shape::convex_polygon(
                    points.clone(),
                    with_alpha(color, 10),
                    Stroke::new(7.0 * size * self.scale, with_alpha(color, 18)),
                ));
                self.painter.add(egui::Shape::convex_polygon(
                    points.clone(),
                    with_alpha(color, 12),
                    Stroke::new(3.0 * size * self.scale, with_alpha(color, 36)),
                ));
            }
            self.painter.add(egui::Shape::convex_polygon(
                points,
                if lit { color } else { OFF },
                Stroke::NONE,
            ));
        }
    }

    fn accidental(&self, position: [f32; 2], accidental: Accidental, color: Color32) {
        if accidental == Accidental::Flat {
            let points = [
                [3.0, 0.0],
                [3.0, 29.0],
                [13.0, 23.0],
                [15.0, 17.0],
                [12.0, 13.0],
                [8.0, 13.0],
                [3.0, 18.0],
            ];
            self.painter.add(egui::Shape::line(
                points
                    .iter()
                    .map(|point| self.point([position[0] + point[0], position[1] + point[1]]))
                    .collect(),
                Stroke::new(2.7 * self.scale, color),
            ));
            return;
        }
        let color = if accidental == Accidental::Sharp {
            color
        } else {
            OFF
        };
        for points in [
            [[7.0, 1.0], [3.0, 29.0]],
            [[18.0, 0.0], [14.0, 28.0]],
            [[0.0, 12.0], [23.0, 7.0]],
            [[0.0, 23.0], [22.0, 18.0]],
        ] {
            self.line(
                points.map(|point| [position[0] + point[0], position[1] + point[1]]),
                2.8,
                color,
            );
        }
    }

    fn arrow(&self, center: [f32; 2], points_right: bool, lit: bool, color: Color32) {
        let direction = if points_right { 1.0 } else { -1.0 };
        let mut points = vec![
            self.point([center[0] - 17.0 * direction, center[1] - 21.0]),
            self.point([center[0] + 22.0 * direction, center[1]]),
            self.point([center[0] - 17.0 * direction, center[1] + 21.0]),
        ];
        if !points_right {
            points.reverse();
        }
        if lit {
            self.painter.add(egui::Shape::convex_polygon(
                points.clone(),
                with_alpha(color, 14),
                Stroke::new(9.0 * self.scale, with_alpha(color, 18)),
            ));
            self.painter.add(egui::Shape::convex_polygon(
                points.clone(),
                with_alpha(color, 14),
                Stroke::new(4.0 * self.scale, with_alpha(color, 32)),
            ));
        }
        self.painter.add(egui::Shape::convex_polygon(
            points,
            if lit {
                color
            } else {
                Color32::from_rgb(24, 35, 27)
            },
            Stroke::new(
                self.scale,
                if lit {
                    mix(color, Color32::WHITE, 0.15)
                } else {
                    OFF
                },
            ),
        ));
    }

    fn input_level(&self, level: f32, listening: bool) {
        let active = if listening && level.is_finite() {
            (level.clamp(0.0, 1.0) * 28.0).round() as usize
        } else {
            0
        };
        for index in 0..28 {
            let x = 61.0 + index as f32 * 3.5;
            let color = if index < active {
                if index >= 25 {
                    AMBER
                } else {
                    CYAN
                }
            } else {
                Color32::from_rgb(25, 50, 61)
            };
            self.line([[x, 204.0], [x, 210.0]], 1.7, color);
        }
        self.text(
            [108.0, 187.0],
            Align2::CENTER_CENTER,
            if listening { "MIC" } else { "PAUSA" },
            11.0,
            if listening { CYAN } else { muted() },
            true,
        );
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Accidental {
    Natural,
    Sharp,
    Flat,
}

struct DisplayNote {
    letter: char,
    accidental: Accidental,
    octave: i16,
}

fn parse_note(note: &str) -> Option<DisplayNote> {
    let mut characters = note.trim().chars().peekable();
    let letter = characters.next()?.to_ascii_uppercase();
    if !('A'..='G').contains(&letter) {
        return None;
    }
    let accidental = match characters.peek().copied() {
        Some('#' | '♯') => {
            characters.next();
            Accidental::Sharp
        }
        Some('b' | '♭') => {
            characters.next();
            Accidental::Flat
        }
        _ => Accidental::Natural,
    };
    let octave = characters.collect::<String>().parse::<i16>().ok()?;
    Some(DisplayNote {
        letter,
        accidental,
        octave,
    })
}

fn segment_mask(character: char) -> u8 {
    match character.to_ascii_uppercase() {
        '0' => 0x3f,
        '1' => 0x06,
        '2' => 0x5b,
        '3' => 0x4f,
        '4' => 0x66,
        '5' => 0x6d,
        '6' => 0x7d,
        '7' => 0x07,
        '8' => 0x7f,
        '9' => 0x6f,
        'A' => 0x77,
        'B' => 0x7c,
        'C' => 0x39,
        'D' => 0x5e,
        'E' => 0x79,
        'F' => 0x71,
        'G' => 0x3d,
        '-' | '—' => 0x40,
        _ => 0,
    }
}

fn muted() -> Color32 {
    Color32::from_rgb(105, 122, 117)
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount) as u8;
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}
