use crate::serial::SensorFieldReader;
use eframe::egui;
use std::{fmt::Display, io::Read};

/// Starts the graphical part of the app.
pub fn start_gui<R>(mut field_reader: SensorFieldReader<R>) -> eframe::Result
where
    R: Read,
{
    env_logger::init();

    let gui_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NILE Stand")
            .with_inner_size([480.0, 320.0]),

        ..eframe::NativeOptions::default()
    };

    // TODO: Call regularly in separate thread:
    field_reader.update_fields().unwrap();

    eframe::run_native(
        "NILE Stand",
        gui_options,
        Box::new(|_| {
            Ok(Box::new(GuiApp {
                mode: StandMode::default(),
                field_reader,
            }))
        }),
    )
}

/// Type holding the state of the app's GUI.
#[derive(Debug)]
pub struct GuiApp<R>
where
    R: Read,
{
    mode: StandMode,
    field_reader: SensorFieldReader<R>,
}

impl<R> GuiApp<R>
where
    R: Read,
{
    /// Produces text with one line per sensor field showing each field's name and value.
    fn make_fields_table(&self) -> String {
        self.field_reader
            .fields()
            .map(|(name, value)| format!("{name} : {value}"))
            .fold(String::new(), |acc, s| format!("{acc}\n{s}"))
    }
}

impl<R> eframe::App for GuiApp<R>
where
    R: Read,
{
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(&ctx, |ui| {
            ui.columns_const(|[left, right]| {
                // Left side:
                egui::TopBottomPanel::top("Data Panel").show_inside(left, |ui| {
                    ui.label("Piping & Instrumentation Diagram:");
                });

                left.label("Hello!!! :D");

                // Right side:
                egui::TopBottomPanel::top("Right Column Top Panel")
                    .show_inside(right, |ui| ui.label("NILE Stand Telemetry:"));

                right.horizontal_top(|ui| {
                    ui.label("Stand Mode: ");

                    ui.menu_button(self.mode.to_string(), |ui| {
                        if ui.button(StandMode::CheckOut.to_string()).clicked() {
                            self.mode = StandMode::CheckOut;
                            ui.close();
                        }

                        if ui.button(StandMode::OxFilling.to_string()).clicked() {
                            self.mode = StandMode::OxFilling;
                            ui.close();
                        }

                        if ui
                            .button(StandMode::PressurizationAndFiring.to_string())
                            .clicked()
                        {
                            self.mode = StandMode::PressurizationAndFiring;
                            ui.close();
                        }

                        if ui.button(StandMode::Safing.to_string()).clicked() {
                            self.mode = StandMode::Safing;
                            ui.close();
                        }
                    })
                });

                right.label(self.make_fields_table());

                egui::TopBottomPanel::bottom("Right Controls Panel").show_inside(right, |ui| {
                    ui.horizontal(|ui| {
                        // Fire button
                        let fire_response = ui.button("Fire");
                        egui::Popup::menu(&fire_response)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                ui.label("Firing >:3");
                            });

                        // Failsafe button
                        let failsafe_response = ui.button("Failsafe");
                        egui::Popup::menu(&failsafe_response)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| ui.label("Oh no! Stop the dangerous thing! :o"))
                    })
                })
            });
        });
    }
}

/// The different modes that the NILE stand software can take on.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
enum StandMode {
    CheckOut,
    OxFilling,
    PressurizationAndFiring,

    #[default]
    Safing,
}

impl StandMode {
    /// Convert the given [`StandMode`] into a [`String`].
    ///
    /// [`StandMode`]: StandMode
    /// [`String`]: String
    fn to_string(self) -> String {
        Into::<String>::into(self)
    }
}

impl Display for StandMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StandMode::CheckOut => write!(f, "Check Out Mode"),
            StandMode::OxFilling => write!(f, "Ox Filling Mode"),
            StandMode::PressurizationAndFiring => write!(f, "Pressurization & Firing Mode"),
            StandMode::Safing => write!(f, "Safing Mode"),
        }
    }
}

impl Into<String> for StandMode {
    fn into(self) -> String {
        format!("{}", self)
    }
}
