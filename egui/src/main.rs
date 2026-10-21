#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod audio;
mod diagnostics;
mod state;
mod tuner_panel;
mod visualization;

use app::App;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        // Keep the instrument's dark palette, including on macOS theme changes.
        follow_system_theme: false,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([680.0, 340.0])
            .with_min_inner_size([520.0, 290.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Guitar Tuner",
        options,
        Box::new(|creation_context| {
            App::configure_style(&creation_context.egui_ctx);
            let mut app = App::default();
            app.restore(creation_context.storage);
            app.refresh_devices();
            Box::new(app)
        }),
    )
}
