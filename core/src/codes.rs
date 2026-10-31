// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Other design codes through the Stabileo engine's checks: Eurocode 3 (EN 1993-1-1) for steel
//! H members as an alternative to AISC 360 (classification, buckling curves of Table 6.2,
//! lateral-torsional buckling with the load on the top flange, Annex B interaction), and a
//! Eurocode 2 (EN 1992-1-1) check of the concrete beams with the bars Pondus chose by ACI 318.
//! Units into the engine: N, m, Pa.

use dedaliano_engine::postprocess::ec2_check::*;
use dedaliano_engine::postprocess::ec3_check::*;
use serde::{Deserialize, Serialize};

use crate::model::{DesignSettings, Props};
use crate::rc::BeamDesign;
use crate::sections::bar_area;
use crate::steel::{HDims, SteelCheck, SteelDemand, SteelDesign};

/// The load factors and strength reduction factors the design uses.
///
/// - `Aci`: ACI 318-19 Table 21.2.1 and AISC 360-16, with the ASCE 7-22 combinations.
/// - `Mr2566` / `Mr2566NoQc`: the Thai Ministerial Regulation on structural design (กฎกระทรวง
///   กำหนดการออกแบบโครงสร้างอาคาร และลักษณะและคุณสมบัติของวัสดุที่ใช้ในงานโครงสร้างอาคาร
///   พ.ศ. 2566, Royal Gazette vol. 140 part 54 ก, 6 September 2023, in force 180 days later),
///   which replaces Ministerial Regulation No. 6 (B.E. 2527). Clause 7 gives the factored loads
///   U = 1.4D + 1.7L, 0.75(1.4D + 1.7L) + 1.6W, 0.9D + 1.6W, 0.75(1.4D + 1.7L) + 1.0E and
///   0.9D + 1.0E, and clause 8 the strength reduction factors of reinforced concrete with
///   specified construction standards and material quality control (flexure and axial tension
///   0.90, tied columns 0.70, spiral 0.75, shear and torsion 0.85, bearing 0.70) or without
///   them (0.75, 0.60, 0.625, 0.70, 0.60), and of structural steel (tension yield 0.90, rupture
///   0.75, compression, flexure and shear 0.90, connections and bolts 0.75).
///
/// Between the compression-controlled and the tension-controlled φ the design goes linearly in
/// the extreme tension strain εt from εty to εty + 0.003, as ACI 318-19 21.2.2 (and ACI 318-05
/// Appendix C, from which the regulation's factors come) does; the regulation itself gives only
/// the two end values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignStandard {
    #[default]
    Aci,
    Mr2566,
    #[serde(rename = "mr2566_noqc")]
    Mr2566NoQc,
}

/// The strength reduction factors of one standard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phi {
    /// Concrete: tension-controlled flexure and axial tension.
    pub flexure: f64,
    /// Concrete: compression-controlled tied members.
    pub tied: f64,
    /// Concrete: shear and torsion.
    pub shear: f64,
    /// Steel: shear of a rolled web (None: AISC 360-16 G2.1, 1.0 for stocky webs, else 0.9).
    pub steel_shear: Option<f64>,
    /// Steel connections: shear yielding of a plate (AISC J4.2: 1.0).
    pub conn_shear_yield: f64,
}

impl DesignStandard {
    pub fn phi(self) -> Phi {
        match self {
            DesignStandard::Aci => Phi { flexure: 0.90, tied: 0.65, shear: 0.75, steel_shear: None, conn_shear_yield: 1.0 },
            DesignStandard::Mr2566 => Phi { flexure: 0.90, tied: 0.70, shear: 0.85, steel_shear: Some(0.90), conn_shear_yield: 0.75 },
            DesignStandard::Mr2566NoQc => Phi { flexure: 0.75, tied: 0.60, shear: 0.70, steel_shear: Some(0.90), conn_shear_yield: 0.75 },
        }
    }
    pub fn is_thai(self) -> bool {
        self != DesignStandard::Aci
    }
    pub fn label(self) -> &'static str {
        match self {
            DesignStandard::Aci => "ACI 318-19 / AISC 360-16 / ASCE 7-22",
            DesignStandard::Mr2566 => "Thai Ministerial Regulation B.E. 2566 (quality controlled)",
            DesignStandard::Mr2566NoQc => "Thai Ministerial Regulation B.E. 2566 (no quality control)",
        }
    }
    /// The factored gravity load on a slab or stair, kPa (dead wd and live wl).
    pub fn gravity(self, wd: f64, wl: f64) -> f64 {
        if self.is_thai() {
            1.4 * wd + 1.7 * wl
        } else {
            (1.2 * wd + 1.6 * wl).max(1.4 * wd)
        }
    }
    /// The strength and service combinations of this standard for the model's load cases (D,
    /// L and Lr, W; the earthquake ones are added at design from the seismic analysis). Each
    /// wind case is taken as defined, so a reversed wind needs its own case.
    pub fn combinations(self, cases: &[crate::model::LoadCase]) -> Vec<crate::model::Combination> {
        use crate::model::{CaseKind, Combination, ComboKind};
        let of = |k: &[CaseKind]| cases.iter().filter(|c| k.contains(&c.kind)).map(|c| c.id).collect::<Vec<_>>();
        let (d, l, lr, w) = (of(&[CaseKind::D]), of(&[CaseKind::L]), of(&[CaseKind::Lr]), of(&[CaseKind::W]));
        let mut out: Vec<Combination> = vec![];
        let mut push = |name: String, kind: ComboKind, f: Vec<(crate::model::Id, f64)>| {
            if !f.is_empty() {
                out.push(Combination { id: out.len() + 1, name, kind, factors: f });
            }
        };
        let with = |ids: &[crate::model::Id], x: f64| ids.iter().map(|c| (*c, x)).collect::<Vec<_>>();
        let cat = |a: Vec<(crate::model::Id, f64)>, b: Vec<(crate::model::Id, f64)>| a.into_iter().chain(b).collect::<Vec<_>>();
        let wname = |id: crate::model::Id| cases.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_default();
        if self.is_thai() {
            // the regulation has one live load (นจ.), so the roof live load goes with it
            let ll: Vec<_> = l.iter().chain(&lr).copied().collect();
            push("ULS 1.4D + 1.7L".into(), ComboKind::Uls, cat(with(&d, 1.4), with(&ll, 1.7)));
            for &wi in &w {
                push(format!("ULS 0.75(1.4D + 1.7L) + 1.6{}", wname(wi)), ComboKind::Uls, cat(cat(with(&d, 1.05), with(&ll, 1.275)), vec![(wi, 1.6)]));
                push(format!("ULS 0.9D + 1.6{}", wname(wi)), ComboKind::Uls, cat(with(&d, 0.9), vec![(wi, 1.6)]));
            }
        } else {
            push("ULS 1.4D".into(), ComboKind::Uls, with(&d, 1.4));
            push(if lr.is_empty() { "ULS 1.2D + 1.6L".into() } else { "ULS 1.2D + 1.6L + 0.5Lr".into() }, ComboKind::Uls, cat(cat(with(&d, 1.2), with(&l, 1.6)), with(&lr, 0.5)));
            if !lr.is_empty() {
                push("ULS 1.2D + 1.6Lr + L".into(), ComboKind::Uls, cat(cat(with(&d, 1.2), with(&lr, 1.6)), with(&l, 1.0)));
            }
            for &wi in &w {
                push(format!("ULS 1.2D + L{} + 1.0{}", if lr.is_empty() { "" } else { " + 0.5Lr" }, wname(wi)), ComboKind::Uls, cat(cat(cat(with(&d, 1.2), with(&l, 1.0)), with(&lr, 0.5)), vec![(wi, 1.0)]));
                push(format!("ULS 0.9D + 1.0{}", wname(wi)), ComboKind::Uls, cat(with(&d, 0.9), vec![(wi, 1.0)]));
            }
        }
        push("SLS D + L".into(), ComboKind::Sls, cat(with(&d, 1.0), with(&l, 1.0)));
        out
    }
}

thread_local! {
    static ACTIVE: std::cell::Cell<DesignStandard> = const { std::cell::Cell::new(DesignStandard::Aci) };
}

/// The strength reduction factors of the standard the current design pass uses.
pub fn phi() -> Phi {
    ACTIVE.with(|a| a.get()).phi()
}

/// Makes `s` the standard of this thread's design until the guard drops (the design pass runs
/// on one thread, so each pass, and each test, sees its own).
pub fn use_standard(s: DesignStandard) -> StandardGuard {
    StandardGuard(ACTIVE.with(|a| a.replace(s)))
}

pub struct StandardGuard(DesignStandard);

impl Drop for StandardGuard {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.set(self.0));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteelCode {
    #[default]
    Aisc,
    Ec3,
}

/// EN 1993-1-1 Table 5.2: the class of a rolled I in compression and bending.
pub fn ec3_class(h: HDims, fy: f64, compression: bool) -> (u8, SectionClass) {
    let eps = (235.0 / fy).sqrt();
    let cf = (h.bf / 2.0 - h.tw / 2.0 - h.r) / h.tf;
    let cw = (h.d - 2.0 * h.tf - 2.0 * h.r) / h.tw;
    let flange = if cf <= 9.0 * eps { 1 } else if cf <= 10.0 * eps { 2 } else if cf <= 14.0 * eps { 3 } else { 4 };
    let (l1, l2, l3) = if compression { (33.0, 38.0, 42.0) } else { (72.0, 83.0, 124.0) };
    let web = if cw <= l1 * eps { 1 } else if cw <= l2 * eps { 2 } else if cw <= l3 * eps { 3 } else { 4 };
    let c = flange.max(web);
    (c, match c {
        1 => SectionClass::Class1,
        2 => SectionClass::Class2,
        3 => SectionClass::Class3,
        _ => SectionClass::Class4,
    })
}

/// EN 1993-1-1 Table 5.2 for a rolled I under compression N (N, compression +) and major-axis
/// bending M (N·mm) together: the web by α (plastic neutral axis, Classes 1 and 2) and ψ
/// (elastic stress ratio, Class 3), the flange as uniformly compressed outstands.
pub fn ec3_class_nm(h: HDims, p: &Props, fy: f64, n: f64, m: f64) -> (u8, SectionClass) {
    let eps = (235.0 / fy).sqrt();
    let cf = (h.bf / 2.0 - h.tw / 2.0 - h.r) / h.tf;
    let c = h.d - 2.0 * h.tf - 2.0 * h.r;
    let cw = c / h.tw;
    let flange = if cf <= 9.0 * eps { 1 } else if cf <= 10.0 * eps { 2 } else if cf <= 14.0 * eps { 3 } else { 4 };
    let n = n.max(0.0);
    let m = m.abs();
    let alpha = ((c / 2.0 + n / (2.0 * h.tw * fy)) / c).clamp(0.0, 1.0);
    let (l1, l2) = if alpha > 0.5 { (396.0 * eps / (13.0 * alpha - 1.0), 456.0 * eps / (13.0 * alpha - 1.0)) } else { (36.0 * eps / alpha.max(1e-9), 41.5 * eps / alpha.max(1e-9)) };
    // ψ from the elastic stresses at the web's ends (compression +)
    let (a, iy) = (p.a * 1e6, p.iy * 1e12);
    let s1 = n / a + m * (c / 2.0) / iy;
    let s2 = n / a - m * (c / 2.0) / iy;
    let psi = if s1.abs() > 1e-12 { s2 / s1 } else { 1.0 };
    let l3 = if psi > -1.0 { 42.0 * eps / (0.67 + 0.33 * psi) } else { 62.0 * eps * (1.0 - psi) * (-psi).sqrt() };
    let web = if cw <= l1 { 1 } else if cw <= l2 { 2 } else if cw <= l3 { 3 } else { 4 };
    let k = flange.max(web);
    (k, match k {
        1 => SectionClass::Class1,
        2 => SectionClass::Class2,
        3 => SectionClass::Class3,
        _ => SectionClass::Class4,
    })
}

/// Table 6.2 (rolled I, tf ≤ 40 mm, S235–S420) and Table 6.4 (LTB, general case).
pub fn curves(h: HDims) -> (BucklingCurve, BucklingCurve, BucklingCurve) {
    let deep = h.d / h.bf > 1.2;
    let (y, z) = if deep { (BucklingCurve::A, BucklingCurve::B) } else { (BucklingCurve::B, BucklingCurve::C) };
    let lt = if h.d / h.bf <= 2.0 { BucklingCurve::A } else { BucklingCurve::B };
    (y, z, lt)
}

/// The engine's member data of an H shape as Pondus sets it up for EC3: Table 5.2 class (in
/// compression when the member ever is), Table 6.2 / 6.4 curves, Av of 6.2.6(3) with η = 1,
/// Lcr = k·L about y and k·Lb about z (the bracing that holds the compression flange holds
/// the weak axis too), LTB over Lb with C1 from the moment diagram (the AISC Cb
/// formula) and the load on the top flange for beams. Units N, m, Pa.
pub fn ec3_member(id: usize, p: &Props, h: HDims, fy: f64, dem: &SteelDemand, c1: f64, compression: bool) -> Ec3MemberData {
    let (_, class) = ec3_class(h, fy, compression);
    let (cy, cz, clt) = curves(h);
    let av = (p.a * 1e6 - 2.0 * h.bf * h.tf + (h.tw + 2.0 * h.r) * h.tf).max((h.d - 2.0 * h.tf) * h.tw) / 1e6;
    Ec3MemberData {
        element_id: id,
        fy: fy * 1e6,
        e: Some(210e9),
        a: p.a,
        a_eff: None,
        wpl_y: p.zy,
        wel_y: p.sy,
        weff_y: None,
        wpl_z: p.zz,
        wel_z: p.sz,
        weff_z: None,
        e_n_y: None,
        e_n_z: None,
        iy: p.iy,
        iz: p.iz,
        it: p.j,
        iw: p.cw,
        lcr_y: dem.k * dem.length,
        lcr_z: dem.k * dem.lb.min(dem.length),
        lb: dem.lb,
        section_class: class,
        buckling_curve_y: cy,
        buckling_curve_z: cz,
        buckling_curve_lt: clt,
        gamma_m0: Some(1.0),
        gamma_m1: Some(1.0),
        c1: Some(c1),
        av: Some(av),
        interaction: None,
        geometry: Some(Ec3Geometry::I { h: h.d / 1e3, b: h.bf / 1e3, tw: h.tw / 1e3, tf: h.tf / 1e3, r: h.r / 1e3 }),
        ltb: Some(Ec3LtbData { c2: None, load_position: Some(if dem.top_flange_load { Ec3LoadPosition::TopFlange } else { Ec3LoadPosition::ShearCentre }), zg: None }),
    }
}

/// C1 of a combination's stations: the AISC Cb formula over the member (1 for a cantilever).
pub fn ec3_c1(dem: &SteelDemand, st: &[(f64, f64, f64, f64, f64, f64)]) -> f64 {
    if dem.cantilever {
        return 1.0;
    }
    diagram(dem, st).0.unwrap_or_else(|| crate::steel::cb(&st.iter().map(|s| (s.0, s.4)).collect::<Vec<_>>(), dem.length).min(2.6))
}

/// The LTB data of a combination (SN003b): the load on the top flange for beams, at the
/// shear centre otherwise. C2 from the moment diagram where SN003 tabulates it: 0 for end
/// moments only (a linear diagram: no transverse load, so the load height does not enter),
/// 0.454 with C1 = 1.127 for a simply supported span under a uniform load (a parabola with no
/// end moments, SN003 Table 3.2); otherwise left to the engine, which takes C2 = C1 for a load
/// on the top flange, a lower bound of Mcr over the transverse-load cases of Table 3.2.
pub fn ec3_ltb(dem: &SteelDemand, st: &[(f64, f64, f64, f64, f64, f64)]) -> Option<Ec3LtbData> {
    let pos = Some(if dem.top_flange_load { Ec3LoadPosition::TopFlange } else { Ec3LoadPosition::ShearCentre });
    Some(Ec3LtbData { c2: diagram(dem, st).1, load_position: pos, zg: None })
}

/// (C1 override, C2) of a moment diagram that SN003 tabulates; (None, None) otherwise.
fn diagram(dem: &SteelDemand, st: &[(f64, f64, f64, f64, f64, f64)]) -> (Option<f64>, Option<f64>) {
    if st.len() < 3 || dem.cantilever || dem.length <= 0.0 {
        return (None, None);
    }
    let l = dem.length;
    let mmax = st.iter().map(|s| s.4.abs()).fold(0.0, f64::max);
    if mmax <= 0.0 {
        return (None, None);
    }
    let (m0, m1) = (st[0].4, st[st.len() - 1].4);
    let tol = 0.01 * mmax;
    // linear: every station on the line between the end moments
    if st.iter().all(|s| (s.4 - (m0 + (m1 - m0) * s.0 / l)).abs() <= tol) {
        return (None, Some(0.0));
    }
    // a parabola with zero end moments (a uniformly loaded simply supported span)
    let mid = st.iter().min_by(|a, b| (a.0 - l / 2.0).abs().total_cmp(&(b.0 - l / 2.0).abs())).map(|s| s.4).unwrap_or(0.0);
    if m0.abs() <= tol && m1.abs() <= tol && st.iter().all(|s| (s.4 - 4.0 * mid * s.0 * (l - s.0) / (l * l)).abs() <= tol) {
        return (Some(1.127), Some(0.454));
    }
    (None, None)
}

/// Checks an H member to EC3 (γM0 = γM1 = 1.0, the recommended values) under every station of
/// every ULS combination; the result has the shape of the AISC one.
#[allow(clippy::too_many_arguments)]
pub fn check_ec3(member: crate::model::Id, bim_id: Option<String>, name: &str, mass: f64, p: &Props, h: HDims, fy: f64, fu: f64, dem: &SteelDemand, set: &DesignSettings) -> SteelDesign {
    let compression = dem.uls.iter().flat_map(|(_, s)| s.iter()).any(|s| s.1 < 0.0);
    let cls = dem.uls.iter().flat_map(|(_, st)| st.iter()).map(|s| ec3_class_nm(h, p, fy, -s.1 * 1e3, s.4 * 1e6).0).max().unwrap_or_else(|| ec3_class(h, fy, compression).0);
    let (cy, cz, clt) = curves(h);
    let mk = |id: usize, c1: f64| ec3_member(id, p, h, fy, dem, c1, compression);
    let mut checks: Vec<SteelCheck> = vec![];
    let mut worst = |nm: &str, r: f64, detail: String| match checks.iter_mut().find(|c| c.name == nm) {
        Some(c) if c.ratio >= r => {}
        Some(c) => {
            c.ratio = r;
            c.detail = detail;
        }
        None => checks.push(SteelCheck { name: nm.into(), ratio: r, detail }),
    };
    for (combo, st) in &dem.uls {
        let c1 = ec3_c1(dem, st);
        let ltb = ec3_ltb(dem, st);
        // Table B.3 from this combination's moment diagrams (1.0 used to be taken throughout)
        let cm = ec3_cm(dem, st);
        let members: Vec<Ec3MemberData> = st
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let mut m = mk(k + 1, c1);
                m.section_class = ec3_class_nm(h, p, fy, -s.1 * 1e3, s.4 * 1e6).1;
                m.ltb = ltb.clone();
                m.interaction = Some(cm.clone());
                // 6.2.8: with VEd over half of Vpl,Rd the web takes (1 − ρ) fy, so the major-axis
                // moduli lose ρ Aw²/4tw (plastic, 6.2.8(5)) and ρ tw hw²/6 (elastic)
                let rho = shear_rho(&m, s.3.abs().max(s.2.abs()) * 1e3);
                if rho > 0.0 {
                    let (tw, hw) = (h.tw / 1e3, (h.d - 2.0 * h.tf) / 1e3);
                    m.wpl_y = (m.wpl_y - rho * (hw * tw).powi(2) / (4.0 * tw)).max(0.0);
                    m.wel_y = (m.wel_y - rho * tw * hw * hw / 6.0).max(0.0);
                }
                m
            })
            .collect();
        let section_nm: Vec<Option<(f64, String)>> = st.iter().zip(&members).map(|(s, m)| section_n_m(m, h, s.1 * 1e3, s.4 * 1e3, s.5 * 1e3)).collect();
        let forces: Vec<Ec3DesignForces> = st.iter().enumerate().map(|(k, s)| Ec3DesignForces { element_id: k + 1, n_ed: Some(s.1 * 1e3), my_ed: Some(s.4 * 1e3), mz_ed: Some(s.5 * 1e3), v_ed: Some(s.3.abs().max(s.2.abs()) * 1e3) }).collect();
        for ((r, s), nm) in check_ec3_members(&Ec3CheckInput { members, forces }).iter().zip(st).zip(&section_nm) {
            let at = format!("{combo}, x = {:.2} m", s.0);
            if let Some((ratio, how)) = nm {
                worst("cross-section N + M (6.2.9.1)", *ratio, format!("{at}: {how}"));
            }
            worst("axial (6.2.3 / 6.3.1)", r.compression_ratio.max(r.tension_ratio), format!("{at}: NEd {:.0} kN, Nb,Rd {:.0} kN (χy {:.2}, χz {:.2})", s.1, r.nb_rd / 1e3, r.chi_y, r.chi_z));
            // the net section in tension, 6.2.3(2)b: Nu,Rd = 0.9 Anet fu / γM2 (γM2 = 1.25)
            if let (Some(t), true) = (&dem.tension, s.1 > 0.0) {
                let (an, _, how) = t.net(p.a * 1e6, h, true);
                let nu = 0.9 * an * fu / 1.25 / 1e3;
                worst("tension net section (6.2.3(2)b)", s.1 / nu, format!("{at}: NEd {:.0} kN, Nu,Rd = 0.9 Anet fu/γM2 = {nu:.0} kN; {how}", s.1));
            }
            worst("flexure y with LTB (6.3.2)", r.flexure_ratio_y, format!("{at}: My,Ed {:.1} kN·m, Mb,Rd {:.1} kN·m (χLT {:.2}, C1 {c1:.2})", s.4.abs(), r.mb_rd / 1e3, r.chi_lt));
            worst("flexure z (6.2.5)", r.flexure_ratio_z, format!("{at}: Mz,Ed {:.1} kN·m", s.5.abs()));
            worst("shear (6.2.6)", r.shear_ratio, format!("{at}: VEd {:.1} kN", s.3.abs().max(s.2.abs())));
            worst("interaction (6.3.3, Annex B)", r.interaction_ratio, format!("{at}: (6.61) {:.2}, (6.62) {:.2}", r.interaction_ratio_y, r.interaction_ratio_z));
        }
    }
    let l = dem.length * 1000.0;
    let mult = if dem.cantilever { 2.0 } else { 1.0 };
    let (lim_l, lim_t) = (mult * l / set.defl_live, mult * l / set.defl_total);
    worst("deflection (service)", (dem.defl_live * 1000.0 / lim_l).max(dem.defl_total * 1000.0 / lim_t), format!("live {:.1} mm (limit {lim_l:.1}), total {:.1} mm (limit {lim_t:.1})", dem.defl_live * 1000.0, dem.defl_total * 1000.0));
    let (ratio, governs) = checks.iter().map(|c| (c.ratio, c.name.clone())).fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a });
    SteelDesign { member, bim_id, section: name.into(), mass, length: dem.length, ratio, governs, ok: ratio <= set.target_ratio + 1e-9, checks, notes: vec![format!("EN 1993-1-1, Class {cls}, γM0 = γM1 = 1.0, curves y {cy:?} z {cz:?} LT {clt:?}")] }
}

/// The equivalent uniform moment factors Cmy, Cmz and CmLT of EN 1993-1-1 Table B.3 from a
/// combination's moment diagrams over the member (the member is the segment between braced
/// points, as Pondus's buckling lengths assume): the end moments Mh (ψ = the smaller over the
/// larger, with its sign) and the largest span moment Ms. With no span moment, Cm = 0.6 + 0.4ψ
/// ≥ 0.4; with one, the uniform-load column, the larger of the two load columns: 0.2 + 0.8αs
/// (αs = Ms/Mh ≥ 0), 0.1(1 − ψ) − 0.8αs or 0.1 − 0.8αs (αs < 0), 0.95 + 0.05αh (αh = Mh/Ms,
/// with αh(1 + 2ψ) when ψ < 0), each ≥ 0.4. A sway member (k > 1) takes 0.9 for y and z.
pub fn ec3_cm(dem: &SteelDemand, st: &[(f64, f64, f64, f64, f64, f64)]) -> Ec3InteractionData {
    let cm_of = |vals: Vec<(f64, f64)>| -> f64 {
        if vals.len() < 2 || dem.cantilever {
            return 1.0;
        }
        let (ma, mb) = (vals[0].1, vals[vals.len() - 1].1);
        let (mh, other) = if ma.abs() >= mb.abs() { (ma, mb) } else { (mb, ma) };
        let l = vals[vals.len() - 1].0 - vals[0].0;
        // the span moment: the largest departure from the straight line between the ends
        let (mut ms, mut dev) = (0.0f64, 0.0f64);
        for &(x, m) in &vals {
            let lin = ma + (mb - ma) * if l > 0.0 { (x - vals[0].0) / l } else { 0.0 };
            if (m - lin).abs() > dev.abs() {
                dev = m - lin;
                ms = m;
            }
        }
        let big = mh.abs().max(ms.abs());
        if big < 1e-9 {
            return 1.0;
        }
        if dev.abs() <= 1e-3 * big {
            // linear diagram
            let psi = if mh.abs() > 1e-12 { other / mh } else { 1.0 };
            return (0.6 + 0.4 * psi).clamp(0.4, 1.0);
        }
        let psi = if mh.abs() > 1e-12 { (other / mh).clamp(-1.0, 1.0) } else { 0.0 };
        let c = if mh.abs() >= ms.abs() {
            let a_s = ms / mh;
            if a_s >= 0.0 {
                0.2 + 0.8 * a_s
            } else if psi >= 0.0 {
                0.1 - 0.8 * a_s
            } else {
                0.1 * (1.0 - psi) - 0.8 * a_s
            }
        } else {
            let a_h = mh / ms;
            if a_h >= 0.0 || psi >= 0.0 {
                0.95 + 0.05 * a_h
            } else {
                0.95 + 0.05 * a_h * (1.0 + 2.0 * psi)
            }
        };
        c.clamp(0.4, 1.0)
    };
    let my: Vec<(f64, f64)> = st.iter().map(|s| (s.0, s.4)).collect();
    let mz: Vec<(f64, f64)> = st.iter().map(|s| (s.0, s.5)).collect();
    let sway = dem.k > 1.0 + 1e-9;
    let (cmy, cmz) = if sway { (0.9, 0.9) } else { (cm_of(my.clone()), cm_of(mz)) };
    Ec3InteractionData { cmy: Some(cmy), cmz: Some(cmz), cm_lt: Some(cm_of(my)), hollow: None }
}

/// ρ of 6.2.8(3) for a shear VEd (N) on a member: 0 up to half of Vpl,Rd = Av fy/√3 (γM0 = 1),
/// then (2 VEd/Vpl,Rd − 1)².
fn shear_rho(m: &Ec3MemberData, v_ed: f64) -> f64 {
    let vpl = m.av.unwrap_or(0.0) * m.fy / 3f64.sqrt();
    if vpl <= 0.0 || v_ed <= 0.5 * vpl {
        return 0.0;
    }
    (2.0 * v_ed / vpl - 1.0).min(1.0).powi(2)
}

/// The plastic cross-section check under axial force and bending of a Class 1 or 2 I section
/// (6.2.9.1(4) – (6), γM0 = 1) with the moduli as the member carries them (reduced for shear
/// when 6.2.8 applies): MN,y,Rd = Mpl,y,Rd (1 − n)/(1 − 0.5a) ≤ Mpl,y,Rd, MN,z,Rd = Mpl,z,Rd for
/// n ≤ a and Mpl,z,Rd [1 − ((n − a)/(1 − a))²] above, a = (A − 2 b tf)/A ≤ 0.5, no reduction
/// while NEd stays within 0.25 Npl,Rd and 0.5 hw tw fy (y) or hw tw fy (z), and the biaxial
/// criterion (6.41) with α = 2, β = 5n ≥ 1. Returns the ratio and how, or None for Class 3 / 4
/// sections and members without axial force (the engine checks those).
fn section_n_m(m: &Ec3MemberData, h: HDims, n_ed: f64, my_ed: f64, mz_ed: f64) -> Option<(f64, String)> {
    if n_ed == 0.0 || !matches!(m.section_class, SectionClass::Class1 | SectionClass::Class2) {
        return None;
    }
    let (b, tf, tw) = (h.bf / 1e3, h.tf / 1e3, h.tw / 1e3);
    let hw = h.d / 1e3 - 2.0 * tf;
    let npl = m.a * m.fy;
    let n = (n_ed.abs() / npl).min(1.0);
    let a = ((m.a - 2.0 * b * tf) / m.a).clamp(0.0, 0.5);
    let (mply, mplz) = (m.wpl_y * m.fy, m.wpl_z * m.fy);
    let mny = if n_ed.abs() <= 0.25 * npl && n_ed.abs() <= 0.5 * hw * tw * m.fy { mply } else { (mply * (1.0 - n) / (1.0 - 0.5 * a)).min(mply) };
    let mnz = if n_ed.abs() <= hw * tw * m.fy || n <= a { mplz } else { mplz * (1.0 - ((n - a) / (1.0 - a)).powi(2)) };
    let (ry, rz) = (my_ed.abs() / mny.max(1e-9), mz_ed.abs() / mnz.max(1e-9));
    let ratio = if rz < 1e-9 {
        ry
    } else if ry < 1e-9 {
        rz
    } else {
        ry.powi(2) + rz.powf((5.0 * n).max(1.0))
    };
    Some((ratio, format!("NEd {:.0} kN (n {n:.3}), MN,y,Rd {:.1} kN·m, MN,z,Rd {:.1} kN·m", n_ed / 1e3, mny / 1e3, mnz / 1e3)))
}

/// The EC2 flexure and shear ratios of a beam ACI 318 designed (fck from f'c, fyk = fy, the
/// beam's bars per zone and its stirrups), under the ULS stations it was designed for.
#[derive(Clone, Debug, Serialize)]
pub struct Ec2Check {
    pub flexure: f64,
    pub shear: f64,
    pub detail: String,
}

pub fn check_ec2_beam(b: &BeamDesign, dem: &crate::rc::BeamDemand, fc: f64, fy: f64, fyt: f64, cover: f64) -> Ec2Check {
    let (bw, h) = (b.b, b.h);
    let mut flex: f64 = 0.0;
    let mut shear: f64 = 0.0;
    let mut detail = String::new();
    let l = b.span;
    let zone = |x: f64| if x < l / 3.0 { "left" } else if x <= 2.0 * l / 3.0 { "middle" } else { "right" };
    let stir = b.shear.iter().find(|s| s.s > 0.0);
    for st in &dem.uls {
        let members: Vec<Ec2MemberData> = st
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let face = if s.1 >= 0.0 { "bottom" } else { "top" };
                let as_t = b.flexure.iter().find(|f| f.face == face && f.zone == zone(s.0)).map(|f| f.as_prov).unwrap_or(0.0) / 1e6;
                let d = h - cover - 0.009 - 0.010;
                // the engine takes one steel grade: the stirrup area is scaled by fyt/fy so that
                // Asw·fyk carries the stirrups' own yield force in (6.8)
                let sh = stir.map(|s| (2.0 * bar_area(9.0) * fyt / fy, s.s / 1000.0));
                Ec2MemberData { element_id: k + 1, fck: fc * 1e6, fyk: fy * 1e6, b: bw, h, d, as_tension: as_t, as_compression: None, d_prime: None, es: None, gamma_c: None, gamma_s: None, alpha_cc: Some(1.0), asw: sh.map(|x| x.0), s_stirrup: sh.map(|x| x.1), theta_shear: None, bw: None, z: None }
            })
            .collect();
        let forces: Vec<Ec2DesignForces> = st.iter().enumerate().map(|(k, s)| Ec2DesignForces { element_id: k + 1, m_ed: Some(s.1.abs() * 1e3), v_ed: Some(s.2.abs() * 1e3), n_ed: Some(s.4 * 1e3) }).collect();
        for (r, s) in check_ec2_members(&Ec2CheckInput { members, forces }).iter().zip(st) {
            if r.flexure_ratio > flex {
                flex = r.flexure_ratio;
                detail = format!("x = {:.2} m: MEd {:.1} kN·m, MRd {:.1} kN·m", s.0, s.1.abs(), r.m_rd / 1e3);
            }
            shear = shear.max(r.shear_ratio);
        }
    }
    Ec2Check { flexure: flex, shear, detail }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_of_rolled_shapes() {
        // H 300x150x6.5x9 in S275 (fy 275): ε = 0.924; flange c/t = (75 − 3.25 − 13)/9 = 6.53
        // ≤ 9ε = 8.32: class 1; web in bending c/t = (300 − 18 − 26)/6.5 = 39.4 ≤ 72ε: class 1
        let h = HDims { d: 300.0, bf: 150.0, tw: 6.5, tf: 9.0, r: 13.0 };
        assert_eq!(ec3_class(h, 275.0, false).0, 1);
        // in compression the web limit 33ε = 30.5 < 39.4 ≤ 38ε = 35.1? no: 39.4 > 42ε = 38.8 → class 4
        assert_eq!(ec3_class(h, 275.0, true).0, 4);
    }

    #[test]
    fn thai_regulation_2566() {
        use crate::model::Model;
        // clause 7 combinations from the default cases D and L, and a wind case
        let mut m = Model::new("t");
        m.cases.push(crate::model::LoadCase { id: 3, name: "Wx".into(), kind: crate::model::CaseKind::W, self_weight: false });
        let c = DesignStandard::Mr2566.combinations(&m.cases);
        let names: Vec<_> = c.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["ULS 1.4D + 1.7L", "ULS 0.75(1.4D + 1.7L) + 1.6Wx", "ULS 0.9D + 1.6Wx", "SLS D + L"]);
        assert_eq!(c[1].factors, vec![(1, 1.05), (2, 1.275), (3, 1.6)]);
        assert_eq!(DesignStandard::Mr2566.gravity(5.0, 2.0), 1.4 * 5.0 + 1.7 * 2.0);
        // clause 8 φ: tied columns 0.70 (0.60 without quality control) against ACI's 0.65
        let (_, aci) = crate::rc::column_p0(400.0 * 400.0, 2000.0, 24.0, 390.0);
        let thai = {
            let _g = use_standard(DesignStandard::Mr2566);
            assert_eq!(crate::rc::phi_tied(0.0, 390.0), 0.70);
            assert_eq!(crate::rc::phi_tied(0.01, 390.0), 0.90);
            crate::rc::column_p0(400.0 * 400.0, 2000.0, 24.0, 390.0).1
        };
        assert!((thai / aci - 0.70 / 0.65).abs() < 1e-12);
        {
            let _g = use_standard(DesignStandard::Mr2566NoQc);
            assert_eq!(crate::rc::phi_tied(0.01, 390.0), 0.75);
            let ok = crate::rc::phi_mn(1000.0, 300.0, 500.0, 24.0, 390.0);
            drop(_g);
            assert!((ok / crate::rc::phi_mn(1000.0, 300.0, 500.0, 24.0, 390.0) - 0.75 / 0.90).abs() < 1e-12);
        }
        // the guard restores ACI
        assert_eq!(phi().tied, 0.65);
        // serde names
        assert_eq!(serde_json::to_value(DesignStandard::Mr2566NoQc).unwrap(), "mr2566_noqc");
    }
}
