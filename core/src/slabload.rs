// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Slab loads on the beams under the panel edges. A two-way panel (long / short ≤ 2) sends
//! its load along the 45° lines: a triangle on each short edge and a trapezoid on each long
//! edge, w(s) = q · min(s, L − s, l_short / 2). A one-way panel sends q · l_short / 2 to each
//! long edge and nothing to the short ones. The panel's own weight (γ_c h) and its
//! superimposed dead load go to the first dead case, its live load to the first live case.
//! Load on an edge with no beam under it is lost, and the totals say how much.

use crate::analysis::MemberLoad;
use crate::model::*;

/// The edges of a panel: start, end (plan), and whether it is a long edge.
pub fn edges(s: &Slab) -> [([f64; 2], [f64; 2]); 4] {
    let (lo, hi) = (s.lo(), s.hi());
    [([lo.0, lo.1], [hi.0, lo.1]), ([hi.0, lo.1], [hi.0, hi.1]), ([hi.0, hi.1], [lo.0, hi.1]), ([lo.0, hi.1], [lo.0, lo.1])]
}

/// The beams lying along a plan segment at height z: (member, s at node i, s at node j)
/// with s measured along the segment from its start.
pub fn beams_on(model: &Model, a: [f64; 2], b: [f64; 2], z: f64) -> Vec<(Id, f64, f64)> {
    let le = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    if le < 1e-9 {
        return vec![];
    }
    let d = [(b[0] - a[0]) / le, (b[1] - a[1]) / le];
    let mut out = vec![];
    for m in &model.members {
        let (Some(p), Some(q)) = (model.node(m.i), model.node(m.j)) else { continue };
        if (p.z - z).abs() > 0.05 || (q.z - z).abs() > 0.05 {
            continue;
        }
        let off = |x: f64, y: f64| ((x - a[0]) * d[1] - (y - a[1]) * d[0]).abs();
        if off(p.x, p.y) > 0.02 || off(q.x, q.y) > 0.02 {
            continue;
        }
        let si = (p.x - a[0]) * d[0] + (p.y - a[1]) * d[1];
        let sj = (q.x - a[0]) * d[0] + (q.y - a[1]) * d[1];
        if si.max(sj) > 1e-6 && si.min(sj) < le - 1e-6 {
            out.push((m.id, si, sj));
        }
    }
    out
}

/// Whether the panel spans one way (long / short > 2).
pub fn one_way(s: &Slab) -> bool {
    let (a, b) = (s.lx(), s.ly());
    a.max(b) / a.min(b).max(1e-9) > 2.0
}

/// The member loads from all slabs, and the slab load applied and expected (dead, live), kN.
pub fn loads(model: &Model, warnings: &mut Vec<String>) -> (Vec<MemberLoad>, [f64; 2], [f64; 2]) {
    let mut out = vec![];
    let (mut applied, mut expected) = ([0.0; 2], [0.0; 2]);
    if model.slabs.is_empty() {
        return (out, applied, expected);
    }
    let (dc, lc) = model.slab_cases();
    let gamma = model.materials.iter().find(|m| m.kind == MaterialKind::Concrete).map(|m| m.gamma).unwrap_or(24.0);
    if dc.is_none() {
        warnings.push("no dead load case: the slabs' own weight and dead loads are left out".into());
    }
    if lc.is_none() && model.slabs.iter().any(|s| s.live > 0.0) {
        warnings.push("no live load case: the slabs' live loads are left out".into());
    }
    // a plate slab carries its own load on its elements
    for s in model.slabs.iter().filter(|s| !model.plate_slab(s)) {
        let ls = s.lx().min(s.ly());
        let ow = one_way(s);
        let qs = [(dc, gamma * s.h + s.dead), (lc, s.live)];
        let area = s.lx() * s.ly();
        for (k, (case, q)) in qs.iter().enumerate() {
            if case.is_some() {
                expected[k] += q * area;
            }
        }
        for (a, b) in edges(s) {
            let le = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let long = le > ls + 1e-6 || (s.lx() - s.ly()).abs() < 1e-6;
            // load intensity per unit q along the edge
            let w = |x: f64| -> f64 {
                if ow {
                    if long {
                        ls / 2.0
                    } else {
                        0.0
                    }
                } else {
                    x.min(le - x).min(ls / 2.0).max(0.0)
                }
            };
            let beams = beams_on(model, a, b, s.z);
            let mut covered = 0.0;
            for (mid, si, sj) in beams {
                let (lo, hi) = (si.min(sj).max(0.0), si.max(sj).min(le));
                if hi - lo < 1e-6 {
                    continue;
                }
                covered += hi - lo;
                let mut br = vec![lo, hi];
                for x in [ls / 2.0, le - ls / 2.0] {
                    if x > lo + 1e-6 && x < hi - 1e-6 && !ow {
                        br.push(x);
                    }
                }
                br.sort_by(f64::total_cmp);
                for p in br.windows(2) {
                    let (x0, x1) = (p[0], p[1]);
                    if w(x0) == 0.0 && w(x1) == 0.0 {
                        continue;
                    }
                    // member coordinate: distance from its node i
                    let (m0, m1) = ((x0 - si).abs(), (x1 - si).abs());
                    for (k, (case, q)) in qs.iter().enumerate() {
                        let Some(case) = case else { continue };
                        if *q == 0.0 {
                            continue;
                        }
                        let (w0, w1) = (q * w(x0), q * w(x1));
                        applied[k] += (w0 + w1) / 2.0 * (x1 - x0);
                        let (a, b, wa, wb) = if m0 <= m1 { (m0, m1, w0, w1) } else { (m1, m0, w1, w0) };
                        out.push(MemberLoad { case: *case, member: mid, a, b, wa: [0.0, 0.0, -wa], wb: [0.0, 0.0, -wb] });
                    }
                }
            }
            let needs = if ow { long } else { true };
            if needs && covered < le - 0.02 {
                warnings.push(format!("slab {} ({}): its edge from ({:.2}, {:.2}) to ({:.2}, {:.2}) has beams under {:.2} of {:.2} m; the load on the rest is not in the model", s.id, s.name, a[0], a[1], b[0], b[1], covered, le));
            }
        }
    }
    (out, applied, expected)
}
