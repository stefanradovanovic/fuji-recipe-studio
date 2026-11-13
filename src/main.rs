#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

fn main() {
    // Reserve one logical core for interaction and other desktop applications.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .saturating_sub(1)
        .clamp(1, 12);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_maximized(true)
        .with_min_inner_size([1000.0, 680.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/studio.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "Fuji Recipe Studio",
        options,
        Box::new(|cc| Ok(Box::new(ui::Studio::new(cc)))),
    ) {
        rfd::MessageDialog::new()
            .set_title("Fuji Recipe Studio could not start")
            .set_description(format!("The Windows graphics interface could not start.\n\n{error}\n\nThis app requires Windows 10/11 and a DirectX 12 compatible graphics driver."))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
