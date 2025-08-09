#[cfg(feature = "gui")]
use eframe::{
    NativeOptions,
    egui::{IconData, ViewportBuilder},
};
#[cfg(feature = "gui")]
use sarpro::gui::models::SarproGui;

#[cfg(feature = "gui")]
fn main() -> Result<(), eframe::Error> {
    let icon = include_bytes!("../assets/sarprogui_icon.png");
    let image = image::load_from_memory(icon)
        .expect("Failed to open icon path")
        .to_rgba8();
    let (icon_width, icon_height) = image.dimensions();

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([600.0, 400.0])
            .with_icon(IconData {
                rgba: image.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        ..Default::default()
    };

    eframe::run_native(
        "SARPRO",
        options,
        Box::new(|cc| {
            // Enforce dark theme globally regardless of OS theme
            cc.egui_ctx
                .set_theme(eframe::egui::ThemePreference::Dark);
            cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());

            Ok(Box::new(SarproGui::default()))
        }),
    )
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("GUI feature is not enabled. Please build with --features gui");
    std::process::exit(1);
}
