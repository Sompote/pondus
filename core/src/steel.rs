// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Steel member design to AISC 360-16 (LRFD) for doubly symmetric rolled H shapes, in N and
//! mm inside: tension (D2, gross yielding, and rupture on Ae = U·An of the end connection in `check`), compression (E3 flexural
//! buckling about both axes, k·L about the major and k·Lb about the minor axis, E7 effective area for slender elements), major-axis flexure
//! (F2 / F3 with lateral-torsional buckling and Cb from the moment diagram), minor-axis
//! flexure (F6), shear (G2.1) and the interaction of H1-1 with the first-order moments of
//! members in compression amplified by B1 (Appendix 8, Cm = 1). Sway (B2) effects are not
//! included: the analysis is first order. A beam's load is taken on its top flange, which
//! lowers Cb by 1.4 (Commentary F1). Deflections under service loads against span/360 (live)
//! and span/240 (total).

use serde::Serialize;

use crate::model::Props;

pub const E: f64 = 200_000.0;

#[derive(Clone, Debug, Default)]
pub struct SteelDemand {
    pub length: f64,
    /// Unbraced length for LTB and effective length factor (m, -).
    pub lb: f64,
    pub k: f64,
    /// Per ULS combination: (name, stations (x, N tension +, Vy, Vz, My major, Mz minor)).
    pub uls: Vec<(String, Vec<(f64, f64, f64, f64, f64, f64)>)>,
    /// Member's own deflection under live load and under the service total, m.
    pub defl_live: f64,
    pub defl_total: f64,
    pub cantilever: bool,
    /// Gravity load acts on the top flange (a beam): Cb is divided by 1.4, the load-height
    /// factor 1.4^(2y/h) with y = −h/2 of the AISC Commentary to F1.
    pub top_flange_load: bool,
    /// The end connection for tensile rupture (None: An = Ae = Ag, yielding only).
    pub tension: Option<crate::model::TensionConnection>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SteelCheck {
    pub name: String,
    /// demand / capacity
    pub ratio: f64,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SteelDesign {
    pub member: crate::model::Id,
    pub bim_id: Option<String>,
    pub section: String,
    pub mass: f64,
    pub length: f64,
    pub checks: Vec<SteelCheck>,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

/// Plate dimensions in mm.
#[derive(Clone, Copy, Debug)]
pub struct HDims {
    pub d: f64,
    pub bf: f64,
    pub tw: f64,
    pub tf: f64,
    pub r: f64,
}

pub struct Capacities {
    pub phi_pt: f64,
    pub phi_pc: f64,
    pub phi_vy: f64,
    pub phi_vz: f64,
    pub phi_mz: f64,
    /// Major-axis φMn for Cb = 1, and the plastic limit φMp (Cb raises the first up to it).
    pub phi_mx1: f64,
    pub phi_mpx: f64,
    pub pe_y: f64,
    pub pe_z: f64,
    pub notes: Vec<String>,
}

/// The capacities of an H shape (props in m, dims in mm) of steel fy, fu (MPa).
pub fn capacities(p: &Props, h: HDims, fy: f64, fu: f64, length: f64, lb: f64, k: f64) -> Capacities {
    let mut notes = vec![];
    let a = p.a * 1e6;
    let (ix, iy) = (p.iy * 1e12, p.iz * 1e12);
    let (sx, zx, sy, zy) = (p.sy * 1e9, p.zy * 1e9, p.sz * 1e9, p.zz * 1e9);
    let (rx, ry) = (p.ry * 1e3, p.rz * 1e3);
    let j = p.j * 1e12;
    let cw = p.cw * 1e18;
    let ef = (E / fy).sqrt();
    // element slenderness (Table B4.1)
    let lf = h.bf / (2.0 * h.tf);
    let hw = h.d - 2.0 * h.tf - 2.0 * h.r;
    let lw = hw / h.tw;
    // tension D2
    let phi_pt = (0.9 * fy * a).min(0.75 * fu * a);
    // compression E3 / E7: Lc = k·L about the major axis and k·Lb about the minor axis (the
    // bracing that holds the compression flange holds the weak axis too)
    let lc = k * length * 1000.0;
    let lcy = k * lb.min(length) * 1000.0;
    let fe_of = |lc: f64, r: f64| std::f64::consts::PI.powi(2) * E / (lc / r).powi(2);
    let fe = fe_of(lc, rx).min(fe_of(lcy, ry));
    let fcr = if fy / fe <= 2.25 { 0.658f64.powf(fy / fe) * fy } else { 0.877 * fe };
    let mut ae = a;
    let (lr_f, lr_w) = (0.56 * ef, 1.49 * ef);
    if lf > lr_f * (fy / fcr).sqrt() {
        let fel = (1.49 * lr_f / lf).powi(2) * fy;
        let be = h.bf / 2.0 * (1.0 - 0.22 * (fel / fcr).sqrt()) * (fel / fcr).sqrt();
        ae -= 4.0 * (h.bf / 2.0 - be).max(0.0) * h.tf;
        notes.push(format!("slender flanges in compression (bf/2tf = {lf:.1}): E7 effective area"));
    }
    if lw > lr_w * (fy / fcr).sqrt() {
        let fel = (1.31 * lr_w / lw).powi(2) * fy;
        let he = hw * (1.0 - 0.18 * (fel / fcr).sqrt()) * (fel / fcr).sqrt();
        ae -= (hw - he).max(0.0) * h.tw;
        notes.push(format!("slender web in compression (h/tw = {lw:.1}): E7 effective area"));
    }
    let phi_pc = 0.9 * fcr * ae;
    let slender = (lc / rx).max(lcy / ry);
    if slender > 200.0 {
        notes.push(format!("KL/r = {slender:.0} is over 200 (E2 note)"));
    }
    // major-axis flexure F2 / F3
    let (lpf, lrf) = (0.38 * ef, 1.0 * ef);
    let mp = fy * zx;
    let lbm = lb * 1000.0;
    let lp = 1.76 * ry * ef;
    let h0 = h.d - h.tf;
    let rts = ((iy * cw).sqrt() / sx).sqrt();
    let jc = j / (sx * h0);
    let lr = 1.95 * rts * E / (0.7 * fy) * (jc + (jc * jc + 6.76 * (0.7 * fy / E).powi(2)).sqrt()).sqrt();
    let ltb = if lbm <= lp {
        mp
    } else if lbm <= lr {
        (mp - (mp - 0.7 * fy * sx) * (lbm - lp) / (lr - lp)).min(mp)
    } else {
        let fcr = std::f64::consts::PI.powi(2) * E / (lbm / rts).powi(2) * (1.0 + 0.078 * jc * (lbm / rts).powi(2)).sqrt();
        (fcr * sx).min(mp)
    };
    let flb = if lf <= lpf {
        mp
    } else if lf <= lrf {
        mp - (mp - 0.7 * fy * sx) * (lf - lpf) / (lrf - lpf)
    } else {
        let kc = (4.0 / lw.sqrt()).clamp(0.35, 0.76);
        notes.push(format!("slender flanges in flexure (bf/2tf = {lf:.1})"));
        0.9 * E * kc * sx / (lf * lf)
    };
    if lw > 3.76 * ef {
        notes.push(format!("noncompact web in flexure (h/tw = {lw:.1}): F4 is not applied, the capacity is overestimated"));
    }
    let phi_mx1 = 0.9 * ltb.min(flb);
    let phi_mpx = 0.9 * mp.min(flb);
    // minor-axis flexure F6
    let mpy = (fy * zy).min(1.6 * fy * sy);
    let mny = if lf <= lpf { mpy } else if lf <= lrf { mpy - (mpy - 0.7 * fy * sy) * (lf - lpf) / (lrf - lpf) } else { 0.69 * E / (lf * lf) * sy };
    let phi_mz = 0.9 * mny;
    // shear G2.1: web for major-axis shear (Vz), flanges for minor (Vy, G6)
    let aw = h.d * h.tw;
    let (phi_v, cv1) = if lw <= 2.24 * ef {
        (crate::codes::phi().steel_shear.unwrap_or(1.0), 1.0)
    } else {
        let kv = 5.34;
        (0.9, if lw <= 1.10 * (kv * E / fy).sqrt() { 1.0 } else { 1.10 * (kv * E / fy).sqrt() / lw })
    };
    let phi_vz = phi_v * 0.6 * fy * aw * cv1;
    let phi_vy = 0.9 * 0.6 * fy * 2.0 * h.bf * h.tf;
    let pe = |i: f64, lc: f64| std::f64::consts::PI.powi(2) * E * i / lc.powi(2);
    Capacities { phi_pt, phi_pc, phi_vy, phi_vz, phi_mz, phi_mx1, phi_mpx, pe_y: pe(ix, lc), pe_z: pe(iy, lcy), notes }
}

/// Cb (F1-1) from the moments along the unbraced segment (the whole member here).
pub fn cb(stations: &[(f64, f64)], length: f64) -> f64 {
    if stations.len() < 3 || length <= 0.0 {
        return 1.0;
    }
    let at = |x: f64| -> f64 {
        let k = stations.iter().position(|s| s.0 >= x - 1e-9).unwrap_or(stations.len() - 1);
        if k == 0 {
            return stations[0].1.abs();
        }
        let (a, b) = (stations[k - 1], stations[k]);
        let t = if b.0 > a.0 { (x - a.0) / (b.0 - a.0) } else { 0.0 };
        (a.1 + (b.1 - a.1) * t).abs()
    };
    let mmax = stations.iter().map(|s| s.1.abs()).fold(0.0, f64::max);
    if mmax <= 0.0 {
        return 1.0;
    }
    let (ma, mb, mc) = (at(length / 4.0), at(length / 2.0), at(3.0 * length / 4.0));
    (12.5 * mmax / (2.5 * mmax + 3.0 * ma + 4.0 * mb + 3.0 * mc)).min(3.0)
}

/// Checks an H shape for a member's demand. `name`/`mass` describe the shape.
#[allow(clippy::too_many_arguments)]
pub fn check(member: crate::model::Id, bim_id: Option<String>, name: &str, mass: f64, p: &Props, h: HDims, fy: f64, fu: f64, dem: &SteelDemand, set: &crate::model::DesignSettings) -> SteelDesign {
    let mut c = capacities(p, h, fy, fu, dem.length, dem.lb, dem.k);
    let _ = set;
    // tensile rupture on the effective net area Ae = U·An of the end connection (D2-2, D3)
    let ag = p.a * 1e6;
    let tension_note = dem.tension.as_ref().map(|t| {
        let (_, ae, how) = t.net(ag, h, false);
        c.phi_pt = (0.9 * fy * ag).min(0.75 * fu * ae);
        format!("tension: φtPn = min(0.9 Fy Ag, 0.75 Fu Ae) with Ae = {ae:.0} mm²; {how}")
    });
    let mut checks: Vec<SteelCheck> = vec![];
    let worst = |nm: &str, r: f64, detail: String, checks: &mut Vec<SteelCheck>| match checks.iter_mut().find(|c| c.name == nm) {
        Some(c) if c.ratio >= r => {}
        Some(c) => {
            c.ratio = r;
            c.detail = detail;
        }
        None => checks.push(SteelCheck { name: nm.into(), ratio: r, detail }),
    };
    for (combo, st) in &dem.uls {
        let cb0 = if dem.cantilever { 1.0 } else { cb(&st.iter().map(|s| (s.0, s.4)).collect::<Vec<_>>(), dem.length) };
        let cbv = if dem.top_flange_load { cb0 / 1.4 } else { cb0 };
        let phi_mx = (cbv * c.phi_mx1).min(c.phi_mpx);
        for s in st {
            let (x, n, vy, vz, my, mz) = (s.0, s.1 * 1e3, s.2 * 1e3, s.3 * 1e3, s.4 * 1e6, s.5 * 1e6);
            let (pr, pc, what) = if n >= 0.0 { (n, c.phi_pt, "tension") } else { (-n, c.phi_pc, "compression") };
            // B1 (A-8-3) for members in compression, Cm = 1
            let (b1x, b1y) = if n < 0.0 { ((1.0 / (1.0 - pr / c.pe_y)).max(1.0), (1.0 / (1.0 - pr / c.pe_z)).max(1.0)) } else { (1.0, 1.0) };
            if n < 0.0 && (pr >= c.pe_y || pr >= c.pe_z) {
                worst("interaction H1-1", f64::INFINITY, format!("{combo}: axial force over the Euler load"), &mut checks);
                continue;
            }
            let (mrx, mry) = (my.abs() * b1x, mz.abs() * b1y);
            worst(&format!("axial ({what})"), pr / pc, format!("{combo}, x = {x:.2} m: Pu {:.0} kN, φPn {:.0} kN", pr / 1e3, pc / 1e3), &mut checks);
            worst("flexure major (F2/F3)", my.abs() / phi_mx, format!("{combo}, x = {x:.2} m: Mu {:.1} kN·m, φMn {:.1} kN·m (Lb {:.2} m, Cb {cbv:.2})", my.abs() / 1e6, phi_mx / 1e6, dem.lb), &mut checks);
            if mz.abs() > 1e-3 {
                worst("flexure minor (F6)", mz.abs() / c.phi_mz, format!("{combo}, x = {x:.2} m: Mu {:.1} kN·m, φMn {:.1} kN·m", mz.abs() / 1e6, c.phi_mz / 1e6), &mut checks);
            }
            worst("shear (G2)", (vz.abs() / c.phi_vz).max(vy.abs() / c.phi_vy), format!("{combo}, x = {x:.2} m: Vu {:.1} kN, φVn {:.0} kN", vz.abs().max(vy.abs()) / 1e3, c.phi_vz / 1e3), &mut checks);
            let pr_pc = pr / pc;
            let mm = mrx / phi_mx + mry / c.phi_mz;
            let r = if pr_pc >= 0.2 { pr_pc + 8.0 / 9.0 * mm } else { pr_pc / 2.0 + mm };
            worst("interaction H1-1", r, format!("{combo}, x = {x:.2} m: Pr/Pc {pr_pc:.2}, Mrx/Mcx {:.2}{}", mrx / phi_mx, if b1x > 1.001 { format!(" (B1 {b1x:.3})") } else { String::new() }), &mut checks);
        }
    }
    let l = dem.length * 1000.0;
    let mult = if dem.cantilever { 2.0 } else { 1.0 };
    let (lim_l, lim_t) = (mult * l / set.defl_live, mult * l / set.defl_total);
    worst("deflection (service)", (dem.defl_live * 1000.0 / lim_l).max(dem.defl_total * 1000.0 / lim_t), format!("live {:.1} mm (limit {lim_l:.1}), total {:.1} mm (limit {lim_t:.1})", dem.defl_live * 1000.0, dem.defl_total * 1000.0), &mut checks);
    let (ratio, governs) = checks.iter().map(|c| (c.ratio, c.name.clone())).fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a });
    let mut notes = c.notes;
    if dem.uls.iter().flat_map(|(_, st)| st.iter()).any(|s| s.1 > 1e-6) {
        notes.extend(tension_note);
    }
    SteelDesign { member, bim_id, section: name.into(), mass, length: dem.length, ratio, governs, ok: ratio <= set.target_ratio + 1e-9, checks, notes }
}
