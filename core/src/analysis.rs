// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Linear static analysis with the Stabileo engine (vendored in `engine/`). Each member is
//! split into elements at the nodes that lie on it, at the ends and break points of its
//! loads and into at least `SEGMENTS` pieces, so every element carries a load that is linear
//! over its whole length and the stations fall on element nodes. The stiffness is factored
//! once and each load case solved on it; a combination is the factored sum of its cases'
//! results, which is exact for a linear analysis.
//!
//! Sign conventions (from the engine, checked in the tests): local x runs from node i to
//! node j; a beam's local z is the global up direction made normal to it, so gravity bends a
//! beam about local y, and `my` is positive where the bottom is in tension (sagging).

use std::collections::HashMap;

use dedaliano_engine::element::compute_local_axes_3d;
use dedaliano_engine::postprocess::diagrams_3d::evaluate_diagram_3d_at;
use dedaliano_engine::solver::linear::prepare_static_3d;
use dedaliano_engine::types::*;
use serde::Serialize;

use crate::model::*;

/// The engine's quad pressure acts along the element normal (up for our meshes): a downward
/// load is negative. Plate moments come out hogging-positive: Pondus takes sagging positive.
/// Both checked against Navier's plate in the tests.
pub const PRESSURE_DOWN: f64 = -1.0;
pub const PLATE_SIGN: f64 = 1.0;

/// Least number of elements per beam or brace (columns: 2).
pub const SEGMENTS: usize = 8;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Station {
    /// Distance from node i, m.
    pub x: f64,
    /// Axial force (tension +), shears, torsion, kN and kN·m, local axes.
    pub n: f64,
    pub vy: f64,
    pub vz: f64,
    pub t: f64,
    /// Bending about local y (major, sagging +) and local z.
    pub my: f64,
    pub mz: f64,
    /// Displacement, global, m.
    pub u: [f64; 3],
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MemberResult {
    pub member: Id,
    pub length: f64,
    pub stations: Vec<Station>,
}

impl MemberResult {
    /// Displacement normal to the chord in the member's local z (m, up +) at each station:
    /// the member's own deflection, without the movement of its ends.
    pub fn local_deflection(&self, ez: [f64; 3]) -> Vec<f64> {
        let (Some(a), Some(b)) = (self.stations.first(), self.stations.last()) else { return vec![] };
        self.stations
            .iter()
            .map(|s| {
                let t = if self.length > 0.0 { s.x / self.length } else { 0.0 };
                let chord: Vec<f64> = (0..3).map(|k| a.u[k] + (b.u[k] - a.u[k]) * t).collect();
                (0..3).map(|k| (s.u[k] - chord[k]) * ez[k]).sum()
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CaseResult {
    pub name: String,
    pub members: Vec<MemberResult>,
    /// Node displacements (ux, uy, uz, rx, ry, rz), model nodes only.
    pub displacements: HashMap<Id, [f64; 6]>,
    pub reactions: Vec<(Id, [f64; 6])>,
    /// Total applied force (kN), global x, y, z.
    pub applied: [f64; 3],
    /// The plate elements of the slabs (plate method), in mesh order.
    pub plates: Vec<crate::slabfe::PlateResult>,
}

impl CaseResult {
    pub fn member(&self, id: Id) -> Option<&MemberResult> {
        self.members.iter().find(|m| m.member == id)
    }
    pub fn reaction_sum(&self) -> [f64; 3] {
        let mut s = [0.0; 3];
        for (_, r) in &self.reactions {
            for k in 0..3 {
                s[k] += r[k];
            }
        }
        s
    }

    pub(crate) fn scaled_add(&mut self, o: &CaseResult, f: f64) {
        if self.plates.is_empty() {
            self.plates = o.plates.iter().map(|p| crate::slabfe::PlateResult { slab: p.slab, x: p.x, y: p.y, dx: p.dx, dy: p.dy, ..Default::default() }).collect();
        }
        for (a, b) in self.plates.iter_mut().zip(&o.plates) {
            a.mx += f * b.mx;
            a.my += f * b.my;
            a.mxy += f * b.mxy;
            a.qx += f * b.qx;
            a.qy += f * b.qy;
            a.uz += f * b.uz;
        }
        for (k, v) in self.applied.iter_mut().enumerate() {
            *v += f * o.applied[k];
        }
        for (id, d) in &o.displacements {
            let e = self.displacements.entry(*id).or_insert([0.0; 6]);
            for k in 0..6 {
                e[k] += f * d[k];
            }
        }
        for (id, r) in &o.reactions {
            match self.reactions.iter_mut().find(|(n, _)| n == id) {
                Some((_, e)) => (0..6).for_each(|k| e[k] += f * r[k]),
                None => self.reactions.push((*id, r.map(|x| f * x))),
            }
        }
        for om in &o.members {
            let m = match self.members.iter().position(|m| m.member == om.member) {
                Some(i) => &mut self.members[i],
                None => {
                    self.members.push(MemberResult { member: om.member, length: om.length, stations: om.stations.iter().map(|s| Station { x: s.x, ..Default::default() }).collect() });
                    self.members.last_mut().unwrap()
                }
            };
            for (s, os) in m.stations.iter_mut().zip(&om.stations) {
                s.n += f * os.n;
                s.vy += f * os.vy;
                s.vz += f * os.vz;
                s.t += f * os.t;
                s.my += f * os.my;
                s.mz += f * os.mz;
                for k in 0..3 {
                    s.u[k] += f * os.u[k];
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Analysis {
    /// One per load case, in the model's order (ids in `case_ids`).
    pub case_ids: Vec<Id>,
    pub cases: Vec<CaseResult>,
    pub warnings: Vec<String>,
    /// Slab load put on the beams against the slabs' total, kN (dead, live).
    pub slab_applied: [f64; 2],
    pub slab_expected: [f64; 2],
    pub elements: usize,
    /// Modal and response spectrum results, when the model's seismic analysis is on.
    pub seismic: Option<crate::seismic::SeismicResult>,
    /// Second-order (P-Delta) results of the ULS combinations by id, when asked for:
    /// (result, iterations, B2-like amplification, converged and stable).
    pub second: HashMap<Id, (CaseResult, usize, f64, bool)>,
    /// The moving load's envelope (largest, least), when the model has one.
    pub moving: Option<(CaseResult, CaseResult)>,
    /// The last staged-construction solve: the state after the last stage.
    pub staged: Option<CaseResult>,
    /// The last buckling analysis: its combination and modes.
    pub buckling: Option<(String, Vec<crate::stability::BucklingMode>)>,
    /// The engine's model, for the analyses run on demand (buckling, time history).
    #[serde(skip)]
    pub engine: Option<std::sync::Arc<EngineModel>>,
}

/// The assembled engine model and each case's engine loads.
#[derive(Debug)]
pub struct EngineModel {
    pub input: SolverInput3D,
    pub case_loads: HashMap<Id, Vec<SolverLoad3D>>,
    pub(crate) node_of: HashMap<Id, usize>,
    pub(crate) pieces: Vec<Piece>,
    pub(crate) quads: Vec<Quad>,
    pub(crate) gravity: HashMap<Id, (Vec<f64>, Vec<f64>)>,
    pub(crate) point_cases: Vec<Id>,
}

impl EngineModel {
    /// A combination's engine loads: each case's loads times its factor.
    pub fn combo_loads(&self, factors: &[(Id, f64)]) -> Vec<SolverLoad3D> {
        let mut out = vec![];
        for (c, f) in factors {
            for l in self.case_loads.get(c).into_iter().flatten() {
                out.push(scale_load(l, *f));
            }
        }
        out
    }

    /// A Pondus result from an engine result.
    pub fn case_result(&self, model: &Model, r: &AnalysisResults3D, name: &str, applied: [f64; 3]) -> CaseResult {
        case_from(model, &self.pieces, &self.quads, &self.node_of, Some(r), name, applied)
    }
}

pub(crate) fn scale_load(l: &SolverLoad3D, f: f64) -> SolverLoad3D {
    match l {
        SolverLoad3D::Nodal(n) => SolverLoad3D::Nodal(SolverNodalLoad3D { fx: n.fx * f, fy: n.fy * f, fz: n.fz * f, mx: n.mx * f, my: n.my * f, mz: n.mz * f, ..n.clone() }),
        SolverLoad3D::Distributed(d) => SolverLoad3D::Distributed(SolverDistributedLoad3D { q_xi: d.q_xi * f, q_xj: d.q_xj * f, q_yi: d.q_yi * f, q_yj: d.q_yj * f, q_zi: d.q_zi * f, q_zj: d.q_zj * f, ..d.clone() }),
        SolverLoad3D::QuadPressure(p) => SolverLoad3D::QuadPressure(SolverPressureLoad { element_id: p.element_id, pressure: p.pressure * f }),
        SolverLoad3D::Thermal(t) => SolverLoad3D::Thermal(SolverThermalLoad3D { dt_uniform: t.dt_uniform * f, dt_gradient_y: t.dt_gradient_y * f, dt_gradient_z: t.dt_gradient_z * f, ..t.clone() }),
        other => other.clone(),
    }
}

/// A Pondus result from an engine result (None: nothing loaded).
pub(crate) fn case_from(model: &Model, pieces: &[Piece], quads: &[Quad], node_of: &HashMap<Id, usize>, r: Option<&AnalysisResults3D>, name: &str, applied: [f64; 3]) -> CaseResult {
    let mut cr = CaseResult { name: name.into(), applied, ..Default::default() };
    let disp: HashMap<usize, [f64; 6]> = r.map(|r| r.displacements.iter().map(|d| (d.node_id, [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz])).collect()).unwrap_or_default();
    for n in &model.nodes {
        cr.displacements.insert(n.id, disp.get(&node_of[&n.id]).copied().unwrap_or([0.0; 6]));
    }
    match r {
        Some(r) => {
            let back: HashMap<usize, Id> = node_of.iter().map(|(a, b)| (*b, *a)).collect();
            // the soil springs under a mat sit at plate nodes that are no model node: their
            // reactions are summed under node 0 (the subgrade), so the balance still shows
            // (a soil spring under a model node that has no support of its own, e.g. a column's
            // load point on a raft, is soil too)
            let mut soil = [0.0; 6];
            let mut any_soil = false;
            for re in &r.reactions {
                if let Some(id) = back.get(&re.node_id).filter(|id| model.supports.iter().any(|s| s.node == **id)) {
                    cr.reactions.push((*id, [re.fx, re.fy, re.fz, re.mx, re.my, re.mz]));
                } else {
                    for (k, v) in [re.fx, re.fy, re.fz, re.mx, re.my, re.mz].into_iter().enumerate() {
                        soil[k] += v;
                    }
                    any_soil = true;
                }
            }
            if any_soil {
                cr.reactions.push((0, soil));
            }
        }
        None => cr.reactions = model.supports.iter().map(|s| (s.node, [0.0; 6])).collect(),
    }
    let forces: HashMap<usize, &ElementForces3D> = r.map(|r| r.element_forces.iter().map(|f| (f.element_id, f)).collect()).unwrap_or_default();
    cr.members = member_results(model, pieces, &forces, &disp);
    let st: HashMap<usize, &QuadStress> = r.map(|r| r.quad_stresses.iter().map(|q| (q.element_id, q)).collect()).unwrap_or_default();
    for (k, q) in quads.iter().enumerate() {
        let s = st.get(&(k + 1));
        let uz = q.nodes.iter().map(|n| disp.get(n).map(|d| d[2]).unwrap_or(0.0)).sum::<f64>() / 4.0;
        cr.plates.push(crate::slabfe::PlateResult { slab: q.slab, x: q.c[0], y: q.c[1], dx: q.size[0], dy: q.size[1], mx: s.map(|s| PLATE_SIGN * s.mx).unwrap_or(0.0), my: s.map(|s| PLATE_SIGN * s.my).unwrap_or(0.0), mxy: s.map(|s| s.mxy).unwrap_or(0.0), qx: s.and_then(|s| s.qx).unwrap_or(0.0), qy: s.and_then(|s| s.qy).unwrap_or(0.0), uz });
    }
    cr
}

impl Analysis {
    pub fn case(&self, id: Id) -> Option<&CaseResult> {
        self.case_ids.iter().position(|c| *c == id).map(|i| &self.cases[i])
    }

    /// A combination's result: the factored sum of its cases.
    pub fn combine(&self, name: &str, factors: &[(Id, f64)]) -> CaseResult {
        let mut r = CaseResult { name: name.into(), ..Default::default() };
        for (c, f) in factors {
            if let Some(cr) = self.case(*c) {
                r.scaled_add(cr, *f);
            }
        }
        r
    }

    /// Results by name: "case:<id>" / a case name, or "combo:<id>" / a combination name.
    pub fn result(&self, model: &Model, which: &str) -> Option<CaseResult> {
        let w = which.trim();
        if let Some(s) = &self.seismic {
            if let Some(r) = s.case(w) {
                return Some(r.clone());
            }
            if let Some((_, r)) = crate::seismic::combos(model, self).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(w)) {
                return Some(r);
            }
        }
        if let Some(st) = self.staged.as_ref().filter(|s| s.name.eq_ignore_ascii_case(w)) {
            return Some(st.clone());
        }
        if let Some((hi, lo)) = &self.moving {
            if hi.name.eq_ignore_ascii_case(w) || w.eq_ignore_ascii_case("moving max") {
                return Some(hi.clone());
            }
            if lo.name.eq_ignore_ascii_case(w) || w.eq_ignore_ascii_case("moving min") {
                return Some(lo.clone());
            }
            if let Some((_, r)) = crate::advanced::moving_combos(model, self).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(w)) {
                return Some(r);
            }
        }
        if let Some(id) = w.strip_prefix("case:").and_then(|s| s.trim().parse::<Id>().ok()) {
            return self.case(id).cloned();
        }
        if let Some((cr, ..)) = model.combos.iter().find(|c| c.name.eq_ignore_ascii_case(w)).and_then(|c| self.second.get(&c.id)) {
            return Some(cr.clone());
        }
        if let Some(id) = w.strip_prefix("combo:").and_then(|s| s.trim().parse::<Id>().ok()) {
            return model.combos.iter().find(|c| c.id == id).map(|c| self.combine(&c.name, &c.factors));
        }
        if let Some(c) = model.combos.iter().find(|c| c.name.eq_ignore_ascii_case(w)) {
            return Some(self.combine(&c.name, &c.factors));
        }
        model.cases.iter().find(|c| c.name.eq_ignore_ascii_case(w)).and_then(|c| self.case(c.id).cloned())
    }
}

/// The member's local axes (ex, ey, ez) as the engine computes them.
pub fn axes(model: &Model, m: &Member) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let a = model.node(m.i)?;
    let b = model.node(m.j)?;
    Some(compute_local_axes_3d(a.x, a.y, a.z, b.x, b.y, b.z, None, None, None, (m.roll != 0.0).then_some(m.roll), false))
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A load on a member, linear between two points along it (m from node i), global kN/m.
#[derive(Clone, Debug)]
pub struct MemberLoad {
    pub case: Id,
    pub member: Id,
    pub a: f64,
    pub b: f64,
    pub wa: [f64; 3],
    pub wb: [f64; 3],
}

/// Every member load of the model with the slab loads and self-weight added.
pub fn member_loads(model: &Model, warnings: &mut Vec<String>) -> (Vec<MemberLoad>, Vec<(Id, Id, f64, [f64; 3])>, [f64; 2], [f64; 2]) {
    let mut out = vec![];
    let mut points = vec![];
    for l in &model.loads {
        match &l.kind {
            LoadKind::Line { member, wi, wj, a, b } => {
                let Some(m) = model.member(*member) else { continue };
                let len = model.length(m);
                let a = a.unwrap_or(0.0).clamp(0.0, len);
                let b = b.unwrap_or(len).clamp(0.0, len);
                if b - a > 1e-6 {
                    out.push(MemberLoad { case: l.case, member: *member, a, b, wa: *wi, wb: *wj });
                }
            }
            LoadKind::Point { member, a, p } => {
                if let Some(m) = model.member(*member) {
                    points.push((l.case, *member, a.clamp(0.0, model.length(m)), *p));
                }
            }
            LoadKind::Nodal { .. } | LoadKind::Settlement { .. } | LoadKind::Thermal { .. } => {}
        }
    }
    for c in model.cases.iter().filter(|c| c.self_weight) {
        for m in &model.members {
            let (Some(s), Some(mat)) = (model.section(m.section), model.material(m.material)) else { continue };
            let w = mat.gamma * s.props().a;
            let len = model.length(m);
            out.push(MemberLoad { case: c.id, member: m.id, a: 0.0, b: len, wa: [0.0, 0.0, -w], wb: [0.0, 0.0, -w] });
        }
    }
    // the slabs designed by coefficients load their beams; the plate slabs load themselves
    let (sl, applied, expected) = crate::slabload::loads(model, warnings);
    out.extend(sl);
    (out, points, applied, expected)
}

/// One analysis element of a member: its stretch along the member and its engine nodes.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub member: Id,
    pub s0: f64,
    pub s1: f64,
    pub ni: usize,
    pub nj: usize,
}

/// One plate element of a slab: its engine nodes, centre, size and its dead and live pressure.
#[derive(Clone, Debug)]
pub(crate) struct Quad {
    pub slab: Id,
    pub nodes: [usize; 4],
    pub c: [f64; 2],
    pub size: [f64; 2],
    pub q: [f64; 2],
}

/// The solvers a model needs beyond the linear one: Winkler foundations (linear), cables and
/// one-way members or lift-off supports (nonlinear, by the engine's cable and contact solvers).
pub(crate) struct Special {
    winkler: Vec<(usize, [f64; 2])>,
    behaviours: HashMap<String, String>,
    uplift: Vec<usize>,
    /// For each lift-off support: (its node, the fixed ground node under it), joined by a gap.
    gaps: Vec<(usize, usize)>,
    cables: bool,
}

impl Special {
    pub(crate) fn of(model: &Model, pieces: &[Piece]) -> Special {
        let mut sp = Special { winkler: vec![], behaviours: HashMap::new(), uplift: vec![], gaps: vec![], cables: false };
        for (k, pc) in pieces.iter().enumerate() {
            let Some(m) = model.member(pc.member) else { continue };
            if let Some(w) = m.winkler {
                sp.winkler.push((k + 1, w));
            }
            match m.behaviour {
                Behaviour::TensionOnly => {
                    sp.behaviours.insert((k + 1).to_string(), "tension_only".into());
                }
                Behaviour::CompressionOnly => {
                    sp.behaviours.insert((k + 1).to_string(), "compression_only".into());
                }
                Behaviour::Cable => sp.cables = true,
                Behaviour::Normal => {}
            }
        }
        sp
    }
    fn with_uplift(mut self, ids: Vec<usize>) -> Self {
        self.uplift = ids;
        self
    }
    pub(crate) fn nonlinear(&self) -> bool {
        self.cables || !self.behaviours.is_empty() || !self.uplift.is_empty()
    }
    pub(crate) fn any(&self) -> bool {
        self.nonlinear() || !self.winkler.is_empty()
    }
    pub(crate) fn solve(&self, inp: &SolverInput3D, model: &Model) -> Result<AnalysisResults3D, String> {
        use dedaliano_engine::solver::{cable, contact, winkler};
        if self.cables {
            if !self.behaviours.is_empty() || !self.uplift.is_empty() || !self.winkler.is_empty() {
                return Err("cables are not solved together with one-way members, lift-off supports or Winkler beams".into());
            }
            let dens: HashMap<String, f64> = inp.materials.keys().map(|k| (k.clone(), model.materials.iter().find(|m| m.id.to_string() == *k).map(|m| m.gamma * 1000.0 / crate::seismic::G).unwrap_or(1.0))).collect();
            let r = cable::solve_cable_3d(inp, &dens, 200, 1e-5)?;
            if !r.converged {
                return Err("the cable iteration did not converge".into());
            }
            return Ok(r.results);
        }
        if !self.behaviours.is_empty() || !self.uplift.is_empty() {
            if !self.winkler.is_empty() {
                return Err("Winkler beams are not solved together with one-way members or lift-off supports".into());
            }
            // a lift-off support: the node rests on a fixed ground node through a gap that only
            // closes (pushes up) when the node moves down
            let gaps = self.gaps.iter().enumerate().map(|(k, (node, ground))| contact::GapElement { id: k + 1, node_i: *ground, node_j: *node, direction: 2, initial_gap: 0.0, stiffness: 1e9, friction: None, friction_direction: None, friction_coefficient: None }).collect();
            let ci = contact::ContactInput3D { solver: inp.clone(), element_behaviors: self.behaviours.clone(), gap_elements: gaps, uplift_supports: vec![], max_iter: Some(60), tolerance: None, augmented_lagrangian: None, max_flips: None, damping_coefficient: None, al_max_iter: None };
            let r = contact::solve_contact_3d(&ci)?;
            if !r.converged {
                return Err("the one-way members / lift-off supports did not settle (no converged contact state)".into());
            }
            return Ok(r.results);
        }
        let springs = self.winkler.iter().map(|(e, k)| winkler::FoundationSpring3D { element_id: *e, ky: (k[1] > 0.0).then_some(k[1]), kz: (k[0] > 0.0).then_some(k[0]) }).collect();
        winkler::solve_winkler_3d(&winkler::WinklerInput3D { solver: inp.clone(), foundation_springs: springs })
    }
}

/// What the dynamic analyses take from the static assembly.
pub(crate) struct Assembly<'a> {
    pub input: &'a SolverInput3D,
    pub pieces: &'a [Piece],
    pub quads: &'a [Quad],
    pub node_of: &'a HashMap<Id, usize>,
    /// Per load case: the downward line load on each piece (kN/m, mean over it) and the
    /// pressure on each quad (kPa), the gravity the mass source turns into mass.
    pub gravity: &'a HashMap<Id, (Vec<f64>, Vec<f64>)>,
    /// Load cases with nodal or point loads, which carry no mass.
    pub point_cases: Vec<Id>,
}

/// The stations of every member from element forces and node displacements.
pub(crate) fn member_results(model: &Model, pieces: &[Piece], forces: &HashMap<usize, &ElementForces3D>, disp: &HashMap<usize, [f64; 6]>) -> Vec<MemberResult> {
    let mut out = vec![];
    for m in &model.members {
        let mut mr = MemberResult { member: m.id, length: model.length(m), stations: vec![] };
        for (k, pc) in pieces.iter().enumerate().filter(|(_, p)| p.member == m.id) {
            let (ui, uj) = (disp.get(&pc.ni).copied().unwrap_or([0.0; 6]), disp.get(&pc.nj).copied().unwrap_or([0.0; 6]));
            let ts: &[f64] = if mr.stations.is_empty() { &[0.0, 0.5, 1.0] } else { &[0.5, 1.0] };
            for &t in ts {
                let mut s = Station { x: pc.s0 + (pc.s1 - pc.s0) * t, ..Default::default() };
                if let Some(ef) = forces.get(&(k + 1)) {
                    s.n = evaluate_diagram_3d_at(ef, "axial", t);
                    s.vy = evaluate_diagram_3d_at(ef, "shearY", t);
                    s.vz = evaluate_diagram_3d_at(ef, "shearZ", t);
                    s.t = evaluate_diagram_3d_at(ef, "torsion", t);
                    s.my = evaluate_diagram_3d_at(ef, "momentY", t);
                    s.mz = evaluate_diagram_3d_at(ef, "momentZ", t);
                }
                // displacement: linear between the element's nodes (stations at t = 0.5
                // take the cubic shape's value only approximately; deflections use the nodes)
                for q in 0..3 {
                    s.u[q] = ui[q] + (uj[q] - ui[q]) * t;
                }
                mr.stations.push(s);
            }
        }
        out.push(mr);
    }
    out
}

/// Analyses the model: every load case on one factorisation.
pub fn analyze(model: &Model) -> Result<Analysis, String> {
    analyze_with(model, SEGMENTS, 2)
}

/// The analysis with at least `beams` elements per beam or brace and `columns` per column
/// (both at least 1). The plastic analysis uses a coarse model (2 and 1): hinges form only at
/// element ends, and the engine checks the mechanism with a dense rank test at every event,
/// whose cost grows with the cube of the degrees of freedom.
pub fn analyze_with(model: &Model, beams: usize, columns: usize) -> Result<Analysis, String> {
    let mut warnings = crate::check::problems(model);
    let plates_only = model.slabs.iter().any(|s| model.plate_slab(s));
    if model.members.is_empty() && !plates_only {
        return Err("the model has no members (a slab resting on supports alone needs slab_method 'plate')".into());
    }
    if model.supports.is_empty() && !model.slabs.iter().any(|s| model.plate_slab(s) && s.subgrade > 0.0) {
        return Err("the model has no supports".into());
    }
    let (mloads, points, slab_applied, slab_expected) = member_loads(model, &mut warnings);

    // split points of every member (fractions of its length)
    let mut engine_nodes: Vec<[f64; 3]> = vec![];
    let mut node_of: HashMap<Id, usize> = HashMap::new();
    // every engine node by its position (to the mm), so beams and slab meshes share nodes
    let key = |p: [f64; 3]| ((p[0] * 1000.0).round() as i64, (p[1] * 1000.0).round() as i64, (p[2] * 1000.0).round() as i64);
    let mut at: HashMap<(i64, i64, i64), usize> = HashMap::new();
    for n in &model.nodes {
        node_of.insert(n.id, engine_nodes.len() + 1);
        engine_nodes.push(n.p());
        at.entry(key(n.p())).or_insert(engine_nodes.len());
    }
    let meshes = crate::slabfe::meshes(model);
    let mesh_points: Vec<[f64; 3]> = meshes.iter().flat_map(|me| (0..me.xs.len()).flat_map(move |i| (0..me.ys.len()).map(move |j| me.point(i, j)))).collect();
    let mut pieces: Vec<Piece> = vec![];
    let mut member_nodes: HashMap<Id, Vec<(f64, usize)>> = HashMap::new();
    for m in &model.members {
        let (Some(a), Some(b)) = (model.node(m.i), model.node(m.j)) else { return Err(format!("member {} has a missing node", m.id)) };
        let len = dist(a.p(), b.p());
        if len < 1e-6 {
            return Err(format!("member {} has no length", m.id));
        }
        let d = [(b.x - a.x) / len, (b.y - a.y) / len, (b.z - a.z) / len];
        let mut cuts: Vec<f64> = vec![0.0, len];
        for q in model.nodes.iter().filter(|n| n.id != m.i && n.id != m.j).map(|n| n.p()).chain(mesh_points.iter().copied()) {
            let n = Node { id: 0, x: q[0], y: q[1], z: q[2] };
            let v = [n.x - a.x, n.y - a.y, n.z - a.z];
            let s = dot(v, d);
            if s <= TOL || s >= len - TOL {
                continue;
            }
            let off = dist(n.p(), [a.x + d[0] * s, a.y + d[1] * s, a.z + d[2] * s]);
            if off < TOL {
                cuts.push(s);
            }
        }
        for l in mloads.iter().filter(|l| l.member == m.id) {
            cuts.push(l.a);
            cuts.push(l.b);
        }
        for p in points.iter().filter(|p| p.1 == m.id) {
            cuts.push(p.2);
        }
        if m.axial_only() {
            // one element end to end: a chain of axial-only pieces would be a mechanism
            cuts = vec![0.0, len];
        }
        cuts.sort_by(f64::total_cmp);
        // cuts closer than the node tolerance would snap to one engine node (a zero-length element)
        cuts.dedup_by(|x, y| (*x - *y).abs() < TOL);
        if cuts.len() > 1 && cuts[cuts.len() - 1] < len - 1e-9 {
            // the end survives the dedup
            let n = cuts.len();
            cuts[n - 1] = len;
        }
        // at least SEGMENTS pieces (columns 2), each long piece divided evenly
        // short members (purlins, rafter pieces between purlins) need fewer: a station every
        // 0.5 m at least, two per member, so a roof of a thousand pieces stays solvable; a
        // beam on a Winkler foundation keeps the full count (its curvature varies along it)
        let want = if m.axial_only() { 1 } else if model.kind(m) == MemberKind::Column { columns.max(1) } else { beams.max(1) };
        let want = if m.winkler.is_some() { want } else { want.min(((len / 0.5).ceil() as usize).max(2)) };
        let mut fine = vec![cuts[0]];
        for w in cuts.windows(2) {
            let k = ((w[1] - w[0]) / len * want as f64).ceil().max(1.0) as usize;
            for i in 1..=k {
                fine.push(w[0] + (w[1] - w[0]) * i as f64 / k as f64);
            }
        }
        let mut ids = vec![];
        for (k, &s) in fine.iter().enumerate() {
            let id = if k == 0 {
                node_of[&m.i]
            } else if k == fine.len() - 1 {
                node_of[&m.j]
            } else {
                let p = [a.x + d[0] * s, a.y + d[1] * s, a.z + d[2] * s];
                match model.nodes.iter().find(|n| dist(n.p(), p) < TOL) {
                    Some(n) => node_of[&n.id],
                    None => *at.entry(key(p)).or_insert_with(|| {
                        engine_nodes.push(p);
                        engine_nodes.len()
                    }),
                }
            };
            ids.push((s, id));
        }
        // two cuts snapped to the same engine node make no element
        ids.dedup_by(|b, a| b.1 == a.1);
        for w in ids.windows(2) {
            pieces.push(Piece { member: m.id, s0: w[0].0, s1: w[1].0, ni: w[0].1, nj: w[1].1 });
        }
        member_nodes.insert(m.id, ids);
    }

    let mut input = SolverInput3D {
        nodes: HashMap::new(),
        materials: HashMap::new(),
        sections: HashMap::new(),
        elements: HashMap::new(),
        supports: HashMap::new(),
        loads: vec![],
        constraints: vec![],
        left_hand: None,
        plates: HashMap::new(),
        quads: HashMap::new(),
        quad9s: HashMap::new(),
        solid_shells: HashMap::new(),
        curved_shells: HashMap::new(),
        curved_beams: vec![],
        connectors: HashMap::new(),
        solver_options: None,
    };
    for (k, p) in engine_nodes.iter().enumerate() {
        input.nodes.insert((k + 1).to_string(), SolverNode3D { id: k + 1, x: p[0], y: p[1], z: p[2] });
    }
    for mt in &model.materials {
        input.materials.insert(mt.id.to_string(), SolverMaterial { id: mt.id, e: mt.e, nu: mt.nu });
    }
    let piers = crate::walls::pier_sections(model);
    for s in &model.sections {
        let p = s.props();
        // wall piers deform in shear (5/6 of the area, a rectangle); frame members too when
        // the analysis includes shear deformation. The engine's as_y belongs to bending about
        // local y, i.e. to shear along local z (the web of an H shape), and as_z to shear
        // along local y.
        let (as_y, as_z) = if piers.contains(&s.id) {
            (Some(5.0 / 6.0 * p.a), Some(5.0 / 6.0 * p.a))
        } else if model.design.shear_deformation {
            match s.shape {
                Shape::Rect { .. } => (Some(5.0 / 6.0 * p.a), Some(5.0 / 6.0 * p.a)),
                Shape::H { d, bf, tw, tf, .. } => (Some(d * tw), Some(5.0 / 3.0 * bf * tf)),
                Shape::General { shear: Some([sy, sz]), .. } => ((sz > 0.0).then_some(sz), (sy > 0.0).then_some(sy)),
                _ => (None, None),
            }
        } else {
            (None, None)
        };
        input.sections.insert(s.id.to_string(), SolverSection3D { id: s.id, name: Some(s.name.clone()), a: p.a, iy: p.iy, iz: p.iz, j: p.j.max(1e-12), cw: None, as_y, as_z });
    }
    // a truss model with rigid diaphragms needs the rotation about z, which the engine numbers
    // only when there is a frame element: its truss members then become frames pinned at both
    // ends (bending released, so they still carry axial force only)
    let pinned_frames = !model.diaphragms.is_empty() && !model.members.iter().any(|m| !m.axial_only()) && meshes.is_empty();
    for (k, pc) in pieces.iter().enumerate() {
        let m = model.member(pc.member).unwrap();
        let first = pc.s0 < 1e-9;
        let last = (pc.s1 - model.length(m)).abs() < 1e-6;
        let as_pinned_frame = pinned_frames && m.truss && m.behaviour == Behaviour::Normal;
        input.elements.insert(
            (k + 1).to_string(),
            SolverElement3D {
                id: k + 1,
                elem_type: match (m.behaviour, m.truss) {
                    (Behaviour::Cable, _) => "cable".into(),
                    _ if as_pinned_frame => "frame".into(),
                    (Behaviour::TensionOnly | Behaviour::CompressionOnly, _) | (_, true) => "truss".into(),
                    _ => "frame".into(),
                },
                node_i: pc.ni,
                node_j: pc.nj,
                material_id: m.material,
                section_id: m.section,
                release_my_start: (m.hinge_i && first) || as_pinned_frame,
                release_my_end: (m.hinge_j && last) || as_pinned_frame,
                release_mz_start: (m.hinge_i && first) || as_pinned_frame,
                release_mz_end: (m.hinge_j && last) || as_pinned_frame,
                release_t_start: false,
                release_t_end: false,
                local_yx: None,
                local_yy: None,
                local_yz: None,
                roll_angle: (m.roll != 0.0).then_some(m.roll),
            },
        );
    }
    for (k, s) in model.supports.iter().enumerate() {
        let Some(&n) = node_of.get(&s.node) else { continue };
        let f = s.fix;
        let spring = |q: usize| (!f[q] && s.springs[q] > 0.0).then_some(s.springs[q]);
        input.supports.insert(
            (k + 1).to_string(),
            SolverSupport3D { node_id: n, rx: f[0], ry: f[1], rz: f[2], rrx: f[3], rry: f[4], rrz: f[5], kx: spring(0), ky: spring(1), kz: spring(2), krx: spring(3), kry: spring(4), krz: spring(5), dx: None, dy: None, dz: None, drx: None, dry: None, drz: None, rw: None, kw: None, normal_x: None, normal_y: None, normal_z: None, is_inclined: None },
        );
    }
    let mut quads: Vec<Quad> = vec![];
    let concrete = model.concrete_id();
    let gamma = model.material(concrete).map(|m| m.gamma).unwrap_or(24.0);
    for me in &meshes {
        let Some(sl) = model.slabs.iter().find(|s| s.id == me.slab) else { continue };
        for i in 0..me.xs.len() - 1 {
            for j in 0..me.ys.len() - 1 {
                let mut ids = [0usize; 4];
                for (k, (a, b)) in [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)].into_iter().enumerate() {
                    let p = me.point(a, b);
                    ids[k] = *at.entry(key(p)).or_insert_with(|| {
                        engine_nodes.push(p);
                        input.nodes.insert(engine_nodes.len().to_string(), SolverNode3D { id: engine_nodes.len(), x: p[0], y: p[1], z: p[2] });
                        engine_nodes.len()
                    });
                }
                let id = quads.len() + 1;
                input.quads.insert(id.to_string(), SolverQuadElement { id, nodes: ids, material_id: concrete, thickness: sl.h });
                quads.push(Quad { slab: sl.id, nodes: ids, c: [(me.xs[i] + me.xs[i + 1]) / 2.0, (me.ys[j] + me.ys[j + 1]) / 2.0], size: [me.xs[i + 1] - me.xs[i], me.ys[j + 1] - me.ys[j]], q: [gamma * sl.h + sl.dead, sl.live] });
            }
        }
    }
    // the soil under a mat: a spring of ks × tributary area at every plate node of a slab with a
    // subgrade modulus, vertical and (the soil's friction) in plan, except at a node that has a
    // support of its own (a pile): the engine takes one support per node
    {
        let mut trib: HashMap<usize, f64> = HashMap::new();
        for q in &quads {
            let ks = model.slabs.iter().find(|s| s.id == q.slab).map(|s| s.subgrade).unwrap_or(0.0);
            if ks <= 0.0 {
                continue;
            }
            let k4 = ks * q.size[0] * q.size[1] / 4.0;
            for n in q.nodes {
                *trib.entry(n).or_insert(0.0) += k4;
            }
        }
        let own: std::collections::HashSet<usize> = model.supports.iter().filter_map(|s| node_of.get(&s.node).copied()).collect();
        let mut gs: Vec<usize> = trib.keys().copied().filter(|g| !own.contains(g)).collect();
        gs.sort_unstable();
        for g in gs {
            let k = trib[&g];
            input.supports.insert((700_000 + g).to_string(), SolverSupport3D { node_id: g, rx: false, ry: false, rz: false, rrx: false, rry: false, rrz: false, kx: Some(k), ky: Some(k), kz: Some(k), krx: None, kry: None, krz: None, dx: None, dy: None, dz: None, drx: None, dry: None, drz: None, rw: None, kw: None, normal_x: None, normal_y: None, normal_z: None, is_inclined: None });
        }
    }
    // a node joined only by axial members has no rotational stiffness when the engine numbers
    // rotations (any frame or shell element): its rotations are held, except the rotation about
    // z on a diaphragm level, which the diaphragm ties to its master
    if input.elements.values().any(|e| e.elem_type == "frame") || !input.quads.is_empty() {
        for n in &model.nodes {
            let at: Vec<&Member> = model.members.iter().filter(|m| m.i == n.id || m.j == n.id).collect();
            if at.is_empty() || !at.iter().all(|m| m.axial_only()) {
                continue;
            }
            let on_diaphragm = model.diaphragms.iter().any(|z| (n.z - z).abs() < 0.005);
            let g = node_of[&n.id];
            let sid = input.supports.len() + 600_000 + g;
            input.supports.insert(sid.to_string(), SolverSupport3D { node_id: g, rx: false, ry: false, rz: false, rrx: true, rry: true, rrz: !on_diaphragm, kx: None, ky: None, kz: None, krx: None, kry: None, krz: None, dx: None, dy: None, dz: None, drx: None, dry: None, drz: None, rw: None, kw: None, normal_x: None, normal_y: None, normal_z: None, is_inclined: None });
        }
    }
    // a node that carries only a lumped mass (a floor's centre of mass) moves with its
    // diaphragm in plan; its other DOFs have no stiffness, so they are held
    for nm in &model.masses {
        if model.members.iter().any(|m| m.i == nm.node || m.j == nm.node) || model.supports.iter().any(|s| s.node == nm.node) {
            continue;
        }
        let Some(n) = model.node(nm.node) else { continue };
        if !model.diaphragms.iter().any(|z| (n.z - z).abs() < 0.005) {
            return Err(format!("the mass at node {} is on no member and on no diaphragm: put it on a member's node or a diaphragm level", nm.node));
        }
        let g = node_of[&nm.node];
        let sid = input.supports.len() + 500_000 + g;
        input.supports.insert(sid.to_string(), SolverSupport3D { node_id: g, rx: false, ry: false, rz: true, rrx: true, rry: true, rrz: false, kx: None, ky: None, kz: None, krx: None, kry: None, krz: None, dx: None, dy: None, dz: None, drx: None, dry: None, drz: None, rw: None, kw: None, normal_x: None, normal_y: None, normal_z: None, is_inclined: None });
    }
    // rigid floor diaphragms: the nodes of each level follow one master in plan
    for z in &model.diaphragms {
        let at_level: Vec<&Node> = model.nodes.iter().filter(|n| (n.z - z).abs() < 0.005 && !model.supports.iter().any(|s| s.node == n.id)).collect();
        if at_level.len() < 2 {
            warnings.push(format!("diaphragm at z = {z:.2}: fewer than two free nodes, left out"));
            continue;
        }
        let (cx, cy) = (at_level.iter().map(|n| n.x).sum::<f64>() / at_level.len() as f64, at_level.iter().map(|n| n.y).sum::<f64>() / at_level.len() as f64);
        // the engine ties a slave's translations to the master but not its rotation about z,
        // so a node that carries only a lumped mass (the floor's centre of mass, with its
        // rotational inertia) must be the master: as a slave its rotation would be free
        let mass_only = |n: &Node| model.masses.iter().any(|x| x.node == n.id) && !model.members.iter().any(|m| m.i == n.id || m.j == n.id);
        let master = match at_level.iter().find(|n| mass_only(n)) {
            Some(n) => n.id,
            None => at_level.iter().min_by(|a, b| ((a.x - cx).hypot(a.y - cy)).total_cmp(&(b.x - cx).hypot(b.y - cy))).unwrap().id,
        };
        if at_level.iter().filter(|n| mass_only(n)).count() > 1 {
            warnings.push(format!("diaphragm at z = {z:.2}: more than one node carries only a mass; give each floor's mass at one node"));
        }
        input.constraints.push(Constraint::Diaphragm(DiaphragmConstraint { master_node: node_of[&master], slave_nodes: at_level.iter().filter(|n| n.id != master).map(|n| node_of[&n.id]).collect(), plane: "XY".into() }));
    }
    let constrained = !input.constraints.is_empty();
    let mut special = Special::of(model, &pieces).with_uplift(model.supports.iter().filter(|s| s.uplift).filter_map(|s| node_of.get(&s.node).copied()).collect());
    for n in special.uplift.clone() {
        if let Some(sp) = input.supports.values_mut().find(|s| s.node_id == n) {
            sp.rz = false;
        }
        let p = engine_nodes[n - 1];
        engine_nodes.push(p);
        let g = engine_nodes.len();
        input.nodes.insert(g.to_string(), SolverNode3D { id: g, x: p[0], y: p[1], z: p[2] });
        let sid = input.supports.len() + 1000 + g;
        input.supports.insert(sid.to_string(), SolverSupport3D { node_id: g, rx: true, ry: true, rz: true, rrx: true, rry: true, rrz: true, kx: None, ky: None, kz: None, krx: None, kry: None, krz: None, dx: None, dy: None, dz: None, drx: None, dry: None, drz: None, rw: None, kw: None, normal_x: None, normal_y: None, normal_z: None, is_inclined: None });
        special.gaps.push((n, g));
    }
    if special.nonlinear() && model.design.second_order {
        warnings.push("P-Delta is not combined with cables, one-way members or lift-off supports: those combinations are solved without it".into());
    }
    // the unconstrained factorisation serves only the models without diaphragms: a diaphragm
    // model is solved case by case on the constrained system, and a node that gets its
    // in-plane stiffness only through its diaphragm (a floor's centre of mass) would make the
    // unconstrained matrix singular
    let prepared = if special.any() || constrained { None } else { Some(prepare_static_3d(&input).map_err(|e| format!("the structure cannot be solved: {e}. Check the supports (is it stable?) and that every member is connected."))?) };

    let (mut slab_applied, mut slab_expected) = (slab_applied, slab_expected);
    if !quads.is_empty() {
        for q in &quads {
            let a = q.size[0] * q.size[1];
            for k in 0..2 {
                slab_applied[k] += q.q[k] * a;
            }
        }
        for s in &model.slabs {
            slab_expected[0] += (gamma * s.h + s.dead) * s.lx() * s.ly();
            slab_expected[1] += s.live * s.lx() * s.ly();
        }
    }
    let mut out = Analysis { warnings, slab_applied, slab_expected, elements: pieces.len() + quads.len(), ..Default::default() };
    let mut gravity: HashMap<Id, (Vec<f64>, Vec<f64>)> = HashMap::new();
    let mut case_loads: HashMap<Id, Vec<SolverLoad3D>> = HashMap::new();
    let mut point_cases = vec![];
    for case in &model.cases {
        let mut loads: Vec<SolverLoad3D> = vec![];
        let mut applied = [0.0; 3];
        let mut g_piece = vec![0.0; pieces.len()];
        let mut g_quad = vec![0.0; quads.len()];
        if model.loads.iter().any(|l| l.case == case.id && !matches!(l.kind, LoadKind::Line { .. })) {
            point_cases.push(case.id);
        }
        for l in model.loads.iter().filter(|l| l.case == case.id) {
            if let LoadKind::Nodal { node, f } = l.kind {
                if let Some(&n) = node_of.get(&node) {
                    loads.push(SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: n, fx: f[0], fy: f[1], fz: f[2], mx: f[3], my: f[4], mz: f[5], bw: None }));
                    (0..3).for_each(|k| applied[k] += f[k]);
                }
            }
        }
        // a truss member's own loads go to its ends by statics
        for l in mloads.iter().filter(|l| l.case == case.id && model.member(l.member).is_some_and(|m| m.axial_only())) {
            let m = model.member(l.member).unwrap();
            let len = model.length(m);
            for k in 0..3 {
                let (wa, wb) = (l.wa[k], l.wb[k]);
                let total = (wa + wb) / 2.0 * (l.b - l.a);
                if total == 0.0 {
                    continue;
                }
                // the load's centroid from node i
                let xc = if (wa + wb).abs() > 1e-12 { l.a + (l.b - l.a) * (wa + 2.0 * wb) / (3.0 * (wa + wb)) } else { (l.a + l.b) / 2.0 };
                for (node, share) in [(m.i, total * (1.0 - xc / len)), (m.j, total * xc / len)] {
                    let mut f = [0.0; 6];
                    f[k] = share;
                    loads.push(SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: node_of[&node], fx: f[0], fy: f[1], fz: f[2], mx: 0.0, my: 0.0, mz: 0.0, bw: None }));
                    applied[k] += share;
                }
            }
        }
        for (c, member, a, p) in points.iter().filter(|p| p.0 == case.id) {
            if let Some(m) = model.member(*member).filter(|m| m.axial_only()) {
                let len = model.length(m);
                for (node, share) in [(m.i, 1.0 - a / len), (m.j, a / len)] {
                    loads.push(SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: node_of[&node], fx: p[0] * share, fy: p[1] * share, fz: p[2] * share, mx: 0.0, my: 0.0, mz: 0.0, bw: None }));
                }
                (0..3).for_each(|k| applied[k] += p[k]);
                let _ = c;
                continue;
            }
            let ids = &member_nodes[member];
            let n = ids.iter().min_by(|x, y| (x.0 - a).abs().total_cmp(&(y.0 - a).abs())).unwrap().1;
            let _ = c;
            loads.push(SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: n, fx: p[0], fy: p[1], fz: p[2], mx: 0.0, my: 0.0, mz: 0.0, bw: None }));
            (0..3).for_each(|k| applied[k] += p[k]);
        }
        for (k, pc) in pieces.iter().enumerate() {
            let m = model.member(pc.member).unwrap();
            if m.axial_only() {
                continue;
            }
            let (ex, ey, ez) = axes(model, m).unwrap();
            let (mut wi, mut wj) = ([0.0; 3], [0.0; 3]);
            let mut any = false;
            for l in mloads.iter().filter(|l| l.case == case.id && l.member == pc.member) {
                // the element lies wholly inside or outside the load (it was split at a and b)
                if pc.s0 < l.a - 1e-6 || pc.s1 > l.b + 1e-6 {
                    continue;
                }
                let at = |s: f64| -> [f64; 3] {
                    let t = if l.b > l.a { (s - l.a) / (l.b - l.a) } else { 0.0 };
                    [l.wa[0] + (l.wb[0] - l.wa[0]) * t, l.wa[1] + (l.wb[1] - l.wa[1]) * t, l.wa[2] + (l.wb[2] - l.wa[2]) * t]
                };
                let (p, q) = (at(pc.s0), at(pc.s1));
                for k in 0..3 {
                    wi[k] += p[k];
                    wj[k] += q[k];
                    applied[k] += (p[k] + q[k]) / 2.0 * (pc.s1 - pc.s0);
                }
                any = true;
            }
            if any {
                g_piece[k] = -(wi[2] + wj[2]) / 2.0;
                loads.push(SolverLoad3D::Distributed(SolverDistributedLoad3D { element_id: k + 1, q_xi: dot(wi, ex), q_xj: dot(wj, ex), q_yi: dot(wi, ey), q_yj: dot(wj, ey), q_zi: dot(wi, ez), q_zj: dot(wj, ez), a: None, b: None }));
            }
        }
        // temperature: on every element of the member
        let mut settle: Vec<(Id, [f64; 6])> = vec![];
        for l in model.loads.iter().filter(|l| l.case == case.id) {
            match l.kind {
                LoadKind::Thermal { member, dt, dt_y, dt_z } => {
                    // the engine takes α = 12e-6 /°C for every material: the temperatures are
                    // scaled so that αΔT is the member material's
                    let f = model.member(member).and_then(|m| model.material(m.material)).map(|mt| mt.alpha() / 12e-6).unwrap_or(1.0);
                    for (k, pc) in pieces.iter().enumerate().filter(|(_, p)| p.member == member) {
                        let _ = pc;
                        loads.push(SolverLoad3D::Thermal(SolverThermalLoad3D { element_id: k + 1, dt_uniform: dt * f, dt_gradient_y: dt_y * f, dt_gradient_z: dt_z * f }));
                    }
                }
                LoadKind::Settlement { node, d } => settle.push((node, d)),
                _ => {}
            }
        }
        let (dc, lc) = model.slab_cases();
        for (k, q) in quads.iter().enumerate() {
            let w = if Some(case.id) == dc { q.q[0] } else if Some(case.id) == lc { q.q[1] } else { 0.0 };
            if w != 0.0 {
                g_quad[k] = w;
                loads.push(SolverLoad3D::QuadPressure(SolverPressureLoad { element_id: k + 1, pressure: PRESSURE_DOWN * w }));
                applied[2] -= w * q.size[0] * q.size[1];
            }
        }
        let r = if loads.is_empty() && settle.is_empty() {
            None
        } else if special.any() {
            let mut inp = input.clone();
            inp.loads = loads.clone();
            if !settle.is_empty() {
                out.warnings.push(format!("case {}: settlements are not combined with cables, one-way members, lift-off supports or Winkler beams", case.name));
            }
            Some(special.solve(&inp, model).map_err(|e| format!("case {}: {e}", case.name))?)
        } else if constrained || !settle.is_empty() {
            // a settlement changes the supports, a diaphragm the system: solve this case on its own
            let mut inp = input.clone();
            inp.loads = loads.clone();
            for (node, d) in &settle {
                let Some(&n) = node_of.get(node) else { continue };
                match inp.supports.values_mut().find(|s| s.node_id == n) {
                    Some(sp) => {
                        let held = [sp.rx, sp.ry, sp.rz, sp.rrx, sp.rry, sp.rrz];
                        let slots = [&mut sp.dx, &mut sp.dy, &mut sp.dz, &mut sp.drx, &mut sp.dry, &mut sp.drz];
                        for (q, slot) in slots.into_iter().enumerate() {
                            if held[q] && d[q] != 0.0 {
                                *slot = Some(d[q]);
                            }
                        }
                    }
                    None => return Err(format!("case {}: a settlement at node {node}, which has no support", case.name)),
                }
            }
            Some(dedaliano_engine::solver::linear::solve_3d(&inp).map_err(|e| format!("case {}: {e}", case.name))?)
        } else {
            Some(prepared.as_ref().unwrap().solve_loads(&loads).map_err(|e| format!("case {}: {e}", case.name))?)
        };
        let cr = case_from(model, &pieces, &quads, &node_of, r.as_ref(), &case.name, applied);
        case_loads.insert(case.id, loads);
        out.case_ids.push(case.id);
        out.cases.push(cr);
        gravity.insert(case.id, (g_piece, g_quad));
    }
    if model.seismic.enabled {
        let asm = Assembly { input: &input, pieces: &pieces, quads: &quads, node_of: &node_of, gravity: &gravity, point_cases: point_cases.clone() };
        out.seismic = Some(crate::seismic::run(model, &asm, &out)?);
    }
    let em = EngineModel { input, case_loads, node_of, pieces, quads, gravity, point_cases };
    // a nonlinear model: every combination solved on its own loads (no superposition)
    if special.nonlinear() {
        for c in &model.combos {
            let mut inp = em.input.clone();
            inp.loads = em.combo_loads(&c.factors);
            if inp.loads.is_empty() {
                continue;
            }
            let applied = out.combine(&c.name, &c.factors).applied;
            match special.solve(&inp, model) {
                Ok(r) => {
                    let cr = em.case_result(model, &r, &c.name, applied);
                    out.second.insert(c.id, (cr, 0, 1.0, true));
                }
                Err(e) => out.warnings.push(format!("combination '{}': {e}", c.name)),
            }
        }
    }
    // second order: every ULS combination solved with P-Delta on its own loads
    if model.design.second_order {
        for c in model.combos.iter().filter(|c| c.kind == ComboKind::Uls) {
            let mut inp = em.input.clone();
            inp.loads = em.combo_loads(&c.factors);
            if inp.loads.is_empty() {
                continue;
            }
            let applied = out.combine(&c.name, &c.factors).applied;
            match dedaliano_engine::solver::pdelta::solve_pdelta_3d(&inp, 40, 1e-5) {
                Ok(p) => {
                    let ok = p.converged && p.is_stable;
                    if !ok {
                        out.warnings.push(format!("P-Delta of '{}' {}: the structure is unstable under it (or near its buckling load)", c.name, if p.converged { "is unstable" } else { "did not converge" }));
                    }
                    let cr = em.case_result(model, &p.results, &c.name, applied);
                    out.second.insert(c.id, (cr, p.iterations, p.b2_factor, ok));
                }
                Err(e) => out.warnings.push(format!("P-Delta of '{}': {e}", c.name)),
            }
        }
    }
    out.engine = Some(std::sync::Arc::new(em));
    if let Some(spec) = &model.moving {
        match crate::advanced::moving_loads(model, &out, spec) {
            Ok(env) => out.moving = Some(env),
            Err(e) => out.warnings.push(e),
        }
    }
    Ok(out)
}
