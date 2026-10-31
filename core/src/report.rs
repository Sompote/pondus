// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The calculation report as a PDF (A4): the basis of design, the model, the loads and their
//! balance, the analysis extremes, the seismic analysis, the design tables of every member
//! type, the full calculation of the governing member of each type, and the warnings. The
//! text is set in Inter (Noto Sans Thai for Thai), embedded and subset by krilla, so the code
//! symbols (φ, ≤, √, ²) print as they read on screen. Pictures come from the same rasteriser as
//! the agent's `show_picture`.
//!
//! The layout is a simple flow: headings, paragraphs and tables with wrapped cells, which
//! break across pages with their header row repeated.

use std::collections::HashMap;
use std::path::Path;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Size, Transform};
use krilla::image::Image as PdfImage;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};
use krilla::Document;
use serde::{Deserialize, Serialize};

use crate::analysis::Analysis;
use crate::design::Report;
use crate::model::*;
use crate::scene;

static INTER: &[u8] = include_bytes!("../../app/assets/fonts/Inter-Regular.ttf");
static INTER_SEMI: &[u8] = include_bytes!("../../app/assets/fonts/Inter-SemiBold.ttf");
static THAI: &[u8] = include_bytes!("../../app/assets/fonts/NotoSansThai-Regular.ttf");

const W: f32 = 595.0;
const H: f32 = 842.0;
const MARGIN: f32 = 48.0;
const TOP: f32 = 64.0;
const BOTTOM: f32 = 790.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportOptions {
    pub project: String,
    pub engineer: String,
    /// "governing" (default): the full calculation of the governing member of each type;
    /// "all": of every member.
    pub detail: String,
    pub pictures: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReportSummary {
    pub path: String,
    pub pages: usize,
    pub sections: Vec<String>,
}

// ------------------------------------------------------------------ layout

#[derive(Clone, Copy, PartialEq)]
enum Face {
    Regular,
    Semi,
}

#[derive(Clone)]
enum Op {
    Text { x: f32, y: f32, size: f32, face: Face, color: [u8; 3], s: String },
    Line { a: [f32; 2], b: [f32; 2], width: f32, color: [u8; 3] },
    Rect { x: f32, y: f32, w: f32, h: f32, color: [u8; 3] },
    Image { x: f32, y: f32, w: f32, h: f32, png: std::sync::Arc<Vec<u8>> },
}

struct Metrics {
    faces: Vec<(Face, ttf_parser::Face<'static>)>,
    thai: ttf_parser::Face<'static>,
}

impl Metrics {
    fn new() -> Self {
        Metrics {
            faces: vec![(Face::Regular, ttf_parser::Face::parse(INTER, 0).unwrap()), (Face::Semi, ttf_parser::Face::parse(INTER_SEMI, 0).unwrap())],
            thai: ttf_parser::Face::parse(THAI, 0).unwrap(),
        }
    }
    fn face(&self, f: Face) -> &ttf_parser::Face<'static> {
        &self.faces.iter().find(|x| x.0 == f).unwrap().1
    }
    fn width(&self, s: &str, size: f32, f: Face) -> f32 {
        let face = self.face(f);
        let upm = face.units_per_em() as f32;
        s.chars()
            .map(|c| match face.glyph_index(c) {
                Some(g) => face.glyph_hor_advance(g).unwrap_or(0) as f32 / upm,
                None => self.thai.glyph_index(c).and_then(|g| self.thai.glyph_hor_advance(g)).map(|a| a as f32 / self.thai.units_per_em() as f32).unwrap_or(0.5),
            })
            .sum::<f32>()
            * size
    }
    /// Greedy word wrap to `w` points.
    fn wrap(&self, s: &str, size: f32, f: Face, w: f32) -> Vec<String> {
        let mut out = vec![];
        for para in s.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if self.width(&cand, size, f) <= w || line.is_empty() {
                    line = cand;
                    // a single word wider than the column: break it by characters
                    while self.width(&line, size, f) > w && line.chars().count() > 1 {
                        let mut cut = String::new();
                        for c in line.chars() {
                            if self.width(&format!("{cut}{c}"), size, f) > w && !cut.is_empty() {
                                break;
                            }
                            cut.push(c);
                        }
                        let rest: String = line.chars().skip(cut.chars().count()).collect();
                        out.push(cut);
                        line = rest;
                    }
                } else {
                    out.push(std::mem::take(&mut line));
                    line = word.to_string();
                }
            }
            out.push(line);
        }
        out
    }
}

struct Flow {
    pages: Vec<Vec<Op>>,
    y: f32,
    m: Metrics,
    sections: Vec<String>,
}

const INK: [u8; 3] = [24, 28, 36];
const DIM: [u8; 3] = [96, 104, 118];
const RULE: [u8; 3] = [200, 205, 214];
const BAND: [u8; 3] = [238, 241, 246];
const ACCENT: [u8; 3] = [38, 99, 210];
const BAD: [u8; 3] = [196, 40, 40];

impl Flow {
    fn new() -> Self {
        Flow { pages: vec![vec![]], y: TOP, m: Metrics::new(), sections: vec![] }
    }
    fn ops(&mut self) -> &mut Vec<Op> {
        self.pages.last_mut().unwrap()
    }
    fn new_page(&mut self) {
        self.pages.push(vec![]);
        self.y = TOP;
    }
    fn need(&mut self, h: f32) {
        if self.y + h > BOTTOM {
            self.new_page();
        }
    }
    fn text(&mut self, x: f32, y: f32, size: f32, face: Face, color: [u8; 3], s: &str) {
        self.ops().push(Op::Text { x, y, size, face, color, s: s.into() });
    }
    fn heading(&mut self, s: &str) {
        self.need(60.0);
        self.y += 10.0;
        self.sections.push(s.into());
        let y = self.y;
        self.text(MARGIN, y + 13.0, 14.0, Face::Semi, INK, s);
        self.ops().push(Op::Line { a: [MARGIN, y + 19.0], b: [W - MARGIN, y + 19.0], width: 0.8, color: ACCENT });
        self.y += 28.0;
    }
    fn sub(&mut self, s: &str) {
        self.need(40.0);
        self.y += 4.0;
        let y = self.y;
        self.text(MARGIN, y + 10.0, 10.5, Face::Semi, INK, s);
        self.y += 16.0;
    }
    fn para(&mut self, s: &str) {
        self.para_with(s, 9.0, DIM_OR_INK);
    }
    fn note(&mut self, s: &str) {
        self.para_with(s, 7.8, DIM);
    }
    fn para_with(&mut self, s: &str, size: f32, color: [u8; 3]) {
        let lines = self.m.wrap(s, size, Face::Regular, W - 2.0 * MARGIN);
        let lh = size * 1.42;
        for l in lines {
            self.need(lh);
            let y = self.y;
            self.text(MARGIN, y + size, size, Face::Regular, color, &l);
            self.y += lh;
        }
        self.y += size * 0.5;
    }
    /// A table: column width shares, the header, the rows; a row flagged true prints red.
    fn table(&mut self, widths: &[f32], head: &[&str], rows: &[(Vec<String>, bool)]) {
        let total: f32 = widths.iter().sum();
        let ws: Vec<f32> = widths.iter().map(|w| w / total * (W - 2.0 * MARGIN)).collect();
        let size = 7.6;
        let lh = size * 1.32;
        let pad = 3.0;
        let head_lines: Vec<Vec<String>> = head.iter().zip(&ws).map(|(h, w)| self.m.wrap(h, size, Face::Semi, w - 2.0 * pad)).collect();
        let hh = head_lines.iter().map(|l| l.len()).max().unwrap_or(1) as f32 * lh + 2.0 * pad;
        let draw_head = |f: &mut Flow| {
            let y = f.y;
            f.ops().push(Op::Rect { x: MARGIN, y, w: W - 2.0 * MARGIN, h: hh, color: BAND });
            let mut x = MARGIN;
            for (k, lines) in head_lines.iter().enumerate() {
                for (i, l) in lines.iter().enumerate() {
                    f.text(x + pad, y + pad + size + i as f32 * lh, size, Face::Semi, INK, l);
                }
                x += ws[k];
            }
            f.y += hh;
        };
        self.need(hh + lh * 2.0 + 2.0 * pad);
        draw_head(self);
        for (cells, bad) in rows {
            let lines: Vec<Vec<String>> = cells.iter().zip(&ws).map(|(c, w)| self.m.wrap(c, size, Face::Regular, w - 2.0 * pad)).collect();
            let rh = lines.iter().map(|l| l.len()).max().unwrap_or(1) as f32 * lh + 2.0 * pad;
            if self.y + rh > BOTTOM {
                self.new_page();
                draw_head(self);
            }
            let y = self.y;
            let mut x = MARGIN;
            for (k, ls) in lines.iter().enumerate() {
                for (i, l) in ls.iter().enumerate() {
                    self.text(x + pad, y + pad + size + i as f32 * lh, size, Face::Regular, if *bad { BAD } else { INK }, l);
                }
                x += ws[k];
            }
            self.ops().push(Op::Line { a: [MARGIN, y + rh], b: [W - MARGIN, y + rh], width: 0.3, color: RULE });
            self.y += rh;
        }
        self.y += 8.0;
    }
    fn image(&mut self, png: Vec<u8>, w: usize, h: usize, caption: &str) {
        let dw = W - 2.0 * MARGIN;
        let dh = dw * h as f32 / w as f32;
        self.need(dh + 24.0);
        let y = self.y;
        self.ops().push(Op::Image { x: MARGIN, y, w: dw, h: dh, png: std::sync::Arc::new(png) });
        self.y += dh + 4.0;
        self.note(caption);
    }
}

const DIM_OR_INK: [u8; 3] = INK;

fn fill(c: [u8; 3]) -> Fill {
    Fill { paint: rgb::Color::new(c[0], c[1], c[2]).into(), ..Default::default() }
}

/// Splits a string into runs that Inter can set and runs that need the Thai face.
fn runs(s: &str, inter: &ttf_parser::Face) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = vec![];
    for c in s.chars() {
        let thai = inter.glyph_index(c).is_none() && ('\u{0E00}'..='\u{0E7F}').contains(&c);
        match out.last_mut() {
            Some((t, r)) if *t == thai => r.push(c),
            _ => out.push((thai, c.to_string())),
        }
    }
    out
}

fn render(pages: &[Vec<Op>], header: &str, footer: &str, title: &str) -> Result<Vec<u8>, String> {
    let mut doc = Document::new();
    doc.set_metadata(krilla::metadata::Metadata::new().title(title.into()).creator("Pondus".into()));
    let reg = Font::new(INTER.into(), 0).ok_or("font")?;
    let semi = Font::new(INTER_SEMI.into(), 0).ok_or("font")?;
    let thai = Font::new(THAI.into(), 0).ok_or("font")?;
    let m = Metrics::new();
    let n = pages.len();
    for (k, ops) in pages.iter().enumerate() {
        let mut page = doc.start_page_with(PageSettings::from_wh(W, H).ok_or("page")?);
        let mut s = page.surface();
        let text = |s: &mut krilla::surface::Surface, x: f32, y: f32, size: f32, face: Face, color: [u8; 3], t: &str| {
            s.set_fill(Some(fill(color)));
            let mut cx = x;
            for (is_thai, r) in runs(t, m.face(face)) {
                let font = if is_thai { thai.clone() } else if face == Face::Semi { semi.clone() } else { reg.clone() };
                s.draw_text(Point::from_xy(cx, y), font, size, &r, false, TextDirection::Auto);
                cx += m.width(&r, size, face);
            }
        };
        // the running header and footer
        text(&mut s, MARGIN, 36.0, 8.0, Face::Semi, DIM, header);
        let pg = format!("page {} of {n}", k + 1);
        let pw = m.width(&pg, 8.0, Face::Regular);
        text(&mut s, W - MARGIN - pw, 36.0, 8.0, Face::Regular, DIM, &pg);
        text(&mut s, MARGIN, 816.0, 7.0, Face::Regular, DIM, footer);
        for op in ops {
            match op {
                Op::Text { x, y, size, face, color, s: t } => text(&mut s, *x, *y, *size, *face, *color, t),
                Op::Rect { x, y, w, h, color } => {
                    let mut pb = PathBuilder::new();
                    pb.move_to(*x, *y);
                    pb.line_to(x + w, *y);
                    pb.line_to(x + w, y + h);
                    pb.line_to(*x, y + h);
                    pb.close();
                    if let Some(p) = pb.finish() {
                        s.set_stroke(None);
                        s.set_fill(Some(fill(*color)));
                        s.draw_path(&p);
                    }
                }
                Op::Line { a, b, width, color } => {
                    let mut pb = PathBuilder::new();
                    pb.move_to(a[0], a[1]);
                    pb.line_to(b[0], b[1]);
                    if let Some(p) = pb.finish() {
                        s.set_fill(None);
                        s.set_stroke(Some(Stroke { paint: rgb::Color::new(color[0], color[1], color[2]).into(), width: *width, ..Default::default() }));
                        s.draw_path(&p);
                        s.set_stroke(None);
                    }
                }
                Op::Image { x, y, w, h, png } => {
                    let img = PdfImage::from_png(png.clone().into(), true)?;
                    s.push_transform(&Transform::from_translate(*x, *y));
                    s.draw_image(img, Size::from_wh(*w, *h).ok_or("image size")?);
                    s.pop();
                }
            }
        }
        s.finish();
        page.finish();
    }
    doc.finish().map_err(|e| format!("pdf: {e:?}"))
}

// ------------------------------------------------------------------ content

fn f(x: f64, d: usize) -> String {
    format!("{x:.d$}")
}

fn ok(b: bool) -> String {
    if b { "OK".into() } else { "FAILS".into() }
}

fn picture(model: &Model, a: Option<&Analysis>, rep: Option<&Report>, diagram: scene::Diagram, result: &str) -> Option<(Vec<u8>, usize, usize)> {
    let spec = scene::Spec { diagram, result: result.into(), ..Default::default() };
    let prims = scene::build(model, a, rep, &spec);
    let (w, h) = (1400, 860);
    let img = crate::picture::render(&prims, &scene::Camera::for_view(&spec), w, h);
    let mut buf = vec![];
    {
        let mut enc = png::Encoder::new(&mut buf, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(&img.rgba).ok()?;
    }
    Some((buf, w, h))
}

fn codes(model: &Model) -> String {
    let steel = if model.design.steel_code == crate::codes::SteelCode::Ec3 { "EN 1993-1-1 (Eurocode 3)" } else { "AISC 360-16 (LRFD)" };
    let mut s = format!("Reinforced concrete is designed to ACI 318-19 and structural steel to {steel}");
    if model.design.standard.is_thai() {
        let f = model.design.standard.phi();
        s = format!(
            "The load factors and strength reduction factors follow the Thai Ministerial Regulation on structural design B.E. 2566 ({}), so the factored loads are 1.4D + 1.7L, 0.75(1.4D + 1.7L) + 1.6W, 0.9D + 1.6W, 0.75(1.4D + 1.7L) ± 1.0E and 0.9D ± 1.0E, and φ is {:.2} for flexure, {:.2} for tied columns and {:.2} for shear and torsion. {s}",
            if model.design.standard == crate::codes::DesignStandard::Mr2566 { "construction standards and material quality control specified" } else { "no construction standards or material quality control specified" },
            f.flexure, f.tied, f.shear
        );
    }
    if model.design.ec2_check {
        s.push_str(", with an EN 1992-1-1 cross-check of the concrete beams");
    }
    if model.seismic.enabled {
        s.push_str(", and the earthquake actions follow ASCE 7-22 and DPT 1301/1302-61");
    }
    s.push_str(". Timber members follow the NDS (ASD), masonry TMS 402 and cold-formed steel AISI S100 where the model has them.");
    s
}

/// Writes the calculation report of the model, its analysis and (when given) its design.
pub fn write_pdf(model: &Model, a: &Analysis, rep: Option<&Report>, opts: &ReportOptions, path: &Path) -> Result<ReportSummary, String> {
    let mut fl = Flow::new();
    let title = if opts.project.is_empty() { model.name.clone() } else { opts.project.clone() };
    let date = today();

    // ---- title block
    fl.text(MARGIN, fl.y + 22.0, 22.0, Face::Semi, INK, "Structural calculation report");
    fl.y += 34.0;
    fl.text(MARGIN, fl.y + 14.0, 13.0, Face::Regular, ACCENT, &title);
    fl.y += 26.0;
    let mut meta = vec![(vec!["Model".into(), model.name.clone()], false), (vec!["Date".into(), date.clone()], false), (vec!["Software".into(), format!("Pondus {} with the Stabileo (dedaliano) engine", env!("CARGO_PKG_VERSION"))], false)];
    if !opts.engineer.is_empty() {
        meta.push((vec!["Engineer".into(), opts.engineer.clone()], false));
    }
    fl.table(&[1.0, 4.0], &["Item", ""], &meta);

    // ---- summary
    fl.heading("1  Summary");
    let frame: Vec<&Member> = model.members.iter().filter(|m| m.role.is_frame()).collect();
    let n_cols = frame.iter().filter(|m| model.kind(m) == MemberKind::Column).count();
    let n_beams = frame.iter().filter(|m| model.kind(m) == MemberKind::Beam).count();
    let mut s = format!(
        "The model has {} nodes and {} frame members ({} columns, {} beams and {} other members), {} structural walls, {} slabs, {} supports, {} load cases and {} combinations. ",
        model.nodes.len(), frame.len(), n_cols, n_beams, frame.len() - n_cols - n_beams, model.walls.len(), model.slabs.len(), model.supports.len(), model.cases.len(), model.combos.len()
    );
    if !model.walls.is_empty() {
        s.push_str("Each wall is analysed by the wide-column analogy: a pier on its centre line with the wall's section, including shear deformation, and rigid arms at every level out to its ends. ");
    }
    match rep {
        Some(r) => {
            let fails = r.failing();
            let worst = all_ratios(r).into_iter().fold((String::new(), 0.0f64), |b, x| if x.1 > b.1 { x } else { b });
            s.push_str(&format!(
                "The design covers {} beams, {} columns, {} wall storeys, {} slabs, {} footings, {} steel members and {} members of other materials, and the largest design ratio is {:.2} ({}). ",
                r.beams.len(), r.columns.len(), r.walls.len(), r.slabs.len(), r.footings.len(), r.steel.len(), r.others.len(), worst.1, worst.0
            ));
            if fails.is_empty() {
                s.push_str("Every designed element passes its checks at the target ratio.");
            } else {
                s.push_str(&format!("{} elements fail and are listed in red in the tables: {}.", fails.len(), fails.join("; ")));
            }
        }
        None => s.push_str("No design has been run, so this report covers the analysis only."),
    }
    fl.para(&s);
    fl.para(&codes(model));

    // ---- basis of design
    fl.heading("2  Basis of design");
    fl.sub("Materials");
    let mats: Vec<(Vec<String>, bool)> = model
        .materials
        .iter()
        .map(|m| (vec![m.id.to_string(), m.name.clone(), format!("{:?}", m.kind), f(m.e, 0), f(m.gamma, 1), if m.fc > 0.0 { f(m.fc, 1) } else { "—".into() }, if m.fy > 0.0 { f(m.fy, 0) } else { "—".into() }, if m.fu > 0.0 { f(m.fu, 0) } else { "—".into() }], false))
        .collect();
    fl.table(&[0.5, 2.0, 1.0, 1.1, 0.9, 0.9, 0.9, 0.9], &["#", "Material", "Kind", "E (MPa)", "γ (kN/m³)", "f'c (MPa)", "fy (MPa)", "fu (MPa)"], &mats);
    let d = &model.design;
    fl.sub("Design settings");
    fl.table(
        &[2.0, 3.0],
        &["Setting", "Value"],
        &[
            (vec!["Main bars fy / stirrups fyt".into(), format!("{:.0} / {:.0} MPa", d.fy, d.fyt)], false),
            (vec!["Deflection limits (live / total)".into(), format!("L/{:.0} / L/{:.0}", d.defl_live, d.defl_total)], false),
            (vec!["Target design ratio".into(), f(d.target_ratio, 2)], false),
            (vec!["Slab method".into(), format!("{:?}", d.slab_method)], false),
            (vec!["Second-order analysis (P-Δ) of ULS combinations".into(), if d.second_order { "yes".into() } else { "no".into() }], false),
        ],
    );

    // ---- model
    fl.heading("3  Model");
    let mut levels: Vec<f64> = model.nodes.iter().map(|n| (n.z * 1000.0).round() / 1000.0).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    fl.para(&format!(
        "The structure is a three-dimensional frame analysed by the direct stiffness method with the members split at every node, slab mesh point and load break, so the member forces below are exact for the frame idealisation. It has {} levels at z = {} m.",
        levels.len(),
        levels.iter().map(|z| f(*z, 2)).collect::<Vec<_>>().join(", ")
    ));
    if opts.pictures {
        if let Some((png, w, h)) = picture(model, Some(a), rep, scene::Diagram::None, "") {
            fl.image(png, w, h, "Figure 1. The model in 3D: columns, beams, braces and slabs.");
        }
    }
    fl.sub("Sections");
    let mut srows = vec![];
    for s in &model.sections {
        let p = s.props();
        let n = model.members.iter().filter(|m| m.section == s.id).count();
        srows.push((vec![s.name.clone(), n.to_string(), format!("{:.4e}", p.a), format!("{:.4e}", p.iy), format!("{:.4e}", p.iz), format!("{:.4e}", p.j), format!("{:.4e}", p.zy)], false));
    }
    fl.table(&[2.2, 0.8, 1.2, 1.2, 1.2, 1.2, 1.2], &["Section", "Members", "A (m²)", "Iy (m⁴)", "Iz (m⁴)", "J (m⁴)", "Zy (m³)"], &srows);
    fl.sub("Supports");
    let mut kinds: HashMap<String, usize> = HashMap::new();
    for s in &model.supports {
        *kinds.entry(s.kind().to_string()).or_default() += 1;
    }
    let mut kv: Vec<(String, usize)> = kinds.into_iter().collect();
    kv.sort();
    fl.para(&format!("The supports are {}.", kv.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", ")));

    // ---- loads
    fl.heading("4  Loads and their balance");
    fl.para("Each load case is solved alone, and the sum of the support reactions is compared with the applied load, slab loads and self-weight included, which checks that every load reached the supports.");
    let mut lrows = vec![];
    for c in &model.cases {
        if let Some(cr) = a.case(c.id) {
            let rs = cr.reaction_sum();
            let bal = (0..3).all(|k| (cr.applied[k] + rs[k]).abs() <= 1e-6 * (1.0 + cr.applied.iter().map(|x| x.abs()).fold(0.0, f64::max)));
            lrows.push((vec![c.name.clone(), format!("{:?}", c.kind), if c.self_weight { "yes".into() } else { "no".into() }, f(cr.applied[2], 2), f(rs[2], 2), f(cr.applied[0], 2), f(cr.applied[1], 2), if bal { "yes".into() } else { "NO".into() }], !bal));
        }
    }
    fl.table(&[1.6, 0.7, 0.8, 1.2, 1.2, 1.0, 1.0, 0.8], &["Case", "Kind", "Self-weight", "Applied Fz (kN)", "Σ reactions Fz (kN)", "Applied Fx", "Applied Fy", "Balanced"], &lrows);
    let names: HashMap<Id, String> = model.cases.iter().map(|c| (c.id, c.name.clone())).collect();
    let crows: Vec<(Vec<String>, bool)> = model
        .combos
        .iter()
        .map(|c| (vec![c.name.clone(), format!("{:?}", c.kind), c.factors.iter().map(|(id, x)| format!("{x} {}", names.get(id).cloned().unwrap_or_default())).collect::<Vec<_>>().join(" + ")], false))
        .collect();
    fl.sub("Combinations");
    fl.table(&[1.6, 0.6, 4.0], &["Combination", "Kind", "Factors"], &crows);

    // ---- analysis
    fl.heading("5  Analysis results");
    let mut arows = vec![];
    for c in &model.combos {
        let cr = a.combine(&c.name, &c.factors);
        let (mut my_max, mut my_min, mut v_max, mut n_min) = ((0, 0.0f64), (0, 0.0f64), (0, 0.0f64), (0, 0.0f64));
        for mr in &cr.members {
            for st in &mr.stations {
                if st.my > my_max.1 {
                    my_max = (mr.member, st.my);
                }
                if st.my < my_min.1 {
                    my_min = (mr.member, st.my);
                }
                if st.vz.abs() > v_max.1 {
                    v_max = (mr.member, st.vz.abs());
                }
                if st.n < n_min.1 {
                    n_min = (mr.member, st.n);
                }
            }
        }
        let dmax = cr.displacements.values().map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()).fold(0.0, f64::max);
        let mm = |x: (Id, f64)| format!("{} (m {})", f(x.1, 1), x.0);
        arows.push((vec![c.name.clone(), mm(my_max), mm(my_min), mm(v_max), mm(n_min), f(dmax * 1000.0, 2)], false));
    }
    fl.para("The extremes of each combination are listed with the member where they occur. Moments are positive in sagging, axial forces positive in tension, and the displacement is the largest node movement.");
    fl.table(&[1.5, 1.3, 1.3, 1.3, 1.3, 0.9], &["Combination", "My max (kN·m)", "My min (kN·m)", "|Vz| max (kN)", "N min (kN)", "δ max (mm)"], &arows);
    if opts.pictures {
        if let Some(c) = model.combos.iter().find(|c| c.kind == ComboKind::Uls) {
            if let Some((png, w, h)) = picture(model, Some(a), rep, scene::Diagram::Moment, &c.name) {
                fl.image(png, w, h, &format!("Figure 2. Bending moment My under '{}', drawn on the tension side.", c.name));
            }
        }
    }
    for (id, (_, iters, amp, conv)) in &a.second {
        if let Some(c) = model.combos.iter().find(|c| c.id == *id) {
            fl.note(&format!("Second-order (P-Δ) analysis of '{}': {} iterations, amplification {:.3}{}.", c.name, iters, amp, if *conv { "" } else { ", not converged" }));
        }
    }
    if !a.warnings.is_empty() {
        fl.sub("Analysis warnings");
        for w in &a.warnings {
            fl.note(&format!("• {w}"));
        }
    }

    // ---- seismic
    if let Some(sr) = &a.seismic {
        fl.heading("6  Earthquake analysis");
        let sc = &model.seismic;
        fl.para(&format!(
            "The seismic weight is {:.1} kN ({:.2} t of mass), taken as the dead load plus {:.0} % of the live load. The modal response spectrum analysis uses R = {}, Ie = {}, Cd = {} and {:.0} % damping, combines the modes by CQC and scales the base shear to {:.0} % of the equivalent lateral force base shear.",
            sr.weight, sr.mass, sc.live_mass * 100.0, sc.r, sc.ie, sc.cd, sc.damping * 100.0, sc.elf_ratio * 100.0
        ));
        let mut run = [0.0; 3];
        let mrows: Vec<(Vec<String>, bool)> = sr
            .modes
            .iter()
            .enumerate()
            .take(12)
            .map(|(k, m)| {
                for q in 0..3 {
                    run[q] += m.mass_ratio[q];
                }
                (vec![(k + 1).to_string(), f(m.period, 3), f(m.frequency, 2), f(m.mass_ratio[0], 3), f(m.mass_ratio[1], 3), f(run[0], 3), f(run[1], 3)], false)
            })
            .collect();
        fl.table(&[0.6, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], &["Mode", "T (s)", "f (Hz)", "mass x", "mass y", "Σ x", "Σ y"], &mrows);
        let drows: Vec<(Vec<String>, bool)> = sr.dirs.iter().map(|d| (vec![d.name.clone(), f(d.t1, 3), f(d.t_used, 3), f(d.cs, 4), f(d.v_elf, 1), f(d.v_rsa, 1), f(d.scale, 3)], false)).collect();
        fl.table(&[0.8, 1.0, 1.0, 0.9, 1.1, 1.1, 0.8], &["Direction", "T1 (s)", "T used (s)", "Cs", "V ELF (kN)", "V RSA (kN)", "Scale"], &drows);
        let mut dr = vec![];
        for d in &sr.dirs {
            for x in &d.drifts {
                dr.push((vec![d.name.clone(), format!("{:.2} – {:.2}", x.z0, x.z1), f(x.delta_e * 1e3, 2), f(x.delta * 1e3, 2), f(x.allowed * 1e3, 1), f(x.ratio, 3)], x.ratio > 1.0));
            }
        }
        fl.sub("Storey drifts (Cd·δe/Ie)");
        fl.table(&[0.8, 1.2, 1.0, 1.0, 1.0, 0.8], &["Direction", "Storey z (m)", "δe (mm)", "δ (mm)", "Allowed (mm)", "Ratio"], &dr);
        for w in &sr.warnings {
            fl.note(&format!("• {w}"));
        }
    }

    // ---- design
    if let Some(r) = rep {
        let n = if a.seismic.is_some() { 7 } else { 6 };
        fl.heading(&format!("{n}  Design"));
        if opts.pictures {
            if let Some((png, w, h)) = picture(model, Some(a), rep, scene::Diagram::Utilisation, "") {
                fl.image(png, w, h, "Figure 3. Design ratios: green below 0.7, yellow below 0.9, orange below 1.0 and red over 1.0.");
            }
        }
        design_tables(&mut fl, model, r);
        details(&mut fl, model, r, opts.detail == "all");
        if !r.changes.is_empty() || !r.warnings.is_empty() || !r.skipped.is_empty() {
            fl.heading(&format!("{}  Changes and warnings", n + 1));
            if !r.changes.is_empty() {
                fl.para(&format!("The design resized members over {} rounds, and the analysis was repeated after each round so that the forces match the final sizes.", r.rounds));
                for c in &r.changes {
                    fl.note(&format!("• {c}"));
                }
            }
            for w in r.warnings.iter().chain(&r.skipped) {
                fl.note(&format!("• {w}"));
            }
        }
    }

    let header = format!("Pondus · {title}");
    let footer = format!("{} · Stabileo engine · {date} · Check the input and the results before you rely on them.", crate::about::short());
    let pdf = render(&fl.pages, &header, &footer, &title)?;
    std::fs::write(path, pdf).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(ReportSummary { path: path.display().to_string(), pages: fl.pages.len(), sections: fl.sections })
}

fn all_ratios(r: &Report) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = vec![];
    v.extend(r.beams.iter().map(|b| (format!("beam {}: {}", b.member, b.governs), b.ratio)));
    v.extend(r.columns.iter().map(|b| (format!("column {}: {}", b.member, b.governs), b.ratio)));
    v.extend(r.slabs.iter().map(|b| (format!("slab {}: {}", b.name, b.governs), b.ratio)));
    v.extend(r.footings.iter().map(|b| (format!("footing at node {}: {}", b.node, b.governs), b.ratio)));
    v.extend(r.steel.iter().map(|b| (format!("steel member {}: {}", b.member, b.governs), b.ratio)));
    v.extend(r.others.iter().map(|b| (format!("member {}: {}", b.member, b.governs), b.ratio)));
    v.extend(r.walls.iter().map(|b| (format!("wall {} at z {:.1}: {}", b.name, b.z0, b.governs), b.ratio)));
    v.into_iter().filter(|x| x.1.is_finite()).collect()
}

fn zone_bars(b: &crate::rc::BeamDesign, face: &str) -> String {
    ["left", "middle", "right"].iter().map(|z| b.flexure.iter().find(|f| f.face == face && f.zone == *z).map(|f| f.bars.clone()).unwrap_or_else(|| "—".into())).collect::<Vec<_>>().join(" / ")
}

fn design_tables(fl: &mut Flow, model: &Model, r: &Report) {
    if !r.beams.is_empty() {
        fl.sub("Concrete beams (ACI 318-19)");
        let rows: Vec<(Vec<String>, bool)> = r
            .beams
            .iter()
            .map(|b| {
                let st = b.shear.iter().map(|s| s.stirrups.clone()).collect::<Vec<_>>().join(" / ");
                (vec![b.member.to_string(), format!("{:.0} x {:.0}", b.b * 1e3, b.h * 1e3), f(b.span, 2), zone_bars(b, "top"), zone_bars(b, "bottom"), st, format!("{:.1} / {:.1}", b.defl_total, b.limit_total), f(b.ratio, 2), b.governs.clone()], !b.ok)
            })
            .collect();
        fl.table(&[0.6, 0.9, 0.6, 1.7, 1.7, 1.6, 0.9, 0.5, 1.3], &["Beam", "b x h (mm)", "Span (m)", "Top bars L / M / R", "Bottom bars L / M / R", "Stirrups L / M / R", "δ / limit (mm)", "Ratio", "Governs"], &rows);
    }
    if !r.columns.is_empty() {
        fl.sub("Concrete columns (ACI 318-19)");
        let rows: Vec<(Vec<String>, bool)> = r.columns.iter().map(|c| (vec![c.member.to_string(), format!("{:.0} x {:.0}", c.b * 1e3, c.h * 1e3), c.bars.clone(), f(c.rho * 100.0, 2), c.ties.clone(), f(c.ratio, 2), c.governs.clone()], !c.ok)).collect();
        fl.table(&[0.6, 1.0, 1.4, 0.7, 1.4, 0.6, 2.0], &["Column", "b x h (mm)", "Bars", "ρ (%)", "Ties", "Ratio", "Governs"], &rows);
    }
    if !r.walls.is_empty() {
        fl.sub(if r.walls.iter().any(|w| w.special) { "Structural walls (ACI 318-19 Ch. 11, special walls 18.10)" } else { "Structural walls (ACI 318-19 Ch. 11)" });
        let rows: Vec<(Vec<String>, bool)> = r
            .walls
            .iter()
            .map(|w| (vec![w.name.clone(), format!("{:.2} – {:.2}", w.z0, w.z1), format!("{:.0} × {:.2}", w.t * 1e3, w.lw), w.vertical.clone(), w.horizontal.clone(), format!("{:.0} / {:.0}", w.vu, w.phi_vn), if w.boundary.is_some() { "required".into() } else { "—".into() }, f(w.ratio, 2), w.governs.clone()], !w.ok))
            .collect();
        fl.table(&[0.6, 0.9, 0.9, 1.3, 1.3, 1.0, 0.8, 0.5, 1.7], &["Wall", "z (m)", "t (mm) × lw (m)", "Vertical", "Horizontal", "Vu / φVn (kN)", "Boundary", "Ratio", "Governs"], &rows);
    }
    if !r.slabs.is_empty() {
        fl.sub("Slabs");
        let rows: Vec<(Vec<String>, bool)> = r
            .slabs
            .iter()
            .map(|s| {
                let bars = s.strips.iter().map(|x| format!("{} {}: {}", x.direction, x.location, x.bars)).collect::<Vec<_>>().join("; ");
                (vec![s.name.clone(), format!("{:.2} x {:.2}", s.lx, s.ly), f(s.h * 1e3, 0), s.method.clone(), bars, f(s.ratio, 2), s.governs.clone()], !s.ok)
            })
            .collect();
        fl.table(&[0.8, 0.9, 0.5, 1.0, 3.6, 0.5, 1.3], &["Slab", "lx x ly (m)", "h (mm)", "Method", "Bars", "Ratio", "Governs"], &rows);
    }
    if !r.footings.is_empty() {
        fl.sub("Footings");
        let rows: Vec<(Vec<String>, bool)> = r.footings.iter().map(|x| (vec![x.node.to_string(), format!("{:.2} x {:.2} x {:.2}", x.l, x.b, x.h), x.bars_x.clone(), x.bars_y.clone(), f(x.q_max, 1), f(x.bearing_ratio, 2), f(x.punching_ratio, 2), f(x.ratio, 2), x.governs.clone()], !x.ok)).collect();
        fl.table(&[0.5, 1.3, 1.1, 1.1, 0.8, 0.7, 0.7, 0.6, 1.4], &["Node", "L x B x h (m)", "Bars x", "Bars y", "q max (kPa)", "Bearing", "Punching", "Ratio", "Governs"], &rows);
    }
    if !r.steel.is_empty() {
        fl.sub(if model.design.steel_code == crate::codes::SteelCode::Ec3 { "Steel members (EN 1993-1-1)" } else { "Steel members (AISC 360-16)" });
        let rows: Vec<(Vec<String>, bool)> = r.steel.iter().map(|s| (vec![s.member.to_string(), s.section.clone(), f(s.length, 2), f(s.mass * s.length, 1), f(s.ratio, 2), s.governs.clone()], !s.ok)).collect();
        fl.table(&[0.6, 1.8, 0.8, 0.8, 0.6, 2.6], &["Member", "Section", "L (m)", "Mass (kg)", "Ratio", "Governs"], &rows);
    }
    if !r.others.is_empty() {
        fl.sub("Timber, masonry and cold-formed members");
        let rows: Vec<(Vec<String>, bool)> = r.others.iter().map(|s| (vec![s.member.to_string(), s.code.clone(), s.section.clone(), f(s.ratio, 2), s.governs.clone()], !s.ok)).collect();
        fl.table(&[0.6, 1.6, 1.6, 0.6, 2.4], &["Member", "Code", "Section", "Ratio", "Governs"], &rows);
    }
    if !r.connections.is_empty() {
        fl.sub("Shear connections (single-plate shear tabs)");
        let rows: Vec<(Vec<String>, bool)> = r.connections.iter().map(|c| (vec![format!("{} {}", c.member, c.end), f(c.vu, 1), c.bolts.clone(), c.plate.clone(), c.weld.clone(), f(c.ratio, 2)], !c.ok)).collect();
        fl.table(&[0.9, 0.6, 2.0, 1.2, 1.4, 0.6], &["Beam end", "Vu (kN)", "Bolts", "Plate", "Weld", "Ratio"], &rows);
    }
    if !r.vibration.is_empty() {
        fl.sub("Floor vibration screen (AISC Design Guide 11)");
        let rows: Vec<(Vec<String>, bool)> = r.vibration.iter().map(|v| (vec![v.member.to_string(), f(v.deflection * 1e3, 2), f(v.frequency, 2), f(v.ratio, 2), ok(v.ok)], !v.ok)).collect();
        fl.table(&[0.8, 1.2, 1.2, 0.8, 0.8], &["Beam", "Δ (mm)", "f (Hz)", "Ratio", "Check"], &rows);
    }
}

/// The full calculation of the governing member of each type (or of all of them).
fn details(fl: &mut Flow, model: &Model, r: &Report, all: bool) {
    fl.sub(if all { "Calculations of every member" } else { "Calculations of the governing members" });
    fl.para("The tables below give the demand, the capacity and the ratio of each check of the most utilised member of each type, so the governing numbers can be followed by hand.");
    let pick = |n: usize, k: usize| all || k == n;
    let worst = |v: Vec<f64>| v.iter().enumerate().fold((usize::MAX, f64::MIN), |b, (k, x)| if *x > b.1 { (k, *x) } else { b }).0;
    let wb = worst(r.beams.iter().map(|b| b.ratio).collect());
    for (k, b) in r.beams.iter().enumerate().filter(|(k, _)| pick(wb, *k)) {
        fl.sub(&format!("Beam {} ({:.0} x {:.0} mm, span {:.2} m)", b.member, b.b * 1e3, b.h * 1e3, b.span));
        let _ = k;
        let rows: Vec<(Vec<String>, bool)> = b.flexure.iter().map(|z| (vec![format!("{} {}", z.face, z.zone), f(z.mu, 1), f(z.as_req, 0), f(z.as_prov, 0), z.bars.clone(), f(z.ratio, 2)], z.ratio > 1.0)).collect();
        fl.table(&[1.4, 1.0, 1.0, 1.0, 1.4, 0.7], &["Flexure", "Mu (kN·m)", "As req (mm²)", "As prov (mm²)", "Bars", "Ratio"], &rows);
        let rows: Vec<(Vec<String>, bool)> = b.shear.iter().map(|z| (vec![z.zone.clone(), f(z.vu, 1), f(z.phi_vc, 1), z.stirrups.clone(), f(z.s, 0), f(z.ratio, 2)], z.ratio > 1.0)).collect();
        fl.table(&[1.4, 1.0, 1.0, 1.4, 1.0, 0.7], &["Shear", "Vu (kN)", "φVc (kN)", "Stirrups", "s (mm)", "Ratio"], &rows);
        if let Some(t) = &b.torsion {
            fl.para(&format!("Torsion: Tu = {:.2} kN·m against the threshold φTth = {:.2} kN·m and φTcr = {:.2} kN·m, so the beam needs At/s = {:.3} mm²/mm per leg and Al = {:.0} mm² ({}), with a section ratio of {:.2} (ACI 318-19 22.7).", t.tu, t.phi_tth, t.phi_tcr, t.at_s, t.al, t.side_bars, t.section_ratio));
        }
        fl.para(&format!("Deflection: {:.1} mm under the live load against L/{} = {:.1} mm, and {:.1} mm in total against {:.1} mm (effective inertia Ie, ACI 318-19 24.2). The minimum depth without a deflection calculation is {:.0} mm ({}).", b.defl_live, model.design.defl_live, b.limit_live, b.defl_total, b.limit_total, b.h_min, b.support));
        for n in &b.notes {
            fl.note(&format!("• {n}"));
        }
    }
    let wc = worst(r.columns.iter().map(|b| b.ratio).collect());
    for (_, c) in r.columns.iter().enumerate().filter(|(k, _)| pick(wc, *k)) {
        fl.sub(&format!("Column {} ({:.0} x {:.0} mm, {}, {})", c.member, c.b * 1e3, c.h * 1e3, c.bars, c.ties));
        let rows: Vec<(Vec<String>, bool)> = c.checks.iter().map(|x| (vec![x.name.clone(), x.detail.clone(), f(x.ratio, 2)], x.ratio > 1.0)).collect();
        fl.table(&[1.6, 4.4, 0.6], &["Check", "Demand and capacity", "Ratio"], &rows);
        for n in &c.notes {
            fl.note(&format!("• {n}"));
        }
    }
    let ws = worst(r.slabs.iter().map(|b| b.ratio).collect());
    for (_, s) in r.slabs.iter().enumerate().filter(|(k, _)| pick(ws, *k)) {
        fl.sub(&format!("Slab {} ({:.2} x {:.2} m, h = {:.0} mm, {})", s.name, s.lx, s.ly, s.h * 1e3, s.method));
        let rows: Vec<(Vec<String>, bool)> = s.strips.iter().map(|x| (vec![format!("{} {}", x.direction, x.location), f(x.mu, 2), f(x.as_req, 0), f(x.as_prov, 0), x.bars.clone(), f(x.ratio, 2)], x.ratio > 1.0)).collect();
        fl.table(&[1.6, 1.0, 1.0, 1.0, 1.4, 0.7], &["Strip", "Mu (kN·m/m)", "As req (mm²/m)", "As prov (mm²/m)", "Bars", "Ratio"], &rows);
        fl.para(&format!("The factored load is wu = {:.2} kPa, the shear Vu = {:.1} kN/m against φVc = {:.1} kN/m, and the minimum thickness is {:.0} mm.", s.wu, s.vu, s.phi_vc, s.h_min));
    }
    let wst = worst(r.steel.iter().map(|b| b.ratio).collect());
    for (_, s) in r.steel.iter().enumerate().filter(|(k, _)| pick(wst, *k)) {
        fl.sub(&format!("Steel member {} ({}, L = {:.2} m)", s.member, s.section, s.length));
        let rows: Vec<(Vec<String>, bool)> = s.checks.iter().map(|x| (vec![x.name.clone(), x.detail.clone(), f(x.ratio, 2)], x.ratio > 1.0)).collect();
        fl.table(&[1.6, 4.4, 0.6], &["Check", "Demand and capacity", "Ratio"], &rows);
        for n in &s.notes {
            fl.note(&format!("• {n}"));
        }
    }
    let wo = worst(r.others.iter().map(|b| b.ratio).collect());
    for (_, s) in r.others.iter().enumerate().filter(|(k, _)| pick(wo, *k)) {
        fl.sub(&format!("Member {} ({}, {})", s.member, s.section, s.code));
        let rows: Vec<(Vec<String>, bool)> = s.checks.iter().map(|x| (vec![x.name.clone(), x.detail.clone(), f(x.ratio, 2)], x.ratio > 1.0)).collect();
        fl.table(&[1.6, 4.4, 0.6], &["Check", "Demand and capacity", "Ratio"], &rows);
    }
    let ww = worst(r.walls.iter().map(|b| b.ratio).collect());
    for (_, w) in r.walls.iter().enumerate().filter(|(k, _)| pick(ww, *k)) {
        fl.sub(&format!("Wall {} at z {:.2}–{:.2} m ({:.0} mm × {:.2} m)", w.name, w.z0, w.z1, w.t * 1e3, w.lw));
        let rows: Vec<(Vec<String>, bool)> = w.checks.iter().map(|x| (vec![x.name.clone(), x.detail.clone(), f(x.ratio, 2)], x.ratio > 1.0)).collect();
        fl.table(&[1.6, 4.4, 0.6], &["Check", "Demand and capacity", "Ratio"], &rows);
        fl.para(&format!("Vertical bars {} (ρl = {:.4}) and horizontal bars {} (ρt = {:.4}) in {} curtain{}.{}", w.vertical, w.rho_l, w.horizontal, w.rho_t, w.curtains, if w.curtains == 2 { "s" } else { "" }, w.boundary.as_ref().map(|b| format!(" Boundary elements: {b}.")).unwrap_or_default()));
        for n in &w.notes {
            fl.note(&format!("• {n}"));
        }
    }
    let wf = worst(r.footings.iter().map(|b| b.ratio).collect());
    for (_, x) in r.footings.iter().enumerate().filter(|(k, _)| pick(wf, *k)) {
        fl.sub(&format!("Footing at node {} ({:.2} x {:.2} x {:.2} m)", x.node, x.l, x.b, x.h));
        fl.para(&format!("The largest soil pressure is {:.1} kPa (bearing ratio {:.2}), the safety factors are {:.2} against overturning and {:.2} against sliding, and the one-way and punching shear ratios are {:.2} and {:.2}, with bars {} in x and {} in y.", x.q_max, x.bearing_ratio, x.overturning_sf, x.sliding_sf, x.oneway_ratio, x.punching_ratio, x.bars_x, x.bars_y));
        for n in &x.notes {
            fl.note(&format!("• {n}"));
        }
    }
}

/// Today's date (UTC), yyyy-mm-dd, from the system clock.
fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs / 86_400;
    // civil from days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{}-{m:02}-{d:02}", if m <= 2 { y + 1 } else { y })
}
