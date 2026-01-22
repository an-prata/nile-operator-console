use crate::{
    diagram::Diagram,
    field_history::ValueHistory,
    record::StandRecord,
    sequence::{Command, CommandSequence, ValveHandle},
    serial::{self, FieldReciever, SensorField, SensorValue},
    stand::{StandMode, StandState},
};
use eframe::egui::{self, Color32};
use std::{collections::HashMap, hash::Hash, sync::mpsc::SendError, time::Duration};

const HISTORY_LENGTH: Duration = Duration::from_secs(60);

/// Starts the graphical part of the app.
pub fn start_gui(field_rx: FieldReciever) -> eframe::Result {
    let gui_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NILE Operator Console")
            .with_inner_size([720.0, 560.0]),

        hardware_acceleration: eframe::HardwareAcceleration::Preferred,

        ..eframe::NativeOptions::default()
    };

    let diagram = Diagram::from_bytes(include_bytes!("../NILE P&ID.png"))
        .expect("Diagram should be valid image");

    eframe::run_native(
        "NILE Operator Console",
        gui_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(GuiApp {
                serial_conn_has_died: false,

                stand_state: StandState::default(),
                stand_state_changed: true, // True so that stuff updates frame 1

                ox_fail_popup: false,

                fire_time_text: "0".to_string(),
                fire_time: Duration::default(),

                target_ox_fuel_ratio: 1.0,
                target_ox_fuel_ratio_text: "1.0".to_string(),
                target_ox_fuel_deviation: 0.5,
                target_ox_fuel_deviation_text: "1.0".to_string(),

                valve_np1_ip1_offset_text: "0".to_string(),
                valve_np1_ip1_offset: 0.0,

                field_reciever: field_rx,
                field_histories: HashMap::new(),

                diagram,

                record_file_path: "Enter Path".to_string(),
                record_file: None,
            }))
        }),
    )
}

/// Type holding the state of the app's GUI.
pub struct GuiApp {
    serial_conn_has_died: bool,

    /// State of the NILE test stand, as reported over serial.
    stand_state: StandState,

    /// Whether or not the NILE test stand's state has changed in the last state update.
    stand_state_changed: bool,

    /// Whether or not to show the ox mode transition failure popup window.
    ox_fail_popup: bool,

    /// The text entered by the user for the duration of the engine burn.
    fire_time_text: String,
    /// The parsed time of the engine burn in the firing sequence.
    fire_time: Duration,

    target_ox_fuel_ratio: f32,
    target_ox_fuel_ratio_text: String,
    target_ox_fuel_deviation: f32,
    target_ox_fuel_deviation_text: String,

    /// Text entered by the user for the offset between the actuation of the two valves used for
    /// firing the engine.
    valve_np1_ip1_offset_text: String,
    /// The parsed delay (signed to indicate order) between the actuation of NP1 and IP1 during
    /// firing.
    valve_np1_ip1_offset: f32,

    /// The I/O or simulation device from which we get field values and send commands.
    field_reciever: FieldReciever,
    /// A history of field values used for
    field_histories: HashMap<String, ValueHistory<SensorField>>,

    /// The piping and instrumentation diagram which displays the valve states visually.
    diagram: Diagram,

    /// CSV file path for saving/recording the history of all fields we get.
    record_file_path: String,
    /// The actual CSV record of fields.
    record_file: Option<StandRecord>,
}

impl GuiApp {
    /// Have the [`GuiApp`]'s internal [`FieldReciever`] recieve [`SensorField`]s.
    ///
    /// [`GuiApp`]: GuiApp
    /// [`FieldReciever`]: FieldReciever
    /// [`SensorField`]: serial::SensorField
    fn recieve_fields(&mut self) {
        if let Err(_) = self.field_reciever.recieve_fields() {
            self.serial_conn_has_died = true;
            log::error!("Serial connection has died!");
        }
    }

    /// Produces text with one line per sensor field showing each field's name and value.
    fn make_fields_table(&self) -> String {
        let mut fields: Vec<(&String, &SensorValue)> = self.field_reciever.fields().collect();
        fields.sort_unstable_by_key(|(k, _)| k.to_owned());

        fields
            .into_iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .fold(String::new(), |acc, s| format!("{acc}\n{s}"))
    }

    /// Update the [`GuiApp`]'s internal record of the NILE stand's state.
    ///
    /// [`GuiApp`]: GuiApp
    fn update_stand_state(&mut self) {
        let fields: Vec<SensorField> = self
            .field_reciever
            .fields()
            .map(|(name, &value)| SensorField {
                name: name.clone(),
                value,
            })
            .collect();

        if let Some(record) = &mut self.record_file {
            if let Err(e) = record.append_frame(&fields) {
                log::error!("Failed to append record frame: {e}");
            }
        }

        let old_state = self.stand_state.clone();
        self.stand_state.update(&fields);
        self.stand_state_changed = old_state != self.stand_state;

        for field in fields {
            match self.field_histories.get_mut(&field.name) {
                Some(hist) => {
                    hist.push(field);
                }

                None => {
                    let mut hist = ValueHistory::new();
                    let key = field.name.clone();
                    hist.push(field);
                    self.field_histories.insert(key, hist);
                }
            }
        }

        for (_, history) in self.field_histories.iter_mut() {
            history.prune(HISTORY_LENGTH);
        }
    }

    /// Logs the failure to switch modes from/to [`StandMode::OxygenFilling`] and sets the failure
    /// popup window to be visible.
    ///
    /// [`StandMode::OxygenFilling`]: StandMode::OxygenFilling
    fn handle_oxygen_filling_failure(&mut self) {
        self.ox_fail_popup = true;
        log::error!("The oxen have been angered ...");
    }

    /// Show the popup for notifying of failure to switch to/from [`StandMode::OxygenFilling`].
    ///
    /// [`StandMode::OxygenFilling`]: StandMode::OxygenFilling
    fn show_oxygen_filling_failure_popup(&mut self, ctx: &egui::Context) {
        let title = "The Oxen Are Unhappy";

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(title),
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([400.0, 300.0])
                .with_resizable(false)
                .with_always_on_top(),
            |ctx, class| {
                assert!(
                    class == egui::ViewportClass::Immediate,
                    "This egui backend doesn't support multiple viewports"
                );

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("The Oxen were unhappy with your offering of valve states.");
                    ui.image(egui::include_image!("../ox.jpg"));
                    ui.label("Please close all valves.");
                });

                if ctx.input(|i| i.viewport().close_requested()) {
                    self.ox_fail_popup = false;
                }
            },
        );
    }

    /// Set the mode and perform setup behaviors.
    fn set_mode(&mut self, mode: StandMode) {
        if mode == StandMode::Safing {
            let seq = CommandSequence::new()
                .then(Command::OpenValve(ValveHandle::NP3))
                .then(Command::OpenValve(ValveHandle::IP3))
                .then(Command::CloseValve(ValveHandle::NP1))
                .then(Command::CloseValve(ValveHandle::NP2))
                .then(Command::CloseValve(ValveHandle::NP4))
                .then(Command::CloseValve(ValveHandle::IP1))
                .then(Command::CloseValve(ValveHandle::IP2));

            match self.field_reciever.run_sequence(seq) {
                Ok(()) => (),

                Err(SendError(_)) => {
                    self.serial_conn_has_died = true;
                }
            };
        }

        if let Err(e) = self.stand_state.transition_mode(mode) {
            if self.stand_state.mode() == StandMode::OxygenFilling
                || mode == StandMode::OxygenFilling
            {
                self.handle_oxygen_filling_failure();
            }

            log::error!("Mode transition failed: {e}");
        }
    }

    /// Adds an `egui` plot with the given dimensions to the [`egui::Ui`]. If the given width or
    /// height is [`None`] then the plot will consume the available space. The plot will be filled
    /// with field data from the stand.
    ///
    /// [`egui::Ui`]: egui::Ui
    fn make_fields_plot(
        &mut self,
        ui: &mut egui::Ui,
        id: impl Hash,
        height: Option<f32>,
        width: Option<f32>,
    ) {
        let plot = egui_plot::Plot::new(id)
            .legend(egui_plot::Legend::default())
            .width(width.unwrap_or(ui.available_width()))
            .height(height.unwrap_or(ui.available_height()));

        plot.show(ui, |plot_ui| {
            for (field_name, history) in
                self.field_histories
                    .iter_mut()
                    .filter(|(k, _)| match k.as_str() {
                        "NP1" | "NP2" | "NP3" | "NP4" | "IP1" | "IP2" | "IP3" => false,
                        _ => true,
                    })
            {
                let points: Vec<egui_plot::PlotPoint> = history
                    .as_points(HISTORY_LENGTH)
                    .into_iter()
                    .map(|(dur, t)| egui_plot::PlotPoint::new(-dur.as_secs_f64(), t.value.to_num()))
                    .collect();

                plot_ui.line(egui_plot::Line::new(
                    field_name,
                    egui_plot::PlotPoints::Owned(points),
                ));
            }
        });
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.serial_conn_has_died || ctx.input(|i| i.viewport().close_requested()) {
            // unfortunately this doesn't close stuff on its own, and the thread which hosts the
            // window must exit, meaning we cant do a nice connection retry thing.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        } else {
            self.recieve_fields();
            ctx.request_repaint();
        }

        self.update_stand_state();

        if self.ox_fail_popup {
            self.show_oxygen_filling_failure_popup(ctx);
        }

        if self.stand_state_changed {
            self.diagram.reset_image();
            self.diagram.plot_valves(self.stand_state);
            self.diagram.reload_texture(ctx);
        }

        // Main view:
        egui::CentralPanel::default().show(&ctx, |ui| {
            ui.columns_const(|[left, right]| {
                // Right side:
                egui::TopBottomPanel::top("Right Column Top Panel")
                    .show_inside(right, |ui| ui.label("NILE Stand Telemetry:"));

                right.horizontal_wrapped(|ui| {
                    ui.label("Stand Mode: ");

                    ui.centered_and_justified(|ui| {
                        ui.menu_button(self.stand_state.mode().to_string(), |ui| {
                            if ui.button(StandMode::CheckOut.to_string()).clicked() {
                                self.set_mode(StandMode::CheckOut);
                                ui.close();
                            }

                            if ui.button(StandMode::OxygenFilling.to_string()).clicked() {
                                self.set_mode(StandMode::OxygenFilling);
                                ui.close();
                            }

                            if ui
                                .button(StandMode::PressurizationAndFiring.to_string())
                                .clicked()
                            {
                                self.set_mode(StandMode::PressurizationAndFiring);
                                ui.close();
                            }

                            if ui.button(StandMode::Safing.to_string()).clicked() {
                                self.set_mode(StandMode::Safing);
                                ui.close();
                            }
                        })
                    })
                });

                right.vertical(|ui| {
                    ui.columns_const(|[left, right]| {
                        egui::ScrollArea::both().max_height(left.available_height() / 3f32).show(left, |ui| {
                            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                            ui.label(self.make_fields_table());
                        });

                        right.label("Target Ox/Fuel Ratio:");
                        right.text_edit_singleline(&mut self.target_ox_fuel_ratio_text);
                        self.target_ox_fuel_ratio = match self.target_ox_fuel_ratio_text.parse() {
                            Ok(n) => n,
                            Err(_) => self.target_ox_fuel_ratio
                        };

                        right.label("Allowed Deviation from Target:");
                        right.text_edit_singleline(&mut self.target_ox_fuel_deviation_text);
                        self.target_ox_fuel_deviation = match self.target_ox_fuel_deviation_text.parse() {
                            Ok(n) => n,
                            Err(_) => self.target_ox_fuel_deviation
                        };

                        right.style_mut().visuals.code_bg_color = match self.field_histories.get("Ox/Fuel Ratio") {
                            Some(hist) => {
                                let points: Vec<f64> = hist
                                    .as_points(Duration::from_secs(3))
                                    .iter()
                                    .map(|(_, val)| val.value.to_num())
                                    .collect();
                                let window = &points[(points.len() - 4) .. points.len()];
                                let ratio = window.iter().sum::<f64>() / window.len() as f64;
                                ox_fuel_color(self.target_ox_fuel_ratio, self.target_ox_fuel_deviation, ratio as _)
                            }
                            _ => Color32::from_rgb(0, 0, 0),
                        };

                        right.code("Ox/Fuel")
                    });
                });

                right.vertical(|ui| {
                    ui.label("Record Fields To:");
                    ui.text_edit_singleline(&mut self.record_file_path);

                    // handle open/close of record
                    match self.record_file {
                        Some(_) => {
                            if ui.button("Stop Recording").clicked() {
                                self.record_file = None;
                            }
                        }

                        None => {
                            if ui.button("Start Recording").clicked() {
                                let names: Vec<String> = self.field_reciever.fields().map(|(k, _)| k.to_owned()).collect();

                                if let Ok(record) = StandRecord::open(self.record_file_path.as_str(), names) {
                                    self.record_file = Some(record);
                                } else {
                                    log::error!("Failed to open record file at {}! Not Recording!", self.record_file_path);
                                }
                            }
                        }
                    };

                });

                right.vertical(|ui| {
                    self.make_fields_plot(ui, "upper".to_string(), Some(ui.available_height() / 2.1), None);

                    ui.columns_const(|[left, right]| {
                        self.make_fields_plot(left, "left".to_string(), None, None);
                        self.make_fields_plot(right, "right".to_string(),None, None);
                    });
                });


                // Left side:
                egui::TopBottomPanel::top("Data Panel").show_inside(left, |ui| {
                    ui.label("Piping & Instrumentation Diagram:");
                });

                match &self.diagram.texture {
                    None => (),

                    Some(texture_handle) => {
                        left.add(
                            egui::Image::new(egui::load::SizedTexture::from_handle(texture_handle))
                                .shrink_to_fit()
                        );
                    }
                }

                egui::TopBottomPanel::bottom("Controls Panel").show_inside(left, |ui| {
                    for valve in self.stand_state.mode().manual_control_valves() {
                        ui.horizontal(|ui| {
                            ui.columns_const(|[left, right]| {
                                left.centered_and_justified(|ui| {
                                    let res = ui.add(
                                        egui::Button::new(format!("Open {valve}"))
                                            .min_size(egui::Vec2 { x: 32.0, y: 32.0 })
                                    );

                                    if res.clicked() {
                                        self.field_reciever
                                            .send_command(serial::ValveCommand::Open(valve))
                                            .expect("Expected to be able to send command");
                                    }
                                });

                                right.centered_and_justified(|ui| {
                                    let res = ui.add(
                                        egui::Button::new(format!("Close {valve}"))
                                            .min_size(egui::Vec2 { x: 32.0, y: 32.0 })
                                    );

                                    if res.clicked() {
                                        self.field_reciever
                                            .send_command(serial::ValveCommand::Close(valve))
                                            .expect("Expected to be able to send command");
                                    }
                                });
                            })
                        });
                    }

                    match self.stand_state.mode() {
                        StandMode::Safing => {
                            ui.horizontal_wrapped(|ui| {
                                ui.centered_and_justified(|ui| {
                                    if ui.button("Depressurize System").clicked() {
                                        let seq = CommandSequence::new()
                                            .then(Command::OpenValve(ValveHandle::NP3))
                                            .then(Command::OpenValve(ValveHandle::IP3))
                                            .then(Command::Wait(Duration::from_secs(1)))
                                            .then(Command::OpenValve(ValveHandle::NP4))
                                            .then(Command::Wait(Duration::from_secs(5)))
                                            .then(Command::CloseValve(ValveHandle::NP4))
                                            .then(Command::Wait(Duration::from_secs(1)))
                                            .then(Command::OpenValve(ValveHandle::IP2))
                                            .then(Command::Wait(Duration::from_secs(5)))
                                            .then(Command::CloseValve(ValveHandle::IP2))
                                            .then(Command::Wait(Duration::from_secs(1)))
                                            .then(Command::OpenValve(ValveHandle::NP2))
                                            .then(Command::Wait(Duration::from_secs(5)))
                                            .then(Command::CloseValve(ValveHandle::NP2))
                                            .then(Command::Wait(Duration::from_secs(1)))
                                            .then(Command::Done);

                                        if !self.serial_conn_has_died {
                                            self.field_reciever.run_sequence_par(seq);
                                        }
                                    }
                                });
                            });
                        }

                        StandMode::PressurizationAndFiring => {
                            ui.label("Enter time difference between NP1 and IP1 in firing sequence, positive times indicate that NP1 should open first, negative values indicate that IP1 should open first:");

                            let valve_np1_ip1_offset_text_res =
                                ui.text_edit_singleline(&mut self.valve_np1_ip1_offset_text);

                            if let Ok(t) = self.valve_np1_ip1_offset_text.parse() {
                                self.valve_np1_ip1_offset = t;
                            } else if valve_np1_ip1_offset_text_res.lost_focus() {
                                self.valve_np1_ip1_offset_text = "0".to_string();
                            }

                            ui.label("\nEnter fire time:");
                            ui.horizontal(|ui| {
                                let fire_time_text_res =
                                    ui.text_edit_singleline(&mut self.fire_time_text);

                                if let Ok(t) = self.fire_time_text.parse() {
                                    self.fire_time = Duration::from_secs_f64(t);
                                } else if fire_time_text_res.lost_focus() {
                                    self.fire_time_text = "0".to_string();
                                }

                                if ui
                                    .add(
                                        egui::Button::new("Fire")
                                            .fill(Color32::from_rgb(64, 128, 64)),
                                    )
                                    .clicked()
                                {
                                    // take time from op
                                    //
                                    // ignite ignitor
                                    // wait some period of time
                                    // open NP1 and IP1
                                    // wait time from op
                                    // wait three seconds
                                    //
                                    // close NP1 IP1 NP2 IP2 all at once
                                    // open NP3 IP3 to vent

                                    let wait_time = Duration::from_secs(1);
                                    let seq = CommandSequence::new()
                                        .then(Command::Ignite)
                                        .then(Command::Wait(wait_time));

                                    let seq = match self.valve_np1_ip1_offset >= 0f32 {
                                        true => seq
                                            .then(Command::OpenValve(ValveHandle::NP1))
                                            .then(Command::Wait(Duration::from_secs_f32(self.valve_np1_ip1_offset)))
                                            .then(Command::OpenValve(ValveHandle::IP1)),
                                        false => seq
                                            .then(Command::OpenValve(ValveHandle::IP1))
                                            .then(Command::Wait(Duration::from_secs_f32(self.valve_np1_ip1_offset.abs())))
                                            .then(Command::OpenValve(ValveHandle::NP1)),
                                    };

                                    let seq = seq
                                        .then(Command::Wait(self.fire_time))
                                        .then(Command::Wait(Duration::from_secs(3)))
                                        .then(Command::CloseValve(ValveHandle::NP2))
                                        .then(Command::CloseValve(ValveHandle::IP2))
                                        .then(Command::OpenValve(ValveHandle::NP3))
                                        .then(Command::OpenValve(ValveHandle::IP3))
                                        .then(Command::Wait(Duration::from_secs(2)))
                                        .then(Command::CloseValve(ValveHandle::NP1))
                                        .then(Command::CloseValve(ValveHandle::IP1))
                                        .then(Command::Done);

                                    if !self.serial_conn_has_died {
                                        self.field_reciever.run_sequence_par(seq);
                                    }
                                }
                            });
                        }

                        _ => (),
                    };

                    ui.add_space(16.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.centered_and_justified(|ui| {
                            if ui
                                .add(
                                    egui::Button::new("Failsafe")
                                        .fill(Color32::from_rgb(182, 96, 96)),
                                )
                                .clicked()
                            {
                                self.set_mode(StandMode::Safing);
                            }
                        });
                    });
                });
            });
        });
    }
}

/// Computes the color of the "Ox/Fuel" label which is used to indicate a good/not good state of the
/// Ox/Fuel ratio.
fn ox_fuel_color(target: f32, deviation: f32, ratio: f32) -> Color32 {
    if target > ratio {
        let percent_difference = (target - ratio) / deviation;
        Color32::from_rgb(
            (255f32 * percent_difference) as _,
            255 - (255f32 * percent_difference) as u8,
            0,
        )
    } else {
        let percent_difference = (ratio - target) / deviation;
        Color32::from_rgb(
            0,
            255 - (255f32 * percent_difference) as u8,
            (255f32 * percent_difference) as _,
        )
    }
}
