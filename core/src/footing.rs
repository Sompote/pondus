// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Spread footings under the supported columns: sized for the allowable bearing pressure under
//! service loads, made thick enough for one-way and punching shear under factored loads, and
//! reinforced for the cantilever moment at the column face. The checks are the Stabileo
//! engine's (`postprocess::foundation_check`: Meyerhof effective area for eccentric loads,
//! overturning, sliding, ACI 318-19 shear with the size factor λs); the sizing and the bars are
//! Pondus's. Loads are the support reactions of the gravity combinations (the earthquake's
//! envelope has no signed reactions: footings under E are not checked).

use dedaliano_engine::postprocess::foundation_check::*;
use serde::{Deserialize, Serialize};

use crate::analysis::Analysis;
use crate::model::*;
use crate::rc;
use crate::sections::bar_name;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FootingSettings {
    /// Allowable (service) bearing pressure, kPa; depth of the footing base below ground, m;
    /// soil unit weight, kN/m³; base friction coefficient; least thickness, m.
    pub q_allow: f64,
    pub depth: f64,
    pub gamma_soil: f64,
    pub mu: f64,
    pub min_h: f64,
    /// Least plan side, m (0: as the bearing pressure needs).
    #[serde(default)]
    pub min_b: f64,
    /// Bars to choose from, mm, and the clear cover to the bottom bars, m.
    pub bars: Vec<f64>,
    pub cover: f64,
    /// Pile caps instead of spread footings: the piles' safe load and layout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub piles: Option<PileSettings>,
}

impl Default for FootingSettings {
    fn default() -> Self {
        FootingSettings { q_allow: 150.0, depth: 1.5, gamma_soil: 18.0, mu: 0.5, min_h: 0.3, min_b: 0.0, bars: vec![12.0, 16.0, 20.0, 25.0], cover: 0.075, piles: None }
    }
}

/// Piles under the columns: each column's cap stands on a group of piles of a safe (service)
/// load `capacity`, kN, at `spacing` centres (0: 3 diameters) with `edge` from a pile's centre
/// to the cap's edge (0: half a diameter plus 150 mm); `length` (m) is for the model only (it
/// comes from the site investigation with the capacity). `bounds` [x0, y0, x1, y1] are the site
/// lines the caps must stay inside (a cap at a boundary column then takes one row of piles
/// along the boundary, its group shifted inward: the support sits at the group's centroid and
/// a rigid link carries the column to it, so the ground beams take the eccentric moment).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PileSettings {
    pub capacity: f64,
    pub diameter: f64,
    pub spacing: f64,
    pub edge: f64,
    pub length: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f64; 4]>,
}

impl Default for PileSettings {
    fn default() -> Self {
        PileSettings { capacity: 300.0, diameter: 0.3, spacing: 0.0, edge: 0.0, length: 12.0, bounds: None }
    }
}

impl PileSettings {
    pub fn s(&self) -> f64 {
        if self.spacing > 0.0 { self.spacing } else { 3.0 * self.diameter }
    }
    pub fn e(&self) -> f64 {
        if self.edge > 0.0 { self.edge } else { self.diameter / 2.0 + 0.15 }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FootingDesign {
    pub node: Id,
    pub at: [f64; 3],
    /// Plan length along x, width along y, thickness, m.
    pub l: f64,
    pub b: f64,
    pub h: f64,
    pub bars_x: String,
    pub bars_y: String,
    /// Service pressure and its ratio; safety factors; shear ratios (factored).
    pub q_max: f64,
    pub bearing_ratio: f64,
    pub overturning_sf: f64,
    pub sliding_sf: f64,
    pub oneway_ratio: f64,
    pub punching_ratio: f64,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
    /// A pile cap: its piles (plan points, m), the largest service load on one (kN) against
    /// the safe load, the pile diameter and length (m), and the column's centre (the cap may sit
    /// off it at a boundary).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub piles: Vec<[f64; 2]>,
    pub pile_load: f64,
    pub pile_capacity: f64,
    pub pile_diameter: f64,
    pub pile_length: f64,
    pub column: [f64; 2],
    /// When no layout fits: how many piles the column needs (so the BIM side can move the cap
    /// in from the site lines to take a row of them).
    pub piles_needed: usize,
}

fn run_check(l: f64, b: f64, h: f64, cl: f64, cw: f64, fs: &FootingSettings, fc: f64, p: f64, mx: f64, my: f64, hz: f64) -> SpreadFootingResult {
    let data = SpreadFootingData { footing_id: 1, length: l, width: b, thickness: h, depth: fs.depth, q_allowable: fs.q_allow * 1e3, gamma_soil: fs.gamma_soil * 1e3, fc: fc * 1e6, col_length: cl, col_width: cw, d: Some(h - fs.cover - 0.016), mu_sliding: Some(fs.mu), rho_w: None };
    let f = SpreadFootingForces { footing_id: 1, p: p * 1e3, mx: Some(mx * 1e3), my: Some(my * 1e3), h: Some(hz * 1e3) };
    check_spread_footings(&SpreadFootingInput { footings: vec![data], forces: vec![f] }).remove(0)
}

/// The factored moment per metre of width at the column face (kN·m/m) under the net factored
/// pressure qu (kPa) on a cantilever c (m) from the face to the edge: qu c²/2.
pub fn face_moment(qu: f64, c: f64) -> f64 {
    qu * c * c / 2.0
}

/// Designs a footing under every supported node that carries a concrete column.
pub fn design(model: &Model, a: &Analysis, columns: &[crate::rc::ColumnDesign]) -> Vec<FootingDesign> {
    let fs = &model.design.footing;
    let conc = model.materials.iter().find(|m| m.kind == MaterialKind::Concrete);
    let (fc, gamma_c) = (conc.map(|m| m.fc).filter(|f| *f > 0.0).unwrap_or(24.0), conc.map(|m| m.gamma).unwrap_or(24.0));
    let uls: Vec<crate::analysis::CaseResult> = model.combos.iter().filter(|c| c.kind == ComboKind::Uls).map(|c| a.second.get(&c.id).map(|s| s.0.clone()).unwrap_or_else(|| a.combine(&c.name, &c.factors))).collect();
    let sls: Vec<crate::analysis::CaseResult> = model.combos.iter().filter(|c| c.kind == ComboKind::Sls).map(|c| a.combine(&c.name, &c.factors)).collect();
    let mut out: Vec<FootingDesign> = vec![];
    for s in &model.supports {
        let Some(n) = model.node(s.node) else { continue };
        // the column on the support, or at the far end of a rigid link from it (a pile group's
        // centroid off the column at a boundary)
        let is_col = |m: &&Member, node: Id| (m.i == node || m.j == node) && model.kind(m) == MemberKind::Column && model.is_concrete(m) && m.role.is_frame();
        let (col, col_node) = match model.members.iter().find(|m| is_col(m, s.node)) {
            Some(c) => (c, s.node),
            None => {
                let link = model.members.iter().find(|m| (m.i == s.node || m.j == s.node) && matches!(model.section(m.section).map(|x| &x.shape), Some(Shape::General { .. })));
                let Some(far) = link.map(|l| if l.i == s.node { l.j } else { l.i }) else { continue };
                let Some(c) = model.members.iter().find(|m| is_col(m, far)) else { continue };
                (c, far)
            }
        };
        let col_at = model.node(col_node).map(|q| [q.x, q.y]).unwrap_or([n.x, n.y]);
        let (cl, cw) = match model.section(col.section).map(|x| &x.shape) {
            // a column with roll 0 has its depth h along x
            Some(Shape::Rect { b, h }) => {
                if (col.roll.abs() - 90.0).abs() < 1.0 {
                    (*b, *h)
                } else {
                    (*h, *b)
                }
            }
            _ => continue,
        };
        let react = |r: &crate::analysis::CaseResult| r.reactions.iter().find(|x| x.0 == s.node).map(|x| x.1).unwrap_or([0.0; 6]);
        // the governing service and factored reactions: the largest vertical load
        let pick = |rs: &[crate::analysis::CaseResult]| rs.iter().map(react).max_by(|x, y| x[2].total_cmp(&y[2])).unwrap_or([0.0; 6]);
        let (rs, ru) = (pick(&sls), pick(&uls));
        if rs[2] <= 0.0 {
            continue;
        }
        if let Some(ps) = &fs.piles {
            let h_dowel = columns.iter().find(|c| c.member == col.id).map(|c| {
                let ldc = (0.24 * model.design.fy / fc.sqrt() * c.db).max(0.043 * model.design.fy * c.db).max(200.0) / 1000.0;
                ldc + fs.cover + 2.0 * 0.016 + 0.01
            }).unwrap_or(0.0);
            // keep clear of the caps placed already and of the least cap (one pile) of every other
            // support with a column (two columns a short way apart then take caps turned apart)
            let mut avoid: Vec<[f64; 4]> = out.iter().filter(|f| !f.piles.is_empty()).map(|f| [f.at[0] - f.l / 2.0, f.at[1] - f.b / 2.0, f.at[0] + f.l / 2.0, f.at[1] + f.b / 2.0]).collect();
            for o in model.supports.iter().filter(|o| o.node != s.node && !out.iter().any(|f| f.node == o.node)) {
                if let Some(q) = model.node(o.node) {
                    avoid.push([q.x - ps.e(), q.y - ps.e(), q.x + ps.e(), q.y + ps.e()]);
                }
            }
            out.push(pile_cap(s.node, n.p(), col_at, (cl, cw), rs, ru, h_dowel, fs, ps, fc, gamma_c, model.design.fy, model.design.fyt, &avoid));
            continue;
        }
        let mut notes = vec![];
        // size: service load plus the footing and the soil over it
        let over = |h: f64| gamma_c * h + fs.gamma_soil * (fs.depth - h).max(0.0);
        // thick enough for the column's dowels to develop in compression (ACI 318-19
        // 25.4.9.2: 0.24 fy db / √f'c, at least 0.043 fy db and 200 mm) above the mat
        let h_dowel = columns.iter().find(|c| c.member == col.id).map(|c| {
            let ldc = (0.24 * model.design.fy / fc.sqrt() * c.db).max(0.043 * model.design.fy * c.db).max(200.0) / 1000.0;
            ldc + fs.cover + 2.0 * 0.016 + 0.01
        }).unwrap_or(0.0);
        if h_dowel > fs.min_h {
            notes.push(format!("thickness {:.0} mm for the column dowels' development length", h_dowel * 1000.0));
        }
        let mut h = fs.min_h.max((h_dowel * 20.0).ceil() / 20.0);
        let mut side = ((rs[2] / (fs.q_allow - over(h)).max(10.0)).sqrt() * 10.0).ceil() / 10.0;
        side = side.max(cl.max(cw) + 0.3).max(fs.min_b);
        let mut chk = None;
        for _ in 0..200 {
            let ps = rs[2] + over(h) * side * side;
            let svc = run_check(side, side, h, cl, cw, fs, fc, ps, rs[3], rs[4], (rs[0] * rs[0] + rs[1] * rs[1]).sqrt());
            if svc.bearing_ratio > 1.0 {
                side += 0.1;
                continue;
            }
            // shear under the factored net load (the footing's own weight bears on the soil directly)
            let fac = run_check(side, side, h, cl, cw, fs, fc, ru[2], ru[3], ru[4], (ru[0] * ru[0] + ru[1] * ru[1]).sqrt());
            if fac.oneway_shear_ratio > 1.0 || fac.punching_shear_ratio > 1.0 {
                h += 0.05;
                continue;
            }
            chk = Some((svc, fac));
            break;
        }
        let Some((svc, fac)) = chk else {
            notes.push("no size up to the search limit passes".into());
            continue;
        };
        h = (h * 1000.0).round() / 1000.0;
        // flexure at the column face, both ways: q_u x B x c²/2 with the cantilever c
        let qu = ru[2] / (side * side) + 6.0 * ru[3].abs().max(ru[4].abs()) / side.powi(3);
        let d = (h - fs.cover - 0.016) * 1000.0;
        let as_min = 0.0018 * 1000.0 * h * 1000.0;
        let bars_for = |c: f64| -> (String, f64) {
            let mu = face_moment(qu, c);
            let (as_req, _as_prov, s, txt, fb) = rc::slab_bars(mu, d, as_min, (3.0 * h * 1000.0).min(450.0), &fs.bars, fc, model.design.fy, model.design.fyt);
            let _ = (as_req, fb);
            let n = ((side * 1000.0 - 2.0 * fs.cover * 1000.0) / s).floor() as usize + 1;
            let db: f64 = txt.split(['B', ' ']).nth(1).and_then(|x| x.parse().ok()).unwrap_or(16.0);
            (format!("{n}-{} @ {s:.0} mm", bar_name(db)), mu)
        };
        let (bx, mux) = bars_for((side - cl) / 2.0);
        let (by, _) = bars_for((side - cw) / 2.0);
        let ot = svc.overturning_sf_x.min(svc.overturning_sf_y);
        let mut checks = [("bearing", svc.bearing_ratio), ("one-way shear", fac.oneway_shear_ratio), ("punching shear", fac.punching_shear_ratio), ("overturning (SF ≥ 1.5)", if ot.is_finite() { 1.5 / ot } else { 0.0 }), ("sliding (SF ≥ 1.5)", if svc.sliding_sf.is_finite() { 1.5 / svc.sliding_sf } else { 0.0 })];
        checks.sort_by(|a, b| b.1.total_cmp(&a.1));
        let _ = mux;
        out.push(FootingDesign {
            node: s.node,
            at: n.p(),
            l: side,
            b: side,
            h,
            bars_x: bx,
            bars_y: by,
            q_max: svc.max_bearing_pressure / 1e3,
            bearing_ratio: svc.bearing_ratio,
            overturning_sf: ot,
            sliding_sf: svc.sliding_sf,
            oneway_ratio: fac.oneway_shear_ratio,
            punching_ratio: fac.punching_shear_ratio,
            ratio: checks[0].1,
            governs: checks[0].0.into(),
            ok: checks[0].1 <= 1.0,
            notes,
            piles: vec![],
            pile_load: 0.0,
            pile_capacity: 0.0,
            pile_diameter: 0.0,
            pile_length: 0.0,
            column: col_at,
            piles_needed: 0,
        });
    }
    out
}

/// The pile layouts tried, fewest piles first: (piles along x, piles along y).
const LAYOUTS: [(usize, usize); 11] = [(1, 1), (2, 1), (1, 2), (3, 1), (1, 3), (2, 2), (4, 1), (1, 4), (3, 2), (2, 3), (3, 3)];

/// ACI 318-19 13.4.2.5: the share of a pile's reaction that acts on one side of a section (the
/// side `outside` of it, the pile centre at signed distance `dist` past the section): all of it
/// at half a diameter or more outside, none at half a diameter or more inside, linear between.
fn pile_share(dist: f64, dp: f64) -> f64 {
    (dist / dp + 0.5).clamp(0.0, 1.0)
}

/// Two-way shear capacity (stress, MPa) of 22.6.5.2 with the size factor.
fn vc_two_way(beta: f64, alpha_s: f64, bo_mm: f64, d_mm: f64, fc: f64) -> f64 {
    let lam = (2.0 / (1.0 + 0.004 * d_mm)).sqrt().min(1.0);
    0.33f64.min(0.17 * (1.0 + 2.0 / beta)).min(0.083 * (alpha_s * d_mm / bo_mm + 2.0)) * lam * fc.sqrt().min(8.3)
}

/// The critical perimeter at d/2 round a loaded area (centre, sides) clipped at the cap's edges:
/// its length (m) and the number of sides left; none when it runs off both edges of the cap
/// in one direction (the section then crosses the whole cap: beam action, the one-way check's).
fn perimeter(c: [f64; 2], a: f64, b: f64, d: f64, half: [f64; 2]) -> (f64, usize) {
    let (ax, ay) = ((a + d) / 2.0, (b + d) / 2.0);
    if (c[0] - ax <= -half[0] + 1e-6 && c[0] + ax >= half[0] - 1e-6) || (c[1] - ay <= -half[1] + 1e-6 && c[1] + ay >= half[1] - 1e-6) {
        return (0.0, 0);
    }
    let (x0, x1) = ((c[0] - ax).max(-half[0]), (c[0] + ax).min(half[0]));
    let (y0, y1) = ((c[1] - ay).max(-half[1]), (c[1] + ay).min(half[1]));
    let (mut bo, mut n) = (0.0, 0);
    for y in [c[1] - ay, c[1] + ay] {
        if y > -half[1] + 1e-6 && y < half[1] - 1e-6 {
            bo += (x1 - x0).max(0.0);
            n += 1;
        }
    }
    for x in [c[0] - ax, c[0] + ax] {
        if x > -half[0] + 1e-6 && x < half[0] - 1e-6 {
            bo += (y1 - y0).max(0.0);
            n += 1;
        }
    }
    (bo, n)
}

/// A pile cap under one support: the fewest piles (of the layouts that fit the site lines and
/// hold the column) whose largest service load, the cap and the soil over it included, is
/// within the safe load; then the cap thickened until it passes the ACI 318-19 checks of
/// Chapter 13 under the factored load: flexure at the column faces (the pile reactions past a
/// face times their lever arms), one-way shear at d from the faces with the pile shares of
/// 13.4.2.5, two-way shear round the column (the piles inside the perimeter taken off) and round
/// each pile, and at least 300 mm of depth over the bottom bars (13.4.2.1). Bars rest 75 mm over
/// the pile heads, which are cast 75 mm into the cap.
#[allow(clippy::too_many_arguments)]
fn pile_cap(node: Id, at: [f64; 3], col: [f64; 2], (cl, cw): (f64, f64), rs: [f64; 6], ru: [f64; 6], h_dowel: f64, fs: &FootingSettings, ps: &PileSettings, fc: f64, gamma_c: f64, fy: f64, fyt: f64, avoid: &[[f64; 4]]) -> FootingDesign {
    let (sp, ed, dp) = (ps.s(), ps.e(), ps.diameter);
    // the column in the cap's frame (origin at the group's centroid, the support)
    let e = [col[0] - at[0], col[1] - at[1]];
    let mut notes = vec![];
    let mut best: Option<FootingDesign> = None;
    'layouts: for (nx, ny) in LAYOUTS {
        let n = (nx * ny) as f64;
        let mut l = (nx - 1) as f64 * sp + 2.0 * ed;
        let mut b = (ny - 1) as f64 * sp + 2.0 * ed;
        // the cap holds the column (grown about the centroid where it must)
        l = l.max(2.0 * (e[0].abs() + cl / 2.0));
        b = b.max(2.0 * (e[1].abs() + cw / 2.0));
        if let Some(bd) = ps.bounds {
            if at[0] - l / 2.0 < bd[0] - 1e-6 || at[0] + l / 2.0 > bd[2] + 1e-6 || at[1] - b / 2.0 < bd[1] - 1e-6 || at[1] + b / 2.0 > bd[3] + 1e-6 {
                continue;
            }
        }
        // clear of the neighbouring caps
        let r = [at[0] - l / 2.0, at[1] - b / 2.0, at[0] + l / 2.0, at[1] + b / 2.0];
        if avoid.iter().any(|o| r[0] < o[2] - 1e-6 && o[0] < r[2] - 1e-6 && r[1] < o[3] - 1e-6 && o[1] < r[3] - 1e-6) {
            continue;
        }
        let piles: Vec<[f64; 2]> = (0..nx).flat_map(|i| (0..ny).map(move |j| [(i as f64 - (nx - 1) as f64 / 2.0) * sp, (j as f64 - (ny - 1) as f64 / 2.0) * sp])).collect();
        let sx2: f64 = piles.iter().map(|q| q[0] * q[0]).sum();
        let sy2: f64 = piles.iter().map(|q| q[1] * q[1]).sum();
        // the reaction on pile k for a support load r (moments about x and y at the centroid)
        let load = |r: &[f64; 6], extra: f64, q: &[f64; 2]| -> f64 {
            (r[2] + extra) / n + if sy2 > 0.0 { r[3] * q[1] / sy2 } else { 0.0 } - if sx2 > 0.0 { r[4] * q[0] / sx2 } else { 0.0 }
        };
        // the thickness: 300 mm over the bottom bars (13.4.2.1), the dowels, then the checks
        let mut h = ((0.30f64 + 0.15 + 0.016).max(fs.min_h).max(h_dowel) * 20.0).ceil() / 20.0;
        for _ in 0..40 {
            let w_cap = gamma_c * l * b * h + fs.gamma_soil * l * b * (fs.depth - h).max(0.0);
            let p_max = piles.iter().map(|q| load(&rs, w_cap, q)).fold(f64::MIN, f64::max);
            if p_max > ps.capacity + 1e-6 {
                continue 'layouts;
            }
            // factored reactions from the column's load (the cap's own weight bears straight on
            // the piles without bending the cap)
            let ru_k: Vec<f64> = piles.iter().map(|q| load(&ru, 0.0, q)).collect();
            let d: f64 = h - 0.15 - 0.008;
            let dmm = d * 1000.0;
            let lam = (2.0 / (1.0 + 0.004 * dmm)).sqrt().min(1.0);
            let half = [l / 2.0, b / 2.0];
            let mut checks: Vec<(String, f64)> = vec![];
            let mut moments = [0.0f64; 2];
            for ax in 0..2 {
                let (c_half, width) = if ax == 0 { (cl / 2.0, b) } else { (cw / 2.0, l) };
                for side in [-1.0, 1.0] {
                    let face = e[ax] + side * c_half;
                    // flexure at the face: the piles past it
                    let m: f64 = piles.iter().zip(&ru_k).map(|(q, r)| { let arm = side * (q[ax] - face); if arm > 0.0 { r * arm } else { 0.0 } }).sum();
                    moments[ax] = moments[ax].max(m);
                    // one-way shear at d past the face
                    let sec = face + side * d;
                    let v: f64 = piles.iter().zip(&ru_k).map(|(q, r)| r * pile_share(side * (q[ax] - sec), dp)).sum();
                    let phi_vc = crate::codes::phi().shear * 0.17 * lam * fc.sqrt() * width * dmm;
                    if n > 1.0 {
                        checks.push((format!("one-way shear across {} ({})", if ax == 0 { "x" } else { "y" }, if side < 0.0 { "−" } else { "+" }), v / phi_vc.max(1e-9)));
                    }
                }
            }
            if n > 1.0 {
                // two-way shear round the column: the piles inside the perimeter come off
                let (bo, sides) = perimeter(e, cl, cw, d, half);
                if bo > 0.0 {
                    let inside: f64 = piles.iter().zip(&ru_k).map(|(q, r)| {
                        let dx = (q[0] - e[0]).abs() - (cl + d) / 2.0;
                        let dy = (q[1] - e[1]).abs() - (cw + d) / 2.0;
                        r * (1.0 - pile_share(dx.max(dy), dp))
                    }).sum();
                    let vu = (ru[2] - inside).max(0.0);
                    let alpha = match sides { 4 => 40.0, 3 => 30.0, _ => 20.0 };
                    let vc = vc_two_way(cl.max(cw) / cl.min(cw).max(1e-6), alpha, bo * 1000.0, dmm, fc);
                    checks.push(("punching at the column".into(), vu / (crate::codes::phi().shear * vc * bo * dmm).max(1e-9)));
                }
                // two-way shear round each pile (a square of the same area)
                let a = dp * (std::f64::consts::PI / 4.0).sqrt();
                for (q, r) in piles.iter().zip(&ru_k) {
                    let (bo, sides) = perimeter(*q, a, a, d, half);
                    if bo <= 0.0 {
                        continue;
                    }
                    let alpha = match sides { 4 => 40.0, 3 => 30.0, _ => 20.0 };
                    let vc = vc_two_way(1.0, alpha, bo * 1000.0, dmm, fc);
                    checks.push(("punching at a pile".into(), r / (crate::codes::phi().shear * vc * bo * dmm).max(1e-9)));
                }
            }
            if checks.iter().any(|c| c.1 > 1.0) {
                h += 0.05;
                continue;
            }
            // bars both ways for the face moments (per metre of the cap's width), at least
            // 0.0018 bh
            let as_min = 0.0018 * 1000.0 * h * 1000.0;
            let bars_for = |mu: f64, width: f64, span: f64| -> String {
                let (_, _, s, txt, _) = rc::slab_bars(mu / width, dmm, as_min, (3.0 * h * 1000.0).min(450.0), &fs.bars, fc, fy, fyt);
                let _ = span;
                let count = ((width * 1000.0 - 2.0 * fs.cover * 1000.0) / s).floor() as usize + 1;
                let db: f64 = txt.split(['B', ' ']).nth(1).and_then(|x| x.parse().ok()).unwrap_or(16.0);
                format!("{count}-{} @ {s:.0} mm", bar_name(db))
            };
            let bars_x = bars_for(moments[0], b, l);
            let bars_y = bars_for(moments[1], l, b);
            let pile_pts: Vec<[f64; 2]> = piles.iter().map(|q| [at[0] + q[0], at[1] + q[1]]).collect();
            let worst = checks.iter().cloned().fold(("pile load".to_string(), p_max / ps.capacity), |a, c| if c.1 > a.1 { c } else { a });
            let punch = checks.iter().filter(|c| c.0.starts_with("punching")).map(|c| c.1).fold(0.0, f64::max);
            let oneway = checks.iter().filter(|c| c.0.starts_with("one-way")).map(|c| c.1).fold(0.0, f64::max);
            notes.push(format!("{} pile(s) {}, Ø{:.0} mm at {:.2} m, edge {:.2} m: largest service load {:.0} kN of {:.0} kN safe (column {:.0} kN + cap and soil {:.0} kN)", n as usize, if nx * ny == 1 { "under the column".to_string() } else { format!("{nx} x {ny}") }, dp * 1000.0, sp, ed, p_max, ps.capacity, rs[2], w_cap));
            if e[0].abs() > 1e-3 || e[1].abs() > 1e-3 {
                notes.push(format!("the group's centroid lies ({:.3}, {:.3}) m from the column (a boundary): the column load is eccentric by {:.0} mm; the ground (strap) beams take P·e (in the frame through the rigid link to the support)", -e[0], -e[1], e[0].hypot(e[1]) * 1000.0));
            }
            if nx * ny == 1 {
                notes.push("one pile: the cap is tied by beams in both directions, which take the pile's eccentricity (75 mm allowance in practice) and the column's moments".into());
            } else if nx == 1 || ny == 1 {
                notes.push("one row of piles: the cap is tied by a beam across the row".into());
            }
            notes.push(format!("cap {:.2} x {:.2} x {:.2} m, d {:.0} mm over the bottom bars (13.4.2.1: ≥ 300 mm); flexure at the column faces Mu {:.1} / {:.1} kN·m (x / y)", l, b, h, dmm, moments[0], moments[1]));
            best = Some(FootingDesign {
                node,
                at,
                l,
                b,
                h,
                bars_x,
                bars_y,
                q_max: p_max,
                bearing_ratio: p_max / ps.capacity,
                overturning_sf: f64::INFINITY,
                sliding_sf: f64::INFINITY,
                oneway_ratio: oneway,
                punching_ratio: punch,
                ratio: worst.1,
                governs: worst.0,
                ok: worst.1 <= 1.0,
                notes: notes.clone(),
                piles: pile_pts,
                pile_load: p_max,
                pile_capacity: ps.capacity,
                pile_diameter: dp,
                pile_length: ps.length,
                column: col,
                piles_needed: 0,
            });
            break 'layouts;
        }
    }
    best.unwrap_or_else(|| {
        // the piles a row would need: the column and a cap of a row of them
        let n_need = (1..=9).find(|k| {
            let (l, b) = (2.0 * ed, (*k as f64 - 1.0) * sp + 2.0 * ed);
            (rs[2] + (gamma_c * 0.5 + fs.gamma_soil * (fs.depth - 0.5).max(0.0)) * l * b) / *k as f64 <= ps.capacity
        }).unwrap_or(9) as f64;
        FootingDesign { node, at, l: 0.0, b: 0.0, h: 0.0, bars_x: String::new(), bars_y: String::new(), q_max: 0.0, bearing_ratio: f64::INFINITY, overturning_sf: f64::INFINITY, sliding_sf: f64::INFINITY, oneway_ratio: 0.0, punching_ratio: 0.0, ratio: 9.99, governs: "no pile layout fits".into(), ok: false, notes: vec![format!("no layout of up to 9 piles fits the site lines and carries {:.0} kN (about {n_need:.0} piles of {:.0} kN): a combined cap with the next column, or larger piles", rs[2], ps.capacity)], piles: vec![], pile_load: 0.0, pile_capacity: ps.capacity, pile_diameter: dp, pile_length: ps.length, column: col, piles_needed: n_need as usize }
    })
}

#[cfg(test)]
mod pile_tests {
    use super::*;

    fn settings(bounds: Option<[f64; 4]>) -> (FootingSettings, PileSettings) {
        let ps = PileSettings { capacity: 300.0, diameter: 0.3, bounds, ..Default::default() };
        (FootingSettings { depth: 1.5, gamma_soil: 18.0, min_h: 0.3, piles: Some(ps.clone()), ..Default::default() }, ps)
    }

    #[test]
    fn four_pile_cap_by_hand() {
        // a 400 x 400 column, 1000 kN service, 1400 kN factored: 3 piles in a line would give
        // (1000 + cap + soil) / 3 > 300? no: four piles at 0.9 m are the first layout that also
        // passes the shear checks of a 2 x 2 group; check the statics by hand
        let (fs, ps) = settings(None);
        let f = pile_cap(1, [0.0, 0.0, -1.0], [0.0, 0.0], (0.4, 0.4), [0.0, 0.0, 1000.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1400.0, 0.0, 0.0, 0.0], 0.0, &fs, &ps, 24.0, 24.0, 390.0, 235.0, &[]);
        eprintln!("{} piles, {:.2} x {:.2} x {:.2}, {} / {}, ratio {:.2} ({})\n{:#?}", f.piles.len(), f.l, f.b, f.h, f.bars_x, f.bars_y, f.ratio, f.governs, f.notes);
        assert!(f.ok, "{}", f.governs);
        let n = f.piles.len() as f64;
        // the service pile load is (P + cap + soil over it) / n, at most the safe load
        let w = 24.0 * f.l * f.b * f.h + 18.0 * f.l * f.b * (1.5 - f.h);
        assert!((f.pile_load - (1000.0 + w) / n).abs() < 1e-6 && f.pile_load <= 300.0, "{} vs {}", f.pile_load, (1000.0 + w) / n);
        // fewer piles cannot carry it
        assert!((1000.0 + 24.0 * 0.6 * 0.6 * f.h) / (n - 1.0) > 300.0 || n <= 3.0);
        if f.piles.len() == 4 {
            // 2 x 2 at 0.9 m: the cap 1.5 m square; face moment = 2 x 350 x (0.45 - 0.20) = 175 kN·m
            assert!((f.l - 1.5).abs() < 1e-9 && (f.b - 1.5).abs() < 1e-9);
            assert!(f.notes.iter().any(|x| x.contains("Mu 175.0 / 175.0")), "{:?}", f.notes);
        }
    }

    #[test]
    fn boundary_cap_takes_one_row_inside_the_site() {
        // a column on the site line x = 0 (its face at -0.115): the group's centroid set 0.185 m
        // in (edge 0.3 from the site line), so only a row of piles along y fits
        let (fs, ps) = settings(Some([-0.115, -10.0, 10.0, 10.0]));
        let f = pile_cap(1, [0.185, 0.0, -1.0], [0.0, 0.0], (0.23, 0.38), [0.0, 0.0, 520.0, 0.0, 0.0, 0.0], [0.0, 0.0, 720.0, 0.0, 0.0, 0.0], 0.0, &fs, &ps, 24.0, 24.0, 390.0, 235.0, &[]);
        eprintln!("{} piles {:?}, {:.2} x {:.2} x {:.2}, {}\n{:#?}", f.piles.len(), f.piles, f.l, f.b, f.h, f.governs, f.notes);
        assert!(f.ok);
        assert!(f.piles.iter().all(|q| (q[0] - 0.185).abs() < 1e-9), "one row along y: {:?}", f.piles);
        assert!(f.at[0] - f.l / 2.0 >= -0.115 - 1e-9, "the cap stays inside the site");
        assert_eq!(f.piles.len(), 2);
        assert!(f.notes.iter().any(|n| n.contains("eccentric by 185 mm")));
        // a corner: one pile
        let (fs, ps) = settings(Some([-0.115, -0.115, 10.0, 10.0]));
        let f = pile_cap(1, [0.185, 0.185, -1.0], [0.0, 0.0], (0.23, 0.23), [0.0, 0.0, 250.0, 0.0, 0.0, 0.0], [0.0, 0.0, 350.0, 0.0, 0.0, 0.0], 0.0, &fs, &ps, 24.0, 24.0, 390.0, 235.0, &[]);
        assert!(f.ok && f.piles.len() == 1, "{} piles: {}", f.piles.len(), f.governs);
        // and more load than a row fits: no layout
        let f = pile_cap(1, [0.185, 0.185, -1.0], [0.0, 0.0], (0.23, 0.23), [0.0, 0.0, 900.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1260.0, 0.0, 0.0, 0.0], 0.0, &fs, &ps, 24.0, 24.0, 390.0, 235.0, &[]);
        assert!(!f.ok && f.piles.is_empty());
    }
}
