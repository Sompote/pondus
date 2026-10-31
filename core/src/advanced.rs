// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Advanced analyses with the Stabileo engine: pushover / plastic collapse
//! (`solver::plastic_3d`, event-to-event hinges at member ends and inside spans), moving loads
//! (`solver::moving_loads`, an axle train along a path of members, the envelope of every
//! member's forces), influence lines (`postprocess::influence`) and staged construction
//! (`solver::staged`, members and load cases added stage by stage, the state carried over).

use std::collections::HashMap;

use dedaliano_engine::types::*;
use serde::{Deserialize, Serialize};

use crate::analysis::{Analysis, CaseResult, Station};
use crate::design::Report;
use crate::model::*;

// ---------------------------------------------------------------- pushover

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PushoverSpec {
    /// A combination to scale to collapse (plastic limit analysis), or else a lateral pattern.
    pub combination: Option<String>,
    /// 0 = x, 1 = y; "triangle" (forces ∝ W·z, the first-mode shape of a regular frame) or
    /// "uniform" (∝ W); the pattern's total is the seismic weight W, so λ = V/W.
    pub direction: usize,
    pub pattern: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Hinge {
    pub member: Id,
    pub position: f64,
    pub at: f64,
    pub step: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PushoverResult {
    pub collapse_factor: f64,
    pub is_mechanism: bool,
    /// Base shear at collapse, kN (lateral pattern), and the seismic weight.
    pub base_shear: f64,
    pub weight: f64,
    /// (load factor, roof displacement m) at each hinge event.
    pub curve: Vec<(f64, f64)>,
    pub hinges: Vec<Hinge>,
    pub warnings: Vec<String>,
}

/// The plastic moments (major, minor) of a member, kN·m: fy·Z of steel, Mn of the bars the
/// design chose for concrete.
/// The plastic moments (about local y and z, kN·m) a member takes in the pushover.
pub fn plastic_moments_of(model: &Model, m: &Member, rep: Option<&Report>) -> Result<[f64; 2], String> {
    plastic_moments(model, m, rep)
}

fn plastic_moments(model: &Model, m: &Member, rep: Option<&Report>) -> Result<[f64; 2], String> {
    let sec = model.section(m.section).ok_or("a member without a section")?;
    let mat = model.material(m.material).ok_or("a member without a material")?;
    let p = sec.props();
    if mat.kind == MaterialKind::Steel {
        return Ok([mat.fy * 1e3 * p.zy, mat.fy * 1e3 * p.zz]);
    }
    if mat.kind != MaterialKind::Concrete {
        return Err(format!("member {}: pushover takes steel and concrete members", m.id));
    }
    let rep = rep.ok_or("design the concrete members first: their plastic moments come from their bars")?;
    let Shape::Rect { b, h } = sec.shape else { return Err(format!("member {}: a concrete member needs a rectangle", m.id)) };
    let fy = model.design.fy;
    let fc = mat.fc.max(1.0);
    let cover = model.design.cover_beam * 1000.0;
    let mn = |as_: f64, bw: f64, d: f64| -> f64 {
        let a = as_ * fy / (0.85 * fc * bw);
        as_ * fy * (d - a / 2.0) / 1e6
    };
    if let Some(d) = rep.beams.iter().find(|d| d.member == m.id) {
        let as_ = d.flexure.iter().map(|f| f.as_prov).fold(0.0, f64::max);
        let dd = h * 1000.0 - cover - 9.0 - 10.0;
        let my = mn(as_, b * 1000.0, dd);
        return Ok([my, 3.0 * my]);
    }
    if let Some(c) = rep.columns.iter().find(|d| d.member == m.id) {
        let ab = crate::sections::bar_area(c.db) * 1e6;
        let (ny, nz) = (c.n_b as f64 * ab, c.n_h as f64 * ab);
        let (hd, bd) = (h * 1000.0, b * 1000.0);
        return Ok([mn(ny, bd, hd - cover - 9.0 - c.db / 2.0), mn(nz, hd, bd - cover - 9.0 - c.db / 2.0)]);
    }
    Err(format!("member {} was not designed: design first", m.id))
}

pub fn pushover(model: &Model, a: &Analysis, rep: Option<&Report>, spec: &PushoverSpec) -> Result<PushoverResult, String> {
    // a coarse model of its own: beams in two elements (ends and midspan), columns in one; a
    // lateral push carries no gravity, so its model has no loads either (no load-break nodes),
    // and the seismic weight comes from the analysis of the full model
    let fine = a.engine.as_ref().ok_or("analyse first")?;
    let coarse = if spec.combination.is_some() { crate::analysis::analyze_with(model, 2, 1)? } else { crate::analysis::analyze_with(&bare(model), 2, 1)? };
    let em = coarse.engine.as_ref().ok_or("analyse first")?;
    let mut inp = em.input.clone();
    // one section per member, so each member keeps its own plastic moments
    let mut sections: HashMap<String, PlasticSectionData3D> = HashMap::new();
    let mut overrides: HashMap<String, [f64; 2]> = HashMap::new();
    let mut mats: HashMap<String, PlasticMaterialData> = HashMap::new();
    let mut per_member: HashMap<Id, usize> = HashMap::new();
    for m in &model.members {
        let mp = plastic_moments(model, m, rep)?;
        let sid = 900_000 + m.id;
        let base = inp.sections[&m.section.to_string()].clone();
        inp.sections.insert(sid.to_string(), SolverSection3D { id: sid, ..base.clone() });
        sections.insert(sid.to_string(), PlasticSectionData3D { a: base.a, iy: base.iy, iz: base.iz, material_id: m.material, b: None, h: None, d: None });
        overrides.insert(sid.to_string(), mp);
        per_member.insert(m.id, sid);
        let mt = model.material(m.material).unwrap();
        mats.insert(m.material.to_string(), PlasticMaterialData { fy: Some(if mt.fy > 0.0 { mt.fy } else { model.design.fy }) });
    }
    for (k, pc) in em.pieces.iter().enumerate() {
        if let Some(el) = inp.elements.get_mut(&(k + 1).to_string()) {
            el.section_id = per_member[&pc.member];
        }
    }
    let mut warnings = vec![];
    let (weight, loads) = match &spec.combination {
        Some(name) => {
            let c = model.combos.iter().find(|c| c.name.eq_ignore_ascii_case(name)).ok_or_else(|| format!("no combination '{name}'"))?;
            (0.0, em.combo_loads(&c.factors))
        }
        None => {
            // the lateral pattern from the seismic weight at the nodes of the full model
            let _ = crate::seismic::mass_model(model, &fine.input, &fine.pieces, &fine.quads, &fine.gravity, &fine.point_cases)?;
            let factor = |c: &LoadCase| match c.kind {
                CaseKind::D => 1.0,
                CaseKind::L | CaseKind::Lr => model.seismic.live_mass,
                _ => 0.0,
            };
            let mut wn: HashMap<usize, f64> = HashMap::new();
            for (k, pc) in fine.pieces.iter().enumerate() {
                let w: f64 = model.cases.iter().map(|c| factor(c) * fine.gravity.get(&c.id).map(|g| g.0[k]).unwrap_or(0.0)).sum::<f64>() * (pc.s1 - pc.s0);
                *wn.entry(pc.ni).or_default() += w / 2.0;
                *wn.entry(pc.nj).or_default() += w / 2.0;
            }
            for (k, q) in fine.quads.iter().enumerate() {
                let w: f64 = model.cases.iter().map(|c| factor(c) * fine.gravity.get(&c.id).map(|g| g.1[k]).unwrap_or(0.0)).sum::<f64>() * q.size[0] * q.size[1];
                for n in q.nodes {
                    *wn.entry(n).or_default() += w / 4.0;
                }
            }
            let wpts: Vec<([f64; 3], f64)> = wn.iter().map(|(n, w)| { let p = &fine.input.nodes[&n.to_string()]; ([p.x, p.y, p.z], *w) }).collect();
            // the weight lumped to the floor levels (the model's node levels, a node between two
            // levels shared by distance), then pushed at the free nodes on each level in
            // proportion to the weight nearest each, so a column's mid-height carries no push
            let supported: Vec<usize> = inp.supports.values().map(|s| s.node_id).collect();
            let z0 = model.nodes.iter().map(|n| n.z).fold(f64::MAX, f64::min);
            let mut levels: Vec<f64> = model.nodes.iter().map(|n| (n.z * 1000.0).round() / 1000.0).collect();
            levels.sort_by(f64::total_cmp);
            levels.dedup();
            let mut wl = vec![0.0; levels.len()];
            for (p, w) in &wpts {
                let z = p[2];
                let up = levels.iter().position(|l| *l >= z - 5e-4).unwrap_or(levels.len() - 1);
                if (levels[up] - z).abs() < 5e-4 || up == 0 {
                    wl[up] += w;
                } else {
                    let (lo, hi) = (levels[up - 1], levels[up]);
                    let t = (z - lo) / (hi - lo);
                    wl[up - 1] += w * (1.0 - t);
                    wl[up] += w * t;
                }
            }
            let at_level = |k: usize| -> Vec<(usize, f64)> {
                let nodes: Vec<(usize, [f64; 3])> = inp.nodes.values().filter(|n| !supported.contains(&n.id) && (n.z - levels[k]).abs() < 5e-4).map(|n| (n.id, [n.x, n.y, n.z])).collect();
                let mut share: HashMap<usize, f64> = nodes.iter().map(|n| (n.0, 0.0)).collect();
                for (p, w) in wpts.iter().filter(|(p, _)| (p[2] - levels[k]).abs() < 5e-4) {
                    if let Some(n) = nodes.iter().min_by(|a, b| crate::model::dist(a.1, *p).total_cmp(&crate::model::dist(b.1, *p))) {
                        *share.get_mut(&n.0).unwrap() += w;
                    }
                }
                let mut v: Vec<(usize, f64)> = share.into_iter().collect();
                v.sort_by_key(|x| x.0);
                v
            };
            let shape = |z: f64| if spec.pattern == "uniform" { 1.0 } else { (z - z0).max(0.0) };
            let total_w: f64 = wn.values().sum();
            let sum: f64 = (0..levels.len()).filter(|k| !at_level(*k).is_empty()).map(|k| wl[k] * shape(levels[k])).sum();
            if sum <= 0.0 {
                return Err("no mass above the supports for a lateral pattern".into());
            }
            let mut loads = vec![];
            for k in 0..levels.len() {
                let nodes = at_level(k);
                let wsum: f64 = nodes.iter().map(|x| x.1).sum();
                for (n, w) in &nodes {
                    let share = if wsum > 0.0 { w / wsum } else { 1.0 / nodes.len() as f64 };
                    let f = total_w * wl[k] * shape(levels[k]) / sum * share;
                    loads.push(SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: *n, fx: if spec.direction == 0 { f } else { 0.0 }, fy: if spec.direction == 1 { f } else { 0.0 }, fz: 0.0, mx: 0.0, my: 0.0, mz: 0.0, bw: None }));
                }
            }
            warnings.push("lateral pattern only: gravity is not held constant (the engine scales every load together)".into());
            (total_w, loads)
        }
    };
    inp.loads = loads;
    let pi = PlasticInput3D { solver: inp, sections, materials: mats, max_hinges: Some(200), mp_overrides: Some(overrides) };
    let r = dedaliano_engine::solver::plastic::solve_plastic_3d(&pi).map_err(|e| format!("pushover: {e}"))?;
    warnings.extend(r.warnings.clone());
    let piece_member: Vec<Id> = em.pieces.iter().map(|p| p.member).collect();
    let hinges = r
        .hinges
        .iter()
        .map(|h| {
            let pc = &em.pieces[h.element_id - 1];
            let len = model.member(pc.member).map(|m| model.length(m)).unwrap_or(1.0);
            Hinge { member: piece_member[h.element_id - 1], position: (pc.s0 + (pc.s1 - pc.s0) * h.position) / len, at: h.load_factor, step: h.step }
        })
        .collect();
    // the roof: the top level's mean displacement in the push direction
    let ztop = model.nodes.iter().map(|n| n.z).fold(f64::MIN, f64::max);
    let top: Vec<usize> = model.nodes.iter().filter(|n| (n.z - ztop).abs() < 0.005).map(|n| em.node_of[&n.id]).collect();
    // each step's results are its increment (the structure with the hinges so far, times Δλ),
    // so the roof displacement is their running sum
    let mut curve = vec![(0.0, 0.0)];
    let mut roof = 0.0;
    for st in &r.steps {
        let d: Vec<f64> = st.results.displacements.iter().filter(|d| top.contains(&d.node_id)).map(|d| if spec.direction == 0 { d.ux } else { d.uy }).collect();
        roof += if d.is_empty() { 0.0 } else { d.iter().sum::<f64>() / d.len() as f64 };
        curve.push((st.load_factor, roof));
    }
    Ok(PushoverResult { collapse_factor: r.collapse_factor, is_mechanism: r.is_mechanism, base_shear: r.collapse_factor * weight, weight, curve, hinges, warnings })
}

// ---------------------------------------------------------------- moving loads

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MovingSpec {
    pub name: String,
    /// The members the train runs along, in order.
    pub path: Vec<Id>,
    /// (offset from the first axle m, weight kN) of each axle.
    pub axles: Vec<(f64, f64)>,
    pub step: f64,
    /// Dynamic allowance (e.g. 0.33) and the load factor of the moving-load combinations.
    pub impact: f64,
    pub factor: f64,
}

/// The envelope: the largest and the least of each force at each station of each member.
pub fn moving_loads(model: &Model, a: &Analysis, spec: &MovingSpec) -> Result<(CaseResult, CaseResult), String> {
    let main = a.engine.as_ref().ok_or("analyse first")?;
    if spec.path.is_empty() || spec.axles.is_empty() {
        return Err("give the path members and the axles".into());
    }
    let step = if spec.step > 0.0 { spec.step } else { 0.25 };
    // The engine envelopes each element from its END forces only, so an axle standing inside an
    // element is missed (a 10 m span with two axles gave 400 instead of 405 kN·m). The train
    // therefore runs over a load-free model of its own whose path members are cut at the step,
    // so every stop is an element end; the envelope is then read at the main stations.
    let longest = spec.path.iter().filter_map(|id| model.member(*id)).map(|m| model.length(m)).fold(0.0, f64::max);
    let segs = ((longest / step).ceil() as usize).clamp(crate::analysis::SEGMENTS, 400);
    let own = crate::analysis::analyze_with(&bare(model), segs, 2)?;
    let em = own.engine.as_ref().ok_or("analyse first")?;
    let mut path = vec![];
    for id in &spec.path {
        let pcs: Vec<usize> = em.pieces.iter().enumerate().filter(|(_, p)| p.member == *id).map(|(k, _)| k + 1).collect();
        if pcs.is_empty() {
            return Err(format!("no member {id}"));
        }
        path.extend(pcs);
    }
    let mut inp = em.input.clone();
    inp.loads.clear();
    let mi = MovingLoadInput3D { solver: inp, train: LoadTrain { name: spec.name.clone(), axles: spec.axles.iter().map(|(o, w)| Axle { offset: *o, weight: w * (1.0 + spec.impact) }).collect() }, step: Some(step), path_element_ids: Some(path), gravity_direction: None };
    let env = dedaliano_engine::solver::moving_loads::solve_moving_loads_3d(&mi).map_err(|e| format!("moving loads: {e}"))?;
    let mut hi = CaseResult { name: format!("{} max", spec.name), ..Default::default() };
    let mut lo = CaseResult { name: format!("{} min", spec.name), ..Default::default() };
    // the envelope at x along a member: the extreme, at x, of the fine elements that start or end
    // there (the engine's per-end envelope; its per-element extreme also holds the value one step
    // away, which put a design example's envelope 0.8 % high where it changes fast), or between
    // the ends of the element x lies in
    let at = |member: Id, x: f64| -> (Station, Station) {
        let (mut p, mut q) = (Station { x, ..Default::default() }, Station { x, ..Default::default() });
        let mut put = |hi: &[f64; 6], lo: &[f64; 6]| {
            // [n, vy, vz, mx, my, mz]
            p.n = p.n.max(hi[0]);
            p.vy = p.vy.max(hi[1]);
            p.vz = p.vz.max(hi[2]);
            p.t = p.t.max(hi[3]);
            p.my = p.my.max(hi[4]);
            p.mz = p.mz.max(hi[5]);
            q.n = q.n.min(lo[0]);
            q.vy = q.vy.min(lo[1]);
            q.vz = q.vz.min(lo[2]);
            q.t = q.t.min(lo[3]);
            q.my = q.my.min(lo[4]);
            q.mz = q.mz.min(lo[5]);
        };
        let mut found = false;
        for (k, pc) in em.pieces.iter().enumerate().filter(|(_, pc)| pc.member == member) {
            let Some(e) = env.elements.get(&(k + 1).to_string()) else { continue };
            if (pc.s0 - x).abs() < 1e-6 {
                put(&e.start_max, &e.start_min);
                found = true;
            }
            if (pc.s1 - x).abs() < 1e-6 {
                put(&e.end_max, &e.end_min);
                found = true;
            }
        }
        // a station inside an element of the train's model: between the envelopes at its two
        // ends (the train stops at the element ends, so that is where its envelope is known)
        if !found {
            for (k, pc) in em.pieces.iter().enumerate().filter(|(_, pc)| pc.member == member && x > pc.s0 && x < pc.s1) {
                if let Some(e) = env.elements.get(&(k + 1).to_string()) {
                    let t = (x - pc.s0) / (pc.s1 - pc.s0);
                    let mix = |a: &[f64; 6], b: &[f64; 6]| -> [f64; 6] { std::array::from_fn(|i| a[i] + t * (b[i] - a[i])) };
                    put(&mix(&e.start_max, &e.end_max), &mix(&e.start_min, &e.end_min));
                }
            }
        }
        (p, q)
    };
    for m in &model.members {
        let mut sh = crate::analysis::MemberResult { member: m.id, length: model.length(m), stations: vec![] };
        let mut sl = sh.clone();
        for pc in main.pieces.iter().filter(|p| p.member == m.id) {
            let ts: &[f64] = if sh.stations.is_empty() { &[0.0, 0.5, 1.0] } else { &[0.5, 1.0] };
            for &t in ts {
                let (p, q) = at(m.id, pc.s0 + (pc.s1 - pc.s0) * t);
                sh.stations.push(p);
                sl.stations.push(q);
            }
        }
        // a peak between two stations (an axle can stop anywhere, the absolute maximum moment of
        // a simple span lies at L/2 - s/4): the nearer station takes it, so no extreme is missed
        // while a station on a slope keeps its exact value
        let fine: Vec<(f64, [f64; 6], [f64; 6])> = em.pieces.iter().enumerate().filter(|(_, pc)| pc.member == m.id).filter_map(|(k, pc)| env.elements.get(&(k + 1).to_string()).map(|e| (pc.s1, e.end_max, e.end_min))).collect();
        let get = |st: &Station, k: usize| [st.n, st.vy, st.vz, st.t, st.my, st.mz][k];
        let set = |st: &mut Station, k: usize, v: f64| match k {
            0 => st.n = v,
            1 => st.vy = v,
            2 => st.vz = v,
            3 => st.t = v,
            4 => st.my = v,
            _ => st.mz = v,
        };
        for j in 0..sh.stations.len().saturating_sub(1) {
            let (xa, xb) = (sh.stations[j].x, sh.stations[j + 1].x);
            for (x, fh, fl) in fine.iter().filter(|(x, _, _)| *x > xa + 1e-6 && *x < xb - 1e-6) {
                let near = if x - xa <= xb - x { j } else { j + 1 };
                for k in 0..6 {
                    if fh[k] > get(&sh.stations[j], k).max(get(&sh.stations[j + 1], k)) {
                        set(&mut sh.stations[near], k, fh[k]);
                    }
                    if fl[k] < get(&sl.stations[j], k).min(get(&sl.stations[j + 1], k)) {
                        set(&mut sl.stations[near], k, fl[k]);
                    }
                }
            }
        }
        hi.members.push(sh);
        lo.members.push(sl);
    }
    for n in &model.nodes {
        hi.displacements.insert(n.id, [0.0; 6]);
        lo.displacements.insert(n.id, [0.0; 6]);
    }
    Ok((hi, lo))
}

/// The moving-load strength combinations: 1.2 D + factor x (M max), and with M min (1.4 D
/// by the Thai Ministerial Regulation B.E. 2566, whose live load factor is 1.7).
pub fn moving_combos(model: &Model, a: &Analysis) -> Vec<(String, CaseResult)> {
    let (Some(spec), Some((hi, lo))) = (&model.moving, &a.moving) else { return vec![] };
    let fd = if model.design.standard.is_thai() { 1.4 } else { 1.2 };
    let d: Vec<(Id, f64)> = model.cases.iter().filter(|c| c.kind == CaseKind::D).map(|c| (c.id, fd)).collect();
    let mut out = vec![];
    for (env, tag) in [(hi, "max"), (lo, "min")] {
        let name = format!("ULS {fd:.1}D + {:.2} {} {tag}", spec.factor, spec.name);
        let mut r = a.combine(&name, &d);
        r.scaled_add(env, spec.factor);
        r.name = name.clone();
        out.push((name, r));
    }
    out
}

// ---------------------------------------------------------------- influence lines

#[derive(Clone, Debug, Serialize)]
pub struct InfluencePoint {
    pub member: Id,
    pub x: f64,
    pub value: f64,
}

/// The influence line of a force at x (m) along a member ("My", "Vz", "N", "T", "Mz", "Vy"),
/// or of a support reaction at a node ("Fz", "Fx", "Fy", "My", ...): a unit gravity load moved
/// over every frame member.
pub fn influence(model: &Model, a: &Analysis, quantity: &str, member: Option<Id>, x: f64, node: Option<Id>) -> Result<Vec<InfluencePoint>, String> {
    let _ = a;
    // the unit load is the only load: a model without loads has no load-break nodes, and four
    // elements a beam (four points each) trace its line closely enough
    let mut b = bare(model);
    // a node at the section, so the line's peak (the unit load over the section) is sampled
    if let (Some(mid), Some(c)) = (member, b.cases.first().map(|c| c.id)) {
        if x > 1e-6 && model.member(mid).is_some_and(|m| x < model.length(m) - 1e-6) {
            b.loads.push(Load { case: c, kind: LoadKind::Point { member: mid, a: x, p: [0.0; 3] } });
        }
    }
    let own = crate::analysis::analyze_with(&b, 4, 1)?;
    let em = own.engine.as_ref().ok_or("analyse first")?;
    let (q, el, t) = match (member, node) {
        (Some(mid), _) => {
            let m = model.member(mid).ok_or("no such member")?;
            let len = model.length(m);
            let (k, pc) = em.pieces.iter().enumerate().find(|(_, p)| p.member == mid && x >= p.s0 - 1e-9 && x <= p.s1 + 1e-9).ok_or("x is off the member")?;
            let t = if pc.s1 > pc.s0 { (x - pc.s0) / (pc.s1 - pc.s0) } else { 0.0 };
            let q = match quantity {
                "My" => "My_diag",
                "Mz" => "Mz_diag",
                other => other,
            };
            let _ = len;
            (q.to_string(), Some(k + 1), t)
        }
        (None, Some(_)) => (quantity.to_string(), None, 0.0),
        _ => return Err("give a member and x, or a support node".into()),
    };
    let mut inp = em.input.clone();
    inp.loads.clear();
    let ii = dedaliano_engine::postprocess::influence::InfluenceLineInput3D { solver: inp, quantity: q, target_node_id: match node {
        Some(n) => Some(*em.node_of.get(&n).ok_or("no such node")?),
        None => None,
    }, target_element_id: el, target_position: t, n_points_per_element: 4, gravity_direction: None };
    let r = dedaliano_engine::postprocess::influence::compute_influence_line_3d(&ii).map_err(|e| format!("influence line: {e}"))?;
    // a gravity load running down a column is no position a load can take: those points go
    let vertical = |id: Id| model.member(id).and_then(|m| Some((model.node(m.i)?, model.node(m.j)?))).is_some_and(|(a, b)| {
        let d = [b.x - a.x, b.y - a.y, b.z - a.z];
        d[2].abs() > 0.99 * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    });
    let mut pts: Vec<InfluencePoint> = r
        .points
        .iter()
        .filter_map(|p| {
            let pc = em.pieces.get(p.element_id - 1)?;
            Some(InfluencePoint { member: pc.member, x: pc.s0 + (pc.s1 - pc.s0) * p.t, value: p.value })
        })
        .filter(|p| !vertical(p.member))
        .collect();
    pts.sort_by(|a, b| a.member.cmp(&b.member).then(a.x.total_cmp(&b.x)));
    Ok(pts)
}

// ---------------------------------------------------------------- staged construction

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stage {
    pub name: String,
    /// Members built in this stage, and the load cases applied in it.
    pub members: Vec<Id>,
    pub cases: Vec<Id>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StageSummary {
    pub name: String,
    pub members: usize,
    pub max_displacement: f64,
    pub max_moment: (Id, f64),
}

/// Solves the stages as Stabileo's staged analysis does, one linear increment per stage on the
/// structure built so far: each stage's loads act on the members active in it, a member built
/// later starts free of stress (it only takes the increments after it is built), and the
/// increments add up. Pondus solves each increment with the linear solver instead of the
/// engine's `solve_staged_3d`, whose member forces leave out the fixed-end forces of the member
/// loads (K·u only), so a loaded beam's end moments came out short. Plate slabs are active in
/// every stage. The state after the last stage is returned as a result too.
pub fn staged(model: &Model, a: &Analysis) -> Result<(Vec<StageSummary>, CaseResult), String> {
    let em = a.engine.as_ref().ok_or("analyse first")?;
    if model.stages.is_empty() {
        return Err("no stages: set_stages first".into());
    }
    let full = &em.input;
    let mut active: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut total = CaseResult { name: "staged (final)".into(), ..Default::default() };
    let mut out = vec![];
    for st in &model.stages {
        for (k, p) in em.pieces.iter().enumerate() {
            if st.members.contains(&p.member) {
                active.insert(k + 1);
            }
        }
        let mut inp = full.clone();
        inp.elements.retain(|_, e| active.contains(&e.id));
        let mut used: std::collections::HashSet<usize> = inp.elements.values().flat_map(|e| [e.node_i, e.node_j]).collect();
        used.extend(inp.quads.values().flat_map(|q| q.nodes));
        inp.nodes.retain(|_, n| used.contains(&n.id));
        inp.supports.retain(|_, s| used.contains(&s.node_id));
        inp.constraints = full
            .constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::Diaphragm(d) if used.contains(&d.master_node) => {
                    let slaves: Vec<usize> = d.slave_nodes.iter().copied().filter(|n| used.contains(n)).collect();
                    (!slaves.is_empty()).then(|| Constraint::Diaphragm(DiaphragmConstraint { slave_nodes: slaves, ..d.clone() }))
                }
                Constraint::Diaphragm(_) => None,
                other => Some(other.clone()),
            })
            .collect();
        let quad_ids: std::collections::HashSet<usize> = inp.quads.values().map(|q| q.id).collect();
        inp.loads = st
            .cases
            .iter()
            .flat_map(|c| em.case_loads.get(c).cloned().unwrap_or_default())
            .filter(|l| match l {
                SolverLoad3D::Nodal(n) => used.contains(&n.node_id),
                SolverLoad3D::Distributed(d) => active.contains(&d.element_id),
                SolverLoad3D::PointOnElement(p) => active.contains(&p.element_id),
                SolverLoad3D::Thermal(t) => active.contains(&t.element_id),
                SolverLoad3D::QuadPressure(q) => quad_ids.contains(&q.element_id),
                _ => true,
            })
            .collect();
        let inc = if inp.loads.is_empty() || inp.elements.is_empty() {
            None
        } else {
            Some(dedaliano_engine::solver::linear::solve_3d(&inp).map_err(|e| format!("stage '{}': {e}", st.name))?)
        };
        let cr = crate::analysis::case_from(model, &em.pieces, &em.quads, &em.node_of, inc.as_ref(), &st.name, [0.0; 3]);
        total.scaled_add(&cr, 1.0);
        let dmax = total.displacements.values().map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()).fold(0.0, f64::max);
        let mmax = total.members.iter().flat_map(|mr| mr.stations.iter().map(move |s| (mr.member, s.my.abs()))).fold((0, 0.0), |acc, x| if x.1 > acc.1 { x } else { acc });
        out.push(StageSummary { name: st.name.clone(), members: st.members.len(), max_displacement: dmax, max_moment: mmax });
    }
    total.name = "staged (final)".into();
    Ok((out, total))
}

/// The model with no loads (and nothing that needs them), for the analyses that bring their
/// own: the stiffness is the same, the engine model is smaller.
fn bare(model: &Model) -> Model {
    let mut m = model.clone();
    m.loads.clear();
    for c in &mut m.cases {
        c.self_weight = false;
    }
    // the plate slabs stay (they are part of the structure) with no load; the others go
    let keep: Vec<bool> = m.slabs.iter().map(|s| m.plate_slab(s)).collect();
    let mut k = keep.iter();
    m.slabs.retain(|_| *k.next().unwrap());
    for s in &mut m.slabs {
        s.dead = 0.0;
        s.live = 0.0;
    }
    m.moving = None;
    m.stages.clear();
    m.seismic.enabled = false;
    m.design.second_order = false;
    m
}
