#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod credentials;
mod emoji;
mod theme;
mod worker;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("icône PNG valide");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LINE → Signal")
            .with_app_id("line2signal")
            .with_icon(icon)
            .with_inner_size([1040.0, 740.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "line2signal",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
