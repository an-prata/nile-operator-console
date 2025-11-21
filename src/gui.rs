use crate::{
    diagram::Diagram,
    field_history::ValueHistory,
    sequence::{Command, CommandSequence, ValveHandle},
    serial::{self, FieldReader, FieldReciever, SensorField, SensorValue},
    stand::{self, StandMode, StandState}
};
use eframe::egui::{self, Color32};
use std::{
    fs, io::{Read, Write}, sync::mpsc::SendError, time::Duration
};

/// Starts the graphical part of the app.
pub fn start_gui<R>(field_reader: FieldReader<R>) -> eframe::Result
where
    R: 'static + Read + Write + Send,
{
    let gui_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NILE Stand")
            .with_inner_size([720.0, 560.0]),

        hardware_acceleration: eframe::HardwareAcceleration::Preferred,

        ..eframe::NativeOptions::default()
    };

    let field_reciever = serial::start_field_thread(field_reader);
    let diagram = Diagram::from_bytes(include_bytes!("../NILE P&ID.png"))
        .expect("Diagram should be valid image");

    eframe::run_native(
        "NILE Stand",
        gui_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(GuiApp {
                serial_conn_has_died: false,
                
                mode: stand::StandMode::default(),
                stand_state: StandState::default(),
                stand_state_changed: true, // True so that stuff updates frame 1

                ox_fail_popup: false,

                fire_time_text: "0".to_string(),
                fire_time: Duration::default(),

                valve_np1_ip1_offset_text: "0".to_string(),
                valve_np1_ip1_offset: 0.0,

                field_reciever,
                field_histories: Vec::new(),

                diagram,

                record_field: "Field to Record".to_string(),
                record_file_path: "Enter Path".to_string(),
                record_file: None
            }))
        }),
    )
}

/// Type holding the state of the app's GUI.
pub struct GuiApp {
    serial_conn_has_died: bool,
    
    /// Mode of operator console as a whole.
    mode: stand::StandMode,

    /// State of the NILE test stand, as reported over serial.
    stand_state: StandState,

    /// Whether or not the NILE test stand's state has changed in the last state update.
    stand_state_changed: bool,

    /// Whether or not to show the ox mode transition failure popup window.
    ox_fail_popup: bool,

    fire_time_text: String,
    fire_time: Duration,

    valve_np1_ip1_offset_text: String,
    valve_np1_ip1_offset: f32,

    field_reciever: FieldReciever,
    field_histories: Vec<ValueHistory<SensorField>>,

    diagram: Diagram,

    record_field: String,
    record_file_path: String,
    record_file: Option<fs::File>
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

        let old_state = self.stand_state.clone();
        self.stand_state.update(&fields);
        self.stand_state_changed = old_state != self.stand_state;

        for field in fields {
            let maybe_find = self.field_histories.iter_mut().find_map(|hist| match hist.top() {
                Some(top) if top.name == field.name => Some(hist),
                _ => None,
            });

            if let Some(history) = maybe_find {
                history.push(field);
            } else {
                let mut hist = ValueHistory::new();
                hist.push(field);
                self.field_histories.push(hist);
            }
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
            if self.stand_state.mode() == StandMode::OxygenFilling || mode == StandMode::OxygenFilling {
                self.handle_oxygen_filling_failure();
            }
            
            log::error!("Mode transition failed: {e}");
        }        
    }

    /// Creates a plot graph
    fn make_plot(&mut self, ui: &mut egui::Ui, id: String, height: Option<f32>, width: f32) {
        let mut plot = egui_plot::Plot::new(id).legend(egui_plot::Legend::default()).width(width);

        if height.is_some() {
            plot = plot.height(height.unwrap());
        }

        plot.show(ui, |plot_ui| {
            for history in self.field_histories.iter_mut() {
                let display_durration = Duration::from_secs(60);
                history.prune(display_durration);

                if let Some(name) = history.top().map(|t| t.name.as_str()) {
                    let points: Vec<egui_plot::PlotPoint> =
                        history
                            .as_point_span(display_durration)
                            .into_iter()
                            .map(|(dur, t)| {
                                egui_plot::PlotPoint::new(-dur.as_secs_f64(), t.value.to_num())
                            })
                            .collect();

                    plot_ui.line(
                        egui_plot::Line::new(
                            name,
                            egui_plot::PlotPoints::Owned(points)
                        )
                    );
                }
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
                        ui.menu_button(self.mode.to_string(), |ui| {
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
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                        ui.label(self.make_fields_table());
                    });
                });

                right.vertical(|ui| {
                    ui.label("Record this field:");
                    ui.text_edit_singleline(&mut self.record_field);
                    
                    ui.label("To this file:");
                    ui.text_edit_singleline(&mut self.record_file_path);

                    let should_close = match &mut self.record_file {
                        Some(file) => {
                            match self.field_reciever.fields().find(|field| field.0.as_str() == self.record_field.as_str()) {
                                Some(field) => {
                                    let value = match field.1 {
                                        SensorValue::UnsignedInt(v) => format!("{}\n", v),
                                        SensorValue::SignedInt(v) => format!("{}\n", v),
                                        SensorValue::Float(v) => format!("{}\n", v),
                                        SensorValue::Boolean(v) => format!("{}\n", v),
                                    };

                                    if let Err(e) = file.write_all(value.as_bytes()) {
                                        log::error!("Failed to write to record file: {e} ...");
                                    }
                                }

                                 None => {
                                     log::warn!("No value to record!");
                                 }
                            }

                            if ui.button(format!("Stop Recording '{}'", self.record_field)).clicked() {
                                let _ = file.flush();
                                true
                            } else { false }
                        }

                        None => {
                            if ui.button(format!("Start Recording '{}'", self.record_field)).clicked() {
                                if let Ok(f) = fs::File::create(self.record_file_path.as_str()) {
                                    self.record_file = Some(f);
                                } else {
                                    log::error!("Failed to open record file at {}! Not Recording!", self.record_file_path);
                                }
                            }

                            false
                        }
                    };

                    if should_close {
                        self.record_file = None;
                    }
                });
                right.vertical(|ui| {
                        self.make_plot(ui, "upper".to_string(), Some(ui.available_height() / 2.1),ui.available_width());
                        ui.columns_const(|[left, right]| {
                            self.make_plot(left, "left".to_string(), None, left.available_width());
                            self.make_plot(right, "right".to_string(),None, right.available_width());
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
                    for valve in self.mode.manual_control_valves() {
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

                    match self.mode {
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

