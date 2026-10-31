// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The model as an Excel workbook, for engineers who check or edit a model in a spreadsheet:
//! one sheet per table (Nodes, Members, Supports, Sections, Materials, Cases, Combinations,
//! Loads, Slabs) and a Settings sheet with the name, the design and seismic settings, the
//! diaphragms, the moving load and the stages as JSON, so a model survives the round trip
//! exactly. With an analysis the workbook also has a Results sheet (each member's extremes
//! under every ULS combination), and with a design one sheet per design table (Design beams,
//! Design columns, ...), each row a member with its ratio and governing check.
//!
//! Import reads the model sheets back by their header names, in any column order, ignoring
//! sheets and columns it does not know; Results and Design sheets are output only. Units are
//! the model's (m, kN, kPa, MPa). The Loads sheet keeps one row per load: `fx..fz` carry a
//! nodal force, a line load's start intensity `wi` (kN/m, global), a point load `p` or the
//! first three components of a settlement, `mx..mz` the moments or settlement rotations,
//! `wj_x..wj_z` a line load's end intensity, `a`, `b` the loaded part (m from node i) and
//! `dt`, `dt_y`, `dt_z` a thermal load.

use std::collections::HashMap;
use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use serde_json::{json, Value};

use crate::analysis::Analysis;
use crate::design::Report;
use crate::model::*;

/// A cell to write: a number, a string, a flag (written 1 / 0 so it reads back as a number)
/// or nothing.
enum C {
    N(f64),
    S(String),
    E,
}

fn n(x: f64) -> C {
    C::N(x)
}
fn s(x: impl Into<String>) -> C {
    C::S(x.into())
}
fn b(x: bool) -> C {
    C::N(if x { 1.0 } else { 0.0 })
}
fn on(x: Option<f64>) -> C {
    x.map(C::N).unwrap_or(C::E)
}
fn os(x: &Option<String>) -> C {
    x.as_ref().map(|v| C::S(v.clone())).unwrap_or(C::E)
}

fn xe(e: rust_xlsxwriter::XlsxError) -> String {
    format!("excel: {e}")
}

/// Writes a table: a bold frozen header row, numbers as numbers, widths from the contents.
fn table(wb: &mut Workbook, name: &str, head: &[&str], rows: &[Vec<C>]) -> Result<(), String> {
    let ws: &mut Worksheet = wb.add_worksheet();
    ws.set_name(name).map_err(xe)?;
    let bold = Format::new().set_bold();
    let mut width: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for (c, h) in head.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &bold).map_err(xe)?;
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let (r, col) = (r as u32 + 1, c as u16);
            let w = match v {
                C::N(x) if x.is_finite() => {
                    ws.write_number(r, col, *x).map_err(xe)?;
                    format!("{:.4}", x).trim_end_matches('0').len()
                }
                C::N(_) => {
                    ws.write_string(r, col, "inf").map_err(xe)?;
                    3
                }
                C::S(t) => {
                    ws.write_string(r, col, t).map_err(xe)?;
                    t.chars().count()
                }
                C::E => 0,
            };
            if c < width.len() {
                width[c] = width[c].max(w);
            } else {
                width.push(w);
            }
        }
    }
    for (c, w) in width.iter().enumerate() {
        ws.set_column_width(c as u16, (*w as f64 + 2.0).clamp(6.0, 60.0)).map_err(xe)?;
    }
    ws.set_freeze_panes(1, 0).map_err(xe)?;
    Ok(())
}

fn kind_name(k: MemberKind) -> &'static str {
    match k {
        MemberKind::Auto => "auto",
        MemberKind::Beam => "beam",
        MemberKind::Column => "column",
        MemberKind::Brace => "brace",
    }
}

fn behaviour_name(b: Behaviour) -> &'static str {
    match b {
        Behaviour::Normal => "normal",
        Behaviour::TensionOnly => "tension_only",
        Behaviour::CompressionOnly => "compression_only",
        Behaviour::Cable => "cable",
    }
}

const FIX: [&str; 6] = ["ux", "uy", "uz", "rx", "ry", "rz"];
const SPRING: [&str; 6] = ["kx", "ky", "kz", "krx", "kry", "krz"];

/// Writes the workbook (see the module notes).
pub fn export(model: &Model, a: Option<&Analysis>, rep: Option<&Report>, path: &Path) -> Result<(), String> {
    let mut wb = Workbook::new();
    let rows: Vec<Vec<C>> = model.nodes.iter().map(|x| vec![n(x.id as f64), n(x.x), n(x.y), n(x.z)]).collect();
    table(&mut wb, "Nodes", &["id", "x", "y", "z"], &rows)?;
    let sec = |id: Id| model.section(id).map(|s| s.name.clone()).unwrap_or_default();
    let mat = |id: Id| model.material(id).map(|s| s.name.clone()).unwrap_or_default();
    // a wall's piers and arms are not written: the Settings sheet's walls rebuild them
    let rows: Vec<Vec<C>> = model
        .members
        .iter()
        .filter(|m| m.role.is_frame())
        .map(|m| {
            let w = m.winkler.unwrap_or([0.0; 2]);
            vec![n(m.id as f64), n(m.i as f64), n(m.j as f64), s(kind_name(m.kind)), s(sec(m.section)), s(mat(m.material)), n(m.roll), b(m.truss), s(behaviour_name(m.behaviour)), b(m.hinge_i), b(m.hinge_j), on(m.lb), on(m.k), n(w[0]), n(w[1]), on(m.rebar), os(&m.bim_id), os(&m.group), n(model.length(m))]
        })
        .collect();
    table(&mut wb, "Members", &["id", "i", "j", "kind", "section", "material", "roll", "truss", "behaviour", "hinge_i", "hinge_j", "lb", "k", "winkler_kz", "winkler_ky", "rebar", "bim_id", "group", "length (out)"], &rows)?;
    let mut head: Vec<&str> = vec!["node"];
    head.extend(FIX);
    head.extend(SPRING);
    head.push("uplift");
    head.push("type (out)");
    let rows: Vec<Vec<C>> = model
        .supports
        .iter()
        .map(|x| {
            let mut r = vec![n(x.node as f64)];
            r.extend(x.fix.iter().map(|f| b(*f)));
            r.extend(x.springs.iter().map(|k| n(*k)));
            r.push(b(x.uplift));
            r.push(s(x.kind()));
            r
        })
        .collect();
    table(&mut wb, "Supports", &head, &rows)?;
    let rows: Vec<Vec<C>> = model
        .sections
        .iter()
        .map(|x| {
            let p = x.props();
            let mut r = vec![n(x.id as f64), s(x.name.clone())];
            // shape, then b h d bf tw tf r t a iy iz j as the shape has them
            let mut v: [C; 12] = std::array::from_fn(|_| C::E);
            let shape = match x.shape {
                Shape::Rect { b, h } => {
                    v[0] = n(b);
                    v[1] = n(h);
                    "rect"
                }
                Shape::H { d, bf, tw, tf, r } => {
                    v[2] = n(d);
                    v[3] = n(bf);
                    v[4] = n(tw);
                    v[5] = n(tf);
                    v[6] = n(r);
                    "h"
                }
                Shape::LippedC { h, b, d, t } => {
                    v[0] = n(b);
                    v[1] = n(h);
                    v[2] = n(d);
                    v[7] = n(t);
                    "lippedc"
                }
                Shape::General { a, iy, iz, j, .. } => {
                    v[8] = n(a);
                    v[9] = n(iy);
                    v[10] = n(iz);
                    v[11] = n(j);
                    "general"
                }
            };
            r.push(s(shape));
            r.extend(v);
            r.extend([n(p.a), n(p.iy), n(p.iz), n(p.j)]);
            r
        })
        .collect();
    table(&mut wb, "Sections", &["id", "name", "shape", "b", "h", "d", "bf", "tw", "tf", "r", "t", "a", "iy", "iz", "j", "A (out)", "Iy (out)", "Iz (out)", "J (out)"], &rows)?;
    let rows: Vec<Vec<C>> = model
        .materials
        .iter()
        .map(|x| vec![n(x.id as f64), s(x.name.clone()), s(enum_name(&x.kind)), n(x.e), n(x.nu), n(x.gamma), n(x.fc), n(x.fy), n(x.fu), n(x.fb), n(x.ft), n(x.fv), n(x.e_min), n(x.alpha)])
        .collect();
    table(&mut wb, "Materials", &["id", "name", "kind", "e", "nu", "gamma", "fc", "fy", "fu", "fb", "ft", "fv", "e_min", "alpha"], &rows)?;
    let rows: Vec<Vec<C>> = model.cases.iter().map(|x| vec![n(x.id as f64), s(x.name.clone()), s(enum_name(&x.kind)), b(x.self_weight)]).collect();
    table(&mut wb, "Cases", &["id", "name", "kind", "self_weight"], &rows)?;
    let mut head: Vec<String> = vec!["id".into(), "name".into(), "kind".into()];
    head.extend(model.cases.iter().map(|c| c.name.clone()));
    let rows: Vec<Vec<C>> = model
        .combos
        .iter()
        .map(|x| {
            let mut r = vec![n(x.id as f64), s(x.name.clone()), s(enum_name(&x.kind))];
            r.extend(model.cases.iter().map(|c| x.factors.iter().find(|f| f.0 == c.id).map(|f| n(f.1)).unwrap_or(C::E)));
            r
        })
        .collect();
    table(&mut wb, "Combinations", &head.iter().map(String::as_str).collect::<Vec<_>>(), &rows)?;
    let case_name = |id: Id| model.case(id).map(|c| c.name.clone()).unwrap_or_else(|| id.to_string());
    let rows: Vec<Vec<C>> = model
        .loads
        .iter()
        .map(|l| {
            // case type node member a b fx fy fz mx my mz wj_x wj_y wj_z dt dt_y dt_z
            let mut r: Vec<C> = (0..18).map(|_| C::E).collect();
            r[0] = s(case_name(l.case));
            match &l.kind {
                LoadKind::Nodal { node, f } => {
                    r[1] = s("nodal");
                    r[2] = n(*node as f64);
                    (0..6).for_each(|k| r[6 + k] = n(f[k]));
                }
                LoadKind::Line { member, wi, wj, a, b } => {
                    r[1] = s("line");
                    r[3] = n(*member as f64);
                    r[4] = on(*a);
                    r[5] = on(*b);
                    (0..3).for_each(|k| r[6 + k] = n(wi[k]));
                    (0..3).for_each(|k| r[12 + k] = n(wj[k]));
                }
                LoadKind::Point { member, a, p } => {
                    r[1] = s("point");
                    r[3] = n(*member as f64);
                    r[4] = n(*a);
                    (0..3).for_each(|k| r[6 + k] = n(p[k]));
                }
                LoadKind::Settlement { node, d } => {
                    r[1] = s("settlement");
                    r[2] = n(*node as f64);
                    (0..6).for_each(|k| r[6 + k] = n(d[k]));
                }
                LoadKind::Thermal { member, dt, dt_y, dt_z } => {
                    r[1] = s("thermal");
                    r[3] = n(*member as f64);
                    r[15] = n(*dt);
                    r[16] = n(*dt_y);
                    r[17] = n(*dt_z);
                }
            }
            r
        })
        .collect();
    table(&mut wb, "Loads", &["case", "type", "node", "member", "a", "b", "fx", "fy", "fz", "mx", "my", "mz", "wj_x", "wj_y", "wj_z", "dt", "dt_y", "dt_z"], &rows)?;
    let rows: Vec<Vec<C>> = model.slabs.iter().map(|x| vec![n(x.id as f64), s(x.name.clone()), n(x.x0), n(x.y0), n(x.x1), n(x.y1), n(x.z), n(x.h), n(x.dead), n(x.live), os(&x.bim_id), n(x.subgrade), n(if x.mat { 1.0 } else { 0.0 }), n(x.support_size), n(if x.plate { 1.0 } else { 0.0 })]).collect();
    table(&mut wb, "Slabs", &["id", "name", "x0", "y0", "x1", "y1", "z", "h", "dead", "live", "bim_id", "subgrade", "mat", "support_size", "plate"], &rows)?;
    let js = |v: Value| s(v.to_string());
    let rows = vec![
        vec![s("name"), js(json!(model.name))],
        vec![s("design"), js(serde_json::to_value(&model.design).unwrap_or_default())],
        vec![s("seismic"), js(serde_json::to_value(&model.seismic).unwrap_or_default())],
        vec![s("diaphragms"), js(json!(model.diaphragms))],
        vec![s("walls"), js(json!(model.walls))],
        vec![s("moving"), js(serde_json::to_value(&model.moving).unwrap_or_default())],
        vec![s("stages"), js(serde_json::to_value(&model.stages).unwrap_or_default())],
    ];
    table(&mut wb, "Settings", &["key", "json"], &rows)?;
    if let Some(a) = a {
        let mut rows = vec![];
        for c in model.combos.iter().filter(|c| c.kind == ComboKind::Uls) {
            let Some(res) = a.result(model, &c.name) else { continue };
            for mr in &res.members {
                let ext = |f: &dyn Fn(&crate::analysis::Station) -> f64, max: bool| -> (C, C) {
                    let st = mr.stations.iter().max_by(|p, q| if max { f(p).total_cmp(&f(q)) } else { f(q).total_cmp(&f(p)) });
                    st.map(|x| (n(f(x)), n(x.x))).unwrap_or((C::E, C::E))
                };
                let mut r = vec![s(c.name.clone()), n(mr.member as f64), n(mr.length)];
                for (f, max) in [(&(|x: &crate::analysis::Station| x.my) as &dyn Fn(&crate::analysis::Station) -> f64, true), (&|x| x.my, false), (&|x| x.vz.abs(), true), (&|x| x.n, false), (&|x| x.n, true), (&|x| x.t.abs(), true), (&|x| x.mz.abs(), true)] {
                    let (v, x) = ext(f, max);
                    r.push(v);
                    r.push(x);
                }
                rows.push(r);
            }
        }
        table(&mut wb, "Results", &["combination", "member", "length", "My max", "x", "My min", "x", "|Vz| max", "x", "N min", "x", "N max", "x", "|T| max", "x", "|Mz| max", "x"], &rows)?;
    }
    if let Some(rep) = rep {
        let v = serde_json::to_value(rep).map_err(|e| e.to_string())?;
        for (key, title) in [("beams", "Design beams"), ("columns", "Design columns"), ("slabs", "Design slabs"), ("steel", "Design steel"), ("footings", "Design footings"), ("others", "Design other"), ("connections", "Design connections"), ("vibration", "Design vibration")] {
            let items = v.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
            if items.is_empty() {
                continue;
            }
            design_table(&mut wb, title, &items)?;
        }
    }
    wb.save(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The lowercase serde name of a unit enum (MaterialKind, CaseKind, ComboKind).
fn enum_name<T: serde::Serialize>(x: &T) -> String {
    serde_json::to_value(x).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

/// A design table from the report's rows: the key columns first (member or slab or node,
/// section, ratio, ok, governs), then every other scalar field; lists and nested records are
/// written as compact JSON.
fn design_table(wb: &mut Workbook, title: &str, items: &[Value]) -> Result<(), String> {
    let first = ["member", "slab", "node", "end", "section", "bim_id", "ratio", "ok", "governs"];
    let mut head: Vec<String> = first.iter().filter(|k| items.iter().any(|i| i.get(**k).is_some())).map(|k| k.to_string()).collect();
    for i in items {
        for k in i.as_object().into_iter().flat_map(|o| o.keys()) {
            if !head.contains(k) {
                head.push(k.clone());
            }
        }
    }
    let rows: Vec<Vec<C>> = items
        .iter()
        .map(|i| {
            head.iter()
                .map(|k| match i.get(k) {
                    None | Some(Value::Null) => C::E,
                    Some(Value::Number(x)) => n(x.as_f64().unwrap_or(0.0)),
                    Some(Value::Bool(x)) => s(if *x { "yes" } else { "no" }),
                    Some(Value::String(x)) => s(x.clone()),
                    Some(other) => {
                        let t = other.to_string();
                        s(if t.len() > 30_000 { format!("{}…", &t[..30_000]) } else { t })
                    }
                })
                .collect()
        })
        .collect();
    table(wb, title, &head.iter().map(String::as_str).collect::<Vec<_>>(), &rows)
}

// ---------------------------------------------------------------- import

/// A sheet as rows of header name → cell.
fn sheet(wb: &mut calamine::Sheets<std::io::BufReader<std::fs::File>>, name: &str) -> Option<Vec<HashMap<String, Data>>> {
    let r = wb.worksheet_range(name).ok()?;
    let mut it = r.rows();
    let head: Vec<String> = it.next()?.iter().map(|c| c.to_string().trim().to_lowercase()).collect();
    Some(
        it.filter(|row| row.iter().any(|c| !matches!(c, Data::Empty)))
            .map(|row| head.iter().cloned().zip(row.iter().cloned()).filter(|(h, _)| !h.is_empty()).collect())
            .collect(),
    )
}

fn num_of(r: &HashMap<String, Data>, k: &str) -> Option<f64> {
    match r.get(k)? {
        Data::Float(x) => Some(*x),
        Data::Int(x) => Some(*x as f64),
        Data::Bool(x) => Some(if *x { 1.0 } else { 0.0 }),
        Data::String(t) => t.trim().parse().ok(),
        _ => None,
    }
}
fn str_of(r: &HashMap<String, Data>, k: &str) -> Option<String> {
    match r.get(k)? {
        Data::Empty => None,
        Data::Float(x) if x.fract() == 0.0 => Some(format!("{}", *x as i64)),
        d => Some(d.to_string()).filter(|t| !t.trim().is_empty()),
    }
}
fn flag(r: &HashMap<String, Data>, k: &str) -> bool {
    match r.get(k) {
        Some(Data::String(t)) => matches!(t.trim().to_lowercase().as_str(), "1" | "yes" | "true" | "y" | "x"),
        _ => num_of(r, k).is_some_and(|x| x != 0.0),
    }
}
fn id_of(r: &HashMap<String, Data>, k: &str) -> Option<Id> {
    num_of(r, k).filter(|x| *x >= 0.0).map(|x| x.round() as Id)
}

fn parse_enum<T: serde::de::DeserializeOwned>(t: &str, what: &str) -> Result<T, String> {
    serde_json::from_value(json!(t.trim())).or_else(|_| serde_json::from_value(json!(t.trim().to_lowercase()))).map_err(|_| format!("{what}: unknown '{t}'"))
}

/// Reads a model from a workbook written by `export` or edited by hand (see the module notes).
pub fn import(path: &Path) -> Result<Model, String> {
    let mut wb = open_workbook_auto(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut m = Model::new(&path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".into()));
    m.combos.clear();
    if let Some(rows) = sheet(&mut wb, "Settings") {
        for r in rows {
            let (Some(k), Some(t)) = (str_of(&r, "key"), str_of(&r, "json")) else { continue };
            let v: Value = serde_json::from_str(&t).map_err(|e| format!("Settings {k}: {e}"))?;
            let bad = |e: serde_json::Error| format!("Settings {k}: {e}");
            match k.as_str() {
                "name" => m.name = v.as_str().unwrap_or_default().into(),
                "design" => m.design = serde_json::from_value(v).map_err(bad)?,
                "seismic" => m.seismic = serde_json::from_value(v).map_err(bad)?,
                "diaphragms" => m.diaphragms = serde_json::from_value(v).map_err(bad)?,
                "walls" => m.walls = serde_json::from_value(v).map_err(bad)?,
                "moving" => m.moving = serde_json::from_value(v).map_err(bad)?,
                "stages" => m.stages = serde_json::from_value(v).map_err(bad)?,
                _ => {}
            }
        }
    }
    if let Some(rows) = sheet(&mut wb, "Materials") {
        m.materials.clear();
        for (k, r) in rows.iter().enumerate() {
            let id = id_of(r, "id").unwrap_or(k + 1);
            let kind: MaterialKind = parse_enum(&str_of(r, "kind").unwrap_or("concrete".into()), "material kind")?;
            let g = |f: &str| num_of(r, f).unwrap_or(0.0);
            m.materials.push(Material { id, name: str_of(r, "name").unwrap_or_else(|| format!("Material {id}")), kind, e: g("e"), nu: num_of(r, "nu").unwrap_or(0.2), gamma: g("gamma"), fc: g("fc"), fy: g("fy"), fu: g("fu"), fb: g("fb"), ft: g("ft"), fv: g("fv"), e_min: g("e_min"), alpha: g("alpha") });
        }
    }
    if let Some(rows) = sheet(&mut wb, "Sections") {
        for (k, r) in rows.iter().enumerate() {
            let id = id_of(r, "id").unwrap_or(k + 1);
            let name = str_of(r, "name").unwrap_or_default();
            let g = |f: &str| num_of(r, f).unwrap_or(0.0);
            let shape = match str_of(r, "shape").map(|t| t.to_lowercase()) {
                Some(t) if t == "rect" => Shape::Rect { b: g("b"), h: g("h") },
                Some(t) if t == "h" => Shape::H { d: g("d"), bf: g("bf"), tw: g("tw"), tf: g("tf"), r: g("r") },
                Some(t) if t == "lippedc" => Shape::LippedC { h: g("h"), b: g("b"), d: g("d"), t: g("t") },
                Some(t) if t == "general" => Shape::General { a: g("a"), iy: g("iy"), iz: g("iz"), j: g("j"), shear: None },
                _ => {
                    // no shape: parse the name as add_members would
                    let mut tmp = Model::new("");
                    let sid = tmp.section_named(&name).map_err(|e| format!("Sections row {}: {e}", k + 2))?;
                    tmp.section(sid).unwrap().shape.clone()
                }
            };
            m.sections.push(Section { id, name, shape });
        }
    }
    let rows = sheet(&mut wb, "Nodes").ok_or("no Nodes sheet")?;
    for r in &rows {
        let id = id_of(r, "id").ok_or("Nodes: a row without id")?;
        m.nodes.push(Node { id, x: num_of(r, "x").unwrap_or(0.0), y: num_of(r, "y").unwrap_or(0.0), z: num_of(r, "z").unwrap_or(0.0) });
    }
    if let Some(rows) = sheet(&mut wb, "Members") {
        for (k, r) in rows.iter().enumerate() {
            let row = k + 2;
            let id = id_of(r, "id").unwrap_or(m.next_member_id());
            let (i, j) = (id_of(r, "i").ok_or(format!("Members row {row}: no i"))?, id_of(r, "j").ok_or(format!("Members row {row}: no j"))?);
            for x in [i, j] {
                m.node(x).ok_or(format!("Members row {row}: no node {x}"))?;
            }
            let section = match str_of(r, "section") {
                Some(t) => m.section_named(&t).map_err(|e| format!("Members row {row}: {e}"))?,
                None => return Err(format!("Members row {row}: no section")),
            };
            let material = match str_of(r, "material") {
                Some(t) => m.materials.iter().find(|x| x.name.eq_ignore_ascii_case(&t)).map(|x| x.id).or_else(|| match t.to_lowercase().as_str() {
                    "concrete" => Some(m.concrete_id()),
                    "steel" => Some(m.steel_id()),
                    _ => None,
                }),
                None => Some(if matches!(m.section(section).map(|s| &s.shape), Some(Shape::H { .. }) | Some(Shape::LippedC { .. })) { m.steel_id() } else { m.concrete_id() }),
            }
            .ok_or(format!("Members row {row}: no material '{}'", str_of(r, "material").unwrap_or_default()))?;
            let w = [num_of(r, "winkler_kz").unwrap_or(0.0), num_of(r, "winkler_ky").unwrap_or(0.0)];
            m.members.push(Member {
                id,
                i,
                j,
                section,
                material,
                kind: str_of(r, "kind").map(|t| parse_enum(&t, "member kind")).transpose()?.unwrap_or_default(),
                hinge_i: flag(r, "hinge_i"),
                hinge_j: flag(r, "hinge_j"),
                lb: num_of(r, "lb"),
                k: num_of(r, "k"),
                roll: num_of(r, "roll").unwrap_or(0.0),
                truss: flag(r, "truss"),
                behaviour: str_of(r, "behaviour").map(|t| parse_enum(&t.replace([' ', '-'], "_"), "behaviour")).transpose()?.unwrap_or_default(),
                winkler: (w[0] > 0.0 || w[1] > 0.0).then_some(w),
                rebar: num_of(r, "rebar"),
                bim_id: str_of(r, "bim_id"),
                tension: None, role: Default::default(),
                group: str_of(r, "group"),
            });
        }
    }
    if let Some(rows) = sheet(&mut wb, "Supports") {
        for r in &rows {
            let node = id_of(r, "node").ok_or("Supports: a row without node")?;
            m.supports.push(Support { node, fix: FIX.map(|k| flag(r, k)), springs: SPRING.map(|k| num_of(r, k).unwrap_or(0.0)), uplift: flag(r, "uplift") });
        }
    }
    if let Some(rows) = sheet(&mut wb, "Cases") {
        m.cases.clear();
        for (k, r) in rows.iter().enumerate() {
            let id = id_of(r, "id").unwrap_or(k + 1);
            let kind: CaseKind = parse_enum(&str_of(r, "kind").unwrap_or("Other".into()), "case kind").or_else(|_| parse_enum(&str_of(r, "kind").unwrap_or_default().to_uppercase(), "case kind"))?;
            m.cases.push(LoadCase { id, name: str_of(r, "name").unwrap_or_else(|| format!("Case {id}")), kind, self_weight: flag(r, "self_weight") });
        }
    }
    let case_id = |m: &Model, t: &str| m.cases.iter().find(|c| c.name.eq_ignore_ascii_case(t.trim())).map(|c| c.id).or_else(|| t.trim().parse().ok().filter(|i| m.case(*i).is_some()));
    if let Some(rows) = sheet(&mut wb, "Combinations") {
        for (k, r) in rows.iter().enumerate() {
            let id = id_of(r, "id").unwrap_or(k + 1);
            let factors: Vec<(Id, f64)> = m.cases.iter().filter_map(|c| num_of(r, &c.name.to_lowercase()).map(|f| (c.id, f))).collect();
            m.combos.push(Combination { id, name: str_of(r, "name").unwrap_or_else(|| format!("Combination {id}")), kind: parse_enum(&str_of(r, "kind").unwrap_or("uls".into()).to_lowercase(), "combination kind")?, factors });
        }
    }
    if let Some(rows) = sheet(&mut wb, "Loads") {
        for (k, r) in rows.iter().enumerate() {
            let row = k + 2;
            let case = str_of(r, "case").and_then(|t| case_id(&m, &t)).ok_or(format!("Loads row {row}: no such case"))?;
            let g = |f: &str| num_of(r, f).unwrap_or(0.0);
            let six = || [g("fx"), g("fy"), g("fz"), g("mx"), g("my"), g("mz")];
            let three = |p: &str| -> [f64; 3] { if p == "w" { [g("fx"), g("fy"), g("fz")] } else { [g("wj_x"), g("wj_y"), g("wj_z")] } };
            let node = || id_of(r, "node").ok_or(format!("Loads row {row}: no node"));
            let member = || id_of(r, "member").ok_or(format!("Loads row {row}: no member"));
            let kind = match str_of(r, "type").unwrap_or_default().to_lowercase().as_str() {
                "nodal" => LoadKind::Nodal { node: node()?, f: six() },
                "line" => {
                    let wi = three("w");
                    // an empty end intensity: uniform
                    let wj = if ["wj_x", "wj_y", "wj_z"].iter().any(|c| num_of(r, c).is_some()) { three("wj") } else { wi };
                    LoadKind::Line { member: member()?, wi, wj, a: num_of(r, "a"), b: num_of(r, "b") }
                }
                "point" => LoadKind::Point { member: member()?, a: g("a"), p: three("w") },
                "settlement" => LoadKind::Settlement { node: node()?, d: six() },
                "thermal" => LoadKind::Thermal { member: member()?, dt: g("dt"), dt_y: g("dt_y"), dt_z: g("dt_z") },
                t => return Err(format!("Loads row {row}: type '{t}' is nodal, line, point, settlement or thermal")),
            };
            m.loads.push(Load { case, kind });
        }
    }
    if let Some(rows) = sheet(&mut wb, "Slabs") {
        for (k, r) in rows.iter().enumerate() {
            let g = |f: &str| num_of(r, f).unwrap_or(0.0);
            m.slabs.push(Slab { id: id_of(r, "id").unwrap_or(k + 1), name: str_of(r, "name").unwrap_or_default(), x0: g("x0"), y0: g("y0"), x1: g("x1"), y1: g("y1"), z: g("z"), h: g("h"), dead: g("dead"), live: g("live"), bim_id: str_of(r, "bim_id"), subgrade: g("subgrade"), mat: g("mat") > 0.5, support_size: g("support_size"), plate: g("plate") > 0.5 });
        }
    }
    crate::walls::sync(&mut m);
    Ok(m)
}
