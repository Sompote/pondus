// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The window: menus and the options bar on top, the model browser on the left, the model view
//! in the centre with the design table under it, and the agent's chat and the properties of
//! the selection on the right — PhotoCraft's layout and Pro theme (craft-ui).

use eframe::egui::{self, RichText};
use pondus_core::analysis::Analysis;
use pondus_core::design::{self, Report};
use pondus_core::model::*;
use pondus_core::scene::{self, Diagram};

use crate::chat::ChatView;
use crate::theme::{self, Tokens};
use crate::view::View;

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum RightTab {
    Agent,
    Properties,
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum BottomTab {
    Design,
    Reactions,
    Messages,
    Nodes,
    Members,
    Supports,
    Loads,
    Sections,
    Slabs,
    Seismic,
    Stability,
}

/// The frame wizard's values (add_frame).
pub(crate) struct FrameForm {
    x_spans: String,
    y_spans: String,
    storeys: String,
    steel: bool,
    column: String,
    beam: String,
    base_fixed: bool,
    slabs: bool,
    h: f64,
    dead: f64,
    live: f64,
    roof_live: f64,
}

impl Default for FrameForm {
    fn default() -> Self {
        FrameForm { x_spans: "6, 6, 6".into(), y_spans: "5, 5".into(), storeys: "4.0, 3.5, 3.5".into(), steel: false, column: "400 x 400".into(), beam: "300 x 600".into(), base_fixed: true, slabs: true, h: 0.15, dead: 1.5, live: 3.0, roof_live: 1.0 }
    }
}

pub(crate) struct MemberForm {
    from: [f64; 3],
    to: [f64; 3],
    section: String,
}

pub struct App {
    pub(crate) model: Model,
    pub(crate) path: Option<std::path::PathBuf>,
    pub(crate) dirty: bool,
    pub(crate) undo: Vec<Model>,
    pub(crate) redo: Vec<Model>,
    pub(crate) analysis: Option<Analysis>,
    pub(crate) report: Option<Report>,
    /// The agent's model while it works (shown read-only).
    pub(crate) live: Option<Model>,
    pub(crate) view: View,
    pub(crate) spec: scene::Spec,
    pub(crate) selected: Vec<Id>,
    pub(crate) selected_slab: Option<Id>,
    pub(crate) hover: Option<Id>,
    pub(crate) chat: ChatView,
    pub(crate) right: RightTab,
    pub(crate) bottom: BottomTab,
    pub(crate) status: String,
    pub(crate) messages: Vec<String>,
    pub(crate) resize: bool,
    pub(crate) select: bool,
    pub(crate) frame_form: Option<FrameForm>,
    pub(crate) member_form: Option<MemberForm>,
    pub(crate) loads_open: bool,
    pub(crate) load_case: Id,
    pub(crate) load_w: f64,
    pub(crate) settings_open: bool,
    pub(crate) combo_name: String,
    pub(crate) combo_factors: String,
    pub(crate) dark: bool,
    pub(crate) tool: crate::tools::Tool,
    pub(crate) opts: crate::tools::ToolOpts,
    /// A table edit in progress (one undo step per gesture).
    pub(crate) editing: bool,
    pub(crate) pending_command: Option<(String, serde_json::Value)>,
    pub(crate) new_section: String,
    pub(crate) seismic_open: bool,
    pub(crate) table_text: String,
    pub(crate) animate: bool,
    pub(crate) pending_analyse: bool,
    pub(crate) th_open: bool,
    pub(crate) th_record: Option<pondus_core::timehistory::Record>,
    pub(crate) th_settings: pondus_core::timehistory::Settings,
    pub(crate) th_result: Option<pondus_core::timehistory::ThResult>,
    pub(crate) th_dt_text: String,
    pub(crate) pending_th: bool,
    pub(crate) adv: crate::advanced_ui::AdvState,
    pub(crate) sec: crate::section_ui::SecState,
    pub(crate) edit: crate::edit_ui::EditState,
    pub(crate) about_open: bool,
    /// The palette item being dragged onto the view.
    pub(crate) palette_drag: Option<usize>,
}

fn parse_list(s: &str) -> Vec<f64> {
    s.split([',', ' ', ';']).filter_map(|x| x.trim().parse::<f64>().ok()).collect()
}

fn ratio_text(t: &Tokens, r: f64) -> RichText {
    let c = if !r.is_finite() || r > 1.0 {
        t.error
    } else if r > 0.9 {
        t.warn
    } else {
        t.ok
    };
    RichText::new(if r.is_finite() { format!("{r:.2}") } else { "∞".into() }).color(c).monospace()
}

impl App {
    pub fn new(cc: &eframe::CreationContext, open: Option<std::path::PathBuf>) -> App {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, Tokens::DARK);
        let mut app = App {
            model: Model::new("Untitled"),
            path: None,
            dirty: false,
            undo: vec![],
            redo: vec![],
            analysis: None,
            report: None,
            live: None,
            view: View::default(),
            spec: scene::Spec { view: "3d".into(), ..Default::default() },
            selected: vec![],
            selected_slab: None,
            hover: None,
            chat: ChatView::new(),
            right: RightTab::Agent,
            bottom: BottomTab::Design,
            status: "New model. Build a frame (Model › Frame…), open a file, or ask the agent.".into(),
            messages: vec![],
            resize: true,
            select: true,
            frame_form: None,
            member_form: None,
            loads_open: false,
            load_case: 1,
            load_w: 10.0,
            settings_open: false,
            combo_name: String::new(),
            combo_factors: "D 1.2, L 1.6".into(),
            dark: true,
            tool: crate::tools::Tool::Select,
            opts: Default::default(),
            editing: false,
            pending_command: None,
            new_section: String::new(),
            seismic_open: false,
            table_text: String::new(),
            animate: true,
            pending_analyse: false,
            th_open: false,
            th_record: None,
            th_settings: Default::default(),
            th_result: None,
            th_dt_text: "0.01".into(),
            pending_th: false,
            adv: Default::default(),
            sec: Default::default(),
            edit: Default::default(),
            about_open: false,
            palette_drag: None,
        };
        if let Some(p) = open {
            app.open_path(p);
        }
        app
    }

    /// Before a change: one undo step.
    pub(crate) fn commit(&mut self) {
        self.undo.push(self.model.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.analysis = None;
        self.report = None;
    }

    pub(crate) fn set_model(&mut self, m: Model) {
        self.commit();
        self.model = m;
        self.selected.clear();
        self.selected_slab = None;
        self.view.fit = true;
    }

    pub(crate) fn undo(&mut self) {
        if let Some(m) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.model, m));
            self.analysis = None;
            self.report = None;
        }
    }

    fn redo(&mut self) {
        if let Some(m) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.model, m));
            self.analysis = None;
            self.report = None;
        }
    }

    pub(crate) fn run_command(&mut self, name: &str, args: serde_json::Value) {
        let before = self.model.clone();
        match pondus_core::commands::run(&mut self.model, name, &args) {
            Ok(v) => {
                let after = std::mem::replace(&mut self.model, before);
                self.commit();
                self.model = after;
                self.status = format!("{name}: {v}");
            }
            Err(e) => {
                self.model = before;
                self.status = format!("{name}: {e}");
            }
        }
    }

    fn open_path(&mut self, p: std::path::PathBuf) {
        match Model::load(&p) {
            Ok(m) => {
                self.set_model(m);
                self.undo.clear();
                self.dirty = false;
                self.status = format!("Opened {}", p.display());
                if p.extension().is_some_and(|e| e == "pondus") {
                    self.path = Some(p);
                } else {
                    self.path = None;
                }
            }
            Err(e) => self.status = format!("Could not open {}: {e}", p.display()),
        }
    }

    fn save(&mut self, as_new: bool) {
        let path = match (&self.path, as_new) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("Pondus model", &["pondus"]).set_file_name(format!("{}.pondus", self.model.name)).save_file(),
        };
        if let Some(p) = path {
            match self.model.save(&p) {
                Ok(()) => {
                    self.status = format!("Saved {}", p.display());
                    self.path = Some(p);
                    self.dirty = false;
                }
                Err(e) => self.status = format!("Could not save: {e}"),
            }
        }
    }

    pub(crate) fn analyze(&mut self) {
        let t0 = std::time::Instant::now();
        let gpu0 = pondus_core::compute::factorisations();
        match pondus_core::analysis::analyze(&self.model) {
            Ok(a) => {
                let s = pondus_core::mcp::analysis_summary(&self.model, &a);
                let ok = s["cases"].as_array().is_some_and(|c| c.iter().all(|c| c["balanced"] == true));
                self.messages = a.warnings.clone();
                let gpu = pondus_core::compute::factorisations() - gpu0;
                self.status = format!("Analysed {} elements in {:.0} ms{}; reactions {} the applied loads; slab load on beams {:.1} of {:.1} kN (dead)", a.elements, t0.elapsed().as_secs_f64() * 1000.0, if gpu > 0 { format!(" ({gpu} factorisations on the GPU)") } else { String::new() }, if ok { "balance" } else { "DO NOT balance" }, a.slab_applied[0], a.slab_expected[0]);
                self.report = Some(design::design_pass(&self.model, &a, &design::Options::default()));
                self.analysis = Some(a);
                if self.spec.diagram == Diagram::None {
                    self.spec.diagram = Diagram::Moment;
                }
            }
            Err(e) => {
                self.status = format!("Analysis: {e}");
                self.messages = vec![e];
            }
        }
    }

    pub(crate) fn design(&mut self) {
        let mut m = self.model.clone();
        let o = design::Options { resize: self.resize, select: self.select, ..Default::default() };
        match design::run(&mut m, &o) {
            Ok((rep, a)) => {
                if m != self.model {
                    self.commit();
                    self.model = m;
                }
                self.status = format!("Designed in {} round(s): {} change(s), {} failing", rep.rounds, rep.changes.len(), rep.failing().len());
                self.messages = rep.warnings.iter().chain(rep.skipped.iter()).chain(rep.changes.iter()).cloned().collect();
                self.analysis = Some(a);
                self.report = Some(rep);
                self.spec.diagram = Diagram::Utilisation;
                self.bottom = BottomTab::Design;
            }
            Err(e) => self.status = format!("Design: {e}"),
        }
    }

    fn export_bim(&mut self) {
        if self.report.is_none() {
            self.analyze();
        }
        let Some(rep) = &self.report else { return };
        let v = design::bim_export(&self.model, rep);
        if let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).set_file_name(format!("{}-bim.json", self.model.name)).save_file() {
            match std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()) {
                Ok(()) => self.status = format!("Wrote the design for BIM: {}", p.display()),
                Err(e) => self.status = format!("{e}"),
            }
        }
    }

    /// The calculation report (PDF): next to the model file when it has one, else where the
    /// engineer picks; the design is run first when there is none.
    pub(crate) fn write_report(&mut self, ask: bool) {
        if self.analysis.is_none() {
            self.analyze();
        }
        if self.report.is_none() {
            self.design();
        }
        let Some(a) = self.analysis.clone() else { return };
        let name = format!("{}-report.pdf", self.model.name.replace([' ', '/'], "_"));
        let path = match (&self.path, ask) {
            (Some(p), false) => p.parent().map(|d| d.join(&name)),
            _ => rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(&name).save_file(),
        };
        let Some(p) = path else { return };
        let opts = pondus_core::report::ReportOptions { project: self.model.name.clone(), detail: "governing".into(), pictures: true, ..Default::default() };
        match pondus_core::report::write_pdf(&self.model, &a, self.report.as_ref(), &opts, &p) {
            Ok(r) => {
                self.status = format!("Wrote the calculation report ({} pages): {}", r.pages, p.display());
                #[cfg(all(not(test), target_os = "macos"))]
                let _ = std::process::Command::new("open").arg(&p).spawn();
                #[cfg(all(not(test), windows))]
                let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&p).spawn();
                #[cfg(all(not(test), target_os = "linux"))]
                let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
            }
            Err(e) => self.status = format!("Report: {e}"),
        }
    }

    /// Help › About: the licence notice and the source link (AGPL-3.0 section 13).
    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        egui::Window::new("About Pondus").open(&mut open).collapsible(false).resizable(false).default_width(620.0).show(ctx, |ui| {
            ui.label(RichText::new(format!("Pondus {}", env!("CARGO_PKG_VERSION"))).font(theme::medium_font(16.0)));
            ui.label("Agentic structural analysis and design, on the Stabileo analysis engine.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Source code:");
                ui.hyperlink(pondus_core::about::SOURCE_URL);
            });
            ui.horizontal(|ui| {
                ui.label("Licence:");
                ui.hyperlink_to("GNU Affero General Public License v3.0 only", "https://www.gnu.org/licenses/agpl-3.0.html");
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                ui.label(RichText::new(pondus_core::about::notice()).monospace().small());
            });
        });
        self.about_open = open;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (z, y, s, del, f) = ctx.input(|i| {
            let c = i.modifiers.command;
            (c && i.key_pressed(egui::Key::Z) && !i.modifiers.shift, c && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift)), c && i.key_pressed(egui::Key::S), i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::F) && !c)
        });
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if z {
            self.undo();
        }
        if y {
            self.redo();
        }
        if s {
            self.save(false);
        }
        if del && !self.selected.is_empty() {
            let ids = std::mem::take(&mut self.selected);
            self.run_command("delete", serde_json::json!({"members": ids}));
        }
        if f {
            self.view.fit = true;
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("Pondus").font(theme::medium_font(14.0)).color(t.accent_text));
            ui.add_space(6.0);
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.set_model(Model::new("Untitled"));
                    self.path = None;
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Pondus or Stabileo / Aedifex JSON", &["pondus", "json"]).pick_file() {
                        self.open_path(p);
                    }
                }
                if ui.button("Save").clicked() {
                    ui.close();
                    self.save(false);
                }
                if ui.button("Save As…").clicked() {
                    ui.close();
                    self.save(true);
                }
                ui.separator();
                if ui.button("Import Stabileo / Aedifex model…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Stabileo model (JSON)", &["json"]).pick_file() {
                        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string())).and_then(|v| pondus_core::stabileo::import(&v)) {
                            Ok((m, rep)) => {
                                self.set_model(m);
                                self.status = format!("Imported {}: {} elements → {} members ({} merged); {}", p.display(), rep.elements, rep.members, rep.merged, rep.skipped.join("; "));
                            }
                            Err(e) => self.status = format!("Import: {e}"),
                        }
                    }
                }
                if ui.button("Export Stabileo model…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).set_file_name(format!("{}-stabileo.json", self.model.name)).save_file() {
                        let v = pondus_core::stabileo::export(&self.model);
                        self.status = match std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()) {
                            Ok(()) => format!("Wrote {}", p.display()),
                            Err(e) => e.to_string(),
                        };
                    }
                }
                if ui.button("Import Excel, DXF or IFC…").clicked() {
                    ui.close();
                    self.import_file();
                }
                for (label, ext) in [("Export Excel workbook…", "xlsx"), ("Export DXF drawing…", "dxf"), ("Export IFC4…", "ifc")] {
                    if ui.button(label).clicked() {
                        ui.close();
                        self.export_file(ext);
                    }
                }
                if ui.button("Export design for BIM (Aedifex)…").clicked() {
                    ui.close();
                    self.export_bim();
                }
                ui.separator();
                if ui.button("Calculation report (PDF)").clicked() {
                    ui.close();
                    self.write_report(false);
                }
                if ui.button("Calculation report as…").clicked() {
                    ui.close();
                    self.write_report(true);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                    self.redo();
                    ui.close();
                }
                if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Delete selected members")).clicked() {
                    let ids = std::mem::take(&mut self.selected);
                    self.run_command("delete", serde_json::json!({"members": ids}));
                    ui.close();
                }
            });
            ui.menu_button("Model", |ui| {
                if ui.button("Frame…").clicked() {
                    self.open_frame_form();
                    ui.close();
                }
                if ui.button("Member…").clicked() {
                    self.member_form = Some(MemberForm { from: [0.0, 0.0, 0.0], to: [6.0, 0.0, 0.0], section: "300 x 600".into() });
                    ui.close();
                }
                if ui.button("Loads and combinations…").clicked() {
                    self.loads_open = true;
                    ui.close();
                }
                if ui.button("Materials and design settings…").clicked() {
                    self.settings_open = true;
                    ui.close();
                }
                let all_on = !self.model.diaphragms.is_empty();
                if ui.selectable_label(all_on, "Rigid floor diaphragms").on_hover_text("Every level above the supports moves rigidly in plan (for lateral loads with coefficient slabs)").clicked() {
                    self.run_command("set_diaphragms", if all_on { serde_json::json!({"levels": []}) } else { serde_json::json!({"all": true}) });
                    ui.close();
                }
                if ui.button("Check model").clicked() {
                    self.messages = pondus_core::check::problems(&self.model);
                    self.status = format!("{} problem(s)", self.messages.len());
                    self.bottom = BottomTab::Messages;
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About Pondus and its licence").clicked() {
                    self.about_open = true;
                    ui.close();
                }
                ui.hyperlink_to("Source code (AGPL-3.0)", pondus_core::about::SOURCE_URL);
            });
            ui.menu_button("View", |ui| {
                if ui.checkbox(&mut self.view.labels, "Member numbers").clicked() {
                    ui.close();
                }
                if ui.checkbox(&mut self.spec.show_loads, "Nodal loads").clicked() {
                    ui.close();
                }
                if ui.button(if self.dark { "Light theme" } else { "Dark theme" }).clicked() {
                    self.dark = !self.dark;
                    theme::apply(ui.ctx(), if self.dark { Tokens::DARK } else { Tokens::LIGHT });
                    ui.close();
                }
            });
            ui.add_space(12.0);
            if ui.add(egui::Button::image_and_text(crate::icons::image("activity", 14.0, t.text), "Analyse")).on_hover_text("Linear static analysis of every load case").clicked() {
                self.analyze();
            }
            if ui.add(egui::Button::image_and_text(crate::icons::image("check", 14.0, t.text), "Design").fill(t.accent_soft)).on_hover_text("ACI 318-19 beams and slabs, AISC 360-16 steel").clicked() {
                self.design();
            }
            ui.checkbox(&mut self.resize, "resize concrete").on_hover_text("Deepen failing concrete beams (50 mm steps) and thicken failing slabs");
            ui.checkbox(&mut self.select, "select steel").on_hover_text("Choose the lightest passing H shape for every steel member (or group)");
        });
    }

    /// The level of the plan view and the frame line of the elevation.
    fn view_pickers(&mut self, ui: &mut egui::Ui) {
        let levels: Vec<f64> = {
            let mut z: Vec<f64> = self.model.nodes.iter().map(|n| (n.z * 1000.0).round() / 1000.0).collect();
            z.sort_by(f64::total_cmp);
            z.dedup();
            z
        };
        let lines: Vec<f64> = {
            let mut y: Vec<f64> = self.model.nodes.iter().map(|n| (n.y * 1000.0).round() / 1000.0).collect();
            y.sort_by(f64::total_cmp);
            y.dedup();
            y
        };
        if self.spec.view == "plan" {
            let cur = self.spec.z.map(|z| format!("plan z = {z:.2} m")).unwrap_or("plan: all levels".into());
            egui::ComboBox::from_id_salt("plan-z").selected_text(cur).show_ui(ui, |ui| {
                if ui.selectable_label(self.spec.z.is_none(), "all levels").clicked() {
                    self.spec.z = None;
                }
                for z in &levels {
                    if ui.selectable_label(self.spec.z == Some(*z), format!("z = {z:.2} m")).clicked() {
                        self.spec.z = Some(*z);
                        self.opts.work_z = *z;
                        self.view.fit = true;
                    }
                }
            });
        }
        if self.spec.view == "elevation" {
            let cur = self.spec.y.map(|y| format!("frame y = {y:.2} m")).unwrap_or("elevation: all frames".into());
            egui::ComboBox::from_id_salt("elev-y").selected_text(cur).show_ui(ui, |ui| {
                if ui.selectable_label(self.spec.y.is_none(), "all frames").clicked() {
                    self.spec.y = None;
                }
                for y in &lines {
                    if ui.selectable_label(self.spec.y == Some(*y), format!("y = {y:.2} m")).clicked() {
                        self.spec.y = Some(*y);
                        self.view.fit = true;
                    }
                }
            });
        }
    }

    fn browser(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        let m = self.live.as_ref().unwrap_or(&self.model).clone();
        ui.label(RichText::new(&m.name).font(theme::medium_font(14.0)));
        let frame = m.members.iter().filter(|x| x.role.is_frame()).count();
        ui.label(RichText::new(format!("{} nodes · {} members · {} walls · {} supports · {} slabs", m.nodes.len(), frame, m.walls.len(), m.supports.len(), m.slabs.len())).small().color(t.text_dim));
        ui.add_space(4.0);
        for (kind, title) in [(MemberKind::Column, "Columns"), (MemberKind::Beam, "Beams"), (MemberKind::Brace, "Braces")] {
            let list: Vec<&Member> = m.members.iter().filter(|x| x.role.is_frame() && m.kind(x) == kind).collect();
            if list.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!("{title} ({})", list.len())).id_salt(title).show(ui, |ui| {
                for x in list {
                    let sec = m.section(x.section).map(|s| s.name.clone()).unwrap_or_default();
                    let r = self.report.as_ref().and_then(|r| scene::member_ratio(r, x.id));
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.selected.contains(&x.id), format!("{} · {}", x.id, sec)).clicked() {
                            self.selected = vec![x.id];
                            self.selected_slab = None;
                            self.right = RightTab::Properties;
                        }
                        if let Some(r) = r {
                            ui.label(ratio_text(&t, r).small());
                        }
                    });
                }
            });
        }
        if !m.walls.is_empty() {
            egui::CollapsingHeader::new(format!("Walls ({})", m.walls.len())).id_salt("walls").show(ui, |ui| {
                for w in &m.walls {
                    let pier = m.members.iter().find(|x| x.role == pondus_core::model::Role::WallPier(w.id)).map(|x| x.id);
                    let r = self.report.as_ref().map(|r| r.walls.iter().filter(|d| d.wall == w.id).map(|d| d.ratio).fold(0.0, f64::max)).filter(|r| *r > 0.0);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(pier.is_some_and(|p| self.selected.contains(&p)), format!("{} · {:.0} mm × {:.2} m · z {:.1}–{:.1}", w.name, w.t * 1e3, w.length(), w.z0, w.z1)).clicked() {
                            self.selected = pier.into_iter().collect();
                            self.selected_slab = None;
                        }
                        if let Some(r) = r {
                            ui.label(ratio_text(&t, r).small());
                        }
                    });
                }
            });
        }
        if !m.slabs.is_empty() {
            egui::CollapsingHeader::new(format!("Slabs ({})", m.slabs.len())).id_salt("slabs").show(ui, |ui| {
                for s in &m.slabs {
                    let r = self.report.as_ref().and_then(|r| r.slabs.iter().find(|d| d.slab == s.id)).map(|d| d.ratio);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.selected_slab == Some(s.id), format!("{} · {:.1} x {:.1} m · {:.0} mm @ z {:.2}", s.name, s.lx(), s.ly(), s.h * 1000.0, s.z)).clicked() {
                            self.selected_slab = Some(s.id);
                            self.selected.clear();
                            self.right = RightTab::Properties;
                        }
                        if let Some(r) = r {
                            ui.label(ratio_text(&t, r).small());
                        }
                    });
                }
            });
        }
        egui::CollapsingHeader::new(format!("Sections ({})", m.sections.len())).id_salt("sections").show(ui, |ui| {
            for s in &m.sections {
                let n = m.members.iter().filter(|x| x.section == s.id).count();
                ui.label(format!("{} · {n} member(s)", s.name));
            }
        });
        egui::CollapsingHeader::new("Load cases and combinations").id_salt("cases").show(ui, |ui| {
            for c in &m.cases {
                ui.label(format!("{} ({:?}{}) · {} load(s)", c.name, c.kind, if c.self_weight { ", self-weight" } else { "" }, m.loads.iter().filter(|l| l.case == c.id).count()));
            }
            for c in &m.combos {
                ui.label(RichText::new(format!("{} [{:?}]", c.name, c.kind)).color(t.text_dim));
            }
        });
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        if let Some(sid) = self.selected_slab {
            let Some(s) = self.model.slabs.iter().find(|s| s.id == sid).cloned() else { return };
            ui.label(RichText::new(format!("Slab {}", s.name)).font(theme::medium_font(14.0)));
            let mut h = s.h * 1000.0;
            let (mut dead, mut live) = (s.dead, s.live);
            let (mut subgrade, mut mat, mut support_size, mut plate) = (s.subgrade, s.mat, s.support_size, s.plate);
            let mut changed = false;
            egui::Grid::new("slab-props").num_columns(2).show(ui, |ui| {
                ui.label("Plan");
                ui.label(format!("({:.2}, {:.2}) – ({:.2}, {:.2}) m at z {:.2}", s.x0, s.y0, s.x1, s.y1, s.z));
                ui.end_row();
                ui.label("Thickness, mm");
                changed |= ui.add(egui::DragValue::new(&mut h).speed(5.0).range(80.0..=5000.0)).changed();
                ui.end_row();
                ui.label("Flat plate");
                changed |= ui.checkbox(&mut plate, "on columns, no beams").on_hover_text("Analysed as plate finite elements whatever the slab method, and designed as a flat plate (ACI 318-19): column and middle strip bars, punching at every column with the unbalanced moments (8.4.4.2), moment transfer by flexure within c2 + 3h (8.4.2.2.3), the minimum thickness of Table 8.3.1.1").changed();
                ui.end_row();
                ui.label("Foundation mat");
                changed |= ui.checkbox(&mut mat, "pile cap / raft").on_hover_text("Designed as a foundation: bars of each face over the whole panel, punching checked at every support and column. Analysed as plate finite elements.").changed();
                ui.end_row();
                ui.label("Subgrade, kN/m³");
                changed |= ui.add(egui::DragValue::new(&mut subgrade).speed(250.0).range(0.0..=1.0e6)).on_hover_text("Winkler modulus of the soil under the panel (0: none): springs under every plate node").changed();
                ui.end_row();
                ui.label("Support width, m");
                changed |= ui.add(egui::DragValue::new(&mut support_size).speed(0.05).range(0.0..=3.0)).on_hover_text("Pile or pedestal width for the punching check (0: 0.3 m)").changed();
                ui.end_row();
                ui.label("Superimposed dead, kPa");
                changed |= ui.add(egui::DragValue::new(&mut dead).speed(0.1).range(0.0..=50.0)).changed();
                ui.end_row();
                ui.label("Live, kPa");
                changed |= ui.add(egui::DragValue::new(&mut live).speed(0.1).range(0.0..=50.0)).changed();
                ui.end_row();
            });
            if changed {
                self.commit();
                if let Some(x) = self.model.slabs.iter_mut().find(|x| x.id == sid) {
                    x.h = h / 1000.0;
                    x.dead = dead;
                    x.live = live;
                    x.subgrade = subgrade;
                    x.mat = mat;
                    x.support_size = support_size;
                    x.plate = plate;
                }
            }
            // the plate results of this slab under the shown result
            if let Some(r) = self.analysis.as_ref().and_then(|a| scene::pick_result(&self.model, a, &self.spec)) {
                let ps: Vec<&pondus_core::slabfe::PlateResult> = r.plates.iter().filter(|p| p.slab == sid).collect();
                if !ps.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(format!("Plate results · {} · {} elements", r.name, ps.len())).font(theme::medium_font(13.0)));
                    egui::Grid::new("plate-extremes").striped(true).num_columns(4).show(ui, |ui| {
                        for h in ["", "most +", "most −", "unit"] {
                            ui.label(RichText::new(h).small().color(t.text_dim));
                        }
                        ui.end_row();
                        for (name, d, u) in [("mx", Diagram::PlateMx, "kN·m/m"), ("my", Diagram::PlateMy, "kN·m/m"), ("mxy", Diagram::PlateMxy, "kN·m/m"), ("qx", Diagram::PlateQx, "kN/m"), ("qy", Diagram::PlateQy, "kN/m")] {
                            let hi = ps.iter().max_by(|a, b| d.plate_value(a).total_cmp(&d.plate_value(b))).unwrap();
                            let lo = ps.iter().min_by(|a, b| d.plate_value(a).total_cmp(&d.plate_value(b))).unwrap();
                            if ui.selectable_label(self.spec.diagram == d, name).on_hover_text("Show it in the view").clicked() {
                                self.spec.diagram = d;
                            }
                            ui.label(RichText::new(format!("{:.2} at ({:.2}, {:.2})", d.plate_value(hi), hi.x, hi.y)).monospace());
                            ui.label(RichText::new(format!("{:.2} at ({:.2}, {:.2})", d.plate_value(lo), lo.x, lo.y)).monospace());
                            ui.label(RichText::new(u).small().color(t.text_dim));
                            ui.end_row();
                        }
                        let wa: Vec<(f64, f64, f64, f64)> = ps.iter().map(|p| pondus_core::slabfe::wood_armer(p.mx, p.my, p.mxy)).collect();
                        let m = |f: fn(&(f64, f64, f64, f64)) -> f64| wa.iter().map(f).fold(0.0, f64::max);
                        ui.label("Wood–Armer");
                        ui.label(RichText::new(format!("bottom x {:.1}, y {:.1}", m(|w| w.0), m(|w| w.1))).monospace());
                        ui.label(RichText::new(format!("top x {:.1}, y {:.1}", m(|w| w.2), m(|w| w.3))).monospace());
                        ui.label(RichText::new("kN·m/m").small().color(t.text_dim));
                        ui.end_row();
                        let w = ps.iter().map(|p| -p.uz).fold(f64::MIN, f64::max) * 1000.0;
                        ui.label("deflection");
                        ui.label(RichText::new(format!("{w:.2} mm (largest)")).monospace());
                        ui.end_row();
                    });
                }
            } else if self.model.design.slab_method == pondus_core::slabfe::SlabMethod::Plate {
                ui.label(RichText::new("Analyse to see the slab's plate moments and shears.").color(t.text_dim));
            }
            if let Some(d) = self.report.as_ref().and_then(|r| r.slabs.iter().find(|d| d.slab == sid)) {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Design").font(theme::medium_font(13.0)));
                    ui.label(ratio_text(&t, d.ratio));
                    ui.label(RichText::new(&d.governs).small().color(t.text_dim));
                });
                let wu = if d.wu > 0.0 { format!(" · wu {:.2} kPa", d.wu) } else { String::new() };
                ui.label(RichText::new(format!("{}{wu} · h min {:.0} mm", d.method, d.h_min)).small().color(t.text_dim));
                egui::Grid::new("slab-bars").striped(true).num_columns(4).show(ui, |ui| {
                    for h in ["Strip", "Mu kN·m/m", "As req / prov mm²/m", "Bars"] {
                        ui.label(RichText::new(h).small().color(t.text_dim));
                    }
                    ui.end_row();
                    for s in &d.strips {
                        ui.label(format!("{} {}", s.direction, s.location));
                        ui.label(format!("{:.1}", s.mu));
                        ui.label(format!("{:.0} / {:.0}", s.as_req, s.as_prov));
                        ui.label(RichText::new(&s.bars).monospace());
                        ui.end_row();
                    }
                });
                ui.label(format!("Shear: Vu {:.1} kN/m, φVc {:.1} kN/m", d.vu, d.phi_vc));
                for n in &d.notes {
                    ui.label(RichText::new(format!("• {n}")).small().color(t.text_dim));
                }
            }
            return;
        }
        let Some(&id) = self.selected.first() else {
            ui.label(RichText::new("Click a member in the view, or a member or slab in the browser, to see and edit it.").color(t.text_dim));
            return;
        };
        let Some(m) = self.model.member(id).cloned() else { return };
        ui.label(RichText::new(format!("Member {} · {:?}", m.id, self.model.kind(&m))).font(theme::medium_font(14.0)));
        let (a, b) = (self.model.node(m.i).map(|n| n.p()).unwrap_or_default(), self.model.node(m.j).map(|n| n.p()).unwrap_or_default());
        ui.label(RichText::new(format!("({:.2}, {:.2}, {:.2}) → ({:.2}, {:.2}, {:.2}) · L {:.3} m", a[0], a[1], a[2], b[0], b[1], b[2], self.model.length(&m))).small().color(t.text_dim));
        let mut e = m.clone();
        let mut sec_name = self.model.section(m.section).map(|s| s.name.clone()).unwrap_or_default();
        let mut new_section: Option<String> = None;
        egui::Grid::new("member-props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Section");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("sec").selected_text(&sec_name).width(160.0).show_ui(ui, |ui| {
                    for s in &self.model.sections {
                        if ui.selectable_label(s.id == m.section, &s.name).clicked() {
                            e.section = s.id;
                        }
                    }
                    ui.separator();
                    for s in pondus_core::sections::catalogue() {
                        if ui.selectable_label(false, &s.name).clicked() {
                            new_section = Some(s.name.clone());
                        }
                    }
                });
            });
            ui.end_row();
            ui.label("New size");
            let r = ui.add(egui::TextEdit::singleline(&mut sec_name).hint_text("300 x 600 or H 300x150x6.5x9").desired_width(170.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                new_section = Some(sec_name.clone());
            }
            ui.end_row();
            ui.label("Material");
            egui::ComboBox::from_id_salt("mat").selected_text(self.model.material(e.material).map(|x| x.name.clone()).unwrap_or_default()).show_ui(ui, |ui| {
                for x in &self.model.materials {
                    ui.selectable_value(&mut e.material, x.id, &x.name);
                }
            });
            ui.end_row();
            ui.label("Kind");
            egui::ComboBox::from_id_salt("kind").selected_text(format!("{:?}", e.kind)).show_ui(ui, |ui| {
                for k in [MemberKind::Auto, MemberKind::Beam, MemberKind::Column, MemberKind::Brace] {
                    ui.selectable_value(&mut e.kind, k, format!("{k:?}"));
                }
            });
            ui.end_row();
            ui.label("Hinges");
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.hinge_i, "start");
                ui.checkbox(&mut e.hinge_j, "end");
            });
            ui.end_row();
            ui.label("Roll, °");
            ui.add(egui::DragValue::new(&mut e.roll).speed(1.0).range(-180.0..=180.0));
            ui.end_row();
            if self.model.is_steel(&m) {
                ui.label("Lb, m (unbraced)");
                let mut lb = e.lb.unwrap_or(self.model.length(&m));
                if ui.add(egui::DragValue::new(&mut lb).speed(0.05).range(0.0..=100.0)).changed() {
                    e.lb = Some(lb);
                }
                ui.end_row();
                ui.label("K");
                let mut k = e.k.unwrap_or(1.0);
                if ui.add(egui::DragValue::new(&mut k).speed(0.01).range(0.5..=3.0)).changed() {
                    e.k = Some(k);
                }
                ui.end_row();
            }
            ui.label("BIM id");
            let mut bim = e.bim_id.clone().unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut bim).desired_width(170.0)).changed() {
                e.bim_id = (!bim.is_empty()).then_some(bim);
            }
            ui.end_row();
        });
        if let Some(n) = new_section {
            match self.model.clone().section_named(&n) {
                Ok(_) => {
                    self.commit();
                    let sid = self.model.section_named(&n).unwrap();
                    let steel = matches!(self.model.section(sid).map(|s| &s.shape), Some(Shape::H { .. }));
                    let steel_id = self.model.steel_id();
                    if let Some(x) = self.model.members.iter_mut().find(|x| x.id == id) {
                        x.section = sid;
                        if steel {
                            x.material = steel_id;
                        }
                    }
                }
                Err(err) => self.status = err,
            }
        } else if e != m {
            self.commit();
            if let Some(x) = self.model.members.iter_mut().find(|x| x.id == id) {
                *x = e;
            }
        }
        // forces and design
        if let Some(a) = &self.analysis {
            if let Some(r) = scene::pick_result(&self.model, a, &self.spec) {
                if let Some(mr) = r.member(id) {
                    let f = |g: &dyn Fn(&pondus_core::analysis::Station) -> f64| mr.stations.iter().map(g).fold(f64::NEG_INFINITY, f64::max);
                    let fmin = |g: &dyn Fn(&pondus_core::analysis::Station) -> f64| mr.stations.iter().map(g).fold(f64::INFINITY, f64::min);
                    ui.separator();
                    ui.label(RichText::new(format!("Forces · {}", r.name)).font(theme::medium_font(13.0)));
                    egui::Grid::new("forces").num_columns(2).show(ui, |ui| {
                        ui.label("My sagging / hogging");
                        ui.label(RichText::new(format!("{:.1} / {:.1} kN·m", f(&|s| s.my).max(0.0), fmin(&|s| s.my).min(0.0))).monospace());
                        ui.end_row();
                        ui.label("Vz max");
                        ui.label(RichText::new(format!("{:.1} kN", f(&|s| s.vz.abs()))).monospace());
                        ui.end_row();
                        ui.label("N min / max");
                        ui.label(RichText::new(format!("{:.1} / {:.1} kN", fmin(&|s| s.n), f(&|s| s.n))).monospace());
                        ui.end_row();
                        ui.label("Mz, T max");
                        ui.label(RichText::new(format!("{:.1}, {:.1} kN·m", f(&|s| s.mz.abs()), f(&|s| s.t.abs()))).monospace());
                        ui.end_row();
                    });
                }
            }
        }
        if let Some(rep) = &self.report {
            if let Some(d) = rep.beams.iter().find(|d| d.member == id) {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ACI 318-19").font(theme::medium_font(13.0)));
                    ui.label(ratio_text(&t, d.ratio));
                    ui.label(RichText::new(&d.governs).small().color(t.text_dim));
                });
                egui::Grid::new("beam-flex").striped(true).num_columns(5).show(ui, |ui| {
                    for h in ["Face", "Zone", "Mu kN·m", "As req/prov", "Bars"] {
                        ui.label(RichText::new(h).small().color(t.text_dim));
                    }
                    ui.end_row();
                    for z in &d.flexure {
                        ui.label(&z.face);
                        ui.label(&z.zone);
                        ui.label(format!("{:.1}", z.mu));
                        ui.label(format!("{:.0}/{:.0}", z.as_req, z.as_prov));
                        ui.label(RichText::new(&z.bars).monospace());
                        ui.end_row();
                    }
                });
                for s in &d.shear {
                    ui.label(format!("Stirrups {}: {} (Vu {:.1} kN, φVc {:.1} kN, ratio {:.2})", s.zone, s.stirrups, s.vu, s.phi_vc, s.ratio));
                }
                if let Some(tq) = &d.torsion {
                    ui.label(format!("Torsion: Tu {:.1} kN·m (φTth {:.1}, φTcr {:.1}); closed stirrups At/s {:.3} mm²/mm; Al {:.0} mm², side bars {}; section {:.2}", tq.tu, tq.phi_tth, tq.phi_tcr, tq.at_s, tq.al, tq.side_bars, tq.section_ratio));
                }
                ui.label(format!("Deflection: live {:.1} mm (≤ {:.1}), long-term {:.1} mm (≤ {:.1}); h min {:.0} mm ({})", d.defl_live, d.limit_live, d.defl_total, d.limit_total, d.h_min, d.support));
                for n in &d.notes {
                    ui.label(RichText::new(format!("• {n}")).small().color(t.warn));
                }
            }
            if let Some(d) = rep.columns.iter().find(|d| d.member == id) {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ACI 318-19 column").font(theme::medium_font(13.0)));
                    ui.label(ratio_text(&t, d.ratio));
                    ui.label(RichText::new(&d.governs).small().color(t.text_dim));
                });
                ui.label(RichText::new(format!("{} — {} along b, {} along h · ρ {:.2} % · ties {}", d.bars, d.n_b, d.n_h, d.rho * 100.0, d.ties)).monospace());
                for c in &d.checks {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(ratio_text(&t, c.ratio));
                        ui.label(RichText::new(format!("{}: {}", c.name, c.detail)).small());
                    });
                }
                for n in &d.notes {
                    ui.label(RichText::new(format!("• {n}")).small().color(t.warn));
                }
            }
            if let Some(d) = rep.steel.iter().find(|d| d.member == id) {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("AISC 360-16").font(theme::medium_font(13.0)));
                    ui.label(ratio_text(&t, d.ratio));
                    ui.label(RichText::new(format!("{} · {:.1} kg/m", d.section, d.mass)).small().color(t.text_dim));
                });
                for c in &d.checks {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(ratio_text(&t, c.ratio));
                        ui.label(RichText::new(format!("{}: {}", c.name, c.detail)).small());
                    });
                }
                for n in &d.notes {
                    ui.label(RichText::new(format!("• {n}")).small().color(t.warn));
                }
            }
        }
    }

    fn bottom_panel(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            for (tab, name) in [(BottomTab::Nodes, format!("Nodes ({})", self.model.nodes.len())), (BottomTab::Members, format!("Members ({})", self.model.members.len())), (BottomTab::Supports, format!("Supports ({})", self.model.supports.len())), (BottomTab::Loads, format!("Loads ({})", self.model.loads.len())), (BottomTab::Sections, format!("Sections ({})", self.model.sections.len())), (BottomTab::Slabs, format!("Slabs ({})", self.model.slabs.len()))] {
                ui.selectable_value(&mut self.bottom, tab, name);
            }
            ui.separator();
            if self.model.seismic.enabled {
                ui.selectable_value(&mut self.bottom, BottomTab::Seismic, "Seismic");
            }
            if self.analysis.as_ref().is_some_and(|a| a.buckling.is_some()) {
                ui.selectable_value(&mut self.bottom, BottomTab::Stability, "Buckling");
            }
            ui.selectable_value(&mut self.bottom, BottomTab::Design, "Design");
            ui.selectable_value(&mut self.bottom, BottomTab::Reactions, "Reactions");
            ui.selectable_value(&mut self.bottom, BottomTab::Messages, format!("Messages ({})", self.messages.len()));
        });
        let tab = self.bottom;
        egui::ScrollArea::both().id_salt("bottom").auto_shrink([false, false]).show(ui, |ui| match tab {
            BottomTab::Nodes | BottomTab::Members | BottomTab::Supports | BottomTab::Loads | BottomTab::Sections | BottomTab::Slabs => self.data_tables(ui, tab),
            BottomTab::Seismic => self.seismic_tab(ui),
            BottomTab::Stability => self.stability_tab(ui),
            BottomTab::Design => {
                let Some(rep) = self.report.clone() else {
                    ui.label(RichText::new("Analyse (or Design) to see the members' design.").color(t.text_dim));
                    return;
                };
                egui::Grid::new("design-table").striped(true).num_columns(6).spacing([14.0, 4.0]).show(ui, |ui| {
                    for h in ["Member", "Section", "Ratio", "Governs", "Reinforcement / shape", "Notes"] {
                        ui.label(RichText::new(h).small().color(t.text_dim));
                    }
                    ui.end_row();
                    for b in &rep.beams {
                        if ui.selectable_label(self.selected.contains(&b.member), format!("B {}", b.member)).clicked() {
                            self.selected = vec![b.member];
                            self.selected_slab = None;
                            self.right = RightTab::Properties;
                        }
                        ui.label(format!("{:.0} x {:.0}", b.b * 1e3, b.h * 1e3));
                        ui.label(ratio_text(&t, b.ratio));
                        ui.label(RichText::new(&b.governs).small());
                        ui.label(RichText::new(format!("top {} | bottom {} | {}", design::zone_bars(b, "top"), design::zone_bars(b, "bottom"), b.shear.iter().map(|s| s.stirrups.clone()).collect::<Vec<_>>().join(", "))).small().monospace());
                        ui.label(RichText::new(b.notes.first().cloned().unwrap_or_default()).small().color(t.warn));
                        ui.end_row();
                    }
                    for c in &rep.columns {
                        if ui.selectable_label(self.selected.contains(&c.member), format!("C {}", c.member)).clicked() {
                            self.selected = vec![c.member];
                            self.selected_slab = None;
                            self.right = RightTab::Properties;
                        }
                        ui.label(format!("{:.0} x {:.0}", c.b * 1e3, c.h * 1e3));
                        ui.label(ratio_text(&t, c.ratio));
                        ui.label(RichText::new(&c.governs).small());
                        ui.label(RichText::new(format!("{} (ρ {:.2} %) | ties {}", c.bars, c.rho * 100.0, c.ties)).small().monospace());
                        ui.label(RichText::new(c.notes.first().cloned().unwrap_or_default()).small().color(t.text_dim));
                        ui.end_row();
                    }
                    for w in &rep.walls {
                        if ui.selectable_label(self.selected.contains(&w.member), format!("W {} z {:.1}", w.name, w.z0)).clicked() {
                            self.selected = vec![w.member];
                            self.selected_slab = None;
                            self.right = RightTab::Properties;
                        }
                        ui.label(format!("{:.0} x {:.0}", w.t * 1e3, w.lw * 1e3));
                        ui.label(ratio_text(&t, w.ratio));
                        ui.label(RichText::new(&w.governs).small());
                        ui.label(RichText::new(format!("vert {} | horiz {} | Vu {:.0}/φVn {:.0} kN", w.vertical, w.horizontal, w.vu, w.phi_vn)).small().monospace());
                        ui.label(RichText::new(w.boundary.clone().unwrap_or_default()).small().color(t.warn));
                        ui.end_row();
                    }
                    for f in &rep.footings {
                        ui.label(format!("F @ {}", f.node));
                        ui.label(format!("{:.2} x {:.2} x {:.2} m", f.l, f.b, f.h));
                        ui.label(ratio_text(&t, f.ratio));
                        ui.label(RichText::new(&f.governs).small());
                        ui.label(RichText::new(format!("x {} | y {} | q {:.0} kPa", f.bars_x, f.bars_y, f.q_max)).small().monospace());
                        ui.label(RichText::new(format!("punching {:.2}, one-way {:.2}", f.punching_ratio, f.oneway_ratio)).small().color(t.text_dim));
                        ui.end_row();
                    }
                    for o in &rep.others {
                        if ui.selectable_label(self.selected.contains(&o.member), format!("M {}", o.member)).clicked() {
                            self.selected = vec![o.member];
                            self.right = RightTab::Properties;
                        }
                        ui.label(&o.section);
                        ui.label(ratio_text(&t, o.ratio));
                        ui.label(RichText::new(&o.governs).small());
                        ui.label(RichText::new(&o.code).small());
                        ui.label(RichText::new(o.notes.first().cloned().unwrap_or_default()).small().color(t.text_dim));
                        ui.end_row();
                    }
                    for c in &rep.connections {
                        ui.label(format!("J {} {}", c.member, c.end));
                        ui.label(RichText::new(format!("Vu {:.0} kN", c.vu)).small());
                        ui.label(ratio_text(&t, c.ratio));
                        ui.label(RichText::new("shear tab").small());
                        ui.label(RichText::new(format!("{} | {} | {}", c.bolts, c.plate, c.weld)).small().monospace());
                        ui.label(RichText::new(format!("bolts {:.2}, bearing {:.2}, plate {:.2}, weld {:.2}", c.bolt_ratio, c.bearing_ratio, c.plate_ratio, c.weld_ratio)).small().color(t.text_dim));
                        ui.end_row();
                    }
                    for s in &rep.steel {
                        if ui.selectable_label(self.selected.contains(&s.member), format!("S {}", s.member)).clicked() {
                            self.selected = vec![s.member];
                            self.selected_slab = None;
                            self.right = RightTab::Properties;
                        }
                        ui.label(&s.section);
                        ui.label(ratio_text(&t, s.ratio));
                        ui.label(RichText::new(&s.governs).small());
                        ui.label(RichText::new(format!("{:.1} kg/m, L {:.2} m", s.mass, s.length)).small());
                        ui.label(RichText::new(s.notes.first().cloned().unwrap_or_default()).small().color(t.warn));
                        ui.end_row();
                    }
                    for s in &rep.slabs {
                        if ui.selectable_label(self.selected_slab == Some(s.slab), format!("Slab {}", s.name)).clicked() {
                            self.selected_slab = Some(s.slab);
                            self.selected.clear();
                            self.right = RightTab::Properties;
                        }
                        ui.label(format!("{:.0} mm", s.h * 1e3));
                        ui.label(ratio_text(&t, s.ratio));
                        ui.label(RichText::new(&s.governs).small());
                        ui.label(RichText::new(s.strips.iter().map(|x| format!("{} {}: {}", if x.direction.starts_with("short") { "S" } else { "L" }, x.location.split(' ').next().unwrap_or(""), x.bars)).collect::<Vec<_>>().join(" | ")).small().monospace());
                        ui.label(RichText::new(&s.method).small().color(t.text_dim));
                        ui.end_row();
                    }
                });
                if !rep.vibration.is_empty() {
                    let low: Vec<String> = rep.vibration.iter().filter(|v| !v.ok).map(|v| format!("{} ({:.1} Hz)", v.member, v.frequency)).collect();
                    let fmin = rep.vibration.iter().map(|v| v.frequency).fold(f64::INFINITY, f64::min);
                    ui.label(RichText::new(format!("Floor vibration: lowest beam frequency {fmin:.1} Hz (limit {:.1}); {}", self.model.design.vibration_hz, if low.is_empty() { "all pass".to_string() } else { format!("under: {}", low.join(", ")) })).small().color(if low.is_empty() { t.text_dim } else { t.warn }));
                }
                for s in &rep.skipped {
                    ui.label(RichText::new(s).small().color(t.text_dim));
                }
            }
            BottomTab::Reactions => {
                let Some(a) = &self.analysis else {
                    ui.label(RichText::new("Analyse to see the reactions.").color(t.text_dim));
                    return;
                };
                let Some(r) = scene::pick_result(&self.model, a, &scene::Spec { result: self.spec.result.clone(), ..Default::default() }) else { return };
                ui.label(RichText::new(format!("{} · applied {:.1} kN down, reactions {:.1} kN up", r.name, -r.applied[2], r.reaction_sum()[2])).color(t.text_dim));
                egui::Grid::new("reactions").striped(true).num_columns(4).show(ui, |ui| {
                    for h in ["Node", "At (m)", "Fx, Fy, Fz (kN)", "Mx, My, Mz (kN·m)"] {
                        ui.label(RichText::new(h).small().color(t.text_dim));
                    }
                    ui.end_row();
                    for (n, f) in &r.reactions {
                        ui.label(n.to_string());
                        let p = self.model.node(*n).map(|x| x.p()).unwrap_or_default();
                        ui.label(format!("{:.2}, {:.2}, {:.2}", p[0], p[1], p[2]));
                        ui.label(RichText::new(format!("{:.1}, {:.1}, {:.1}", f[0], f[1], f[2])).monospace());
                        ui.label(RichText::new(format!("{:.1}, {:.1}, {:.1}", f[3], f[4], f[5])).monospace());
                        ui.end_row();
                    }
                });
            }
            BottomTab::Messages => {
                if self.messages.is_empty() {
                    ui.label(RichText::new("No messages.").color(t.text_dim));
                }
                for m in &self.messages {
                    ui.label(m);
                }
            }
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if let Some(mut f) = self.frame_form.take() {
            let mut open = true;
            let mut go = false;
            egui::Window::new("Frame").open(&mut open).default_width(360.0).show(ctx, |ui| {
                egui::Grid::new("frame-form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Bays along x, m");
                    ui.text_edit_singleline(&mut f.x_spans);
                    ui.end_row();
                    ui.label("Bays along y, m");
                    ui.add(egui::TextEdit::singleline(&mut f.y_spans).hint_text("empty: a plane frame"));
                    ui.end_row();
                    ui.label("Storeys, m");
                    ui.text_edit_singleline(&mut f.storeys);
                    ui.end_row();
                    ui.label("Material");
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!f.steel, "Concrete").clicked() {
                            f.steel = false;
                            f.column = "400 x 400".into();
                            f.beam = "300 x 600".into();
                        }
                        if ui.selectable_label(f.steel, "Steel").clicked() {
                            f.steel = true;
                            f.column = "H 300x300x10x15".into();
                            f.beam = "H 400x200x8x13".into();
                        }
                    });
                    ui.end_row();
                    ui.label("Columns");
                    ui.text_edit_singleline(&mut f.column);
                    ui.end_row();
                    ui.label("Beams");
                    ui.text_edit_singleline(&mut f.beam);
                    ui.end_row();
                    ui.label("Bases");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut f.base_fixed, true, "Fixed");
                        ui.selectable_value(&mut f.base_fixed, false, "Pinned");
                    });
                    ui.end_row();
                    ui.label("Slabs");
                    ui.checkbox(&mut f.slabs, "a panel in every bay");
                    ui.end_row();
                    if f.slabs {
                        ui.label("h (m), SDL, live, roof live (kPa)");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut f.h).speed(0.005).range(0.08..=0.5));
                            ui.add(egui::DragValue::new(&mut f.dead).speed(0.1));
                            ui.add(egui::DragValue::new(&mut f.live).speed(0.1));
                            ui.add(egui::DragValue::new(&mut f.roof_live).speed(0.1));
                        });
                        ui.end_row();
                    }
                });
                if ui.add(egui::Button::new("Add frame")).clicked() {
                    go = true;
                }
            });
            if go {
                let mut a = serde_json::json!({"x_spans": parse_list(&f.x_spans), "y_spans": parse_list(&f.y_spans), "storeys": parse_list(&f.storeys), "material": if f.steel { "steel" } else { "concrete" }, "column": f.column, "beam": f.beam, "base": if f.base_fixed { "fixed" } else { "pinned" }});
                if f.slabs {
                    a["slab"] = serde_json::json!({"h": f.h, "dead": f.dead, "live": f.live, "roof_live": f.roof_live});
                }
                self.run_command("add_frame", a);
                self.view.fit = true;
            } else if open {
                self.frame_form = Some(f);
            }
        }
        if let Some(mut f) = self.member_form.take() {
            let mut open = true;
            let mut go = false;
            egui::Window::new("Member").open(&mut open).show(ctx, |ui| {
                egui::Grid::new("member-form").num_columns(4).show(ui, |ui| {
                    for (label, p) in [("From", &mut f.from), ("To", &mut f.to)] {
                        ui.label(label);
                        for v in p.iter_mut() {
                            ui.add(egui::DragValue::new(v).speed(0.1));
                        }
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Section");
                    ui.text_edit_singleline(&mut f.section);
                });
                go = ui.button("Add member").clicked();
            });
            if go {
                self.run_command("add_members", serde_json::json!({"members": [{"from": f.from, "to": f.to, "section": f.section}]}));
                self.view.fit = true;
            }
            if open && !go {
                self.member_form = Some(f);
            }
        }
        if self.loads_open {
            let mut open = true;
            egui::Window::new("Loads and combinations").open(&mut open).default_width(420.0).show(ctx, |ui| {
                let cases = self.model.cases.clone();
                ui.horizontal(|ui| {
                    ui.label("Case");
                    egui::ComboBox::from_id_salt("load-case").selected_text(self.model.case(self.load_case).map(|c| c.name.clone()).unwrap_or("?".into())).show_ui(ui, |ui| {
                        for c in &cases {
                            ui.selectable_value(&mut self.load_case, c.id, &c.name);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Uniform load on the selected members, kN/m down");
                    ui.add(egui::DragValue::new(&mut self.load_w).speed(0.5));
                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Add")).clicked() {
                        let ids = self.selected.clone();
                        self.run_command("add_loads", serde_json::json!({"case": self.load_case, "loads": [{"type": "line", "members": ids, "w": [0.0, 0.0, -self.load_w]}]}));
                    }
                });
                let n = self.model.loads.iter().filter(|l| l.case == self.load_case).count();
                ui.horizontal(|ui| {
                    ui.label(format!("{n} load(s) in this case (slab loads come from the slabs)"));
                    if n > 0 && ui.button("Delete them").clicked() {
                        self.run_command("delete", serde_json::json!({"loads_of_case": self.load_case}));
                    }
                });
                ui.separator();
                ui.label(RichText::new("Combinations").font(theme::medium_font(13.0)));
                for c in self.model.combos.clone() {
                    ui.label(format!("{} [{:?}]: {}", c.name, c.kind, c.factors.iter().map(|(id, f)| format!("{f} {}", self.model.case(*id).map(|c| c.name.clone()).unwrap_or_default())).collect::<Vec<_>>().join(" + ")));
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.combo_name).hint_text("name").desired_width(110.0));
                    ui.add(egui::TextEdit::singleline(&mut self.combo_factors).hint_text("D 1.2, L 1.6").desired_width(140.0));
                    let uls = ui.button("Add ULS").clicked();
                    let sls = ui.button("Add SLS").clicked();
                    if uls || sls {
                        let mut f = serde_json::Map::new();
                        for part in self.combo_factors.split(',') {
                            let w: Vec<&str> = part.split_whitespace().collect();
                            if w.len() == 2 {
                                if let Ok(v) = w[1].parse::<f64>() {
                                    f.insert(w[0].to_string(), serde_json::json!(v));
                                }
                            }
                        }
                        let name = if self.combo_name.is_empty() { self.combo_factors.clone() } else { self.combo_name.clone() };
                        self.run_command("add_combination", serde_json::json!({"name": name, "kind": if sls { "sls" } else { "uls" }, "factors": f}));
                    }
                });
            });
            self.loads_open = open;
        }
        if self.settings_open {
            let mut open = true;
            egui::Window::new("Materials and design settings").open(&mut open).default_width(360.0).show(ctx, |ui| {
                let mut d = self.model.design.clone();
                let mut mats = self.model.materials.clone();
                let mut set_combos = None;
                egui::Grid::new("settings").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    for m in mats.iter_mut() {
                        ui.label(&m.name);
                        match m.kind {
                            MaterialKind::Concrete => {
                                if ui.add(egui::DragValue::new(&mut m.fc).prefix("f'c ").suffix(" MPa").speed(0.5).range(15.0..=80.0)).changed() {
                                    m.e = (4700.0 * m.fc.sqrt()).round();
                                }
                            }
                            _ => {
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(&mut m.fy).prefix("Fy ").suffix(" MPa").speed(1.0));
                                    ui.add(egui::DragValue::new(&mut m.fu).prefix("Fu ").suffix(" MPa").speed(1.0));
                                });
                            }
                        }
                        ui.end_row();
                    }
                    ui.label("Main bars fy, stirrups fyt");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.fy).suffix(" MPa"));
                        ui.add(egui::DragValue::new(&mut d.fyt).suffix(" MPa"));
                    });
                    ui.end_row();
                    ui.label("Cover beams / slabs, mm");
                    ui.horizontal(|ui| {
                        let (mut cb, mut cs) = (d.cover_beam * 1e3, d.cover_slab * 1e3);
                        if ui.add(egui::DragValue::new(&mut cb).range(15.0..=75.0)).changed() {
                            d.cover_beam = cb / 1e3;
                        }
                        if ui.add(egui::DragValue::new(&mut cs).range(15.0..=75.0)).changed() {
                            d.cover_slab = cs / 1e3;
                        }
                    });
                    ui.end_row();
                    ui.label("Deflection limits L /");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.defl_live).prefix("live "));
                        ui.add(egui::DragValue::new(&mut d.defl_total).prefix("total "));
                    });
                    ui.end_row();
                    ui.label("Target ratio");
                    ui.add(egui::DragValue::new(&mut d.target_ratio).speed(0.01).range(0.5..=1.0));
                    ui.end_row();
                    ui.label("Design standard").on_hover_text("Load and strength reduction factors. Thai Ministerial Regulation on structural design B.E. 2566 (กฎกระทรวง กำหนดการออกแบบโครงสร้างอาคาร พ.ศ. 2566): U = 1.4D + 1.7L, 0.75(1.4D + 1.7L) + 1.6W or 1.0E, 0.9D + 1.6W or 1.0E; φ with quality control 0.90 flexure, 0.70 tied columns, 0.85 shear, without it 0.75, 0.60, 0.70");
                    ui.horizontal(|ui| {
                        use pondus_core::codes::DesignStandard;
                        egui::ComboBox::from_id_salt("standard").selected_text(match d.standard {
                            DesignStandard::Aci => "ACI 318-19 / ASCE 7-22",
                            DesignStandard::Mr2566 => "กฎกระทรวง 2566 (QC)",
                            DesignStandard::Mr2566NoQc => "กฎกระทรวง 2566 (no QC)",
                        }).width(170.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut d.standard, DesignStandard::Aci, "ACI 318-19 / AISC 360-16 / ASCE 7-22");
                            ui.selectable_value(&mut d.standard, DesignStandard::Mr2566, "กฎกระทรวง 2566, quality controlled");
                            ui.selectable_value(&mut d.standard, DesignStandard::Mr2566NoQc, "กฎกระทรวง 2566, no quality control");
                        });
                        if ui.button("Use its combinations").on_hover_text("Replace the model's load combinations with the standard's, from the load case kinds").clicked() {
                            set_combos = Some(d.standard);
                        }
                    });
                    ui.end_row();
                    ui.label("Steel code");
                    ui.horizontal(|ui| {
                        use pondus_core::codes::SteelCode;
                        ui.selectable_value(&mut d.steel_code, SteelCode::Aisc, "AISC 360-16");
                        ui.selectable_value(&mut d.steel_code, SteelCode::Ec3, "EN 1993-1-1");
                    });
                    ui.end_row();
                    ui.label("Steel depth for select, mm");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.steel_min_depth).speed(10.0).range(0.0..=1000.0).prefix("min "));
                        ui.add(egui::DragValue::new(&mut d.steel_max_depth).speed(10.0).range(0.0..=1000.0).prefix("max "));
                    });
                    ui.end_row();
                    ui.label("Also check beams to EC2");
                    ui.checkbox(&mut d.ec2_check, "EN 1992-1-1 with the ACI bars");
                    ui.end_row();
                    ui.label("Floor vibration limit");
                    ui.add(egui::DragValue::new(&mut d.vibration_hz).speed(0.1).range(0.0..=10.0).suffix(" Hz"));
                    ui.end_row();
                    ui.label("Shear tab bolts");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.connection.bolt_d).speed(1.0).range(12.0..=36.0).prefix("M"));
                        ui.add(egui::DragValue::new(&mut d.connection.fnv).speed(5.0).prefix("Fnv ").suffix(" MPa"));
                        ui.add(egui::DragValue::new(&mut d.connection.plate_t).speed(1.0).prefix("PL ").suffix(" mm"));
                    });
                    ui.end_row();
                    ui.label("Tension members' ends").on_hover_text("For tensile rupture: the net area An and the shear lag U (AISC D3, Table D3.1) or Anet (EC3 6.2.3) of the end connection; a member can override it (set_members tension)");
                    ui.horizontal(|ui| {
                        let tc = &mut d.tension_connection;
                        egui::ComboBox::from_id_salt("tc-kind").selected_text(tc.kind.clone()).width(70.0).show_ui(ui, |ui| {
                            for k in ["bolted", "welded", "none"] {
                                ui.selectable_value(&mut tc.kind, k.to_string(), k);
                            }
                        });
                        egui::ComboBox::from_id_salt("tc-conn").selected_text(tc.connected.clone()).width(70.0).show_ui(ui, |ui| {
                            for k in ["flanges", "web", "all"] {
                                ui.selectable_value(&mut tc.connected, k.to_string(), k);
                            }
                        });
                        ui.add(egui::DragValue::new(&mut tc.bolt_d).speed(1.0).range(10.0..=36.0).prefix("M"));
                        ui.add(egui::DragValue::new(&mut tc.holes_flange).range(0..=8).prefix("holes/flange "));
                        ui.add(egui::DragValue::new(&mut tc.holes_web).range(0..=8).prefix("web "));
                        ui.add(egui::DragValue::new(&mut tc.bolts_per_line).range(1..=12).prefix("bolts/line "));
                        ui.add(egui::DragValue::new(&mut tc.length).speed(0.01).range(0.0..=2.0).prefix("l ").suffix(" m"));
                    });
                    ui.end_row();
                    ui.label("Second order");
                    ui.checkbox(&mut d.second_order, "P-Delta for every ULS combination");
                    ui.end_row();
                    ui.label("Footings: q allow, depth");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.footing.q_allow).speed(5.0).range(20.0..=2000.0).suffix(" kPa"));
                        ui.add(egui::DragValue::new(&mut d.footing.depth).speed(0.1).range(0.3..=10.0).suffix(" m"));
                    });
                    ui.end_row();
                    ui.label("Soil γ, base friction μ");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.footing.gamma_soil).speed(0.5).range(10.0..=25.0).suffix(" kN/m³"));
                        ui.add(egui::DragValue::new(&mut d.footing.mu).speed(0.05).range(0.2..=1.0));
                    });
                    ui.end_row();
                    ui.label("Slabs");
                    ui.horizontal(|ui| {
                        use pondus_core::slabfe::SlabMethod;
                        ui.selectable_value(&mut d.slab_method, SlabMethod::Coefficients, "coefficients");
                        ui.selectable_value(&mut d.slab_method, SlabMethod::Plate, "plate FE");
                    });
                    ui.end_row();
                    if d.slab_method == pondus_core::slabfe::SlabMethod::Plate {
                        ui.label("Plate mesh, m");
                        ui.add(egui::DragValue::new(&mut d.slab_mesh).speed(0.05).range(0.1..=2.0));
                        ui.end_row();
                    }
                });
                if d != self.model.design || mats != self.model.materials {
                    self.commit();
                    self.model.design = d;
                    self.model.materials = mats;
                }
                if let Some(st) = set_combos {
                    self.commit();
                    self.model.combos = st.combinations(&self.model.cases);
                }
            });
            self.settings_open = open;
        }
    }
}

impl App {
    /// The slab element under the pointer: the pointer carried onto each shown slab's plane.
    fn plate_under<'a>(&self, picked: &crate::view::Picked, r: &'a pondus_core::analysis::CaseResult) -> Option<&'a pondus_core::slabfe::PlateResult> {
        let (sx, sy) = picked.proj?;
        let cam = scene::Camera { yaw: self.view.yaw, pitch: self.view.pitch };
        let mut best: Option<(&pondus_core::slabfe::PlateResult, f64)> = None;
        for s in &self.model.slabs {
            if self.spec.view == "plan" && self.spec.z.is_some_and(|z| (s.z - z).abs() > 0.05) {
                continue;
            }
            let Some(q) = cam.unproject_on(sx, sy, 2, s.z) else { continue };
            let (lo, hi) = (s.lo(), s.hi());
            if q[0] < lo.0 || q[0] > hi.0 || q[1] < lo.1 || q[1] > hi.1 {
                continue;
            }
            // the nearest slab to the eye wins: the largest depth along the view is the farthest
            let depth = cam.project(q).2;
            if let Some(p) = r.plates.iter().find(|p| p.slab == s.id && (q[0] - p.x).abs() <= p.dx / 2.0 + 1e-9 && (q[1] - p.y).abs() <= p.dy / 2.0 + 1e-9) {
                if best.is_none_or(|(_, d)| depth < d) {
                    best = Some((p, depth));
                }
            }
        }
        best.map(|b| b.0)
    }

    /// The colour scale of a slab diagram, in the view's top-left corner.
    fn plate_legend(&self, ui: &egui::Ui, rect: egui::Rect, r: &pondus_core::analysis::CaseResult, t: &Tokens) {
        let vmax = scene::plate_range(&self.model, r, &self.spec);
        let (name, unit) = self.spec.diagram.plate_label();
        let painter = ui.painter_at(rect);
        let o = rect.left_top() + egui::vec2(14.0, 12.0);
        let bg = egui::Rect::from_min_size(o - egui::vec2(8.0, 6.0), egui::vec2(350.0, 64.0));
        painter.rect_filled(bg, 6.0, t.card.gamma_multiply(0.92));
        painter.text(o, egui::Align2::LEFT_TOP, format!("{name} · {}", r.name), egui::FontId::proportional(11.0), t.text);
        painter.text(o + egui::vec2(0.0, 50.0), egui::Align2::LEFT_TOP, "hover a slab for its values", egui::FontId::proportional(9.5), t.text_faint);
        let (w, n) = (280.0f32, 40);
        let o = o + egui::vec2(14.0, 0.0);
        for k in 0..n {
            let v = -1.0 + 2.0 * (k as f64 + 0.5) / n as f64;
            let c = scene::diverging(v);
            let x0 = o.x + w * k as f32 / n as f32;
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, o.y + 18.0), egui::vec2(w / n as f32 + 0.5, 12.0)), 0.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
        }
        for (f, v) in [(0.0f32, -vmax), (0.5, 0.0), (1.0, vmax)] {
            painter.text(egui::pos2(o.x + w * f, o.y + 34.0), egui::Align2::CENTER_TOP, format!("{v:.1}"), egui::FontId::monospace(10.0), t.text_dim);
        }
        painter.text(egui::pos2(o.x + w + 4.0, o.y + 18.0), egui::Align2::LEFT_TOP, unit, egui::FontId::proportional(10.0), t.text_dim);
    }
}

#[cfg(test)]
impl App {
    pub fn test_frame(&mut self) {
        self.right = RightTab::Properties;
        self.run_command("add_frame", serde_json::json!({"x_spans": [6.0, 6.0, 6.0], "y_spans": [5.0, 5.0], "storeys": [4.0, 3.5, 3.5], "column": "400 x 400", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0, "roof_live": 1.0}}));
        self.view.fit = true;
    }
    pub fn test_analyze(&mut self) {
        self.analyze();
    }
    pub fn test_design(&mut self) {
        self.design();
    }
    pub fn test_select_first_beam(&mut self) {
        let b = self.model.members.iter().find(|m| self.model.kind(m) == MemberKind::Beam).map(|m| m.id).unwrap();
        self.selected = vec![b];
    }
    pub fn test_tool(&mut self, t: crate::tools::Tool) {
        self.set_tool(t);
    }
    pub fn test_plate(&mut self) {
        self.model.design.slab_method = pondus_core::slabfe::SlabMethod::Plate;
        self.design();
        self.spec.diagram = Diagram::PlateMy;
        self.bottom = BottomTab::Design;
    }
    pub fn test_plate_shear(&mut self) {
        self.spec.diagram = Diagram::PlateQx;
        self.spec.values = true;
        self.selected.clear();
        self.selected_slab = self.model.slabs.first().map(|s| s.id);
        self.right = RightTab::Properties;
    }
    /// The element the hover finds at a plan point of the first slab.
    pub fn test_plate_under(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let s = self.model.slabs.first()?;
        let cam = scene::Camera { yaw: self.view.yaw, pitch: self.view.pitch };
        let (sx, sy, _) = cam.project([x, y, s.z]);
        let picked = crate::view::Picked { member: None, node: None, plane: None, clicked: false, secondary: false, proj: Some((sx, sy)), rect: egui::Rect::NOTHING, drag: None, inside: true };
        let r = scene::pick_result(&self.model, self.analysis.as_ref()?, &self.spec)?;
        self.plate_under(&picked, &r).map(|p| (p.x, p.y))
    }
    pub fn test_seismic(&mut self) {
        self.model.design.slab_method = pondus_core::slabfe::SlabMethod::Coefficients;
        self.model.seismic.enabled = true;
        self.analyze();
        self.spec.diagram = Diagram::Mode;
        self.spec.mode = 0;
        self.animate = false;
        self.bottom = BottomTab::Seismic;
        self.seismic_open = true;
        self.spec.view = "3d".into();
        self.view.set_preset("3d");
    }
    pub fn test_phase1(&mut self) {
        self.seismic_open = false;
        self.model.seismic.enabled = false;
        self.model.design.slab_method = pondus_core::slabfe::SlabMethod::Coefficients;
        self.design();
        self.run_buckling();
        self.bottom = BottomTab::Design;
        let c = self.model.members.iter().find(|m| self.model.kind(m) == MemberKind::Column).map(|m| m.id).unwrap();
        self.selected = vec![c];
        self.right = RightTab::Properties;
    }
    pub fn test_tab(&mut self, t: BottomTab) {
        self.bottom = t;
    }
    pub fn test_view(&mut self, v: &str) {
        self.spec.view = v.into();
        self.spec.z = Some(4.0);
        self.view.set_preset(v);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        // a file macOS asked to open (double click in Finder, the Dock icon, `open f.pondus`)
        if let Some(p) = crate::macopen::take().into_iter().last() {
            self.open_path(p);
        }
        // a Pondus file opens; images and documents go to the chat
        let (models, files): (Vec<_>, Vec<_>) = dropped.into_iter().partition(|p| crate::attach::is_model(p));
        if let Some(p) = models.into_iter().next() {
            self.open_path(p);
        }
        if !files.is_empty() {
            self.right = RightTab::Agent;
            self.status = format!("{} file(s) attached to the next message to the agent", files.len());
            self.chat.attach(files);
        }
        self.shortcuts(&ctx);
        self.tool_keys(&ctx);
        if let Some((name, args)) = self.pending_command.take() {
            self.run_command(&name, args);
        }
        let t = Tokens::get(&ctx);
        let title = format!("Pondus — {}{}", self.path.as_ref().map(|p| p.display().to_string()).unwrap_or(self.model.name.clone()), if self.dirty { " •" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::Panel::top("menu").frame(egui::Frame::new().fill(t.dock).inner_margin(egui::Margin::symmetric(8, 4))).show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("ribbon").frame(egui::Frame::new().fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 5))).show(ui, |ui| self.ribbon(ui));
        egui::Panel::top("tool-options").frame(egui::Frame::new().fill(t.card).inner_margin(egui::Margin::symmetric(10, 4))).show(ui, |ui| {
            ui.horizontal(|ui| {
                self.view_pickers(ui);
                self.tool_options(ui);
            });
        });
        egui::Panel::bottom("status").frame(egui::Frame::new().fill(t.dock).inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.live.is_some() {
                    ui.label(RichText::new("Agent at work: the views follow its model (read-only)").color(t.warn));
                    ui.separator();
                }
                ui.label(RichText::new(&self.status).small().color(t.text_dim));
            });
        });
        egui::Panel::left("browser").default_size(250.0).size_range(190.0..=420.0).resizable(true).frame(egui::Frame::new().fill(t.chrome).inner_margin(10.0)).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("browser").auto_shrink([false, false]).show(ui, |ui| self.browser(ui));
        });
        let max = (ctx.content_rect().width() * 0.45).max(320.0);
        egui::Panel::right("right").default_size(400.0).size_range(300.0..=max).resizable(true).frame(egui::Frame::new().fill(t.chrome).inner_margin(10.0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.right, RightTab::Agent, "Agent");
                ui.selectable_value(&mut self.right, RightTab::Properties, "Properties");
            });
            ui.separator();
            match self.right {
                RightTab::Agent => {
                    let can_undo = !self.undo.is_empty();
                    let fx = self.chat.ui(ui, &self.model, can_undo);
                    if let Some(l) = fx.live {
                        self.live = l;
                    }
                    if let Some(m) = fx.apply {
                        self.live = None;
                        self.set_model(m);
                        self.view.fit = false;
                        self.analyze();
                        self.status = format!("The agent's changes are applied (Undo takes them back). {}", self.status);
                    }
                    if fx.undo {
                        self.undo();
                    }
                }
                RightTab::Properties => {
                    egui::ScrollArea::vertical().id_salt("props").auto_shrink([false, false]).show(ui, |ui| self.properties(ui));
                }
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.canvas)).show(ui, |ui| {
            // the design table under the view, inside the centre
            egui::Panel::bottom("design").default_size(210.0).size_range(80.0..=600.0).resizable(true).frame(egui::Frame::new().fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| self.bottom_panel(ui));
            // the creation palette down the left of the view
            if self.live.is_none() {
                egui::Panel::left("palette").exact_size(44.0).resizable(false).frame(egui::Frame::new().fill(t.chrome).inner_margin(egui::Margin::symmetric(4, 4))).show(ui, |ui| self.palette(ui));
            }
            let shown = self.live.as_ref().unwrap_or(&self.model);
            let (a, r) = if self.live.is_some() { (None, None) } else { (self.analysis.as_ref(), self.report.as_ref()) };
            let mut spec = self.spec.clone();
            spec.highlight = self.hover.into_iter().filter(|h| !self.selected.contains(h)).collect();
            let prims = scene::build(shown, a, r, &spec);
            let dropping = self.palette_drag.map(|k| crate::tools::PALETTE[k]);
            let ov = match (self.live.is_none(), dropping) {
                (false, _) => Default::default(),
                (true, Some((_, _, tool, _))) => self.overlay_for(tool, true),
                (true, None) => self.overlay(),
            };
            let picked = self.view.show(ui, shown, &prims, &self.selected, &ov, &t);
            if prims.is_empty() {
                ui.painter().text(picked.rect.center_top() + egui::vec2(0.0, 28.0), egui::Align2::CENTER_TOP, "Empty model: draw with the tools on the left (Beam, Column, Bay…), use Model › Frame…, or ask the agent on the right.", egui::FontId::proportional(13.0), t.text_dim);
            }
            // a palette item dropped on the view, and its ghost under the pointer while dragged
            if let Some((icon, _, tool, sup)) = dropping {
                if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("palette-ghost")));
                    let r = egui::Rect::from_center_size(pos + egui::vec2(14.0, 14.0), egui::vec2(26.0, 26.0));
                    painter.rect_filled(r, 6.0, t.accent_soft);
                    crate::icons::image(icon, 18.0, t.accent_text).paint_at(ui, egui::Rect::from_center_size(r.center(), egui::vec2(18.0, 18.0)));
                }
                if ui.input(|i| i.pointer.any_released()) {
                    self.palette_drag = None;
                    if picked.inside {
                        self.tool_drop(tool, sup, &picked);
                    }
                }
            }
            if let (Some(d), true) = (picked.drag, self.live.is_none()) {
                let copy = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
                self.tool_drag(&d, copy);
            }
            self.hover = picked.member;
            if picked.secondary && self.opts.from.is_some() {
                self.opts.from = None;
            }
            let used = self.live.is_none() && self.tool_click(&picked);
            if picked.clicked && self.live.is_none() && !used {
                let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                match picked.member {
                    Some(m) if add => {
                        if let Some(k) = self.selected.iter().position(|x| *x == m) {
                            self.selected.remove(k);
                        } else {
                            self.selected.push(m);
                        }
                    }
                    Some(m) => {
                        self.selected = vec![m];
                        self.selected_slab = None;
                        self.right = RightTab::Properties;
                    }
                    None => self.selected.clear(),
                }
            }
            // slab results: the legend, and the element under the pointer
            if self.live.is_none() && self.spec.diagram.is_plate() {
                if let Some(r) = self.analysis.as_ref().and_then(|a| scene::pick_result(&self.model, a, &self.spec)) {
                    self.plate_legend(ui, picked.rect, &r, &t);
                    if let Some(p) = self.plate_under(&picked, &r) {
                        let (bx, by, tx, ty) = pondus_core::slabfe::wood_armer(p.mx, p.my, p.mxy);
                        let name = self.model.slabs.iter().find(|s| s.id == p.slab).map(|s| s.name.clone()).unwrap_or_default();
                        egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::new("plate-tip"), egui::PopupAnchor::Pointer).show(|ui| {
                            ui.label(RichText::new(format!("Slab {name} · element at ({:.2}, {:.2}) · {}", p.x, p.y, r.name)).strong());
                            egui::Grid::new("plate-tip-grid").num_columns(2).show(ui, |ui| {
                                for (k, v, u) in [("mx", p.mx, "kN·m/m"), ("my", p.my, "kN·m/m"), ("mxy", p.mxy, "kN·m/m"), ("qx", p.qx, "kN/m"), ("qy", p.qy, "kN/m")] {
                                    ui.label(k);
                                    ui.label(RichText::new(format!("{v:8.2} {u}")).monospace());
                                    ui.end_row();
                                }
                                ui.label("Wood–Armer");
                                ui.label(RichText::new(format!("bottom x {bx:.1}, y {by:.1} · top x {tx:.1}, y {ty:.1}")).monospace());
                                ui.end_row();
                                ui.label("deflection");
                                ui.label(RichText::new(format!("{:.2} mm", -p.uz * 1000.0)).monospace());
                                ui.end_row();
                            });
                        });
                    }
                }
            } else if let (Some(m), None) = (picked.member, picked.node) {
                let s = self.model.member(m).and_then(|x| self.model.section(x.section)).map(|s| s.name.clone()).unwrap_or_default();
                let r = self.report.as_ref().and_then(|r| scene::member_ratio(r, m)).map(|r| format!(" · ratio {r:.2}")).unwrap_or_default();
                egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::new("hover-tip"), egui::PopupAnchor::Pointer).show(|ui| {
                    ui.label(format!("Member {m} · {s}{r}"));
                });
            }
        });
        self.windows(&ctx);
        self.seismic_window(&ctx);
        self.time_history_window(&ctx);
        self.advanced_window(&ctx);
        self.section_window(&ctx);
        self.edit_window(&ctx);
        self.about_window(&ctx);
        // the mode shape swings while it is shown
        if self.spec.diagram == Diagram::Mode && self.animate {
            let t = ctx.input(|i| i.time);
            self.spec.phase = Some((t * 2.0 * std::f64::consts::PI * 0.7).sin());
            ctx.request_repaint();
        } else {
            self.spec.phase = None;
        }
    }
}
