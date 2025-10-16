use crate::{
    diagram::Diagram, sequence::{Command, CommandSequence, ValveHandle}, serial::{self, FieldReader, FieldReciever, SensorField, SensorValue}, stand::{StandState, ValveState}
};
use eframe::egui::{self, Color32};
use std::{
    fmt::Display,
    io::{Read, Write},
    time::Duration,
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
                mode: StandMode::default(),
                stand_state: StandState::default(),
                stand_state_changed: true, // True so that stuff updates frame 1

                ox_fail_popup: false,

                fire_time_text: "0".to_string(),
                fire_time: Duration::default(),

                valve_np1_ip1_offset_text: "0".to_string(),
                valve_np1_ip1_offset: 0.0,

                field_reciever,

                diagram
            }))
        }),
    )
}

/// Type holding the state of the app's GUI.
pub struct GuiApp {
    /// Mode of operator console as a whole.
    mode: StandMode,

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

    diagram: Diagram
}

impl GuiApp {
    /// Have the [`GuiApp`]'s internal [`FieldReciever`] recieve [`SensorField`]s.
    ///
    /// [`GuiApp`]: GuiApp
    /// [`FieldReciever`]: FieldReciever
    /// [`SensorField`]: serial::SensorField
    fn recieve_fields(&mut self) {
        self.field_reciever.recieve_fields();
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

        let new_state = StandState::from_fields(&fields);
        self.stand_state_changed = new_state != self.stand_state;
        self.stand_state = new_state;
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
        if self.mode == StandMode::OxygenFilling {
            // Check that valves are closed when leaving ox filling mode
            match self.stand_state {
                StandState {
                    valve_np3: Some(ValveState::Closed),
                    valve_np4: Some(ValveState::Closed),
                    ..
                } => {
                    self.handle_oxygen_filling_failure();
                    return;
                }

                _ => (),
            }
        }

        match mode {
            StandMode::CheckOut => self.mode = StandMode::CheckOut,

            StandMode::OxygenFilling => match self.stand_state {
                StandState {
                    valve_np1: Some(ValveState::Closed),
                    valve_np2: Some(ValveState::Closed),
                    valve_np3: Some(ValveState::Closed),
                    valve_np4: Some(ValveState::Closed),

                    valve_ip1: Some(ValveState::Closed),
                    valve_ip2: Some(ValveState::Closed),
                    valve_ip3: Some(ValveState::Closed),
                    ..
                } => self.mode = StandMode::OxygenFilling,

                _ => {
                    self.handle_oxygen_filling_failure();
                    return;
                }
            },

            StandMode::PressurizationAndFiring => self.mode = StandMode::PressurizationAndFiring,

            StandMode::Safing => {
                let seq = CommandSequence::new()
                    .then(Command::OpenValve(ValveHandle::NP3))
                    .then(Command::OpenValve(ValveHandle::IP3))
                    .then(Command::CloseValve(ValveHandle::NP1))
                    .then(Command::CloseValve(ValveHandle::NP2))
                    .then(Command::CloseValve(ValveHandle::NP4))
                    .then(Command::CloseValve(ValveHandle::IP1))
                    .then(Command::CloseValve(ValveHandle::IP2));

                self.field_reciever.run_sequence(seq).unwrap();
                self.mode = StandMode::Safing;
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

                egui::TopBottomPanel::bottom("Valve Control Panel").show_inside(left, |ui| {
                    for valve in self.mode.manual_control_valves() {
                        ui.horizontal(|ui| {
                            ui.columns_const(|[left, right]| {
                                left.centered_and_justified(|ui| {
                                    if ui.button(format!("Open {valve}")).clicked() {
                                        self.field_reciever
                                            .send_command(serial::ValveCommand::Open(valve))
                                            .expect("Expected to be able to send command");
                                    }
                                });

                                right.centered_and_justified(|ui| {
                                    if ui.button(format!("Close {valve}")).clicked() {
                                        self.field_reciever
                                            .send_command(serial::ValveCommand::Close(valve))
                                            .expect("Expected to be able to send command");
                                    }
                                });
                            })
                        });
                    }
                });

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

                self.recieve_fields();

                right.vertical(|ui| {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                        ui.label(self.make_fields_table());
                    });
                });

                egui::TopBottomPanel::bottom("Controls Panel").show_inside(right, |ui| {
                    match self.mode {
                        StandMode::Safing => {
                            ui.horizontal_wrapped(|ui| {
                                ui.centered_and_justified(|ui| {
                                    if ui.button("Depressurize System").clicked() {
                                        let seq = CommandSequence::new()
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

                                        self.field_reciever.run_sequence_par(seq);
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
                                    let seq_start = CommandSequence::new()
                                        .then(Command::Ignite)
                                        .then(Command::Wait(wait_time));

                                    let seq = if self.valve_np1_ip1_offset >= 0f32 {
                                        seq_start
                                            .then(Command::OpenValve(ValveHandle::NP1))
                                            .then(Command::Wait(Duration::from_secs_f32(self.valve_np1_ip1_offset)))
                                            .then(Command::OpenValve(ValveHandle::IP1))
                                        
                                    } else {
                                        seq_start
                                            .then(Command::OpenValve(ValveHandle::IP1))
                                            .then(Command::Wait(Duration::from_secs_f32(self.valve_np1_ip1_offset.abs())))
                                            .then(Command::OpenValve(ValveHandle::NP1))
                                    }
                                    .then(Command::Wait(self.fire_time))
                                    .then(Command::Wait(Duration::from_secs(3)))
                                    .then(Command::CloseValve(ValveHandle::NP1))
                                    .then(Command::CloseValve(ValveHandle::IP1))
                                    .then(Command::CloseValve(ValveHandle::NP2))
                                    .then(Command::CloseValve(ValveHandle::IP2))
                                    .then(Command::OpenValve(ValveHandle::NP3))
                                    .then(Command::OpenValve(ValveHandle::IP3))
                                    .then(Command::Done);

                                    self.field_reciever.run_sequence_par(seq);
                                }
                            });
                        }

                        _ => (),
                    };

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

        ctx.request_repaint();
    }
}

/// The different modes that the NILE stand software can take on.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
enum StandMode {
    /// Complete manual control of valves.
    CheckOut,

    /// Limits control to control of valves [`serial::NILE_VALVE_NP3`] and
    /// [`serial::NILE_VALVE_NP4`].
    ///
    /// [`serial::NILE_VALVE_NP3`]: serial::NILE_VALVE_NP3
    /// [`serial::NILE_VALVE_NP4`]: serial::NILE_VALVE_NP4
    OxygenFilling,

    /// Manual control over valves [`serial::NILE_VALVE_NP2`], [`serial::NILE_VALVE_IP2`],
    /// [`serial::NILE_VALVE_NP3`], and [`serial::NILE_VALVE_IP3`]. Ability to begin sequence which
    /// ingnites the ignitor, then opens [`serial::NILE_VALVE_NP1`] and [`serial::NILE_VALVE_IP1`]
    /// simultaniously. Operators can enter a firing time which holds [`serial::NILE_VALVE_NP1`] and
    /// [`serial::NILE_VALVE_IP1`] open for that time plus three seconds to clear excess propellant.
    /// After this time plus three seconds [`serial::NILE_VALVE_NP1`], [`serial::NILE_VALVE_IP1`],
    /// [`serial::NILE_VALVE_NP2`], and [`serial::NILE_VALVE_IP2`] will all close while
    /// [`serial::NILE_VALVE_NP3`] and [`serial::NILE_VALVE_IP3`] open to vent excess nitrogen
    /// - "Fire".
    ///
    /// NOTE: Maybe have entry for timing delays between NP1 and IP1, though this is probably best
    /// done on the stand side.
    ///
    /// [`serial::NILE_VALVE_NP1`]: serial::NILE_VALVE_NP1
    /// [`serial::NILE_VALVE_IP1`]: serial::NILE_VALVE_IP1
    /// [`serial::NILE_VALVE_NP2`]: serial::NILE_VALVE_NP2
    /// [`serial::NILE_VALVE_IP2`]: serial::NILE_VALVE_IP2
    /// [`serial::NILE_VALVE_NP3`]: serial::NILE_VALVE_NP3
    /// [`serial::NILE_VALVE_IP3`]: serial::NILE_VALVE_IP3
    PressurizationAndFiring,

    /// Sets [`serial::NILE_VALVE_NP3`] and [`serial::NILE_VALVE_IP3`] open and closes all others.
    /// Also allows for operators to use a "Depress System" button which will open
    /// [`serial::NILE_VALVE_NP4`] for five seconds then closes it, followed by opening
    /// [`serial::NILE_VALVE_IP2`] for five seconds then closing it, followed by finally opening
    /// [`serial::NILE_VALVE_NP2`] for five seconds and then closing it, there should be one second
    /// delay between all valve openings - "Depressurize System".
    ///
    /// [`serial::NILE_VALVE_NP2`]: serial::NILE_VALVE_NP2
    /// [`serial::NILE_VALVE_IP2`]: serial::NILE_VALVE_IP2
    /// [`serial::NILE_VALVE_NP3`]: serial::NILE_VALVE_NP3
    /// [`serial::NILE_VALVE_IP3`]: serial::NILE_VALVE_IP3
    /// [`serial::NILE_VALVE_NP4`]: serial::NILE_VALVE_NP4
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

    /// Returns a [`Vec`] of the valves which may be manually controlled in the given [`StandMode`].
    ///
    /// [`Vec`]: Vec
    /// [`StandMode`]: StandMode
    fn manual_control_valves(self) -> Vec<&'static str> {
        match self {
            Self::CheckOut => vec![
                serial::NILE_VALVE_NP1,
                serial::NILE_VALVE_NP2,
                serial::NILE_VALVE_NP3,
                serial::NILE_VALVE_NP4,
                serial::NILE_VALVE_IP1,
                serial::NILE_VALVE_IP2,
                serial::NILE_VALVE_IP3,
            ],

            Self::OxygenFilling => vec![serial::NILE_VALVE_NP3, serial::NILE_VALVE_NP4],

            Self::PressurizationAndFiring => vec![
                serial::NILE_VALVE_NP2,
                serial::NILE_VALVE_NP3,
                serial::NILE_VALVE_IP2,
                serial::NILE_VALVE_IP3,
            ],

            Self::Safing => vec![],
        }
    }
}

impl Display for StandMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StandMode::CheckOut => write!(f, "Check Out Mode"),
            StandMode::OxygenFilling => write!(f, "Ox Filling Mode"),
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
