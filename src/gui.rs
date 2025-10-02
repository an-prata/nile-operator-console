use eframe::egui;

/// Starts the graphical part of the app.
pub fn start_gui() -> eframe::Result {
    env_logger::init();

    let gui_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NILE Stand")
            .with_inner_size([480.0, 320.0]),

        ..Default::default()
    };

    eframe::run_native(
        "NILE Stand",
        gui_options,
        Box::new(|_| Ok(Box::<GuiApp>::default())),
    )
}

/// Type holding the state of the app's GUI.
#[derive(Debug, Default)]
pub struct GuiApp {}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(&ctx, |ui| {
            ui.label("Hello!!! :D");
        });
    }
}
