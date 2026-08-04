#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod fat;
mod ovmf;
mod vm;

use eframe::egui;
use std::path::PathBuf;
use vm::{Accel, Backend, VmConfig};

const LOG_CAP: usize = 250_000;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1150.0, 720.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mochivm manager",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}

struct App {
    vms: Vec<VmConfig>,
    selected: usize,
    backend: Backend,
    log: String,
    auto_scroll: bool,
    efi_path: PathBuf,
    ovmf_code: PathBuf,
    ovmf_vars: PathBuf,
    status: String,
}

impl App {
    fn new() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();
        let (ovmf_code, ovmf_vars) = ovmf::detect().unwrap_or_else(|| {
            (
                exe_dir.join("ovmf/x64/code.fd"),
                exe_dir.join("ovmf/x64/vars.fd"),
            )
        });
        Self {
            vms: vec![VmConfig::default()],
            selected: 0,
            backend: Backend::new(exe_dir.join("runtime")),
            log: String::new(),
            auto_scroll: true,
            efi_path: exe_dir.join("mochivm.efi"),
            ovmf_code,
            ovmf_vars,
            status: "idle".into(),
        }
    }

    fn run(&mut self, cfg: &VmConfig) {
        let efi = self.efi_path.clone();
        let code = self.ovmf_code.clone();
        let vars = self.ovmf_vars.clone();
        match self.backend.start(cfg, &efi, &code, &vars) {
            Ok(()) => {
                self.status = "running".into();
                self.log.clear();
                self.log.push_str(">> qemu started\n");
            }
            Err(e) => {
                self.status = e.clone();
                self.log.push_str(&format!(">> {e}\n"));
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let chunk = self.backend.drain_log();
        if !chunk.is_empty() {
            self.log.push_str(&chunk);
            if self.log.len() > LOG_CAP {
                self.log.drain(..self.log.len() - LOG_CAP);
            }
        }

        egui::SidePanel::left("sidebar")
            .default_width(190.0)
            .show(ctx, |ui| {
                ui.heading("vms");
                ui.separator();
                let mut to_delete = None;
                for i in 0..self.vms.len() {
                    let running = i == self.selected && self.backend.is_running();
                    let name = &self.vms[i].name;
                    ui.horizontal(|ui| {
                        ui.selectable_label(
                            self.selected == i,
                            format!("{}{}", if running { "● " } else { "○ " }, name),
                        )
                        .clicked()
                        .then(|| self.selected = i);
                        if ui.small_button("x").clicked() {
                            to_delete = Some(i);
                        }
                    });
                }
                if ui.button("+ new vm").clicked() {
                    self.vms.push(VmConfig::default());
                    self.selected = self.vms.len() - 1;
                }
                if let Some(i) = to_delete {
                    if self.vms.len() > 1 {
                        self.vms.remove(i);
                        self.selected = self.selected.saturating_sub(1);
                    }
                }
            });

        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("status:");
                ui.colored_label(egui::Color32::LIGHT_GREEN, &self.status);
                ui.separator();
                ui.label(format!(
                    "qemu: {}",
                    if self.backend.is_running() {
                        "running"
                    } else {
                        "stopped"
                    }
                ));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let cfg = self.vms[self.selected].clone();

            ui.heading(&cfg.name);
            ui.horizontal(|ui| {
                if ui.button("start").clicked() {
                    self.run(&cfg);
                }
                if ui.button("stop").clicked() {
                    self.backend.stop();
                    self.status = "stopped".into();
                }
                if ui.button("reset").clicked() {
                    self.run(&cfg);
                }
            });
            ui.separator();

            egui::CollapsingHeader::new("vm settings")
                .default_open(true)
                .show(ui, |ui| {
                    let mut cfg = self.vms[self.selected].clone();
                    egui::Grid::new("settings")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("name");
                            ui.text_edit_singleline(&mut cfg.name);
                            ui.end_row();

                            ui.label("memory");
                            ui.add(
                                egui::DragValue::new(&mut cfg.ram_mb)
                                    .range(64..=8192)
                                    .suffix(" MB"),
                            );
                            ui.end_row();

                            ui.label("cpus");
                            ui.add(egui::DragValue::new(&mut cfg.vcpus).range(1..=16));
                            ui.end_row();

                            ui.label("acceleration");
                            egui::ComboBox::from_id_salt("accel")
                                .selected_text(cfg.accel.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut cfg.accel,
                                        Accel::Tcg,
                                        Accel::Tcg.label(),
                                    );
                                    ui.selectable_value(
                                        &mut cfg.accel,
                                        Accel::Kvm,
                                        Accel::Kvm.label(),
                                    );
                                    ui.selectable_value(
                                        &mut cfg.accel,
                                        Accel::Whpx,
                                        Accel::Whpx.label(),
                                    );
                                });
                            ui.end_row();

                            ui.label("mochivm.efi");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.efi_path.to_string_lossy().to_string(),
                                    )
                                    .desired_width(320.0),
                                );
                                if ui.button("...").clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .add_filter("uefi app", &["efi"])
                                        .pick_file()
                                    {
                                        self.efi_path = p;
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label("ovmf code.fd");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.ovmf_code.to_string_lossy().to_string(),
                                    )
                                    .desired_width(320.0),
                                );
                                if ui.button("...").clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .add_filter("ovmf", &["fd"])
                                        .pick_file()
                                    {
                                        self.ovmf_code = p;
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label("ovmf vars.fd");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.ovmf_vars.to_string_lossy().to_string(),
                                    )
                                    .desired_width(320.0),
                                );
                                if ui.button("...").clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .add_filter("ovmf", &["fd"])
                                        .pick_file()
                                    {
                                        self.ovmf_vars = p;
                                    }
                                }
                            });
                            ui.end_row();
                        });

                    ui.horizontal(|ui| {
                        if ui.button("auto-detect ovmf").clicked() {
                            match ovmf::detect() {
                                Some((c, v)) => {
                                    self.ovmf_code = c;
                                    self.ovmf_vars = v;
                                    self.status = "ovmf found".into();
                                }
                                None => self.status = "ovmf not found - try download".into(),
                            }
                        }
                        if ui.button("download ovmf").clicked() {
                            match ovmf::download() {
                                Ok((c, v)) => {
                                    self.ovmf_code = c;
                                    self.ovmf_vars = v;
                                    self.status = "ovmf downloaded".into();
                                }
                                Err(e) => self.status = format!("ovmf download failed: {e}"),
                            }
                        }
                    });

                    self.vms[self.selected] = cfg;
                });

            ui.separator();

            ui.horizontal(|ui| {
                ui.heading("serial console");
                ui.checkbox(&mut self.auto_scroll, "auto-scroll");
                if ui.small_button("clear").clicked() {
                    self.log.clear();
                }
            });
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.auto_scroll)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.monospace(&self.log);
                });
        });
    }
}
