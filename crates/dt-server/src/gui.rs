use crate::config::ServerConfig;
use crate::icons::{icon_image, Icon};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{runtime::Runtime, task::JoinHandle};

const GREEN: Color32 = Color32::from_rgb(25, 128, 74);
const INK: Color32 = Color32::from_rgb(34, 42, 38);
const MUTED: Color32 = Color32::from_rgb(96, 111, 105);
const BG: Color32 = Color32::from_rgb(243, 246, 245);
const BORDER: Color32 = Color32::from_rgb(220, 227, 223);

#[derive(Clone, Default)]
pub struct ServerStats {
    pub captured: u64,
    pub encoded: u64,
    pub sent: u64,
    pub dropped: u64,
    pub keyframes: u64,
    pub fragments: u64,
    pub errors: u64,
    pub connected_client: Option<String>,
    pub resolution: String,
    pub status: String,
    pub mapping: String,
    pub stylus_buttons: [bool; 2],
    pub logs: VecDeque<String>,
}
impl ServerStats {
    pub fn log(&mut self, msg: String) {
        self.logs.push_back(format!(
            "{}   {}",
            chrono::Local::now().format("%H:%M:%S"),
            msg
        ));
        if self.logs.len() > 100 {
            self.logs.pop_front();
        }
    }
}

pub struct DtServerApp {
    stats: Arc<Mutex<ServerStats>>,
    running_flag: Arc<AtomicBool>,
    port_input: String,
    bitrate_input: String,
    keyframe_input: String,
    fps_input: String,
    output_input: String,
    config: ServerConfig,
    address: String,
    started: Option<String>,
    show_help: bool,
    rt: Runtime,
    server_task: Option<JoinHandle<()>>,
}
impl DtServerApp {
    pub fn new(
        stats: Arc<Mutex<ServerStats>>,
        running_flag: Arc<AtomicBool>,
        config: ServerConfig,
    ) -> Self {
        // Connecting a UDP socket only chooses a local route; it sends no packets.
        let address = std::net::UdpSocket::bind("0.0.0.0:0")
            .ok()
            .and_then(|s| {
                s.connect("192.0.2.1:9").ok()?;
                Some(s.local_addr().ok()?.ip().to_string())
            })
            .unwrap_or_else(|| "<server IP>".into());
        Self {
            stats,
            running_flag,
            port_input: config.port.to_string(),
            bitrate_input: config.bitrate.to_string(),
            keyframe_input: config.keyframe_interval.to_string(),
            fps_input: config.fps.to_string(),
            output_input: config.output_name.clone().unwrap_or_default(),
            config,
            address,
            started: None,
            show_help: false,
            rt: Runtime::new().expect("Tokio runtime"),
            server_task: None,
        }
    }
    fn start_server(&mut self) {
        let parsed = (
            self.port_input.parse::<u16>(),
            self.fps_input.parse::<u32>(),
            self.bitrate_input.parse::<u32>(),
            self.keyframe_input.parse::<u32>(),
        );
        let (Ok(port), Ok(fps), Ok(bitrate), Ok(keyframe_interval)) = parsed else {
            self.stats.lock().unwrap().status = "Enter valid numeric stream settings".into();
            return;
        };
        if port == 0 || !(1..=240).contains(&fps) || bitrate < 1000 || keyframe_interval == 0 {
            self.stats.lock().unwrap().status =
                "Use port 1–65535, FPS 1–240, bitrate ≥1000 and keyframe interval ≥1".into();
            return;
        }
        let config = ServerConfig {
            port,
            fps,
            bitrate,
            keyframe_interval,
            output_name: if self.output_input.trim().is_empty() {
                None
            } else {
                Some(self.output_input.trim().into())
            },
            tablet: self.config.tablet.clone(),
        };
        self.config = config.clone();
        let stats = self.stats.clone();
        let running = self.running_flag.clone();
        {
            let mut s = stats.lock().unwrap();
            let logs = std::mem::take(&mut s.logs);
            *s = ServerStats::default();
            s.logs = logs;
            s.status = "Selecting screen…".into();
            s.log("Starting server".into());
        }
        running.store(true, Ordering::SeqCst);
        self.started = Some(chrono::Local::now().format("%H:%M").to_string());
        self.server_task = Some(self.rt.spawn(async move {
            let result = crate::run_server_async(config, stats.clone(), running.clone()).await;
            running.store(false, Ordering::SeqCst);
            let mut s = stats.lock().unwrap();
            s.connected_client = None;
            s.stylus_buttons = [false; 2];
            match result {
                Ok(()) => {
                    s.status = "Stopped".into();
                    s.log("Server stopped".into());
                }
                Err(e) => {
                    s.status = format!("Error: {e}");
                    let message = s.status.clone();
                    s.log(message);
                }
            }
        }));
    }
}
fn device_icon(ui: &mut egui::Ui, tablet: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(54.0, 42.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        10.0,
        Color32::WHITE,
        egui::Stroke::new(1.0_f32, BORDER),
        egui::StrokeKind::Inside,
    );
    ui.put(
        rect,
        icon_image(
            ui.ctx(),
            if tablet { Icon::Tablet } else { Icon::Monitor },
            32.0,
        ),
    );
}
fn banner_slot(ui: &mut egui::Ui, width: f32, contents: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 88.0), egui::Sense::hover());
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        contents,
    );
}
fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .corner_radius(12)
        .inner_margin(20)
}
fn field(ui: &mut egui::Ui, title: &str, value: &mut String, unit: &str) {
    ui.label(title);
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.add_sized(
            [ui.available_width() - 48.0, 36.0],
            egui::TextEdit::singleline(value).margin(egui::vec2(12.0, 9.0)),
        );
        ui.label(RichText::new(unit).color(MUTED));
    });
    ui.add_space(16.0);
}
fn metric(ui: &mut egui::Ui, title: &str, value: String, unit: &str) {
    card().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).color(MUTED));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(value).size(34.0).strong());
            ui.label(RichText::new(unit).color(MUTED));
        });
    });
}
impl eframe::App for DtServerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = BG;
        visuals.window_fill = BG;
        visuals.override_text_color = Some(INK);
        visuals.widgets.inactive.bg_fill = Color32::WHITE;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
        visuals.selection.bg_fill = Color32::from_rgb(203, 230, 214);
        ctx.set_visuals(visuals);
        if self.server_task.as_ref().is_some_and(|t| t.is_finished()) {
            self.server_task = None;
            self.running_flag.store(false, Ordering::SeqCst);
        }
        let running = self.running_flag.load(Ordering::SeqCst);
        let busy = self.server_task.is_some();
        let stats = self.stats.lock().unwrap().clone();
        let streaming = running && stats.connected_client.is_some() && stats.status == "Running";
        egui::TopBottomPanel::top("stream_banner")
            .exact_height(136.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(230, 240, 234))
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let flow_width = (ui.available_width() - 774.0).max(70.0);
                    banner_slot(ui, 205.0, |ui| {
                        ui.set_width(205.0);
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                            ui.painter()
                                .circle_filled(r.center(), 9.0, GREEN.gamma_multiply(0.18));
                            ui.painter().circle_filled(
                                r.center(),
                                5.0,
                                if running { GREEN } else { MUTED },
                            );
                            ui.heading(if streaming {
                                "Streaming"
                            } else if running {
                                "Starting / waiting"
                            } else {
                                "Stopped"
                            });
                        });
                        ui.label(
                            RichText::new(if running {
                                format!("Started {}", self.started.as_deref().unwrap_or(""))
                            } else {
                                stats.status.clone()
                            })
                            .small()
                            .color(MUTED),
                        );
                    });
                    banner_slot(ui, 90.0, |ui| {
                        ui.set_width(90.0);
                        device_icon(ui, false);
                        ui.label("This screen");
                        ui.label(
                            RichText::new(if stats.resolution.is_empty() {
                                "Select on start"
                            } else {
                                &stats.resolution
                            })
                            .small()
                            .color(MUTED),
                        );
                    });
                    let arrow_width = flow_width;
                    banner_slot(ui, arrow_width, |ui| {
                        ui.set_width(arrow_width);
                        ui.label(RichText::new("video").small().color(MUTED));
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(arrow_width, 32.0),
                            egui::Sense::hover(),
                        );
                        let p = ui.painter();
                        for row in 0..2 {
                            let y = rect.top() + 8.0 + row as f32 * 16.0;
                            let mut x = rect.left();
                            while x < rect.right() - 10.0 {
                                p.line_segment(
                                    [
                                        egui::pos2(x, y),
                                        egui::pos2((x + 9.0).min(rect.right() - 10.0), y),
                                    ],
                                    egui::Stroke::new(1.5_f32, GREEN),
                                );
                                x += 18.0;
                            }
                            let x = if row == 0 {
                                rect.right() - 5.0
                            } else {
                                rect.left()
                            };
                            let sign = if row == 0 { -1.0 } else { 1.0 };
                            p.line_segment(
                                [egui::pos2(x, y), egui::pos2(x + sign * 5.0, y - 4.0)],
                                egui::Stroke::new(1.5_f32, GREEN),
                            );
                            p.line_segment(
                                [egui::pos2(x, y), egui::pos2(x + sign * 5.0, y + 4.0)],
                                egui::Stroke::new(1.5_f32, GREEN),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("stylus").small().color(MUTED));
                        });
                    });
                    banner_slot(ui, 115.0, |ui| {
                        ui.set_width(115.0);
                        device_icon(ui, true);
                        ui.label("Tablet");
                        ui.label(
                            RichText::new(
                                stats.connected_client.as_deref().unwrap_or("Not connected"),
                            )
                            .small()
                            .color(MUTED),
                        );
                    });
                    banner_slot(ui, 190.0, |ui| {
                        ui.set_width(190.0);
                        ui.label(RichText::new("Tablet connects to").small().color(MUTED));
                        let address = format!("{}:{}", self.address, self.config.port);
                        if ui
                            .add(
                                egui::Button::image_and_text(
                                    icon_image(ui.ctx(), Icon::Copy, 16.0),
                                    RichText::new(&address).monospace(),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Copy server address")
                            .clicked()
                        {
                            ctx.copy_text(address);
                        }
                    });
                    if ui
                        .add_enabled(
                            running || !busy,
                            egui::Button::image_and_text(
                                icon_image(
                                    ui.ctx(),
                                    if running || busy {
                                        Icon::Stop
                                    } else {
                                        Icon::Start
                                    },
                                    18.0,
                                ),
                                if running {
                                    "Stop server"
                                } else if busy {
                                    "Stopping…"
                                } else {
                                    "Start server"
                                },
                            )
                            .min_size(Vec2::new(126.0, 40.0)),
                        )
                        .clicked()
                    {
                        if running {
                            self.running_flag.store(false, Ordering::SeqCst);
                            self.stats.lock().unwrap().log("Stopping server…".into());
                        } else {
                            self.start_server();
                        }
                    }
                });
            });
        egui::SidePanel::left("settings")
            .exact_width(320.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(26))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icon_image(ui.ctx(), Icon::Pen, 28.0));
                    ui.label(RichText::new("Drawing Tablet").strong());
                    ui.label(RichText::new("Server").color(MUTED));
                });
                ui.add_space(32.0);
                ui.label(RichText::new("Stream settings").color(MUTED));
                ui.add_space(16.0);
                ui.add_enabled_ui(!busy, |ui| {
                    field(ui, "Port", &mut self.port_input, "");
                    field(ui, "Bitrate", &mut self.bitrate_input, "kbps");
                    field(ui, "Keyframe interval", &mut self.keyframe_input, "frames");
                    field(ui, "Frame rate", &mut self.fps_input, "fps");
                    field(ui, "Input monitor", &mut self.output_input, "");
                    ui.label(
                        RichText::new(
                            "Leave blank to follow screen selection.\nOverride example: DP-3",
                        )
                        .small()
                        .color(MUTED),
                    );
                });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui
                        .add(
                            egui::Button::image_and_text(
                                icon_image(ui.ctx(), Icon::Help, 18.0),
                                "Input mapping help",
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.show_help = true;
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(28))
            .show(ctx, |ui| {
                ui.columns(3, |cols| {
                    metric(
                        &mut cols[0],
                        "Encoded frame rate",
                        stats.encoded.to_string(),
                        "fps",
                    );
                    metric(&mut cols[1], "Packets sent", stats.sent.to_string(), "");
                    metric(
                        &mut cols[2],
                        "Dropped frames",
                        stats.dropped.to_string(),
                        "",
                    );
                });
                ui.add_space(20.0);
                card().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(icon_image(ui.ctx(), Icon::Stylus, 20.0));
                        ui.label(RichText::new("Stylus").color(MUTED));
                        ui.add_space(48.0);
                        for (i, label) in ["Button 1   side", "Button 2   secondary"]
                            .iter()
                            .enumerate()
                        {
                            egui::Frame::new()
                                .fill(if stats.stylus_buttons[i] {
                                    Color32::from_rgb(231, 242, 235)
                                } else {
                                    BG
                                })
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    if stats.stylus_buttons[i] {
                                        GREEN
                                    } else {
                                        BORDER
                                    },
                                ))
                                .corner_radius(7)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.label(format!(
                                        "{}  {label}",
                                        if stats.stylus_buttons[i] {
                                            "●"
                                        } else {
                                            "○"
                                        }
                                    ));
                                });
                        }
                    });
                });
                ui.add_space(20.0);
                card().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Log").color(MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::image_and_text(
                                        icon_image(ui.ctx(), Icon::Clear, 16.0),
                                        "Clear",
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.stats.lock().unwrap().logs.clear();
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.set_min_height(ui.available_height().max(100.0) - 5.0);
                            for log in &stats.logs {
                                ui.label(RichText::new(log).monospace().size(12.0));
                            }
                        });
                });
            });
        if self.show_help {
            egui::Window::new("Input mapping help").open(&mut self.show_help).show(ctx, |ui| {ui.set_max_width(480.0); ui.label("On KDE Wayland, the server maps its virtual pen and touch devices to the screen selected in the sharing dialog. Your mouse can stay on another screen."); ui.add_space(12.0); ui.label(&stats.mapping); ui.add_space(12.0); ui.label("If automatic detection fails, stop the server, enter the display connector (for example DP-3) in Input monitor, then select that same screen when restarting. List connectors with kscreen-doctor -o."); ui.label("On other desktops, map Drawing Tablet Pen and Drawing Tablet Touch to the selected output in your desktop settings. A shared desktop may still change keyboard focus when you interact with a window."); });
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
impl Drop for DtServerApp {
    fn drop(&mut self) {
        self.running_flag.store(false, Ordering::SeqCst);
    }
}
