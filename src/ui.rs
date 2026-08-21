use crate::{autostart, config, stats};
use eframe::egui::{self, Color32, Stroke};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tray::{Tray, TrayEvent};

const AXIS_LABELS: [&str; 3] = ["raw X", "raw Y", "raw Z"];
/// Solid-amber tray icon, 16x16.
const TRAY_ICON_RGBA: [u8; 4] = [211, 123, 48, 255];
const TRAY_ICON_SIZE: u32 = 16;

const BG: Color32 = Color32::from_rgb(20, 20, 18);
const SURFACE: Color32 = Color32::from_rgb(31, 31, 28);
const SURFACE_DEEP: Color32 = Color32::from_rgb(24, 24, 22);
const BORDER: Color32 = Color32::from_rgb(75, 70, 61);
const BORDER_STRONG: Color32 = Color32::from_rgb(208, 120, 47);
const TEXT: Color32 = Color32::from_rgb(241, 234, 218);
const MUTED: Color32 = Color32::from_rgb(187, 176, 155);
const DIM: Color32 = Color32::from_rgb(142, 133, 118);
const AMBER: Color32 = Color32::from_rgb(244, 177, 82);
const AMBER_BRIGHT: Color32 = Color32::from_rgb(255, 198, 104);
const ORANGE: Color32 = Color32::from_rgb(216, 116, 43);
const SUCCESS: Color32 = Color32::from_rgb(104, 190, 124);

pub struct App {
    shutdown: Arc<AtomicBool>,
    ui_wants_device: Arc<AtomicBool>,
    cfg: config::Config,
    port: String,
    note: String,
    tray: Option<Tray>,
    visible: bool,
    confirm_defaults: bool,
    display_slot: u8,
    show_motion_settings: bool,
    show_system_settings: bool,
}

impl App {
    fn new(
        shutdown: Arc<AtomicBool>,
        ui_wants_device: Arc<AtomicBool>,
        start_minimized: bool,
        wake: tray::Wake,
    ) -> Self {
        let cfg = config::snapshot();
        let visible = !(cfg.start_minimized || start_minimized);
        ui_wants_device.store(visible, Ordering::Relaxed);
        let tray = Tray::spawn(wake)
            .map_err(|e| eprintln!("ui: system tray unavailable: {e}"))
            .ok();
        Self {
            shutdown,
            ui_wants_device,
            port: cfg.port.to_string(),
            cfg,
            note: String::new(),
            tray,
            visible,
            confirm_defaults: false,
            display_slot: 0,
            show_motion_settings: false,
            show_system_settings: false,
        }
    }

    fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.visible = visible;
        self.ui_wants_device.store(visible, Ordering::Relaxed);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn save(&mut self, note: impl Into<String>) {
        match config::update_and_save(self.cfg.clone()) {
            Ok(()) => self.note = note.into(),
            Err(e) => self.note = format!("save failed: {e}"),
        }
    }

    fn axis_row(ui: &mut egui::Ui, id: &str, label: &str, axis: &mut config::Axis) -> bool {
        let mut changed = false;
        ui.label(label);
        egui::ComboBox::from_id_salt(id)
            .selected_text(AXIS_LABELS[(axis.source as usize).min(2)])
            .show_ui(ui, |ui| {
                for (source, text) in AXIS_LABELS.iter().enumerate() {
                    changed |= ui
                        .selectable_value(&mut axis.source, source as u8, *text)
                        .changed();
                }
            });
        changed |= ui.checkbox(&mut axis.invert, "invert").changed();
        ui.end_row();
        changed
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.heading("Gyro axis mapping");
                egui::Grid::new("gyro-map").show(ui, |ui| {
                    changed |= Self::axis_row(ui, "gx", "DSU X (pitch)", &mut self.cfg.gyro.x);
                    changed |= Self::axis_row(ui, "gy", "DSU Y (yaw)", &mut self.cfg.gyro.y);
                    changed |= Self::axis_row(ui, "gz", "DSU Z (roll)", &mut self.cfg.gyro.z);
                });
                changed |= ui
                    .add(
                        egui::Slider::new(
                            &mut self.cfg.gyro_sensitivity,
                            config::GYRO_SENSITIVITY_MIN..=config::GYRO_SENSITIVITY_MAX,
                        )
                        .text("Sensitivity")
                        .fixed_decimals(2),
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.cfg.auto_calibrate, "Auto-calibrate bias")
                    .changed();
            });
            columns[1].group(|ui| {
                ui.horizontal(|ui| {
                    ui.heading("Accel axis mapping");
                    if ui.button("Copy from gyro").clicked() {
                        self.cfg.accel = self.cfg.gyro;
                        changed = true;
                    }
                });
                egui::Grid::new("accel-map").show(ui, |ui| {
                    changed |= Self::axis_row(ui, "ax", "DSU X", &mut self.cfg.accel.x);
                    changed |= Self::axis_row(ui, "ay", "DSU Y", &mut self.cfg.accel.y);
                    changed |= Self::axis_row(ui, "az", "DSU Z", &mut self.cfg.accel.z);
                });
            });
        });
        if changed {
            self.save("settings saved.");
        }
    }

    fn system_settings(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        ui.group(|ui| {
            ui.heading("Server endpoint");
            ui.horizontal_wrapped(|ui| {
                ui.label("UDP port (next launch):");
                if ui
                    .add(egui::TextEdit::singleline(&mut self.port).desired_width(75.0))
                    .lost_focus()
                {
                    match self.port.parse::<u16>() {
                        Ok(port) => {
                            self.cfg.port = port;
                            save = true;
                        }
                        Err(_) => self.note = "port must be a number from 0 to 65535.".into(),
                    }
                }
            });
        });
        if save {
            self.save("system settings saved.");
        }
    }

    fn app_preferences(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        ui.label(egui::RichText::new("APP").size(11.0).strong().color(AMBER));
        save |= ui
            .checkbox(&mut self.cfg.expose_to_network, "Open to network")
            .changed();
        save |= ui
            .checkbox(&mut self.cfg.close_to_tray, "Close to tray")
            .changed();
        let mut enabled = autostart::is_enabled();
        if ui.checkbox(&mut enabled, "Start with Windows").changed() {
            let result = if enabled {
                autostart::enable()
            } else {
                autostart::disable()
            };
            self.note = match result {
                Ok(()) => format!(
                    "start with Windows {}.",
                    if enabled { "enabled" } else { "disabled" }
                ),
                Err(e) => format!("start with Windows change failed: {e}"),
            };
        }
        if save {
            self.save("app preferences saved.");
        }
    }

    fn panel_frame(active: bool) -> egui::Frame {
        egui::Frame::NONE
            .fill(SURFACE)
            .stroke(Stroke::new(
                if active { 1.5_f32 } else { 1.0_f32 },
                if active { BORDER_STRONG } else { BORDER },
            ))
            .corner_radius(4)
            .inner_margin(egui::Margin::same(12))
    }

    fn dashboard_header(&mut self, ui: &mut egui::Ui, s: &stats::ServerStats) {
        egui::Frame::NONE
            .fill(SURFACE_DEEP)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("SC2DSU")
                                .size(22.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.label(
                            egui::RichText::new("Steam Controller motion server")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Server & app").clicked() {
                            self.show_system_settings = !self.show_system_settings;
                        }
                        let host = config::bind_host(self.cfg.expose_to_network);
                        let endpoint = if s.server.bound_port == 0 {
                            "Starting server…".into()
                        } else {
                            format!("{host}:{}", s.server.bound_port)
                        };
                        ui.label(
                            egui::RichText::new(endpoint)
                                .monospace()
                                .size(12.0)
                                .color(AMBER),
                        );
                        ui.separator();
                        let throughput = ui.label(
                            egui::RichText::new(format!(
                                "{} controller{} · {} client{} · {:.0} Hz IMU · {:.0} pkt/s · {:.0} req/s",
                                s.server.controllers,
                                if s.server.controllers == 1 { "" } else { "s" },
                                s.server.subscribers,
                                if s.server.subscribers == 1 { "" } else { "s" },
                                s.server.samples_per_sec,
                                s.server.packets_per_sec,
                                s.server.requests_per_sec,
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                        throughput.on_hover_text(format!(
                            "Server ID 0x{:08X} · input {}",
                            s.server.server_id,
                            if s.server.device_active { "awake" } else { "idle" }
                        ));
                        ui.separator();
                        ui.colored_label(
                            if s.server.bound_port == 0 {
                                AMBER
                            } else {
                                SUCCESS
                            },
                            if s.server.bound_port == 0 {
                                "STARTING"
                            } else {
                                "DSU LIVE"
                            },
                        );
                    });
                });
            });
    }

    fn slot_card(&mut self, ui: &mut egui::Ui, slot: u8, status: stats::SlotSection) {
        let selected = self.display_slot == slot;
        let response = Self::panel_frame(selected).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("SLOT {:02}", slot + 1))
                            .size(11.0)
                            .strong()
                            .color(if selected { AMBER } else { DIM }),
                    );
                    ui.label(
                        egui::RichText::new(if status.connected { "ON" } else { "--" })
                            .size(20.0)
                            .color(if status.connected { SUCCESS } else { DIM }),
                    );
                });
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    if let Some(controller) = status.controller {
                        ui.label(
                            egui::RichText::new(controller.name)
                                .size(15.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {:.0} Hz",
                                controller.transport, status.samples_per_sec
                            ))
                            .size(12.0)
                            .color(MUTED),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("No controller connected")
                                .size(15.0)
                                .color(MUTED),
                        );
                        ui.label(
                            egui::RichText::new("Waiting for a Steam Controller")
                                .size(12.0)
                                .color(DIM),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(controller) = status.controller {
                        let current = self.cfg.slot_for_controller(controller.id);
                        let mut requested = current;
                        let current_label = current
                            .map(|slot| format!("DSU Slot {}", slot + 1))
                            .unwrap_or_else(|| format!("Auto · Slot {}", slot + 1));
                        egui::ComboBox::from_id_salt(("slot-routing", controller.id))
                            .selected_text(current_label)
                            .width(128.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut requested, None, "Automatic");
                                for target in 0..stats::MAX_SLOTS as u8 {
                                    ui.selectable_value(
                                        &mut requested,
                                        Some(target),
                                        format!("DSU Slot {}", target + 1),
                                    );
                                }
                            });
                        if requested != current {
                            match requested {
                                Some(target) => {
                                    self.cfg.assign_controller_to_slot(controller.id, target)
                                }
                                None => self.cfg.clear_controller_assignment(controller.id),
                            }
                            self.save("slot routing saved.");
                        }
                    } else {
                        ui.label(
                            egui::RichText::new(format!("DSU Slot {}", slot + 1))
                                .size(12.0)
                                .color(DIM),
                        );
                    }
                });
            });
        });
        if response.response.clicked() {
            self.display_slot = slot;
        }
    }

    fn controllers_panel(&mut self, ui: &mut egui::Ui, s: &stats::ServerStats) {
        Self::panel_frame(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Controllers & slots")
                            .size(18.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Choose where each controller appears in your emulator.",
                        )
                        .size(12.0)
                        .color(MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("4 DSU PORTS")
                            .size(11.0)
                            .strong()
                            .color(AMBER),
                    );
                });
            });
            ui.add_space(8.0);
            for (slot, status) in s.slots.iter().copied().enumerate() {
                self.slot_card(ui, slot as u8, status);
                if slot + 1 < stats::MAX_SLOTS {
                    ui.add_space(6.0);
                }
            }
        });
    }

    fn metric_card(ui: &mut egui::Ui, name: &str, value: f32) {
        Self::panel_frame(false).show(ui, |ui| {
            ui.label(egui::RichText::new(name).size(11.0).color(MUTED));
            ui.label(
                egui::RichText::new(format!("{value:+.1}°/s"))
                    .size(20.0)
                    .strong()
                    .color(AMBER_BRIGHT),
            );
        });
    }

    fn motion_panel(&mut self, ui: &mut egui::Ui, s: &stats::ServerStats) {
        let selected = s.slots[usize::from(self.display_slot)];
        Self::panel_frame(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Live motion").size(18.0).strong());
                    ui.label(
                        egui::RichText::new("Choose the controller feeding this display.")
                            .size(12.0)
                            .color(MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let name = selected
                        .controller
                        .map(|controller| controller.name)
                        .unwrap_or("No controller");
                    egui::ComboBox::from_id_salt("motion-display-slot")
                        .selected_text(format!("Viewing: Slot {} · {name}", self.display_slot + 1))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for slot in 0..stats::MAX_SLOTS as u8 {
                                let name = s.slots[usize::from(slot)]
                                    .controller
                                    .map(|controller| controller.name)
                                    .unwrap_or("No controller");
                                ui.selectable_value(
                                    &mut self.display_slot,
                                    slot,
                                    format!("Slot {} · {name}", slot + 1),
                                );
                            }
                        });
                });
            });
            ui.add_space(12.0);
            ui.columns(3, |columns| {
                Self::metric_card(&mut columns[0], "PITCH", selected.motion.last_gyro_dps[0]);
                Self::metric_card(&mut columns[1], "YAW", selected.motion.last_gyro_dps[1]);
                Self::metric_card(&mut columns[2], "ROLL", selected.motion.last_gyro_dps[2]);
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let calibration = if !s.calibration.active {
                    "Calibration off".to_owned()
                } else if s.calibration.steady {
                    format!(
                        "{:.0} Hz · Calibrated {:.0}%",
                        selected.samples_per_sec,
                        s.calibration.confidence * 100.0
                    )
                } else {
                    "Calibrating…".to_owned()
                };
                ui.colored_label(SUCCESS, calibration);
                ui.label(
                    egui::RichText::new(format!(
                        "accel {:+.2} {:+.2} {:+.2} g",
                        selected.motion.last_accel_g[0],
                        selected.motion.last_accel_g[1],
                        selected.motion.last_accel_g[2]
                    ))
                    .monospace()
                    .size(11.0)
                    .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [154.0, 30.0],
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "Recalibrate Slot {}",
                                    self.display_slot + 1
                                ))
                                .color(Color32::WHITE),
                            )
                            .fill(ORANGE),
                        )
                        .clicked()
                    {
                        stats::RECENTER_SLOT_REQUEST
                            .store(self.display_slot + 1, Ordering::Relaxed);
                        stats::RECALIBRATE_SLOT_REQUEST
                            .store(self.display_slot + 1, Ordering::Relaxed);
                        self.note = format!("recalibrating slot {} gyro.", self.display_slot + 1);
                    }
                });
            });
        });
    }

    fn motion_settings_panel(&mut self, ui: &mut egui::Ui) {
        Self::panel_frame(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Motion settings").size(16.0).strong());
                    ui.label(
                        egui::RichText::new(
                            "Sensitivity, axis mapping and calibration · applies to all slots",
                        )
                        .size(12.0)
                        .color(MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(if self.show_motion_settings {
                            "Hide advanced"
                        } else {
                            "Advanced"
                        })
                        .clicked()
                    {
                        self.show_motion_settings = !self.show_motion_settings;
                    }
                });
            });
            if self.show_motion_settings {
                ui.add_space(10.0);
                self.settings(ui);
            }
        });
    }

    fn install_visuals(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = SURFACE_DEEP;
        visuals.extreme_bg_color = Color32::from_rgb(15, 15, 14);
        visuals.faint_bg_color = SURFACE;
        visuals.selection.bg_fill = Color32::from_rgb(120, 70, 31);
        visuals.selection.stroke = Stroke::new(1.0_f32, AMBER_BRIGHT);
        visuals.widgets.noninteractive.bg_fill = SURFACE;
        visuals.widgets.noninteractive.weak_bg_fill = SURFACE;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(3);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(44, 42, 37);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(44, 42, 37);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(78, 58, 38);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(78, 58, 38);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, AMBER);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        visuals.widgets.active.bg_fill = Color32::from_rgb(173, 91, 34);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(173, 91, 34);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, AMBER_BRIGHT);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
        ctx.set_visuals(visuals);
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else {
            return;
        };
        let events: Vec<TrayEvent> = std::iter::from_fn(|| tray.try_recv()).collect();
        for event in events {
            match event {
                TrayEvent::Toggle => self.set_visible(ctx, !self.visible),
                TrayEvent::Show => self.set_visible(ctx, true),
                TrayEvent::Quit => self.quit(ctx),
            }
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.shutdown.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        Self::install_visuals(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.cfg.close_to_tray && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.set_visible(ctx, false);
            } else {
                self.shutdown.store(true, Ordering::Relaxed);
            }
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(BG)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                let stats = stats::snapshot();
                self.dashboard_header(ui, &stats);
                ui.add_space(10.0);

                if self.show_system_settings {
                    self.system_settings(ui);
                    ui.add_space(10.0);
                }

                ui.columns(2, |columns| {
                    self.controllers_panel(&mut columns[0], &stats);
                    self.motion_panel(&mut columns[1], &stats);
                });

                ui.add_space(10.0);
                self.motion_settings_panel(ui);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let host = config::bind_host(self.cfg.expose_to_network);
                    ui.colored_label(SUCCESS, format!("Listening on {host}:{}", self.cfg.port));
                    ui.separator();
                    self.app_preferences(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Quit").clicked() {
                            self.quit(ctx);
                        }
                        if ui
                            .button(if self.confirm_defaults {
                                "Confirm restore"
                            } else {
                                "Restore defaults"
                            })
                            .clicked()
                        {
                            if self.confirm_defaults {
                                self.cfg = config::Config::DEFAULT;
                                self.port = self.cfg.port.to_string();
                                self.save("restored defaults.");
                            }
                            self.confirm_defaults = !self.confirm_defaults;
                        }
                        if ui.button("Hide to tray").clicked() {
                            self.set_visible(ctx, false);
                        }
                    });
                    if !self.note.is_empty() {
                        ui.label(egui::RichText::new(&self.note).size(11.0).color(MUTED));
                    }
                });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

/// System tray integration.
///
/// Both platforms expose the same [`Tray`] handle and [`TrayEvent`] stream. Linux speaks
/// the StatusNotifierItem D-Bus protocol directly through `ksni`; using `tray-icon` there
/// would link GTK3, libxdo and libayatana-appindicator into the binary and cost us a
/// portable release build for no behavioural gain (libayatana-appindicator is itself just
/// a StatusNotifierItem implementation).
mod tray {
    /// What the user asked for by clicking the tray icon or one of its menu entries.
    pub enum TrayEvent {
        /// Icon clicked: show the window if hidden, hide it if visible.
        Toggle,
        Show,
        Quit,
    }

    #[cfg(target_os = "linux")]
    mod imp {
        use super::TrayEvent;
        use std::sync::mpsc::{Receiver, Sender, channel};

        struct SniTray {
            tx: Sender<TrayEvent>,
        }

        impl ksni::Tray for SniTray {
            fn id(&self) -> String {
                "sc2dsu".into()
            }

            fn title(&self) -> String {
                "SC2DSU".into()
            }

            fn icon_pixmap(&self) -> Vec<ksni::Icon> {
                let [r, g, b, a] = super::super::TRAY_ICON_RGBA;
                let size = super::super::TRAY_ICON_SIZE as i32;
                vec![ksni::Icon {
                    width: size,
                    height: size,
                    // ARGB32, network byte order.
                    data: [a, r, g, b].repeat((size * size) as usize),
                }]
            }

            fn activate(&mut self, _x: i32, _y: i32) {
                let _ = self.tx.send(TrayEvent::Toggle);
            }

            fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
                use ksni::menu::StandardItem;
                vec![
                    StandardItem {
                        label: "Show settings".into(),
                        activate: Box::new(|this: &mut Self| {
                            let _ = this.tx.send(TrayEvent::Show);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    StandardItem {
                        label: "Quit".into(),
                        activate: Box::new(|this: &mut Self| {
                            let _ = this.tx.send(TrayEvent::Quit);
                        }),
                        ..Default::default()
                    }
                    .into(),
                ]
            }
        }

        pub struct Tray {
            rx: Receiver<TrayEvent>,
            // Dropping the handle removes the icon, so it is held for the app's lifetime.
            _handle: ksni::blocking::Handle<SniTray>,
        }

        impl Tray {
            pub fn spawn(_wake: super::Wake) -> Result<Self, String> {
                use ksni::blocking::TrayMethods;
                let (tx, rx) = channel();
                let handle = SniTray { tx }.spawn().map_err(|e| e.to_string())?;
                Ok(Self {
                    rx,
                    _handle: handle,
                })
            }

            pub fn try_recv(&self) -> Option<TrayEvent> {
                self.rx.try_recv().ok()
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    mod imp {
        use super::{TrayEvent, Wake};
        use std::sync::mpsc::{Receiver, Sender, channel};
        use tray_icon::menu::{Menu, MenuEvent, MenuItem};
        use tray_icon::{
            Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
        };

        pub struct Tray {
            _icon: TrayIcon,
            rx: Receiver<TrayEvent>,
        }

        impl Tray {
            /// Creates the tray icon on the calling thread, which must be the one running
            /// the winit event loop: `tray-icon` delivers its events through that thread's
            /// Win32 message pump.
            ///
            /// Events are forwarded through our own channel rather than polled from the
            /// crate's global receivers, so each one can also `wake` the UI. Without that,
            /// a window hidden to the tray never repaints (Windows drops redraw requests
            /// for invisible windows), `update` never runs, and the menu appears dead.
            pub fn spawn(wake: Wake) -> Result<Self, String> {
                let menu = Menu::new();
                let show = MenuItem::new("Show settings", true, None);
                let quit = MenuItem::new("Quit", true, None);
                menu.append(&show).map_err(|e| e.to_string())?;
                menu.append(&quit).map_err(|e| e.to_string())?;
                let show_id = show.id().clone();
                let quit_id = quit.id().clone();
                let (tx, rx) = channel();

                let forward = {
                    let tx: Sender<TrayEvent> = tx.clone();
                    move |event: TrayEvent| {
                        let _ = tx.send(event);
                        wake.wake();
                    }
                };
                {
                    let forward = forward.clone();
                    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                        if event.id == show_id {
                            forward(TrayEvent::Show);
                        } else if event.id == quit_id {
                            forward(TrayEvent::Quit);
                        }
                    }));
                }
                TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        forward(TrayEvent::Toggle);
                    }
                }));

                let size = super::super::TRAY_ICON_SIZE;
                let icon = Icon::from_rgba(
                    super::super::TRAY_ICON_RGBA.repeat((size * size) as usize),
                    size,
                    size,
                )
                .map_err(|e| e.to_string())?;
                let icon = TrayIconBuilder::new()
                    .with_tooltip("SC2DSU")
                    .with_icon(icon)
                    .with_menu(Box::new(menu))
                    .build()
                    .map_err(|e| e.to_string())?;
                Ok(Self { _icon: icon, rx })
            }

            pub fn try_recv(&self) -> Option<TrayEvent> {
                self.rx.try_recv().ok()
            }
        }
    }

    /// Forces the egui window to run a frame so queued tray events get handled.
    ///
    /// `egui::Context::request_repaint` is not enough: on Windows, winit implements it with
    /// `RedrawWindow`, which the OS ignores for hidden windows, so an app hidden to the tray
    /// would never see the event. Showing the window via Win32 makes the OS emit `WM_PAINT`,
    /// eframe runs `update`, and `handle_tray` takes it from there.
    #[derive(Clone)]
    pub struct Wake {
        #[cfg(windows)]
        hwnd: Option<isize>,
    }

    impl Wake {
        pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
            #[cfg(windows)]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Win32(h)) => Some(h.hwnd.get()),
                    _ => None,
                };
                Self { hwnd }
            }
            #[cfg(not(windows))]
            {
                let _ = cc;
                Self {}
            }
        }

        #[cfg_attr(not(windows), allow(dead_code))]
        pub fn wake(&self) {
            #[cfg(windows)]
            if let Some(hwnd) = self.hwnd {
                use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNA, ShowWindow};
                // SAFETY: plain Win32 call on a window handle eframe owns for the app's
                // whole lifetime; the tray is dropped together with the `App`.
                unsafe {
                    ShowWindow(hwnd as _, SW_SHOWNA);
                }
            }
        }
    }

    pub use imp::Tray;
}

pub fn run(
    shutdown: Arc<AtomicBool>,
    ui_wants_device: Arc<AtomicBool>,
    start_minimized: bool,
) -> Result<(), String> {
    let cfg = config::snapshot();
    let visible = !(cfg.start_minimized || start_minimized);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 580.0])
            .with_min_inner_size([850.0, 510.0])
            .with_visible(visible),
        ..Default::default()
    };
    eframe::run_native(
        "SC2DSU — Steam Controller gyro to Cemuhook",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                shutdown,
                ui_wants_device,
                start_minimized,
                tray::Wake::new(cc),
            )))
        }),
    )
    .map_err(|e| e.to_string())
}
