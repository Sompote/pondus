// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Earthquake: modal analysis and response spectrum analysis with the Stabileo engine, the
//! base shear of the equivalent lateral force procedure, storey drifts, and the seismic load
//! combinations, after ASCE 7-22 Ch. 12 and DPT 1301/1302-61 (มยผ. 1301/1302-61), which follows
//! it. As in Stabileo's editor, the mass is the seismic weight W = D + f·L of the model's own
//! load cases (Stabileo: `mass-source.ts`), turned into a mass density per element so the
//! engine's consistent mass carries it; densities are mass, not weight (kg/m³ = kN/m³ x 1000/g),
//! the mistake Stabileo found made every period √(1000/g) times too short.
//!
//! Response spectrum: each mode's displacement field u = Γ·Sd·φ (Sd = Sa/ω², Sa from the design
//! spectrum x Ie/R), its member forces from u by the engine (no element loads), combined over
//! the modes by CQC (5 % damping) at every station, node and storey. The result is an envelope
//! of magnitudes, so a seismic combination is taken with +E and with −E. The base shear is
//! scaled up to `elf_ratio` of the ELF base shear (ASCE 7-22 12.9.1.4: 100 %). Drifts are
//! Cd·δe/Ie from the unscaled response (12.8.6), against the allowable storey drift.
//! Not included: accidental torsion, P-Δ (12.8.7), orthogonal combination (12.5), the slabs'
//! plate moments under E (slabs are designed for gravity) and support reactions under E.

use std::collections::HashMap;

use dedaliano_engine::solver::dof::DofNumbering;
use dedaliano_engine::solver::linear::compute_internal_forces_3d_with_loads;
use dedaliano_engine::solver::modal::{solve_modal_3d_with_masses, ModalResult3D};
use dedaliano_engine::types::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analysis::{member_results, Analysis, Assembly, CaseResult};
use crate::model::*;

pub const G: f64 = 9.81;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Spectrum {
    /// The two-parameter design spectrum of ASCE 7-22 11.4.5 / DPT 1301/1302-61: SDS, SD1 (g)
    /// and the long period TL (s).
    Code { sds: f64, sd1: f64, tl: f64 },
    /// A table of (period s, Sa g), e.g. a Bangkok basin zone of DPT 1302-61.
    Table { points: Vec<(f64, f64)> },
}

impl Spectrum {
    pub fn sa(&self, t: f64) -> f64 {
        match self {
            Spectrum::Code { sds, sd1, tl } => {
                let (t0, ts) = (0.2 * sd1 / sds, sd1 / sds);
                if t < t0 {
                    sds * (0.4 + 0.6 * t / t0)
                } else if t <= ts {
                    *sds
                } else if t <= *tl {
                    sd1 / t
                } else {
                    sd1 * tl / (t * t)
                }
            }
            Spectrum::Table { points } => {
                let p = points;
                if p.is_empty() {
                    return 0.0;
                }
                if t <= p[0].0 {
                    return p[0].1;
                }
                for w in p.windows(2) {
                    if t <= w[1].0 {
                        return w[0].1 + (w[1].1 - w[0].1) * (t - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
                    }
                }
                p[p.len() - 1].1
            }
        }
    }
    pub fn sds(&self) -> f64 {
        match self {
            Spectrum::Code { sds, .. } => *sds,
            Spectrum::Table { points } => points.iter().map(|p| p.1).fold(0.0, f64::max),
        }
    }
}

/// The model's earthquake settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seismic {
    pub enabled: bool,
    pub spectrum: Spectrum,
    /// Response modification R, importance Ie, deflection amplification Cd, overstrength Ω0,
    /// redundancy ρ (Table 12.2-1; DPT 1301/1302-61 Table 2.4-1 / 3.2-1).
    pub r: f64,
    pub ie: f64,
    pub cd: f64,
    pub omega0: f64,
    pub rho: f64,
    /// Share of each live case in the seismic weight (25 % is the usual rule for floor live).
    pub live_mass: f64,
    pub damping: f64,
    /// Modes until this share of the mass takes part in x and y.
    pub mass_target: f64,
    /// The modal combination: "cqc" (default) or "srss".
    pub combination: String,
    pub max_modes: usize,
    /// The response spectrum base shear is scaled up to this share of the ELF base shear.
    pub elf_ratio: f64,
    /// Approximate period Ta = Ct·hn^x (12.8.2.1) and its upper-limit coefficient Cu.
    pub ct: f64,
    pub x: f64,
    pub cu: f64,
    /// Allowable storey drift as a share of the storey height (Table 12.12-1: 0.020).
    pub drift_limit: f64,
    /// The live load factor in the seismic combinations (1.0; 0.5 allowed for L ≤ 4.8 kPa).
    pub live_in_e: f64,
}

impl Default for Seismic {
    fn default() -> Self {
        Seismic { enabled: false, spectrum: Spectrum::Code { sds: 0.5, sd1: 0.3, tl: 6.0 }, r: 8.0, ie: 1.0, cd: 5.5, omega0: 3.0, rho: 1.0, live_mass: 0.25, damping: 0.05, mass_target: 0.9, combination: "cqc".into(), max_modes: 60, elf_ratio: 1.0, ct: 0.0466, x: 0.9, cu: 1.4, drift_limit: 0.020, live_in_e: 1.0 }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ModeInfo {
    pub period: f64,
    pub frequency: f64,
    pub mass_ratio: [f64; 3],
    /// The mode shape at the model's nodes (translations), normalised to a largest of 1.
    pub shape: HashMap<Id, [f64; 3]>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Drift {
    pub z0: f64,
    pub z1: f64,
    /// Elastic storey drift δe and the design drift Cd·δe/Ie, m; ratio to the allowable.
    pub delta_e: f64,
    pub delta: f64,
    pub allowed: f64,
    pub ratio: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Direction {
    pub name: String,
    /// The fundamental period in this direction (the mode with most mass in it), the period
    /// used for the ELF (≤ Cu·Ta), Cs and the base shears (kN).
    pub t1: f64,
    pub t_used: f64,
    pub cs: f64,
    pub v_elf: f64,
    pub v_rsa: f64,
    pub scale: f64,
    /// The CQC envelope (magnitudes) of the scaled response.
    pub case: CaseResult,
    pub drifts: Vec<Drift>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SeismicResult {
    /// Seismic weight, kN, and mass, t.
    pub weight: f64,
    pub mass: f64,
    pub modes: Vec<ModeInfo>,
    pub mass_reached: [f64; 3],
    pub dirs: Vec<Direction>,
    pub warnings: Vec<String>,
}

impl SeismicResult {
    /// "Ex" or "Ey".
    pub fn case(&self, name: &str) -> Option<&CaseResult> {
        self.dirs.iter().find(|d| d.name.eq_ignore_ascii_case(name)).map(|d| &d.case)
    }
}

/// CQC correlation of two modes (Der Kiureghian), equal damping ξ.
fn rho_cqc(wi: f64, wj: f64, xi: f64) -> f64 {
    let r = wj / wi;
    8.0 * xi * xi * (1.0 + r) * r.powf(1.5) / ((1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2))
}

fn cqc(v: &[f64], corr: &[Vec<f64>]) -> f64 {
    let mut s = 0.0;
    for i in 0..v.len() {
        for j in 0..v.len() {
            s += corr[i][j] * v[i] * v[j];
        }
    }
    s.max(0.0).sqrt()
}

pub(crate) fn run(model: &Model, asm: &Assembly, a: &Analysis) -> Result<SeismicResult, String> {
    let sc = &model.seismic;
    let mut warnings = vec![];
    if sc.r <= 0.0 || sc.ie <= 0.0 {
        return Err("seismic: R and Ie must be positive".into());
    }
    let (input, densities, weight, mut w2) = mass_model(model, asm.input, asm.pieces, asm.quads, asm.gravity, &asm.point_cases)?;
    let lumped = nodal_masses(model, asm.node_of);
    warnings.append(&mut w2);
    // modes until mass_target in x and y
    let mut n = 12usize.min(sc.max_modes.max(1));
    let modal: ModalResult3D = loop {
        let r = solve_modal_3d_with_masses(&input, &densities, &lumped, n).map_err(|e| format!("modal analysis: {e}"))?;
        let mx: f64 = r.modes.iter().map(|m| m.mass_ratio_x).sum();
        let my: f64 = r.modes.iter().map(|m| m.mass_ratio_y).sum();
        let got = r.modes.len();
        if (mx >= sc.mass_target && my >= sc.mass_target) || got < n || n >= sc.max_modes {
            break r;
        }
        n = (n * 2).min(sc.max_modes);
    };
    let total: [f64; 3] = [0, 1, 2].map(|d| modal.modes.iter().map(|m| [m.mass_ratio_x, m.mass_ratio_y, m.mass_ratio_z][d]).sum());
    if total[0] < sc.mass_target || total[1] < sc.mass_target {
        warnings.push(format!("{} modes reach {:.0} % of the mass in x and {:.0} % in y, under the {:.0} % target (12.9.1.1)", modal.modes.len(), total[0] * 100.0, total[1] * 100.0, sc.mass_target * 100.0));
    }
    let back: HashMap<usize, Id> = asm.node_of.iter().map(|(a, b)| (*b, *a)).collect();
    let modes: Vec<ModeInfo> = modal
        .modes
        .iter()
        .map(|m| {
            let mut shape: HashMap<Id, [f64; 3]> = m.displacements.iter().filter_map(|d| back.get(&d.node_id).map(|id| (*id, [d.ux, d.uy, d.uz]))).collect();
            let big = shape.values().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0, f64::max).max(1e-30);
            for v in shape.values_mut() {
                *v = v.map(|x| x / big);
            }
            ModeInfo { period: m.period, frequency: m.frequency, mass_ratio: [m.mass_ratio_x, m.mass_ratio_y, m.mass_ratio_z], shape }
        })
        .collect();
    let omegas: Vec<f64> = modal.modes.iter().map(|m| m.omega).collect();
    let srss = sc.combination.eq_ignore_ascii_case("srss");
    let corr: Vec<Vec<f64>> = omegas.iter().enumerate().map(|(i, wi)| omegas.iter().enumerate().map(|(j, wj)| if srss { if i == j { 1.0 } else { 0.0 } } else { rho_cqc(*wi, *wj, sc.damping) }).collect()).collect();
    let dof = DofNumbering::build_3d(&input);
    let mass_t = weight / G;
    // the storeys: levels with columns
    let mut levels: Vec<f64> = model.members.iter().filter(|m| model.kind(m) == MemberKind::Column).flat_map(|m| [m.i, m.j]).filter_map(|n| model.node(n).map(|n| (n.z * 1000.0).round() / 1000.0)).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let hn = levels.last().copied().unwrap_or(0.0) - levels.first().copied().unwrap_or(0.0);
    let ta = sc.ct * hn.max(0.0).powf(sc.x);
    let mut dirs = vec![];
    // per mode: the members' stations and node displacements for the mode shape φ, found once
    // and in parallel; a direction's mode result is that one times Γ·Sd (the recovery is linear
    // in u). It was recomputed for each direction, serially, at 0.5 s a mode on a 41,000-element
    // tower.
    let unit: Vec<CaseResult> = modal
        .modes
        .par_iter()
        .map(|m| {
            let mut u = vec![0.0; dof.n_total];
            let mut disp: HashMap<usize, [f64; 6]> = HashMap::new();
            for dd in &m.displacements {
                let vals = [dd.ux, dd.uy, dd.uz, dd.rx, dd.ry, dd.rz];
                for (i, v) in vals.iter().enumerate() {
                    if let Some(&idx) = dof.map.get(&(dd.node_id, i)) {
                        u[idx] = *v;
                    }
                }
                disp.insert(dd.node_id, vals);
            }
            let ef = compute_internal_forces_3d_with_loads(&input, &[], &dof, &u);
            let forces: HashMap<usize, &ElementForces3D> = ef.iter().map(|f| (f.element_id, f)).collect();
            let mut cr = CaseResult { members: member_results(model, asm.pieces, &forces, &disp), ..Default::default() };
            for nd in &model.nodes {
                cr.displacements.insert(nd.id, disp.get(&asm.node_of[&nd.id]).copied().unwrap_or([0.0; 6]));
            }
            cr
        })
        .collect();
    for (d, name) in [(0usize, "Ex"), (1, "Ey")] {
        // u = Γ Sd φ: the factor on each mode's unit result
        let mut fac = vec![];
        let mut v_mode = vec![];
        for m in &modal.modes {
            let gamma = [m.participation_x, m.participation_y, m.participation_z][d];
            let meff = [m.effective_mass_x, m.effective_mass_y, m.effective_mass_z][d];
            let sa = sc.spectrum.sa(m.period) * G * sc.ie / sc.r;
            let sd = if m.omega > 1e-10 { sa / (m.omega * m.omega) } else { 0.0 };
            fac.push(gamma * sd);
            // the engine works in kN, m and t: effective masses come out in t, so Meff·Sa is kN
            v_mode.push(meff * sa);
        }
        let v_rsa = cqc(&v_mode, &corr);
        // the ELF base shear (12.8)
        let t1 = modes.iter().max_by(|a, b| a.mass_ratio[d].total_cmp(&b.mass_ratio[d])).map(|m| m.period).unwrap_or(0.0);
        let t_used = if ta > 0.0 { t1.min(sc.cu * ta) } else { t1 };
        let sds = sc.spectrum.sds();
        // Eq. 12.8-2 to 12.8-4: SDS on the whole short-period plateau, also below T0 (the
        // design spectrum's ascending branch is for the modal analysis, not the ELF)
        let mut cs = sc.spectrum.sa(t_used) / (sc.r / sc.ie);
        if let Spectrum::Code { sd1, .. } = &sc.spectrum {
            if t_used <= sd1 / sds {
                cs = sds / (sc.r / sc.ie);
            }
            cs = cs.max(0.044 * sds * sc.ie);
        }
        cs = cs.max(0.01);
        let v_elf = cs * weight;
        let scale = if v_rsa > 1e-9 && v_rsa < sc.elf_ratio * v_elf { sc.elf_ratio * v_elf / v_rsa } else { 1.0 };
        // CQC at every station and node, scaled
        let mut env = unit[0].clone();
        env.name = name.into();
        env.applied = [0.0; 3];
        env.members.par_iter_mut().enumerate().for_each(|(mi, mr)| {
            for (si, st) in mr.stations.iter_mut().enumerate() {
                let get = |f: &dyn Fn(&crate::analysis::Station) -> f64| cqc(&unit.iter().zip(&fac).map(|(c, k)| k * f(&c.members[mi].stations[si])).collect::<Vec<_>>(), &corr) * scale;
                st.n = get(&|s| s.n);
                st.vy = get(&|s| s.vy);
                st.vz = get(&|s| s.vz);
                st.t = get(&|s| s.t);
                st.my = get(&|s| s.my);
                st.mz = get(&|s| s.mz);
                for q in 0..3 {
                    st.u[q] = get(&|s| s.u[q]);
                }
            }
        });
        for (id, v) in env.displacements.iter_mut() {
            for q in 0..6 {
                *v.get_mut(q).unwrap() = cqc(&unit.iter().zip(&fac).map(|(c, k)| k * c.displacements[id][q]).collect::<Vec<_>>(), &corr) * scale;
            }
        }
        // storey drifts: per mode the mean displacement of each level, its differences, CQC
        let mut drifts = vec![];
        let level_disp = |c: &CaseResult, z: f64| -> f64 {
            let v: Vec<f64> = model.nodes.iter().filter(|n| (n.z - z).abs() < 0.005).map(|n| c.displacements[&n.id][d]).collect();
            if v.is_empty() {
                0.0
            } else {
                v.iter().sum::<f64>() / v.len() as f64
            }
        };
        for w in levels.windows(2) {
            let per: Vec<f64> = unit.iter().zip(&fac).map(|(c, k)| k * (level_disp(c, w[1]) - level_disp(c, w[0]))).collect();
            let delta_e = cqc(&per, &corr);
            let delta = sc.cd * delta_e / sc.ie;
            let allowed = sc.drift_limit * (w[1] - w[0]);
            drifts.push(Drift { z0: w[0], z1: w[1], delta_e, delta, allowed, ratio: delta / allowed.max(1e-12) });
        }
        dirs.push(Direction { name: name.into(), t1, t_used, cs, v_elf, v_rsa, scale, case: env, drifts });
    }
    let _ = a;
    Ok(SeismicResult { weight, mass: mass_t, modes, mass_reached: total, dirs, warnings })
}

/// The mass source as the engine takes it: an analysis-only material per element whose mass
/// density carries the element's share of W = D + live_mass·L (kg/m³ from kN: x 1000/g).
/// Returns the input with those materials, the densities, the seismic weight (kN) and warnings.
pub(crate) fn mass_model(model: &Model, input0: &SolverInput3D, pieces: &[crate::analysis::Piece], quads: &[crate::analysis::Quad], gravity: &HashMap<Id, (Vec<f64>, Vec<f64>)>, point_cases: &[Id]) -> Result<(SolverInput3D, HashMap<String, f64>, f64, Vec<String>), String> {
    let sc = &model.seismic;
    let mut warnings = vec![];
    // the mass source: D cases x 1, live cases x live_mass, as densities per element
    let factor = |c: &LoadCase| match c.kind {
        CaseKind::D => 1.0,
        CaseKind::L | CaseKind::Lr => sc.live_mass,
        _ => 0.0,
    };
    for c in model.cases.iter().filter(|c| factor(c) > 0.0 && point_cases.contains(&c.id)) {
        warnings.push(format!("case {} has nodal or point loads: they carry no mass (only line, slab and self-weight loads become mass)", c.name));
    }
    let mut input = input0.clone();
    let mut densities: HashMap<String, f64> = HashMap::new();
    let mut weight = 0.0;
    let mut next_mat = 100_000usize;
    for (k, pc) in pieces.iter().enumerate() {
        let w: f64 = model.cases.iter().map(|c| factor(c) * gravity.get(&c.id).map(|g| g.0[k]).unwrap_or(0.0)).sum::<f64>().max(0.0);
        let Some(el) = input.elements.get_mut(&(k + 1).to_string()) else { continue };
        let area = model.member(pc.member).and_then(|m| model.section(m.section)).map(|s| s.props().a).unwrap_or(1.0).max(1e-9);
        let base = input.materials[&el.material_id.to_string()].clone();
        next_mat += 1;
        input.materials.insert(next_mat.to_string(), SolverMaterial { id: next_mat, e: base.e, nu: base.nu });
        el.material_id = next_mat;
        // kN/m of weight -> kg/m of mass -> kg/m³ over the section
        densities.insert(next_mat.to_string(), (w * 1000.0 / G / area).max(1.0));
        weight += w * (pc.s1 - pc.s0);
    }
    for (k, q) in quads.iter().enumerate() {
        let w: f64 = model.cases.iter().map(|c| factor(c) * gravity.get(&c.id).map(|g| g.1[k]).unwrap_or(0.0)).sum::<f64>().max(0.0);
        let Some(el) = input.quads.get_mut(&(k + 1).to_string()) else { continue };
        let base = input.materials[&el.material_id.to_string()].clone();
        next_mat += 1;
        input.materials.insert(next_mat.to_string(), SolverMaterial { id: next_mat, e: base.e, nu: base.nu });
        el.material_id = next_mat;
        densities.insert(next_mat.to_string(), (w * 1000.0 / G / el.thickness.max(1e-6)).max(1.0));
        weight += w * q.size[0] * q.size[1];
    }
    for m in input.materials.keys().cloned().collect::<Vec<_>>() {
        densities.entry(m).or_insert(1.0);
    }
    // the lumped masses' weight (their larger horizontal mass)
    weight += model.masses.iter().map(|m| m.m[0].max(m.m[1]) * G).sum::<f64>();
    if weight <= 0.0 {
        return Err("seismic: the model has no mass (no dead load, self-weight, slabs or lumped masses)".into());
    }
    Ok((input, densities, weight, warnings))
}

/// The model's lumped masses by engine node: [mx, my, mz, Ixx, Iyy, Izz] in t and t·m².
pub(crate) fn nodal_masses(model: &Model, node_of: &HashMap<Id, usize>) -> HashMap<usize, [f64; 6]> {
    let mut out: HashMap<usize, [f64; 6]> = HashMap::new();
    for nm in &model.masses {
        if let Some(&g) = node_of.get(&nm.node) {
            let e = out.entry(g).or_insert([0.0; 6]);
            for k in 0..3 {
                e[k] += nm.m[k];
                e[k + 3] += nm.r[k];
            }
        }
    }
    out
}

/// The seismic strength combinations (ASCE 7-22 2.3.6 with E = ρQE ± 0.2·SDS·D):
/// (1.2 + 0.2 SDS) D + f·L ± ρ Ex / Ey, and (0.9 − 0.2 SDS) D ± ρ Ex / Ey. By the Thai
/// Ministerial Regulation B.E. 2566 (clause 7(3)): 0.75(1.4D + 1.7L) ± 1.0 ρ Ex / Ey and
/// 0.9D ± 1.0 ρ Ex / Ey, with no vertical term.
pub fn combos(model: &Model, a: &Analysis) -> Vec<(String, CaseResult)> {
    let Some(s) = &a.seismic else { return vec![] };
    let sc = &model.seismic;
    let sds = sc.spectrum.sds();
    let thai = model.design.standard.is_thai();
    let d: Vec<Id> = model.cases.iter().filter(|c| c.kind == CaseKind::D).map(|c| c.id).collect();
    let l: Vec<Id> = model.cases.iter().filter(|c| matches!(c.kind, CaseKind::L | CaseKind::Lr)).map(|c| c.id).collect();
    let mut out = vec![];
    for dir in &s.dirs {
        let sets = if thai {
            [(1.05, "0.75(1.4D + 1.7L)".to_string(), 1.275), (0.9, "0.9D".to_string(), 0.0)]
        } else {
            [(1.2 + 0.2 * sds, format!("{:.2}D + {}L", 1.2 + 0.2 * sds, sc.live_in_e), sc.live_in_e), (0.9 - 0.2 * sds, format!("{:.2}D", 0.9 - 0.2 * sds), 0.0)]
        };
        for (base, label, live) in sets {
            let mut f: Vec<(Id, f64)> = d.iter().map(|c| (*c, base)).collect();
            f.extend(l.iter().map(|c| (*c, live)));
            for sign in [1.0, -1.0] {
                let name = format!("ULS {label} {} {}{}", if sign > 0.0 { "+" } else { "−" }, if sc.rho != 1.0 { format!("{:.1}", sc.rho) } else { String::new() }, dir.name);
                let mut r = a.combine(&name, &f);
                r.scaled_add(&dir.case, sign * sc.rho);
                r.name = name.clone();
                out.push((name, r));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_spectrum_shape() {
        let s = Spectrum::Code { sds: 0.6, sd1: 0.3, tl: 6.0 };
        // T0 = 0.1, Ts = 0.5
        assert!((s.sa(0.0) - 0.24).abs() < 1e-12);
        assert!((s.sa(0.3) - 0.6).abs() < 1e-12);
        assert!((s.sa(1.0) - 0.3).abs() < 1e-12);
        assert!((s.sa(8.0) - 0.3 * 6.0 / 64.0).abs() < 1e-12);
        assert!((rho_cqc(10.0, 10.0, 0.05) - 1.0).abs() < 1e-12);
    }
}
