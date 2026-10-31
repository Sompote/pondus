// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Design of the whole model: concrete beams and slabs (ACI 318-19, `rc.rs`) and steel
//! members (AISC 360-16, `steel.rs`) from the analysis. With `resize`, failing concrete
//! beams are deepened (and widened past h = 3b) in 50 mm steps and failing slabs thickened
//! (to the minimum thickness, then in 10 mm steps); with `select`, every steel
//! member (or group) gets the lightest catalogue shape that passes. A changed section changes
//! the stiffness, so the model is analysed again until no section changes (at most 6 rounds).

use serde::{Deserialize, Serialize};

use crate::analysis::{analyze, axes, Analysis, CaseResult};
use crate::model::*;
use crate::rc::{self, BeamDemand, RcMat};
use crate::steel::{self, HDims, SteelDemand};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Members to design (empty: all); slabs to design (empty: all).
    pub members: Vec<Id>,
    pub slabs: Vec<Id>,
    pub beams: bool,
    pub do_slabs: bool,
    pub steel: bool,
    /// Grow failing concrete beams; choose the lightest passing steel shapes.
    pub resize: bool,
    pub select: bool,
    /// Spread footings under the concrete columns.
    pub footings: bool,
    /// Sets the model's slab method first ("coefficients" or "plate").
    pub slab_method: Option<crate::slabfe::SlabMethod>,
}

impl Default for Options {
    fn default() -> Self {
        Options { members: vec![], slabs: vec![], beams: true, do_slabs: true, steel: true, resize: false, select: false, footings: true, slab_method: None }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub beams: Vec<rc::BeamDesign>,
    pub columns: Vec<rc::ColumnDesign>,
    pub slabs: Vec<rc::SlabDesign>,
    pub footings: Vec<crate::footing::FootingDesign>,
    pub steel: Vec<steel::SteelDesign>,
    /// Timber, masonry and cold-formed members.
    pub others: Vec<crate::othermat::OtherDesign>,
    /// Shear tabs at the ends of the steel beams; the beams' floor vibration screen.
    pub connections: Vec<crate::connections::ConnectionDesign>,
    pub vibration: Vec<crate::connections::Vibration>,
    /// Structural walls, one row per wall and storey (ACI 318-19 Ch. 11, 18.10).
    pub walls: Vec<crate::walls::WallDesign>,
    /// Stair flights and landings (one-way slabs).
    pub stairs: Vec<crate::stairs::StairDesign>,
    /// Members not designed, and why.
    pub skipped: Vec<String>,
    /// Section changes made by resize / select.
    pub changes: Vec<String>,
    pub rounds: usize,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn failing(&self) -> Vec<String> {
        let mut v: Vec<String> = self.beams.iter().filter(|b| !b.ok).map(|b| format!("beam {} ({:.2})", b.member, b.ratio)).collect();
        v.extend(self.columns.iter().filter(|b| !b.ok).map(|b| format!("column {} ({:.2})", b.member, b.ratio)));
        v.extend(self.slabs.iter().filter(|b| !b.ok).map(|b| format!("slab {} ({:.2})", b.slab, b.ratio)));
        v.extend(self.footings.iter().filter(|b| !b.ok).map(|b| format!("footing at node {} ({:.2})", b.node, b.ratio)));
        v.extend(self.steel.iter().filter(|b| !b.ok).map(|b| format!("steel member {} ({:.2})", b.member, b.ratio)));
        v.extend(self.others.iter().filter(|b| !b.ok).map(|b| format!("member {} ({}, {:.2})", b.member, b.code, b.ratio)));
        v.extend(self.connections.iter().filter(|b| !b.ok).map(|b| format!("connection of member {} at {} ({:.2})", b.member, b.end, b.ratio)));
        v.extend(self.vibration.iter().filter(|b| !b.ok).map(|b| format!("beam {} vibration {:.1} Hz", b.member, b.frequency)));
        v.extend(self.walls.iter().filter(|b| !b.ok).map(|b| format!("wall {} at z {:.2}–{:.2} ({:.2}, {})", b.name, b.z0, b.z1, b.ratio, b.governs)));
        v
    }

    /// A short summary for the agent and the chat.
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "rounds": self.rounds,
            "changes": self.changes,
            "failing": self.failing(),
            "skipped": self.skipped,
            "warnings": self.warnings,
            "beams": self.beams.iter().map(|b| serde_json::json!({
                "member": b.member, "bim_id": b.bim_id, "section_mm": format!("{:.0} x {:.0}", b.b * 1e3, b.h * 1e3), "span_m": round(b.span, 3),
                "top_bars": zone_bars(b, "top"), "bottom_bars": zone_bars(b, "bottom"),
                "stirrups": b.shear.iter().map(|s| format!("{}: {}", s.zone, s.stirrups)).collect::<Vec<_>>(),
                "torsion": b.torsion.as_ref().map(|t| serde_json::json!({"Tu_kNm": round(t.tu, 1), "phi_Tth": round(t.phi_tth, 1), "At_s_mm2_per_mm": round(t.at_s, 3), "Al_mm2": round(t.al, 0), "side_bars": t.side_bars, "section_ratio": round(t.section_ratio, 2)})),
                "deflection_mm": {"live": round(b.defl_live, 1), "long_term": round(b.defl_total, 1), "limits": [round(b.limit_live, 1), round(b.limit_total, 1)]},
                "ratio": round(b.ratio, 3), "governs": b.governs, "ok": b.ok, "notes": b.notes,
                "ec2": b.ec2.as_ref().map(|e| serde_json::json!({"flexure": round(e.flexure, 3), "shear": round(e.shear, 3), "detail": e.detail})),
            })).collect::<Vec<_>>(),
            "walls": self.walls.iter().map(|w| serde_json::json!({
                "wall": w.wall, "name": w.name, "bim_id": w.bim_id, "storey_z_m": [round(w.z0, 2), round(w.z1, 2)], "t_mm": round(w.t * 1e3, 0), "length_m": round(w.lw, 3),
                "vertical": w.vertical, "horizontal": w.horizontal, "rho_l": round(w.rho_l, 4), "rho_t": round(w.rho_t, 4),
                "Pu_kN": round(w.pu, 0), "Mu_inplane_kNm": round(w.mu, 0), "Vu_kN": round(w.vu, 0), "phi_Vn_kN": round(w.phi_vn, 0),
                "special_wall": w.special, "boundary_elements": w.boundary, "ratio": round(w.ratio, 3), "governs": w.governs, "ok": w.ok,
                "checks": w.checks.iter().map(|k| format!("{}: {:.2} ({})", k.name, k.ratio, k.detail)).collect::<Vec<_>>(), "notes": w.notes,
            })).collect::<Vec<_>>(),
            "columns": self.columns.iter().map(|c| serde_json::json!({
                "member": c.member, "bim_id": c.bim_id, "section_mm": format!("{:.0} x {:.0}", c.b * 1e3, c.h * 1e3), "bars": c.bars, "rho_percent": round(c.rho * 100.0, 2), "ties": c.ties,
                "ratio": round(c.ratio, 3), "governs": c.governs, "ok": c.ok,
                "checks": c.checks.iter().map(|k| format!("{}: {:.2} ({})", k.name, k.ratio, k.detail)).collect::<Vec<_>>(), "notes": c.notes,
            })).collect::<Vec<_>>(),
            "footings": self.footings.iter().map(|f| serde_json::json!({
                "node": f.node, "at": f.at, "size_m": format!("{:.2} x {:.2} x {:.2}", f.l, f.b, f.h), "bars_x": f.bars_x, "bars_y": f.bars_y,
                "q_max_kPa": round(f.q_max, 1), "bearing": round(f.bearing_ratio, 3), "one_way_shear": round(f.oneway_ratio, 3), "punching": round(f.punching_ratio, 3),
                "overturning_sf": round(f.overturning_sf, 2), "sliding_sf": round(f.sliding_sf, 2), "ratio": round(f.ratio, 3), "governs": f.governs, "ok": f.ok, "notes": f.notes,
            })).collect::<Vec<_>>(),
            "connections": self.connections.iter().map(|c| serde_json::json!({
                "member": c.member, "end": c.end, "Vu_kN": round(c.vu, 1), "bolts": c.bolts, "plate": c.plate, "weld": c.weld,
                "ratios": {"bolts": round(c.bolt_ratio, 3), "bearing": round(c.bearing_ratio, 3), "plate": round(c.plate_ratio, 3), "weld": round(c.weld_ratio, 3)}, "ok": c.ok, "notes": c.notes,
            })).collect::<Vec<_>>(),
            "vibration": self.vibration.iter().map(|v| serde_json::json!({"member": v.member, "f_Hz": round(v.frequency, 2), "deflection_mm": round(v.deflection * 1e3, 2), "ok": v.ok})).collect::<Vec<_>>(),
            "others": self.others.iter().map(|o| serde_json::json!({
                "member": o.member, "bim_id": o.bim_id, "code": o.code, "section": o.section, "ratio": round(o.ratio, 3), "governs": o.governs, "ok": o.ok,
                "checks": o.checks.iter().map(|c| format!("{}: {:.2} ({})", c.name, c.ratio, c.detail)).collect::<Vec<_>>(), "notes": o.notes,
            })).collect::<Vec<_>>(),
            "stairs": self.stairs.iter().map(|s| serde_json::json!({
                "stair": s.stair, "bim_id": s.bim_id, "name": s.name, "kind": s.kind, "width_m": round(s.width, 2), "waist_mm": round(s.waist * 1e3, 0), "span_m": round(s.span, 2),
                "wd_kPa": round(s.wd, 2), "wl_kPa": round(s.wl, 2), "wu_kPa": round(s.wu, 2),
                "bars": s.bars.iter().map(|b| format!("{}: {} (Mu {:.1} kN·m/m, As {:.0} / {:.0} mm²/m)", b.location, b.bars, b.mu, b.as_req, b.as_prov)).collect::<Vec<_>>(),
                "Vu_kN_m": round(s.vu, 1), "phiVc_kN_m": round(s.phi_vc, 1), "h_min_mm": round(s.h_min, 0), "reaction_kN_m": [round(s.reaction[0], 2), round(s.reaction[1], 2)],
                "ratio": round(s.ratio, 3), "governs": s.governs, "ok": s.ok, "notes": s.notes,
            })).collect::<Vec<_>>(),
            "slabs": self.slabs.iter().map(|s| serde_json::json!({
                "slab": s.slab, "bim_id": s.bim_id, "size_m": format!("{:.2} x {:.2}", s.lx, s.ly), "h_mm": round(s.h * 1e3, 0), "method": s.method, "wu_kPa": round(s.wu, 2),
                "bars": s.strips.iter().map(|t| format!("{} {}: {} (Mu {:.1} kN·m/m)", t.direction, t.location, t.bars, t.mu)).collect::<Vec<_>>(),
                "h_min_mm": round(s.h_min, 0), "ratio": round(s.ratio, 3), "governs": s.governs, "ok": s.ok, "notes": s.notes,
            })).collect::<Vec<_>>(),
            "steel": self.steel.iter().map(|s| serde_json::json!({
                "member": s.member, "bim_id": s.bim_id, "section": s.section, "mass_kg_m": round(s.mass, 1), "length_m": round(s.length, 3),
                "ratio": round(s.ratio, 3), "governs": s.governs, "ok": s.ok,
                "checks": s.checks.iter().map(|c| format!("{}: {:.2} ({})", c.name, c.ratio, c.detail)).collect::<Vec<_>>(), "notes": s.notes,
            })).collect::<Vec<_>>(),
        })
    }
}

pub fn round(x: f64, n: i32) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let k = 10f64.powi(n);
    (x * k).round() / k
}

pub fn zone_bars(b: &rc::BeamDesign, face: &str) -> String {
    b.flexure.iter().filter(|f| f.face == face).map(|f| format!("{} {}", f.zone, f.bars)).collect::<Vec<_>>().join(", ")
}

/// The ULS combinations, and the service dead and dead + live results.
pub struct Results {
    pub uls: Vec<(String, CaseResult)>,
    pub sls: Vec<(String, CaseResult)>,
    pub dead: CaseResult,
    pub dead_live: CaseResult,
}

pub fn results(model: &Model, a: &Analysis) -> Results {
    let mut uls: Vec<(String, CaseResult)> = model.combos.iter().filter(|c| c.kind == ComboKind::Uls).map(|c| (c.name.clone(), a.second.get(&c.id).map(|s| s.0.clone()).unwrap_or_else(|| a.combine(&c.name, &c.factors)))).collect();
    uls.extend(crate::seismic::combos(model, a));
    uls.extend(crate::advanced::moving_combos(model, a));
    let d: Vec<(Id, f64)> = model.cases.iter().filter(|c| c.kind == CaseKind::D).map(|c| (c.id, 1.0)).collect();
    let dl: Vec<(Id, f64)> = model.cases.iter().filter(|c| matches!(c.kind, CaseKind::D | CaseKind::L | CaseKind::Lr)).map(|c| (c.id, 1.0)).collect();
    let sls = model.combos.iter().filter(|c| c.kind == ComboKind::Sls).map(|c| (c.name.clone(), a.second.get(&c.id).map(|s| s.0.clone()).unwrap_or_else(|| a.combine(&c.name, &c.factors)))).collect();
    Results { uls, sls, dead: a.combine("D", &d), dead_live: a.combine("D + L", &dl) }
}

/// A member's stations under some combinations: (combination, x, N, Vy, Vz, My, Mz).
fn stations(rs: &[(String, CaseResult)], id: Id) -> crate::othermat::Stations {
    rs.iter().filter_map(|(n, r)| r.member(id).map(|mr| (n, mr))).flat_map(|(n, mr)| mr.stations.iter().map(move |s| (n.clone(), s.x, s.n, s.vy, s.vz, s.my, s.mz))).collect()
}

/// Whether a member continues past its end at `node`: another beam meets it there in line.
fn continues(model: &Model, m: &Member, node: Id) -> bool {
    let Some((ex, _, _)) = axes(model, m) else { return false };
    model.members.iter().filter(|o| o.id != m.id && (o.i == node || o.j == node) && model.kind(o) == MemberKind::Beam).any(|o| {
        axes(model, o).is_some_and(|(ox, _, _)| (ex[0] * ox[0] + ex[1] * ox[1] + ex[2] * ox[2]).abs() > 0.985)
    })
}

/// A member end that nothing holds: no other member and no support.
fn free_end(model: &Model, m: &Member, node: Id) -> bool {
    !model.supports.iter().any(|s| s.node == node) && !model.members.iter().any(|o| o.id != m.id && (o.i == node || o.j == node))
}

fn own_deflection(model: &Model, m: &Member, r: &CaseResult, cantilever: bool) -> f64 {
    let Some(mr) = r.member(m.id) else { return 0.0 };
    let Some((_, _, ez)) = axes(model, m) else { return 0.0 };
    if cantilever {
        let root = if free_end(model, m, m.i) { mr.stations.last() } else { mr.stations.first() };
        let root = root.map(|s| s.u).unwrap_or([0.0; 3]);
        return mr.stations.iter().map(|s| -((s.u[0] - root[0]) * ez[0] + (s.u[1] - root[1]) * ez[1] + (s.u[2] - root[2]) * ez[2])).fold(0.0, f64::max);
    }
    mr.local_deflection(ez).into_iter().map(|d| -d).fold(0.0, f64::max)
}

fn max_sag(r: &CaseResult, id: Id) -> f64 {
    r.member(id).map(|m| m.stations.iter().map(|s| s.my).fold(0.0, f64::max)).unwrap_or(0.0)
}

pub fn beam_demand(model: &Model, m: &Member, res: &Results) -> BeamDemand {
    let cant = free_end(model, m, m.i) || free_end(model, m, m.j);
    BeamDemand {
        span: model.length(m),
        uls: res.uls.iter().filter_map(|(_, r)| r.member(m.id)).map(|mr| mr.stations.iter().map(|s| (s.x, s.my, s.vz, s.t, s.n)).collect()).collect(),
        ma_d: max_sag(&res.dead, m.id),
        ma_dl: max_sag(&res.dead_live, m.id),
        defl_d: own_deflection(model, m, &res.dead, cant),
        defl_dl: own_deflection(model, m, &res.dead_live, cant),
        cont_i: continues(model, m, m.i),
        cont_j: continues(model, m, m.j),
        cantilever: cant,
    }
}

pub fn column_demand(model: &Model, m: &Member, res: &Results) -> rc::ColumnDemand {
    rc::ColumnDemand {
        lu: model.length(m),
        k: m.k.unwrap_or(1.0),
        uls: res.uls.iter().filter_map(|(n, r)| r.member(m.id).map(|mr| (n.clone(), mr.stations.iter().map(|s| (s.x, s.n, s.my, s.mz, s.vy, s.vz)).collect()))).collect(),
        second_order: model.design.second_order,
    }
}

pub fn steel_demand(model: &Model, m: &Member, res: &Results) -> SteelDemand {
    let cant = free_end(model, m, m.i) || free_end(model, m, m.j);
    let len = model.length(m);
    let dl = own_deflection(model, m, &res.dead_live, cant);
    let d = own_deflection(model, m, &res.dead, cant);
    SteelDemand {
        length: len,
        lb: m.lb.unwrap_or(len),
        k: m.k.unwrap_or(1.0),
        uls: res.uls.iter().filter_map(|(n, r)| r.member(m.id).map(|mr| (n.clone(), mr.stations.iter().map(|s| (s.x, s.n, s.vy, s.vz, s.my, s.mz)).collect()))).collect(),
        defl_live: (dl - d).max(0.0),
        defl_total: dl,
        cantilever: cant,
        top_flange_load: model.kind(m) == MemberKind::Beam,
        tension: Some(m.tension.clone().unwrap_or_else(|| model.design.tension_connection.clone())),
    }
}

/// Continuity of a slab's edges (bottom, right, top, left): another panel at the same level
/// shares at least half of the edge.
pub fn slab_continuity(model: &Model, s: &Slab) -> [bool; 4] {
    let mut out = [false; 4];
    for (k, (a, b)) in crate::slabload::edges(s).iter().enumerate() {
        let horizontal = (a[1] - b[1]).abs() < 1e-9;
        let (lo, hi) = if horizontal { (a[0].min(b[0]), a[0].max(b[0])) } else { (a[1].min(b[1]), a[1].max(b[1])) };
        for o in model.slabs.iter().filter(|o| o.id != s.id && (o.z - s.z).abs() < 0.05) {
            for (c, d) in crate::slabload::edges(o) {
                let oh = (c[1] - d[1]).abs() < 1e-9;
                if oh != horizontal {
                    continue;
                }
                let same_line = if horizontal { (c[1] - a[1]).abs() < 0.02 } else { (c[0] - a[0]).abs() < 0.02 };
                if !same_line {
                    continue;
                }
                let (olo, ohi) = if horizontal { (c[0].min(d[0]), c[0].max(d[0])) } else { (c[1].min(d[1]), c[1].max(d[1])) };
                if (hi.min(ohi) - lo.max(olo)) >= 0.5 * (hi - lo) {
                    out[k] = true;
                }
            }
        }
    }
    out
}

fn rc_mat(model: &Model, m: &Member) -> RcMat {
    let mt = model.material(m.material);
    let fc = mt.map(|x| x.fc).filter(|f| *f > 0.0).unwrap_or(24.0);
    RcMat { fc, ec: mt.map(|x| x.e).unwrap_or(4700.0 * fc.sqrt()), fy: model.design.fy, fyt: model.design.fyt }
}

fn h_dims(sh: &Shape) -> Option<HDims> {
    match *sh {
        Shape::H { d, bf, tw, tf, r } => Some(HDims { d: d * 1e3, bf: bf * 1e3, tw: tw * 1e3, tf: tf * 1e3, r: r * 1e3 }),
        _ => None,
    }
}

/// One design pass on an analysis.
pub fn design_pass(model: &Model, a: &Analysis, o: &Options) -> Report {
    let _standard = crate::codes::use_standard(model.design.standard);
    let res = results(model, a);
    let mut rep = Report { warnings: a.warnings.clone(), ..Default::default() };
    if res.uls.is_empty() {
        rep.warnings.push("no ULS combination: nothing designed".into());
        return rep;
    }
    let wanted = |id: Id| o.members.is_empty() || o.members.contains(&id);
    let mut skipped_rc: Vec<(Id, &str)> = vec![];
    for m in model.members.iter().filter(|m| wanted(m.id) && m.role.is_frame()) {
        let Some(sec) = model.section(m.section) else { continue };
        let kind = model.kind(m);
        if model.is_concrete(m) {
            if !o.beams {
                continue;
            }
            match (kind, &sec.shape) {
                (MemberKind::Beam, Shape::Rect { b, h }) => {
                    let dem = beam_demand(model, m, &res);
                    let mat = rc_mat(model, m);
                    let mut d = rc::design_beam(m.id, m.bim_id.clone(), *b, *h, &dem, &mat, &model.design);
                    if model.design.ec2_check {
                        d.ec2 = Some(crate::codes::check_ec2_beam(&d, &dem, mat.fc, mat.fy, mat.fyt, model.design.cover_beam));
                    }
                    rep.beams.push(d);
                }
                (MemberKind::Beam, _) => rep.skipped.push(format!("member {}: a concrete beam needs a rectangular section to be designed", m.id)),
                (MemberKind::Column, Shape::Rect { b, h }) => {
                    let dem = column_demand(model, m, &res);
                    rep.columns.push(rc::design_column(m.id, m.bim_id.clone(), *b, *h, &dem, &rc_mat(model, m), &model.design));
                }
                (k, _) => skipped_rc.push((m.id, if k == MemberKind::Column { "column" } else { "brace" })),
            }
        } else if let (Some(mt), true) = (model.material(m.material), matches!(model.material(m.material).map(|x| x.kind), Some(MaterialKind::Timber | MaterialKind::Masonry))) {
            let Shape::Rect { b, h } = sec.shape else {
                rep.skipped.push(format!("member {}: timber and masonry members need a rectangle", m.id));
                continue;
            };
            let len = model.length(m);
            if mt.kind == MaterialKind::Timber {
                // allowable stress design: the service combinations
                let st = stations(&res.sls, m.id);
                rep.others.push(crate::othermat::timber(m, mt, b, h, len, &st, &model.design));
            } else {
                let st = stations(&res.uls, m.id);
                rep.others.push(crate::othermat::masonry(m, mt, b, h, len, &st, &model.design));
            }
        } else if let (true, Shape::LippedC { h, b, d, t }) = (model.is_steel(m), &sec.shape) {
            if !o.steel {
                continue;
            }
            let mt = model.material(m.material).unwrap();
            let st = stations(&res.uls, m.id);
            match crate::othermat::cold_formed(m, mt, *h, *b, *d, *t, model.length(m), &st, &model.design) {
                Ok(dsg) => rep.others.push(dsg),
                Err(e) => rep.skipped.push(format!("member {}: {e}", m.id)),
            }
        } else if model.is_steel(m) {
            if !o.steel {
                continue;
            }
            let Some(mt) = model.material(m.material) else { continue };
            let dem = steel_demand(model, m, &res);
            match h_dims(&sec.shape) {
                Some(hd) => {
                    let mass = sec.props().a * 7850.0;
                    rep.steel.push(steel_check(model, m.id, m.bim_id.clone(), &sec.name, mass, &sec.props(), hd, mt.fy, mt.fu, &dem));
                }
                None => rep.skipped.push(format!("member {} ({}): only H shapes are checked; design with select to choose one", m.id, sec.name)),
            }
        }
    }
    for kind in ["column", "brace"] {
        let ids: Vec<String> = skipped_rc.iter().filter(|(_, k)| *k == kind).map(|(i, _)| i.to_string()).collect();
        if !ids.is_empty() {
            rep.skipped.push(format!("{} concrete {kind}(s) analysed but not designed (Pondus designs concrete beams and slabs): members {}", ids.len(), ids.join(", ")));
        }
    }
    // the walls, pier by pier (one row per wall and storey)
    let special = model.seismic.enabled;
    for w in &model.walls {
        let mut piers: Vec<&Member> = model.members.iter().filter(|m| m.role == Role::WallPier(w.id) && wanted(m.id)).collect();
        piers.sort_by(|a, b| model.node(a.i).map(|n| n.z).unwrap_or(0.0).total_cmp(&model.node(b.i).map(|n| n.z).unwrap_or(0.0)));
        let fc = model.material(w.material).map(|x| x.fc).filter(|f| *f > 0.0).unwrap_or(24.0);
        for m in piers {
            let uls: crate::walls::Stations = res.uls.iter().filter_map(|(n, r)| r.member(m.id).map(|mr| (n.clone(), mr.stations.iter().map(|s| (s.x, s.n, s.vy, s.vz, s.my, s.mz)).collect()))).collect();
            let (z0, z1) = (model.node(m.i).map(|n| n.z).unwrap_or(0.0), model.node(m.j).map(|n| n.z).unwrap_or(0.0));
            rep.walls.push(crate::walls::design_pier(w, m, z0.min(z1), z0.max(z1), fc, model.design.fy, &uls, special, &model.design));
        }
    }
    if o.footings && o.beams {
        rep.footings = crate::footing::design(model, a, &rep.columns);
        if model.walls.iter().any(|w| model.supports.iter().any(|s| model.members.iter().any(|m| m.role == Role::WallPier(w.id) && (m.i == s.node || m.j == s.node)))) {
            rep.skipped.push("wall footings are not designed (a strip footing or a raft under each wall is for the engineer)".into());
        }
    }
    // the steel beams' end connections
    if o.steel {
        // beams and the sloping rafters (braces by inclination), not columns
        for m in model.members.iter().filter(|m| wanted(m.id) && model.is_steel(m) && model.kind(m) != MemberKind::Column) {
            let (Some(sec), Some(mt)) = (model.section(m.section), model.material(m.material)) else { continue };
            // an H shape by its web; a lipped channel (purlins, rafters) by its web plate t,
            // the bolts bearing on the thin web
            let (d, bf, tw, tf, r) = match sec.shape {
                Shape::H { d, bf, tw, tf, r } => (d, bf, tw, tf, r),
                Shape::LippedC { h, b, t, .. } => (h, b, t, t, 0.0),
                _ => continue,
            };
            for (node, end, first) in [(m.i, "start", true), (m.j, "end", false)] {
                if free_end(model, m, node) {
                    continue;
                }
                let vu = res.uls.iter().filter_map(|(_, r)| r.member(m.id)).map(|mr| {
                    let s = if first { mr.stations.first() } else { mr.stations.last() };
                    s.map(|s| s.vz.abs().max(s.vy.abs())).unwrap_or(0.0)
                }).fold(0.0, f64::max);
                rep.connections.push(crate::connections::shear_tab(m.id, end, vu, (d, bf, tw, tf, r), mt.fu, &model.design.connection));
            }
        }
    }
    // floor vibration: the beams' own deflection under D + their live share
    if model.design.vibration_hz > 0.0 {
        let share: Vec<(Id, f64)> = model.cases.iter().map(|c| (c.id, match c.kind { CaseKind::D => 1.0, CaseKind::L => model.seismic.live_mass, _ => 0.0 })).collect();
        let r = a.combine("D + live share", &share);
        let beams: Vec<(Id, f64, f64)> = model.members.iter().filter(|m| wanted(m.id) && m.role.is_frame() && model.kind(m) == MemberKind::Beam && !free_end(model, m, m.i) && !free_end(model, m, m.j)).map(|m| (m.id, model.length(m), own_deflection(model, m, &r, false))).collect();
        rep.vibration = crate::connections::vibration(&beams, model.design.vibration_hz);
    }
    if o.do_slabs {
        let gamma = model.materials.iter().find(|m| m.kind == MaterialKind::Concrete).map(|m| m.gamma).unwrap_or(24.0);
        let mat = model.materials.iter().find(|m| m.kind == MaterialKind::Concrete).map(|c| RcMat { fc: c.fc.max(1.0), ec: c.e, fy: model.design.fy, fyt: model.design.fyt }).unwrap_or(RcMat { fc: 24.0, ec: 23_000.0, fy: model.design.fy, fyt: model.design.fyt });
        for s in model.slabs.iter().filter(|s| o.slabs.is_empty() || o.slabs.contains(&s.id)) {
            if model.plate_slab(s) {
                let uls: Vec<Vec<&crate::slabfe::PlateResult>> = res.uls.iter().map(|(_, r)| r.plates.iter().filter(|p| p.slab == s.id).collect()).collect();
                let sls: Vec<&crate::slabfe::PlateResult> = res.dead_live.plates.iter().filter(|p| p.slab == s.id).collect();
                // the columns on the panel and the supports under it: punching is checked there
                let punch = crate::slabfe::punch_points(model, s, &res.uls);
                rep.slabs.push(crate::slabfe::design(s, &uls, &sls, &punch, &mat, &model.design));
            } else {
                let cont = slab_continuity(model, s);
                rep.slabs.push(rc::design_slab(s, cont, gamma * s.h + s.dead, s.live, &mat, &model.design));
            }
        }
        for s in &model.stairs {
            rep.stairs.push(crate::stairs::design(s, &mat, &model.design, gamma));
        }
    }
    rep
}

/// A steel member's check to the model's steel code.
#[allow(clippy::too_many_arguments)]
fn steel_check(model: &Model, id: Id, bim: Option<String>, name: &str, mass: f64, p: &crate::model::Props, hd: HDims, fy: f64, fu: f64, dem: &SteelDemand) -> steel::SteelDesign {
    match model.design.steel_code {
        crate::codes::SteelCode::Aisc => steel::check(id, bim, name, mass, p, hd, fy, fu, dem, &model.design),
        crate::codes::SteelCode::Ec3 => crate::codes::check_ec3(id, bim, name, mass, p, hd, fy, fu, dem, &model.design),
    }
}

/// The lightest catalogue shape that passes for a member's demand (deflections scaled by
/// the stiffness ratio until the model is analysed again).
fn select_shape(model: &Model, m: &Member, dem: &SteelDemand) -> Option<crate::sections::HShape> {
    let mt = model.material(m.material)?;
    let cur = model.section(m.section)?.props();
    let names = &model.design.steel_shapes;
    for s in crate::sections::catalogue().into_iter().filter(|s| names.is_empty() || names.iter().any(|n| crate::sections::find(n).is_some_and(|x| x.name == s.name))) {
        let p = crate::sections::h_props(s.d / 1e3, s.bf / 1e3, s.tw / 1e3, s.tf / 1e3, s.r / 1e3);
        let mut d = dem.clone();
        if cur.iy > 0.0 {
            d.defl_live *= cur.iy / p.iy;
            d.defl_total *= cur.iy / p.iy;
        }
        let hd = HDims { d: s.d, bf: s.bf, tw: s.tw, tf: s.tf, r: s.r };
        let (dmin, dmax) = (model.design.steel_min_depth, model.design.steel_max_depth);
        if (dmin > 0.0 && s.d < dmin) || (dmax > 0.0 && s.d > dmax) {
            continue;
        }
        if steel_check(model, m.id, None, &s.name, s.mass(), &p, hd, mt.fy, mt.fu, &d).ok {
            return Some(s);
        }
    }
    None
}

/// The lightest lipped channel of the catalogue that passes the cold-formed checks for a
/// member's stations (`steel_shapes` names starting with C limit the choice).
fn select_lipped(model: &Model, m: &Member, st: &crate::othermat::Stations) -> Option<crate::sections::LippedShape> {
    let mt = model.material(m.material)?;
    let names: Vec<(f64, f64, f64, f64)> = model.design.steel_shapes.iter().filter_map(|n| parse_lipped(n)).collect();
    let len = model.length(m);
    for s in crate::sections::lipped_catalogue() {
        if !names.is_empty() && !names.iter().any(|&(h, b, d, t)| (h - s.h).abs() < 1e-6 && (b - s.b).abs() < 1e-6 && (d - s.d).abs() < 1e-6 && (t - s.t).abs() < 1e-6) {
            continue;
        }
        if crate::othermat::cold_formed(m, mt, s.h, s.b, s.d, s.t, len, st, &model.design).is_ok_and(|d| d.ok) {
            return Some(s);
        }
    }
    None
}

/// Designs the model, resizing and re-analysing when asked. The model's sections change in
/// place when `resize` or `select` changes them.
pub fn run(model: &mut Model, o: &Options) -> Result<(Report, Analysis), String> {
    if let Some(sm) = o.slab_method {
        model.design.slab_method = sm;
    }
    let mut changes = vec![];
    for round in 1..=6 {
        let a = analyze(model)?;
        let mut rep = design_pass(model, &a, o);
        rep.rounds = round;
        let mut changed = false;
        if o.select {
            // names the catalogue does not know would silently narrow the choice
            if round == 1 {
                for n in &model.design.steel_shapes {
                    let lipped_known = parse_lipped(n).is_some_and(|(h, b, d, t)| crate::sections::lipped_catalogue().iter().any(|s| (h - s.h).abs() < 1e-6 && (b - s.b).abs() < 1e-6 && (d - s.d).abs() < 1e-6 && (t - s.t).abs() < 1e-6));
                    if !lipped_known && crate::sections::find(n).is_none() {
                        rep.warnings.push(format!("steel_shapes: '{n}' is not in the catalogue (lipped channels: {}; H shapes by their catalogue names)", crate::sections::lipped_catalogue().iter().map(|s| s.name.clone()).collect::<Vec<_>>().join(", ")));
                    }
                }
            }
            let res = results(model, &a);
            let wanted = |id: Id| o.members.is_empty() || o.members.contains(&id);
            // picks: (member, group, section name, mass kg/m); a lipped channel stays a
            // lipped channel (purlins, rafters), an H shape an H shape
            let mut picks: Vec<(Id, Option<String>, String, f64)> = vec![];
            for m in model.members.iter().filter(|m| model.is_steel(m) && wanted(m.id)) {
                let lipped = model.section(m.section).is_some_and(|s| matches!(s.shape, Shape::LippedC { .. }));
                let pick = if lipped {
                    let st = stations(&res.uls, m.id);
                    select_lipped(model, m, &st).map(|s| (s.name.clone(), s.mass()))
                } else {
                    let dem = steel_demand(model, m, &res);
                    select_shape(model, m, &dem).map(|s| (s.name.clone(), s.mass()))
                };
                // the pieces of one BIM member (a rafter between its purlins) are one group:
                // a physical member has one section
                let group = m.group.clone().or_else(|| m.bim_id.clone());
                match pick {
                    Some((name, mass)) => picks.push((m.id, group, name, mass)),
                    None => {
                        rep.warnings.push(format!("member {}: no catalogue shape passes", m.id));
                        // the heaviest allowed shape, so its group goes up to it
                        let heaviest = if lipped {
                            let names: Vec<(f64, f64, f64, f64)> = model.design.steel_shapes.iter().filter_map(|n| parse_lipped(n)).collect();
                            crate::sections::lipped_catalogue().into_iter().filter(|s| names.is_empty() || names.iter().any(|&(h, b, d, t)| (h - s.h).abs() < 1e-6 && (b - s.b).abs() < 1e-6 && (d - s.d).abs() < 1e-6 && (t - s.t).abs() < 1e-6)).last().map(|s| (s.name.clone(), s.mass()))
                        } else {
                            let names = &model.design.steel_shapes;
                            crate::sections::catalogue().into_iter().filter(|s| names.is_empty() || names.iter().any(|n| crate::sections::find(n).is_some_and(|x| x.name == s.name))).max_by(|a, b| a.mass().total_cmp(&b.mass())).map(|s| (s.name.clone(), s.mass()))
                        };
                        if let Some((name, mass)) = heaviest {
                            picks.push((m.id, group, name, mass));
                        }
                    }
                }
            }
            // a group takes its heaviest pick
            let mut by_group: std::collections::HashMap<String, (String, f64)> = Default::default();
            for (_, g, name, mass) in &picks {
                if let Some(g) = g {
                    let e = by_group.entry(g.clone()).or_insert_with(|| (name.clone(), *mass));
                    if *mass > e.1 {
                        *e = (name.clone(), *mass);
                    }
                }
            }
            for (id, g, name, _) in picks {
                let name = g.and_then(|g| by_group.get(&g).map(|x| x.0.clone())).unwrap_or(name);
                let sid = model.section_named(&name)?;
                let m = model.members.iter_mut().find(|m| m.id == id).unwrap();
                if m.section != sid {
                    m.section = sid;
                    changed = true;
                    changes.push(format!("member {id}: {name}"));
                }
            }
        }
        if o.resize {
            for c in rep.columns.iter().filter(|c| !c.ok) {
                let (b, h) = (((c.b + 0.05) * 1000.0).round() / 1000.0, ((c.h + 0.05) * 1000.0).round() / 1000.0);
                if b > 1.5 {
                    rep.warnings.push(format!("column {}: still failing at 1500 mm, left as it is", c.member));
                    continue;
                }
                let sid = model.rect_section(b, h);
                if let Some(m) = model.members.iter_mut().find(|m| m.id == c.member) {
                    m.section = sid;
                    changed = true;
                    changes.push(format!("column {}: {}", c.member, rect_name(b, h)));
                }
            }
            for b in rep.beams.iter().filter(|b| !b.ok) {
                let (mut bw, mut h) = (b.b, b.h + 0.05);
                if h > 3.0 * bw {
                    bw += 0.05;
                }
                if h > 1.5 {
                    rep.warnings.push(format!("beam {}: still failing at 1500 mm deep, left as it is", b.member));
                    continue;
                }
                h = (h * 1000.0).round() / 1000.0;
                bw = (bw * 1000.0).round() / 1000.0;
                let sid = model.rect_section(bw, h);
                if let Some(m) = model.members.iter_mut().find(|m| m.id == b.member) {
                    m.section = sid;
                    changed = true;
                    changes.push(format!("beam {}: {}", b.member, rect_name(bw, h)));
                }
            }
        }
        if o.resize {
            // a failing wall is made 50 mm thicker (its bars are already the least that pass)
            let mut thicker: Vec<Id> = rep.walls.iter().filter(|w| !w.ok).map(|w| w.wall).collect();
            thicker.sort();
            thicker.dedup();
            for id in thicker {
                if let Some(w) = model.walls.iter_mut().find(|w| w.id == id) {
                    if w.t >= 0.8 - 1e-9 {
                        rep.warnings.push(format!("wall {}: still failing at 800 mm, left as it is", w.name));
                        continue;
                    }
                    w.t = ((w.t + 0.05) * 1000.0).round() / 1000.0;
                    changed = true;
                    changes.push(format!("wall {}: t {:.0} mm", w.name, w.t * 1000.0));
                }
            }
            if changed {
                crate::walls::sync(model);
            }
            for d in rep.slabs.iter().filter(|d| !d.ok) {
                // up to the minimum thickness, else 10 mm more, in 10 mm steps
                let h = ((d.h_min / 1000.0).max(d.h + 0.01) * 100.0 - 1e-6).ceil() / 100.0;
                if h > 0.4 {
                    rep.warnings.push(format!("slab {}: still failing at 400 mm, left as it is", d.slab));
                    continue;
                }
                if let Some(s) = model.slabs.iter_mut().find(|s| s.id == d.slab) {
                    s.h = h;
                    changed = true;
                    changes.push(format!("slab {}: h {:.0} mm", d.slab, h * 1000.0));
                }
            }
            for d in rep.stairs.iter().filter(|d| !d.ok) {
                // a flight's waist or a landing's thickness: up to L/20 or L/24, else 25 mm more
                let h = ((d.h_min / 1000.0).max(d.waist + 0.025) * 40.0 - 1e-6).ceil() / 40.0;
                if h > 0.35 {
                    rep.warnings.push(format!("stair {}: still failing at 350 mm, left as it is", d.stair));
                    continue;
                }
                if let Some(s) = model.stairs.iter_mut().find(|s| s.id == d.stair) {
                    s.waist = h;
                    changed = true;
                    changes.push(format!("stair {}: waist {:.0} mm", d.stair, h * 1000.0));
                }
            }
        }
        if !changed || round == 6 {
            // keep only the last change of each member
            let mut last: Vec<String> = vec![];
            for c in changes.iter().rev() {
                let key = c.split(':').next().unwrap_or("").to_string();
                if !last.iter().any(|l| l.starts_with(&format!("{key}:"))) {
                    last.push(c.clone());
                }
            }
            last.reverse();
            rep.changes = last;
            // unused sections go
            let used: Vec<Id> = model.members.iter().map(|m| m.section).collect();
            model.sections.retain(|s| used.contains(&s.id));
            return Ok((rep, a));
        }
    }
    unreachable!()
}

/// The design for the BIM model: per member and slab, the Aedifex family and type it maps to
/// (with the type parameters in m), the reinforcement as text, and the utilisation.
pub fn bim_export(model: &Model, rep: &Report) -> serde_json::Value {
    use serde_json::json;
    let mut elems = vec![];
    // a wall's piers and arms are exported as the wall below, not as members
    for m in model.members.iter().filter(|m| m.role.is_frame()) {
        let Some(sec) = model.section(m.section) else { continue };
        let kind = model.kind(m);
        let (family, params) = match (&sec.shape, kind) {
            (Shape::Rect { b, h }, MemberKind::Column) => ("Concrete Rectangular Column", json!({"Width": b, "Depth": h})),
            (Shape::Rect { b, h }, _) => ("Concrete Rectangular Beam", json!({"Width": b, "Depth": h})),
            (Shape::H { d, bf, tw, tf, .. }, MemberKind::Column) => ("Steel W Column", json!({"Height": d, "Flange_Width": bf, "Web_Thickness": tw, "Flange_Thickness": tf})),
            (Shape::H { d, bf, tw, tf, .. }, _) => ("Steel I Beam (W)", json!({"Height": d, "Flange_Width": bf, "Web_Thickness": tw, "Flange_Thickness": tf})),
            (Shape::LippedC { h, b, d, t }, _) => ("Steel Channel (C)", json!({"Height": h, "Flange_Width": b, "Lip": d, "Web_Thickness": t, "Flange_Thickness": t})),
            (Shape::General { .. }, _) => ("", json!({})),
        };
        let (a, b) = (model.node(m.i).map(|n| n.p()), model.node(m.j).map(|n| n.p()));
        let mut e = json!({
            "member": m.id, "bim_id": m.bim_id, "kind": format!("{kind:?}").to_lowercase(), "material": model.material(m.material).map(|x| x.name.clone()),
            "start": a, "end": b, "section": sec.name, "family": family, "type": sec.name, "type_params_m": params,
        });
        if let Some(d) = rep.beams.iter().find(|d| d.member == m.id) {
            e["reinforcement"] = json!({"top": zone_bars(d, "top"), "bottom": zone_bars(d, "bottom"), "stirrups": d.shear.iter().map(|s| format!("{}: {}", s.zone, s.stirrups)).collect::<Vec<_>>(), "side_bars": d.torsion.as_ref().map(|t| t.side_bars.clone()), "cover_mm": model.design.cover_beam * 1e3});
            e["ratio"] = json!(round(d.ratio, 3));
            e["ok"] = json!(d.ok);
            e["code"] = json!("ACI 318-19");
        } else if let Some(d) = rep.columns.iter().find(|d| d.member == m.id) {
            e["reinforcement"] = json!({"longitudinal": d.bars, "per_face": {"along_b": d.n_b, "along_h": d.n_h}, "rho_percent": round(d.rho * 100.0, 2), "ties": d.ties, "cover_mm": model.design.cover_beam * 1e3});
            e["ratio"] = json!(round(d.ratio, 3));
            e["ok"] = json!(d.ok);
            e["code"] = json!("ACI 318-19");
        } else if let Some(d) = rep.steel.iter().find(|d| d.member == m.id) {
            let conns: Vec<_> = rep.connections.iter().filter(|c| c.member == m.id).map(|c| json!({"end": c.end, "bolts": c.bolts, "plate": c.plate, "weld": c.weld, "ok": c.ok})).collect();
            if !conns.is_empty() {
                e["connections"] = json!(conns);
            }
            e["ratio"] = json!(round(d.ratio, 3));
            e["ok"] = json!(d.ok);
            e["code"] = json!("AISC 360-16 LRFD");
            e["mass_kg_m"] = json!(round(d.mass, 1));
        } else if let Some(d) = rep.others.iter().find(|d| d.member == m.id) {
            e["ratio"] = json!(round(d.ratio, 3));
            e["ok"] = json!(d.ok);
            e["code"] = json!(d.code);
            e["governs"] = json!(d.governs);
            let conns: Vec<_> = rep.connections.iter().filter(|c| c.member == m.id).map(|c| json!({"end": c.end, "bolts": c.bolts, "plate": c.plate, "weld": c.weld, "ok": c.ok})).collect();
            if !conns.is_empty() {
                e["connections"] = json!(conns);
            }
        } else {
            e["ok"] = serde_json::Value::Null;
            e["code"] = json!("not designed");
        }
        elems.push(e);
    }
    let slabs: Vec<_> = model.slabs.iter().map(|s| {
        let cont = slab_continuity(model, s);
        let mut e = json!({"slab": s.id, "bim_id": s.bim_id, "name": s.name, "outline": [[s.x0, s.y0], [s.x1, s.y0], [s.x1, s.y1], [s.x0, s.y1]], "z_top": s.z, "family": "Floor", "type": format!("Concrete - {:.0} mm", s.h * 1e3), "thickness_m": s.h,
            // which edges continue into a neighbouring panel (top bars run over them): the
            // edges y = y0, x = x1, y = y1, x = x0
            "continuous_edges": {"y0": cont[0], "x1": cont[1], "y1": cont[2], "x0": cont[3]}});
        if let Some(d) = rep.slabs.iter().find(|d| d.slab == s.id) {
            e["reinforcement"] = json!(d.strips.iter().map(|t| format!("{} {}: {}", t.direction, t.location, t.bars)).collect::<Vec<_>>());
            e["method"] = json!(d.method);
            if let Some(f) = &d.flat {
                e["flat_plate"] = json!({"lines_x": f.lines_x, "lines_y": f.lines_y, "strip_half_width": f.half_width, "columns": f.columns});
            }
            e["ratio"] = json!(round(d.ratio, 3));
            e["ok"] = json!(d.ok);
            e["code"] = json!("ACI 318-19");
        }
        e
    }).collect();
    // Width along x and Length along y (the BIM family's box), l along x and b along y here
    let footings: Vec<_> = rep.footings.iter().map(|f| {
        let mut e = json!({
            "node": f.node, "column_base": f.at, "family": "Footing - Rectangular", "type": format!("{:.0} x {:.0} x {:.0} mm", f.l * 1e3, f.b * 1e3, f.h * 1e3),
            "type_params_m": {"Width": f.l, "Length": f.b, "Thickness": f.h}, "top_z": f.at[2], "reinforcement": {"bottom_x": f.bars_x, "bottom_y": f.bars_y},
            "ratio": round(f.ratio, 3), "ok": f.ok, "code": "ACI 318-19 / allowable bearing",
        });
        if f.pile_capacity > 0.0 {
            e["family"] = json!("Pile Cap - Rectangular");
            e["notes"] = json!(f.notes);
            e["piles_needed"] = json!(f.piles_needed);
            e["column"] = json!(f.column);
        }
        if !f.piles.is_empty() {
            e["code"] = json!("ACI 318-19 Ch. 13 / pile safe load");
            e["cap_centre"] = json!(f.at);
            e["column"] = json!(f.column);
            e["piles"] = json!(f.piles);
            e["pile"] = json!({"diameter_m": f.pile_diameter, "length_m": f.pile_length, "safe_load_kN": f.pile_capacity, "largest_load_kN": round(f.pile_load, 1)});
            e["notes"] = json!(f.notes);
        }
        e
    }).collect();
    let walls: Vec<_> = model.walls.iter().map(|w| {
        let rows: Vec<_> = rep.walls.iter().filter(|d| d.wall == w.id).map(|d| json!({"z": [round(d.z0, 3), round(d.z1, 3)], "vertical": d.vertical, "horizontal": d.horizontal, "boundary_elements": d.boundary, "ratio": round(d.ratio, 3), "ok": d.ok})).collect();
        let ok = rows.iter().all(|r| r["ok"].as_bool().unwrap_or(true));
        json!({
            "wall": w.id, "bim_id": w.bim_id, "name": w.name, "start": [w.a[0], w.a[1], w.z0], "end": [w.b[0], w.b[1], w.z0], "base_z": w.z0, "top_z": w.z1,
            "family": "Basic Wall", "type": format!("Concrete - {:.0} mm", w.t * 1e3), "thickness_m": w.t, "structural": true,
            "material": model.material(w.material).map(|x| x.name.clone()), "reinforcement_by_storey": rows, "ok": if rep.walls.iter().any(|d| d.wall == w.id) { json!(ok) } else { serde_json::Value::Null },
            "code": "ACI 318-19 Ch. 11 / 18.10",
        })
    }).collect();
    let conc = model.materials.iter().find(|m| m.kind == MaterialKind::Concrete);
    json!({
        "format": "pondus-bim-1",
        // what the bars were designed with, so the BIM side can lay them out and check them
        "settings": {"code": "ACI 318-19", "fc_MPa": conc.map(|m| m.fc), "fy_MPa": model.design.fy, "fyt_MPa": model.design.fyt, "cover_beam_mm": model.design.cover_beam * 1e3, "cover_slab_mm": model.design.cover_slab * 1e3, "cover_footing_mm": model.design.footing.cover * 1e3, "aggregate_mm": model.design.aggregate, "piles": model.design.footing.piles.as_ref().map(|p| json!({"spacing_m": p.s(), "edge_m": p.e(), "diameter_m": p.diameter, "length_m": p.length, "safe_load_kN": p.capacity, "bounds": p.bounds}))},
        "walls": walls,
        "footings": footings,
        "units": "m, kN, MPa",
        "model": model.name,
        "how_to_apply": "In Aedifex: for each member find the element by bim_id (or by start/end), duplicate_type of 'family' with type_params_m to get 'type' if the project lacks it, then change_type; write the reinforcement into the element's comments/marks. Slabs: set the floor type's thickness. Walls: a 'Basic Wall' of the given type (one concrete layer of thickness_m) from start to end, base_z to top_z, marked structural.",
        "members": elems,
        "stairs": model.stairs.iter().map(|s| {
            let mut e = json!({"stair": s.id, "bim_id": s.bim_id, "name": s.name, "kind": s.kind, "width_m": s.width, "waist_m": s.waist, "span_m": s.span, "riser_m": s.riser, "tread_m": s.tread, "risers": s.risers, "fixity": s.fixity, "z0": s.z0, "z1": s.z1});
            if let Some(d) = rep.stairs.iter().find(|d| d.stair == s.id) {
                e["reinforcement"] = json!(d.bars.iter().map(|b| json!({"location": b.location, "bars": b.bars})).collect::<Vec<_>>());
                e["ratio"] = json!(round(d.ratio, 3));
                e["ok"] = json!(d.ok);
                e["code"] = json!("ACI 318-19");
                e["h_min_mm"] = json!(round(d.h_min, 0));
                e["reaction_kN_m"] = json!([round(d.reaction[0], 2), round(d.reaction[1], 2)]);
                e["notes"] = json!(d.notes);
            }
            e
        }).collect::<Vec<_>>(),
        "slabs": slabs,
    })
}
