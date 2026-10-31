// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Reinforced concrete design to ACI 318-19 (strength design), in N and mm inside.
//!
//! Beams: flexure in each third of the span from the moment envelope (top bars from hogging,
//! bottom bars from sagging; compression steel when the section cannot stay
//! tension-controlled), shear stirrups in the end quarters and the middle half, the minimum
//! depth of Table 9.3.1.1 and the deflection with the effective inertia of Table 24.2.3.5
//! and the long-term multiplier of 24.2.4.1. Torsion over the threshold of 22.7.4.1 is
//! designed (22.7.6, θ = 45°): closed stirrups At/s added to the shear stirrups, longitudinal
//! steel Al (a quarter added to the top and to the bottom bars, half as side bars), the
//! cross-section limit of 22.7.7.1 and the minimums of 9.6.4.
//!
//! Slabs: two-way panels on beams by the moment coefficients of ACI 318-63 Method 2 (the
//! method of EIT 1008 in Thailand), one-way panels by the coefficients of ACI 318-19 6.5;
//! minimum steel 0.0018 bh (24.4.3.2), spacing limits 8.7.2.2 / 7.7.2.3, minimum thickness
//! of Table 8.3.1.2 (beams stiff, αfm > 2) or Table 7.3.1.1, and one-way shear.

use serde::Serialize;

use crate::sections::{bar_area, bar_name};

pub const ES: f64 = 200_000.0;

pub fn beta1(fc: f64) -> f64 {
    if fc <= 28.0 {
        0.85
    } else {
        (0.85 - 0.05 * (fc - 28.0) / 7.0).max(0.65)
    }
}

/// √f'c, capped at √69 MPa (22.5.3.1).
fn sqrt_fc(fc: f64) -> f64 {
    fc.min(69.0).sqrt()
}

/// Tension steel (and compression steel) for a moment Mu (N·mm) on a rectangle b x d (mm),
/// with compression steel at d' when needed. φ is the tension-controlled one (0.9 by ACI
/// 318-19): the design keeps εt ≥ 0.005.
pub fn flexure_steel(mu: f64, b: f64, d: f64, d2: f64, fc: f64, fy: f64) -> (f64, f64) {
    if mu <= 0.0 {
        return (0.0, 0.0);
    }
    let phi = crate::codes::phi().flexure;
    let b1 = beta1(fc);
    // the largest tension-controlled singly reinforced capacity: c = 3d/8 (εt = 0.005)
    let c_max = 0.375 * d;
    let a_max = b1 * c_max;
    let mn_max = 0.85 * fc * b * a_max * (d - a_max / 2.0);
    if mu <= phi * mn_max {
        let rn = mu / (phi * b * d * d);
        let rho = 0.85 * fc / fy * (1.0 - (1.0 - 2.0 * rn / (0.85 * fc)).max(0.0).sqrt());
        return (rho * b * d, 0.0);
    }
    let as1 = 0.85 * fc * b * a_max / fy;
    let m2 = mu / phi - mn_max;
    let fs2 = (ES * 0.003 * (c_max - d2) / c_max).clamp(0.0, fy);
    if fs2 <= 0.0 || d - d2 <= 0.0 {
        return (f64::INFINITY, f64::INFINITY);
    }
    // the compression bars displace concrete: their net stress is fs' − 0.85 f'c
    let as2 = m2 / ((fs2 - 0.85 * fc).max(1.0) * (d - d2));
    (as1 + as2 * (fs2 - 0.85 * fc).max(1.0) / fy, as2)
}

/// φMn (N·mm) of a rectangle with tension steel As only.
pub fn phi_mn(as_: f64, b: f64, d: f64, fc: f64, fy: f64) -> f64 {
    let a = as_ * fy / (0.85 * fc * b);
    let c = a / beta1(fc);
    let et = 0.003 * (d - c) / c.max(1e-9);
    let f = crate::codes::phi();
    let phi = if et >= 0.005 { f.flexure } else if et <= 0.002 { f.tied } else { f.tied + (f.flexure - f.tied) * (et - 0.002) / 0.003 };
    phi * as_ * fy * (d - a / 2.0)
}

/// Minimum flexural steel of a beam (9.6.1.2), mm².
pub fn as_min_beam(b: f64, d: f64, fc: f64, fy: f64) -> f64 {
    (0.25 * fc.sqrt() / fy).max(1.4 / fy) * b * d
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Bars {
    pub n: usize,
    /// mm
    pub db: f64,
    pub layers: usize,
}

impl Bars {
    pub fn area(&self) -> f64 {
        self.n as f64 * bar_area(self.db) * 1e6
    }
    pub fn text(&self) -> String {
        let s = format!("{}-{}", self.n, bar_name(self.db));
        if self.layers > 1 {
            format!("{s} (2 layers)")
        } else {
            s
        }
    }
}

/// Clear spacing between parallel bars: the largest of 25 mm, db and 4/3 of the aggregate
/// (25.2.1).
fn s_clear(db: f64, agg: f64) -> f64 {
    25f64.max(db).max(4.0 / 3.0 * agg)
}

/// Bars for As (mm²) across a width of b with the given cover and stirrup: the arrangement
/// with the least area that fits in one layer, else in two; at least two bars.
pub fn choose_bars(as_req: f64, b: f64, cover: f64, ds: f64, bars: &[f64], agg: f64) -> Option<Bars> {
    let mut best: Option<Bars> = None;
    for &db in bars {
        let ab = bar_area(db) * 1e6;
        let n = ((as_req / ab).ceil() as usize).max(2);
        let sc = s_clear(db, agg);
        let per_layer = (((b - 2.0 * cover - 2.0 * ds + sc) / (db + sc)).floor() as usize).max(0);
        if per_layer < 2 {
            continue;
        }
        let layers = n.div_ceil(per_layer);
        if layers > 2 {
            continue;
        }
        let cand = Bars { n, db, layers };
        let better = match &best {
            None => true,
            Some(b0) => (cand.layers, (cand.area() / 10.0).round() as i64, cand.n) < (b0.layers, (b0.area() / 10.0).round() as i64, b0.n),
        };
        if better {
            best = Some(cand);
        }
    }
    best
}

/// Depth to the centroid of the bars from the compression face.
pub fn eff_depth(h: f64, cover: f64, ds: f64, bars: &Bars) -> f64 {
    if bars.layers > 1 {
        h - cover - ds - bars.db - s_clear(bars.db, 0.0) / 2.0
    } else {
        h - cover - ds - bars.db / 2.0
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FlexZone {
    /// "top" or "bottom"; the zone "left", "middle" or "right" third.
    pub face: String,
    pub zone: String,
    /// Design moment, kN·m (positive), required and provided steel, mm².
    pub mu: f64,
    pub as_req: f64,
    pub as_prov: f64,
    pub bars: String,
    /// Mu / φMn.
    pub ratio: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShearZone {
    pub zone: String,
    /// kN
    pub vu: f64,
    pub phi_vc: f64,
    pub stirrups: String,
    /// spacing, mm
    pub s: f64,
    pub ratio: f64,
}

/// Torsion design of a beam (22.7), N and mm inside, reported in kN·m, mm²/mm, mm².
#[derive(Clone, Debug, Serialize)]
pub struct TorsionDesign {
    pub tu: f64,
    pub phi_tth: f64,
    pub phi_tcr: f64,
    /// Closed-stirrup steel per leg per spacing, mm²/mm, and the longitudinal steel, mm².
    pub at_s: f64,
    pub al: f64,
    pub side_bars: String,
    /// The 22.7.7.1 cross-section ratio (≤ 1).
    pub section_ratio: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BeamDesign {
    pub member: crate::model::Id,
    pub bim_id: Option<String>,
    pub b: f64,
    pub h: f64,
    pub span: f64,
    pub flexure: Vec<FlexZone>,
    pub shear: Vec<ShearZone>,
    pub torsion: Option<TorsionDesign>,
    /// The EN 1992-1-1 check of these bars, when asked for.
    pub ec2: Option<crate::codes::Ec2Check>,
    /// Table 9.3.1.1 minimum depth, mm, and the end condition it is for.
    pub h_min: f64,
    pub support: String,
    /// mm: immediate live, long-term after attachment; their limits.
    pub defl_live: f64,
    pub defl_total: f64,
    pub limit_live: f64,
    pub limit_total: f64,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

/// What a beam carries, from the analysis (kN, kN·m, m; sagging moment positive).
#[derive(Clone, Debug, Default)]
pub struct BeamDemand {
    pub span: f64,
    /// (x, My, Vz, T, N) at every station of every ULS combination.
    pub uls: Vec<Vec<(f64, f64, f64, f64, f64)>>,
    /// Largest sagging moments (kN·m) and own deflections (m, down +) of the member under
    /// the dead load and under dead + live, service level.
    pub ma_d: f64,
    pub ma_dl: f64,
    pub defl_d: f64,
    pub defl_dl: f64,
    /// End conditions: continuous at i, at j; cantilever.
    pub cont_i: bool,
    pub cont_j: bool,
    pub cantilever: bool,
}

pub struct RcMat {
    pub fc: f64,
    pub ec: f64,
    pub fy: f64,
    pub fyt: f64,
}

/// Designs a rectangular beam b x h (m). `set` gives the cover, bars and limits.
pub fn design_beam(member: crate::model::Id, bim_id: Option<String>, b_m: f64, h_m: f64, dem: &BeamDemand, mat: &RcMat, set: &crate::model::DesignSettings) -> BeamDesign {
    let (b, h) = (b_m * 1000.0, h_m * 1000.0);
    let l = dem.span;
    let cover = set.cover_beam * 1000.0;
    let ds = set.stirrup_bars.first().copied().unwrap_or(9.0);
    let (fc, fy, fyt) = (mat.fc, mat.fy, mat.fyt);
    let mut notes = vec![];
    let mut ok = true;
    // envelopes per third
    let zones = [("left", 0.0, l / 3.0), ("middle", l / 3.0, 2.0 * l / 3.0), ("right", 2.0 * l / 3.0, l)];
    let mut flex = vec![];
    let mut top_bars: Vec<Option<Bars>> = vec![];
    let mut comp_need = [0.0f64; 3];
    let mut ratio: f64 = 0.0;
    let mut governs = String::new();
    let all = || dem.uls.iter().flatten();
    // torsion (22.7): over the threshold it needs closed stirrups and longitudinal steel
    let sq = sqrt_fc(fc);
    let phi_t = crate::codes::phi().shear;
    let tu_all = all().map(|s| s.3.abs()).fold(0.0, f64::max);
    let tt = torsion_terms(b, h, cover, ds, fc, fy, fyt, tu_all);
    let (phi_tth, aoh, ph, ao) = (tt.phi_tth, tt.aoh, tt.ph, tt.ao);
    let torsion = tu_all > phi_tth;
    // At/s for a torque (kN·m), mm²/mm per leg
    let at_s_for = |tu: f64| if torsion { tu * 1e6 / (2.0 * phi_t * ao * fyt) } else { 0.0 };
    let at_s_max = at_s_for(tu_all);
    let al = if torsion { tt.al.max(tt.al_min) } else { 0.0 };
    for (face, sign) in [("bottom", 1.0), ("top", -1.0)] {
        for (zi, (zn, x0, x1)) in zones.iter().enumerate() {
            let mu = all().filter(|s| s.0 >= x0 - 1e-6 && s.0 <= x1 + 1e-6).map(|s| sign * s.1).fold(0.0, f64::max);
            let mu_n = mu * 1e6;
            // the bottom bars at midspan carry at least the minimum steel of 9.6.1.2 even where
            // the analysis gives no sagging there (a tie or strap beam), as they run through it
            let mid_bottom = face == "bottom" && *zn == "middle";
            // first trial with a 20 mm bar in one layer, then with the bars chosen
            let mut d = h - cover - ds - 10.0;
            let mut chosen: Option<Bars> = None;
            let mut req = (0.0, 0.0);
            for _ in 0..3 {
                req = flexure_steel(mu_n, b, d, cover + ds + 10.0, fc, fy);
                let as_t = req.0.max(if mu > 0.0 || mid_bottom { as_min_beam(b, d, fc, fy) } else { 0.0 }) + al / 4.0;
                chosen = choose_bars(as_t.max(1.0), b, cover, ds, &set.beam_bars, set.aggregate);
                match &chosen {
                    Some(bars) => {
                        let d2 = eff_depth(h, cover, ds, bars);
                        if (d2 - d).abs() < 1.0 {
                            break;
                        }
                        d = d2;
                    }
                    None => break,
                }
            }
            let as_req = req.0.max(if mu > 0.0 || mid_bottom { as_min_beam(b, d, fc, fy) } else { 0.0 }) + al / 4.0;
            if !req.0.is_finite() {
                ok = false;
                notes.push(format!("{face} {zn}: Mu {mu:.1} kN·m is beyond what compression steel can add to this section: make it deeper"));
            }
            if req.1 > 0.0 {
                comp_need[zi] = comp_need[zi].max(req.1);
                notes.push(format!("{face} {zn}: doubly reinforced, compression steel {:.0} mm² in the {} face", req.1, if sign > 0.0 { "top" } else { "bottom" }));
            }
            let (bars_txt, as_prov, r) = match &chosen {
                Some(bars) => {
                    let dd = eff_depth(h, cover, ds, bars);
                    let cap = phi_mn(bars.area(), b, dd, fc, fy);
                    let r = if req.1 > 0.0 { if bars.area() >= as_req { 1.0f64.min(mu_n / cap.max(1.0)) } else { mu_n / cap.max(1.0) } } else { mu_n / cap.max(1.0) };
                    (bars.text(), bars.area(), r)
                }
                None => {
                    ok = false;
                    notes.push(format!("{face} {zn}: the bars for {as_req:.0} mm² do not fit in two layers across {b:.0} mm"));
                    ("does not fit".into(), 0.0, f64::INFINITY)
                }
            };
            if r > ratio {
                ratio = r;
                governs = format!("flexure, {face} {zn}");
            }
            if face == "top" {
                top_bars.push(chosen.clone());
            }
            flex.push(FlexZone { face: face.into(), zone: (*zn).into(), mu, as_req, as_prov, bars: bars_txt, ratio: r });
        }
    }
    // compression steel: the opposite face must hold it
    for (zi, need) in comp_need.iter().enumerate() {
        if *need <= 0.0 {
            continue;
        }
        for f in flex.iter_mut().filter(|f| f.zone == zones[zi].0) {
            if f.as_prov < *need && f.face == "top" {
                if let Some(bars) = choose_bars(*need, b, cover, ds, &set.beam_bars, set.aggregate) {
                    f.as_req = f.as_req.max(*need);
                    f.as_prov = bars.area();
                    f.bars = bars.text();
                }
            }
        }
    }
    // shear (22.5, 9.6.3, 9.7.6.2.2): end quarters from d off the support, middle half
    let d = h - cover - ds - 10.0;
    let ag = b * h;
    let mut section_ratio: f64 = 0.0;
    let mut shear = vec![];
    for (zn, x0, x1) in [("ends", 0.0, l / 4.0), ("middle", l / 4.0, 3.0 * l / 4.0)] {
        let dm = d / 1000.0;
        let in_zone = |x: f64| -> bool {
            let from_end = x.min(l - x);
            if zn == "ends" {
                (from_end >= dm.min(l / 4.0) - 1e-6 || dem.cantilever) && (x <= x1 + 1e-6 || x >= l - x1 - 1e-6)
            } else {
                x >= x0 - 1e-6 && x <= x1 + 1e-6
            }
        };
        let (mut vu, mut nu, mut tu) = (0.0f64, 0.0f64, 0.0f64);
        for s in all().filter(|s| in_zone(s.0)) {
            if s.2.abs() > vu {
                vu = s.2.abs();
                nu = s.4;
            }
            tu = tu.max(s.3.abs());
        }
        let at_s = at_s_for(tu);
        let phi = crate::codes::phi().shear;
        let vu_n = vu * 1000.0;
        let st = shear_terms(fc, b, d, vu_n, nu * 1000.0, ag, fyt);
        let (vc, vs, vs_max) = (st.vc, st.vs, st.vs_max);
        if torsion {
            // 22.7.7.1: the combined shear stresses of a solid section
            let lhs = ((vu_n / (b * d)).powi(2) + (tu * 1e6 * ph / (1.7 * aoh * aoh)).powi(2)).sqrt();
            section_ratio = section_ratio.max(lhs / (phi * (vc / (b * d) + 0.66 * sq)));
        }
        let mut s_best = None;
        for &dsb in &set.stirrup_bars {
            for legs in [2usize, 4] {
                if legs == 4 && b < 300.0 {
                    continue;
                }
                let ab = bar_area(dsb) * 1e6;
                let av = legs as f64 * ab;
                // the outer (closed) legs carry the torsion too: per leg Vs/(n fyt d) + At/s
                let per_leg = if vs > 0.0 { vs / (legs as f64 * fyt * d) } else { 0.0 } + at_s;
                let s_str = if per_leg > 0.0 { ab / per_leg } else { f64::INFINITY };
                // minimum (Av + 2At)/s (9.6.3.4 / 9.6.4.2): the stirrup's legs against it
                let s_min_av = av / st.av_s_min;
                let mut s_max = st.s_max;
                if torsion {
                    s_max = s_max.min(ph / 8.0).min(300.0);
                }
                let s = (s_str.min(s_min_av).min(s_max) / 25.0).floor() * 25.0;
                if s >= 75.0 {
                    s_best = Some((dsb, legs, s, av));
                    break;
                }
            }
            if s_best.is_some() {
                break;
            }
        }
        let (txt, s, r) = if vs > vs_max {
            ok = false;
            notes.push(format!("shear {zn}: Vu {vu:.1} kN needs Vs {:.0} kN, over the limit 0.66√f'c bw d = {:.0} kN: enlarge the section", vs / 1000.0, vs_max / 1000.0));
            ("section too small".to_string(), 0.0, vu_n / (phi * (vc + vs_max)))
        } else {
            match s_best {
                Some((dsb, legs, s, av)) => {
                    let ab = bar_area(dsb) * 1e6;
                    // what the stirrups leave for shear after the torsion's share
                    let av_v = (av - legs as f64 * at_s * s).max(0.0);
                    let cap = phi * (vc + av_v * fyt * d / s);
                    let closed = if torsion { " closed" } else { "" };
                    let t = if legs == 2 { format!("{}{closed} @ {:.0} mm", bar_name(dsb), s) } else { format!("{}{closed} 4 legs @ {:.0} mm", bar_name(dsb), s) };
                    (t, s, (vu_n / cap.max(1.0)).max(if at_s > 0.0 { at_s * s / ab } else { 0.0 }))
                }
                None => {
                    ok = false;
                    ("closer than 75 mm".to_string(), 0.0, f64::INFINITY)
                }
            }
        };
        if r > ratio {
            ratio = r;
            governs = format!("shear, {zn}");
        }
        shear.push(ShearZone { zone: zn.into(), vu, phi_vc: phi * vc / 1000.0, stirrups: txt, s, ratio: r });
    }
    // torsion: the section limit, side bars, and what the agent and the engineer should know
    let torsion_design = torsion.then(|| {
        if section_ratio > ratio {
            ratio = section_ratio;
            governs = "torsion + shear section limit (22.7.7.1)".into();
        }
        if section_ratio > 1.0 {
            notes.push(format!("torsion and shear exceed the section limit of 22.7.7.1 ({section_ratio:.2}): enlarge the section"));
        }
        let side = choose_bars(al / 4.0, h - 2.0 * cover, cover, ds, &set.beam_bars, set.aggregate).map(|b| format!("{} each side", b.text())).unwrap_or_else(|| "see Al".into());
        notes.push(format!("torsion Tu {tu_all:.1} kN·m over φTth {phi_tth:.1}: closed stirrups, Al {al:.0} mm² (Al/4 added to the top and bottom bars, side bars {side}); compatibility torsion may be capped at φTcr {:.1} kN·m (22.7.3.2)", 4.0 * phi_tth));
        TorsionDesign { tu: tu_all, phi_tth, phi_tcr: 4.0 * phi_tth, at_s: at_s_max, al, side_bars: side, section_ratio }
    });
    // axial force
    let nmax = all().map(|s| -s.4).fold(0.0, f64::max);
    if nmax * 1000.0 > 0.1 * fc * ag {
        notes.push(format!("axial compression {nmax:.0} kN is over 0.1 f'c Ag: design it as a beam-column"));
    }
    // minimum depth (Table 9.3.1.1, × (0.4 + fy/700) for fy ≠ 420)
    let (div, cond) = if dem.cantilever {
        (8.0, "cantilever")
    } else {
        match (dem.cont_i, dem.cont_j) {
            (true, true) => (21.0, "both ends continuous"),
            (true, false) | (false, true) => (18.5, "one end continuous"),
            _ => (16.0, "simply supported"),
        }
    };
    let h_min = l * 1000.0 / div * (0.4 + fy / 700.0);
    // deflection with Ie (24.2.3.5) and the long-term multiplier (24.2.4.1)
    let mid_bot = flex.iter().find(|f| f.face == "bottom" && f.zone == "middle").map(|f| f.as_prov).unwrap_or(0.0);
    let mid_top = flex.iter().find(|f| f.face == "top" && f.zone == "middle").map(|f| f.as_prov).unwrap_or(0.0);
    let ig = b * h.powi(3) / 12.0;
    let n = ES / mat.ec;
    let dd = h - cover - ds - 10.0;
    let (_, icr) = cracked_inertia(b, dd, mid_bot, n);
    let mcr = 0.62 * fc.sqrt() * ig / (h / 2.0);
    let ie = |ma: f64| -> f64 { effective_inertia(ma * 1e6, mcr, icr, ig) };
    let d_dl = dem.defl_dl * 1000.0 * ig / ie(dem.ma_dl);
    let d_d = dem.defl_d * 1000.0 * ig / ie(dem.ma_d);
    let d_l = (d_dl - d_d).max(0.0);
    let rho2 = mid_top / (b * dd);
    let lam = 2.0 / (1.0 + 50.0 * rho2);
    let d_total = lam * d_d + d_l;
    let (lim_l, lim_t) = (l * 1000.0 / set.defl_live, l * 1000.0 / set.defl_total);
    let rd = (d_l / lim_l).max(d_total / lim_t);
    if rd > ratio {
        ratio = rd;
        governs = if d_l / lim_l > d_total / lim_t { "live-load deflection".into() } else { "long-term deflection".into() };
    }
    if h < h_min {
        notes.push(format!("h {h:.0} mm is under the Table 9.3.1.1 minimum {h_min:.0} mm ({cond}); the deflection was calculated and is what counts"));
    }
    if ratio > set.target_ratio + 1e-9 {
        ok = false;
    }
    BeamDesign { member, bim_id, b: b_m, h: h_m, span: l, flexure: flex, shear, torsion: torsion_design, ec2: None, h_min, support: cond.into(), defl_live: d_l, defl_total: d_total, limit_live: lim_l, limit_total: lim_t, ratio, governs, ok, notes }
}

/// One-way shear of a beam (ACI 318-19 22.5, 9.6.3, 9.7.6.2.2), N and mm: Vc = (0.17√f'c +
/// Nu/6Ag) bw d with Nu negative in tension and compression left out (conservative, and
/// 22.5.5.1 with Av ≥ Av,min), the Vs needed for Vu with φ = 0.75, the limit Vs ≤ 0.66√f'c bw d,
/// the largest spacing d/2 ≤ 600 (d/4 ≤ 300 over 0.33√f'c bw d) and (Av/s)min =
/// max(0.062√f'c, 0.35) bw / fyt (mm²/mm).
#[derive(Clone, Copy, Debug)]
pub struct ShearTerms {
    pub vc: f64,
    pub vs: f64,
    pub vs_max: f64,
    pub s_max: f64,
    pub av_s_min: f64,
}

pub fn shear_terms(fc: f64, bw: f64, d: f64, vu: f64, nu: f64, ag: f64, fyt: f64) -> ShearTerms {
    let sq = sqrt_fc(fc);
    let vc = ((0.17 * sq + nu.min(0.0) / (6.0 * ag)).max(0.0)) * bw * d;
    let vs = (vu / crate::codes::phi().shear - vc).max(0.0);
    let s_max = if vs <= 0.33 * sq * bw * d { (d / 2.0).min(600.0) } else { (d / 4.0).min(300.0) };
    ShearTerms { vc, vs, vs_max: 0.66 * sq * bw * d, s_max, av_s_min: (0.062 * sq).max(0.35) * bw / fyt }
}

/// The nominal axial strength P0 = 0.85 f'c (Ag − Ast) + fy Ast (22.4.2.2) and φPn,max =
/// 0.80 φ P0 of a tied column (Table 22.4.2.1, φ = 0.65; 0.70 by the Thai regulation), N.
pub fn column_p0(ag: f64, ast: f64, fc: f64, fy: f64) -> (f64, f64) {
    let p0 = 0.85 * fc * (ag - ast) + fy * ast;
    (p0, 0.80 * crate::codes::phi().tied * p0)
}

/// The torsion terms of a solid rectangle b x h (mm) with closed stirrups of diameter `ds` at
/// clear cover `cover` (ACI 318-19 22.7), for a torque Tu (kN·m): Acp, pcp, Aoh, ph and Ao
/// (mm², mm), φTth and φTcr (kN·m, φ = 0.75), At/s per leg (mm²/mm), Al (22.7.6.1b) and the
/// least Al,min of 9.6.4.3 (mm²), all with θ = 45°.
#[derive(Clone, Copy, Debug)]
pub struct TorsionTerms {
    pub acp: f64,
    pub pcp: f64,
    pub aoh: f64,
    pub ph: f64,
    pub ao: f64,
    pub phi_tth: f64,
    pub phi_tcr: f64,
    pub at_s: f64,
    pub al: f64,
    pub al_min: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn torsion_terms(b: f64, h: f64, cover: f64, ds: f64, fc: f64, fy: f64, fyt: f64, tu: f64) -> TorsionTerms {
    let sq = sqrt_fc(fc);
    let (acp, pcp) = (b * h, 2.0 * (b + h));
    let phi = crate::codes::phi().shear;
    let phi_tth = phi * 0.083 * sq * acp * acp / pcp / 1e6;
    let (x1, y1) = (b - 2.0 * cover - ds, h - 2.0 * cover - ds);
    let (aoh, ph) = (x1 * y1, 2.0 * (x1 + y1));
    let ao = 0.85 * aoh;
    let at_s = tu * 1e6 / (2.0 * phi * ao * fyt);
    let al = at_s * ph * fyt / fy;
    let al_min = 0.42 * sq * acp / fy - at_s.max(0.175 * b / fyt) * ph * fyt / fy;
    TorsionTerms { acp, pcp, aoh, ph, ao, phi_tth, phi_tcr: 4.0 * phi_tth, at_s, al, al_min }
}

/// The 22.7.7.1 limit of a solid section: √((Vu/bw d)² + (Tu ph/1.7 Aoh²)²) over
/// φ(Vc/bw d + 0.66√f'c), with Vu in N, Tu in N·mm, Vc in N, lengths in mm.
pub fn torsion_section_ratio(vu: f64, tu: f64, bw: f64, d: f64, vc: f64, aoh: f64, ph: f64, fc: f64) -> f64 {
    let lhs = ((vu / (bw * d)).powi(2) + (tu * ph / (1.7 * aoh * aoh)).powi(2)).sqrt();
    lhs / (crate::codes::phi().shear * (vc / (bw * d) + 0.66 * sqrt_fc(fc)))
}

/// The cracked transformed section of a rectangle with tension steel only: the neutral-axis
/// ratio k = c/d and Icr (mm⁴), for the modular ratio n = Es/Ec.
pub fn cracked_inertia(b: f64, d: f64, as_: f64, n: f64) -> (f64, f64) {
    let rho_n = n * as_ / (b * d);
    let k = ((rho_n * rho_n + 2.0 * rho_n).sqrt() - rho_n).max(0.0);
    (k, b * (k * d).powi(3) / 3.0 + n * as_ * (d - k * d).powi(2))
}

/// The effective moment of inertia of ACI 318-19 Table 24.2.3.5 (N·mm, mm⁴): Ig up to
/// (2/3)Mcr, then Icr / (1 − ((2/3)Mcr/Ma)² (1 − Icr/Ig)).
pub fn effective_inertia(ma: f64, mcr: f64, icr: f64, ig: f64) -> f64 {
    if ma <= 2.0 / 3.0 * mcr || icr <= 0.0 {
        ig
    } else {
        (icr / (1.0 - (2.0 / 3.0 * mcr / ma).powi(2) * (1.0 - icr / ig))).min(ig)
    }
}

// ---------------------------------------------------------------- slabs

/// ACI 318-63 Method 2 moment coefficients, M = C w S² per unit width (S the short span),
/// by m = S/L at 1.0, 0.9, 0.8, 0.7, 0.6, 0.5, then the long-span value for every m.
/// Rows per case: negative at continuous edges, negative at discontinuous edges, positive.
/// Cases: 1 interior panel, 2 one edge discontinuous, 3 two edges, 4 three edges, 5 four.
const METHOD2: [[[f64; 7]; 3]; 5] = [
    [[0.033, 0.040, 0.048, 0.055, 0.063, 0.083, 0.033], [0.0; 7], [0.025, 0.030, 0.036, 0.041, 0.047, 0.062, 0.025]],
    [[0.041, 0.048, 0.055, 0.062, 0.069, 0.085, 0.041], [0.021, 0.024, 0.027, 0.031, 0.035, 0.042, 0.021], [0.031, 0.036, 0.041, 0.047, 0.052, 0.064, 0.031]],
    [[0.049, 0.057, 0.064, 0.071, 0.078, 0.090, 0.049], [0.025, 0.028, 0.032, 0.036, 0.039, 0.045, 0.025], [0.037, 0.043, 0.048, 0.054, 0.059, 0.068, 0.037]],
    [[0.058, 0.066, 0.074, 0.082, 0.090, 0.098, 0.058], [0.029, 0.033, 0.037, 0.041, 0.045, 0.049, 0.029], [0.044, 0.050, 0.056, 0.062, 0.068, 0.074, 0.044]],
    [[0.0; 7], [0.033, 0.038, 0.043, 0.047, 0.053, 0.055, 0.033], [0.050, 0.057, 0.064, 0.072, 0.080, 0.083, 0.050]],
];

/// The Method 2 coefficients (short: neg cont, neg disc, pos; long: the same) for m and the
/// number of discontinuous edges.
pub fn method2(m: f64, discontinuous: usize) -> ([f64; 3], [f64; 3]) {
    let t = &METHOD2[discontinuous.min(4)];
    let ms = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5];
    let m = m.clamp(0.5, 1.0);
    let k = ms.iter().position(|&x| x <= m + 1e-12).unwrap_or(5);
    let interp = |row: &[f64; 7]| -> f64 {
        if k == 0 {
            return row[0];
        }
        let (m0, m1) = (ms[k - 1], ms[k]);
        row[k - 1] + (row[k] - row[k - 1]) * (m0 - m) / (m0 - m1)
    };
    ([interp(&t[0]), interp(&t[1]), interp(&t[2])], [t[0][6], t[1][6], t[2][6]])
}

#[derive(Clone, Debug, Serialize)]
pub struct SlabStrip {
    /// "short span" / "long span"; "midspan (bottom)", "continuous edge (top)", ...
    pub direction: String,
    pub location: String,
    /// kN·m per m, mm² per m.
    pub mu: f64,
    pub as_req: f64,
    pub as_prov: f64,
    pub bars: String,
    pub ratio: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SlabDesign {
    pub slab: crate::model::Id,
    pub bim_id: Option<String>,
    pub name: String,
    pub lx: f64,
    pub ly: f64,
    pub h: f64,
    /// "two-way (ACI 318-63 Method 2, case 3)" or "one-way (ACI 318-19 6.5)".
    pub method: String,
    pub discontinuous_edges: usize,
    /// Factored load, kPa.
    pub wu: f64,
    pub strips: Vec<SlabStrip>,
    pub h_min: f64,
    pub vu: f64,
    pub phi_vc: f64,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
    /// A flat plate's strips: where its bars go (for the bar layout in the BIM model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flat: Option<FlatStrips>,
}

/// The geometry of a flat plate's strips: the column lines (x of the lines along y, y of the
/// lines along x), the column strips' half width each side of a line, and the columns (x, y,
/// c1 along x, c2 along y), m.
#[derive(Clone, Debug, Serialize)]
pub struct FlatStrips {
    pub lines_x: Vec<f64>,
    pub lines_y: Vec<f64>,
    pub half_width: f64,
    pub columns: Vec<[f64; 4]>,
}

/// Whether a bar is a round bar (RB6, RB9: grade SR24) rather than a deformed one.
pub fn is_round(db_mm: f64) -> bool {
    db_mm == 6.0 || db_mm == 9.0
}

/// A slab's bars for a moment per metre: for each bar size its steel (with that bar's grade:
/// round bars take `fy_round`, deformed bars `fy`), the spacing in 25 mm steps at most
/// `s_max`, and the first size whose spacing is at least 100 mm. Returns (As required with
/// that bar's grade, As provided, spacing, text, fy of the bar), mm²/m, mm, MPa.
pub fn slab_bars(mu_knm: f64, d: f64, as_min: f64, s_max: f64, bars: &[f64], fc: f64, fy: f64, fy_round: f64) -> (f64, f64, f64, String, f64) {
    let mut pick = None;
    for &db in bars {
        let f = if is_round(db) { fy_round } else { fy };
        let as_req = flexure_steel(mu_knm * 1e6, 1000.0, d, 0.0, fc, f).0.max(as_min);
        let ab = bar_area(db) * 1e6;
        let s = ((ab * 1000.0 / as_req.max(1.0)).min(s_max) / 25.0).floor() * 25.0;
        pick = Some((db, s.max(50.0), ab, as_req, f));
        if s >= 100.0 {
            break;
        }
    }
    let (db, s, ab, as_req, f) = pick.unwrap_or((12.0, 100.0, bar_area(12.0) * 1e6, as_min, fy));
    (as_req, ab * 1000.0 / s, s, format!("{} @ {:.0} mm", bar_name(db), s), f)
}

/// Designs a slab panel. `cont` = continuity of the edges (bottom, right, top, left in plan);
/// `wd`, `wl` dead (with self-weight) and live loads, kPa.
#[allow(clippy::too_many_arguments)]
pub fn design_slab(slab: &crate::model::Slab, cont: [bool; 4], wd: f64, wl: f64, mat: &RcMat, set: &crate::model::DesignSettings) -> SlabDesign {
    let (lx, ly) = (slab.lx(), slab.ly());
    let (s, l) = (lx.min(ly), lx.max(ly));
    let short_is_x = lx <= ly;
    let h = slab.h * 1000.0;
    let fc = mat.fc;
    let fy = mat.fy;
    let wu = set.standard.gravity(wd, wl);
    let cover = set.cover_slab * 1000.0;
    let db0 = set.slab_bars.first().copied().unwrap_or(9.0);
    let d_short = h - cover - db0 / 2.0;
    let d_long = d_short - db0;
    let as_min = 0.0018 * 1000.0 * h;
    let mut strips = vec![];
    let mut notes = vec![];
    let mut ratio: f64 = 0.0;
    let mut governs = String::new();
    let disc = cont.iter().filter(|c| !**c).count();
    let m = s / l;
    let one_way = m < 0.5;
    // edges perpendicular to the short span: the long edges (bottom/top when x is the long
    // direction, i.e. the short span runs in y)
    let long_edges = if short_is_x { [cont[1], cont[3]] } else { [cont[0], cont[2]] };
    let short_edges = if short_is_x { [cont[0], cont[2]] } else { [cont[1], cont[3]] };
    let fy_round = mat.fyt;
    let mut push = |dir: &str, loc: &str, mu: f64, d: f64, s_max: f64, min_only: bool, strips: &mut Vec<SlabStrip>| {
        let (as_req, as_prov, _, bars, fb) = slab_bars(if min_only { 0.0 } else { mu }, d, as_min, s_max, &set.slab_bars, fc, fy, fy_round);
        let cap = phi_mn(as_prov, 1000.0, d, fc, fb) / 1e6;
        let r = if mu > 0.0 { mu / cap } else { 0.0 };
        if r > ratio {
            ratio = r;
            governs = format!("flexure, {dir} {loc}");
        }
        strips.push(SlabStrip { direction: dir.into(), location: loc.into(), mu, as_req, as_prov, bars, ratio: r });
    };
    let method;
    let h_min;
    let vu;
    if one_way {
        method = "one-way (ACI 318-19 6.5 coefficients)".to_string();
        let ln = s;
        let w = wu * ln * ln;
        let n_cont = long_edges.iter().filter(|c| **c).count();
        let (pos, neg_c, neg_d) = match n_cont {
            0 => (w / 8.0, 0.0, w / 24.0),
            // the end span of a continuous slab: the first interior support takes wl²/9, the
            // value of two spans, because the panel does not know how many follow (more than
            // two: wl²/10); the positive moment wl²/11 is that of an unrestrained end, which the
            // wl²/14 of an end integral with its support does not exceed
            1 => (w / 11.0, w / 9.0, w / 24.0),
            _ => (w / 16.0, w / 11.0, 0.0),
        };
        let smax_main = (3.0 * h).min(450.0);
        push("short span", "midspan (bottom)", pos, d_short, smax_main, false, &mut strips);
        if n_cont > 0 {
            push("short span", "continuous edge (top)", neg_c, d_short, smax_main, false, &mut strips);
        }
        if n_cont < 2 {
            push("short span", "discontinuous edge (top)", neg_d, d_short, smax_main, false, &mut strips);
        }
        push("long span", "shrinkage and temperature", 0.0, d_long, (5.0 * h).min(450.0), true, &mut strips);
        let div = match n_cont {
            0 => 20.0,
            1 => 24.0,
            _ => 28.0,
        };
        h_min = ln * 1000.0 / div * (0.4 + fy / 700.0);
        vu = if n_cont == 1 { 1.15 * wu * ln / 2.0 } else { wu * ln / 2.0 };
        if n_cont == 0 {
            notes.push("a single simply supported span: wl²/8, with wl²/24 top steel at the edges for the restraint of the beams".into());
        }
    } else {
        let (cs, cl) = method2(m, disc);
        method = format!("two-way (ACI 318-63 Method 2, {disc} discontinuous edge{})", if disc == 1 { "" } else { "s" });
        let w = wu * s * s;
        let smax = (2.0 * h).min(450.0);
        push("short span", "midspan (bottom)", cs[2] * w, d_short, smax, false, &mut strips);
        if long_edges.iter().any(|c| *c) {
            push("short span", "continuous edge (top)", cs[0] * w, d_short, smax, false, &mut strips);
        }
        if long_edges.iter().any(|c| !*c) {
            push("short span", "discontinuous edge (top)", cs[1] * w, d_short, smax, false, &mut strips);
        }
        push("long span", "midspan (bottom)", cl[2] * w, d_long, smax, false, &mut strips);
        if short_edges.iter().any(|c| *c) {
            push("long span", "continuous edge (top)", cl[0] * w, d_long, smax, false, &mut strips);
        }
        if short_edges.iter().any(|c| !*c) {
            push("long span", "discontinuous edge (top)", cl[1] * w, d_long, smax, false, &mut strips);
        }
        let beta = l / s;
        h_min = (l * 1000.0 * (0.8 + fy / 1400.0) / (36.0 + 9.0 * beta)).max(90.0);
        vu = wu * s / 2.0;
        notes.push("Method 2 moments are those of the middle strip; the column strips may take 2/3 of them, here they get the same bars".into());
        notes.push("minimum thickness from Table 8.3.1.2 assumes stiff edge beams (αfm > 2)".into());
    }
    let phi_vc = crate::codes::phi().shear * 0.17 * sqrt_fc(fc) * 1000.0 * d_short / 1000.0;
    let rv = vu / phi_vc;
    if rv > ratio {
        ratio = rv;
        governs = "one-way shear".into();
    }
    if h < h_min {
        notes.push(format!("h {h:.0} mm is under the minimum thickness {h_min:.0} mm: deflection is not checked otherwise, make it thicker"));
        ratio = ratio.max(h_min / h);
        if governs.is_empty() || h_min / h >= ratio {
            governs = "minimum thickness".into();
        }
    }
    let ok = ratio <= set.target_ratio + 1e-9;
    SlabDesign { slab: slab.id, bim_id: slab.bim_id.clone(), name: slab.name.clone(), lx, ly, h: slab.h, method, discontinuous_edges: disc, wu, strips, h_min, vu, phi_vc, ratio, governs, ok, notes, flat: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singly_reinforced_textbook() {
        // b 300, d 540, f'c 28, fy 420, Mu 300 kN·m: Rn = 3.81 MPa, ρ = 0.00995, As ≈ 1 611 mm²
        let (as_, comp) = flexure_steel(300e6, 300.0, 540.0, 60.0, 28.0, 420.0);
        assert_eq!(comp, 0.0);
        let rn: f64 = 300e6 / (0.9 * 300.0 * 540.0 * 540.0);
        let rho = 0.85 * 28.0 / 420.0 * (1.0 - (1.0 - 2.0 * rn / (0.85 * 28.0)).sqrt());
        assert!((as_ - rho * 300.0 * 540.0).abs() < 1e-6);
        assert!((as_ - 1611.0).abs() < 3.0, "{as_}");
        // the steel found gives back φMn = Mu
        assert!((phi_mn(as_, 300.0, 540.0, 28.0, 420.0) - 300e6).abs() / 300e6 < 1e-6);
    }

    #[test]
    fn doubly_when_needed() {
        let (as_, comp) = flexure_steel(700e6, 300.0, 540.0, 60.0, 28.0, 420.0);
        assert!(comp > 0.0 && as_ > 3000.0, "{as_} {comp}");
    }

    #[test]
    fn method2_corners() {
        let (s, l) = method2(1.0, 0);
        assert_eq!(s, [0.033, 0.0, 0.025]);
        assert_eq!(l, [0.033, 0.0, 0.025]);
        // worked example (H. K. Hussain, U. of Basrah lecture notes): interior panel, m = 0.8332,
        // +C = 0.03402; its −C is printed 0.05436, a transposition of 0.04534 (no case-1 value
        // at m = 0.83 is near 0.054)
        let (s, l) = method2(0.8332, 0);
        assert!((s[2] - 0.03402).abs() < 5e-5 && (s[0] - 0.04534).abs() < 5e-5 && l == [0.033, 0.0, 0.025], "{s:?}");
        let (s, _) = method2(0.75, 2);
        assert!((s[2] - (0.048 + 0.054) / 2.0).abs() < 1e-12);
    }

    #[test]
    fn bars_fit() {
        let b = choose_bars(1650.0, 300.0, 30.0, 9.0, &[12.0, 16.0, 20.0, 25.0, 28.0], 20.0).unwrap();
        assert!(b.area() >= 1650.0 && b.layers == 1, "{b:?}");
    }

    #[test]
    fn torsion_by_hand() {
        // 300 x 600, f'c 24, SD40 / SR24, cover 30, RB9: Tu 20 kN·m along the whole span, no
        // shear or moment. Ao = 0.85 x 231 x 531 = 104 262 mm², ph = 1 524 mm:
        // At/s = 20e6 / (2 x 0.75 x 104 262 x 235) = 0.544 mm²/mm, Al = 0.544 x 1 524 x 235/390 = 500 mm²
        let st: Vec<(f64, f64, f64, f64, f64)> = (0..=8).map(|k| (k as f64 * 0.75, 0.0, 0.0, 20.0, 0.0)).collect();
        let dem = BeamDemand { span: 6.0, uls: vec![st], cont_i: true, cont_j: true, ..Default::default() };
        let mat = RcMat { fc: 24.0, ec: 23_025.0, fy: 390.0, fyt: 235.0 };
        let d = design_beam(1, None, 0.3, 0.6, &dem, &mat, &crate::model::DesignSettings::default());
        let t = d.torsion.expect("torsion designed");
        assert!((t.at_s - 0.544).abs() < 0.002, "{}", t.at_s);
        assert!((t.al - 500.0).abs() < 2.0, "{}", t.al);
        // RB9 closed: 63.6 / 0.544 = 117 mm, in 25 mm steps 100 mm
        assert!(d.shear.iter().all(|s| s.stirrups.contains("closed") && s.s <= 117.0), "{:?}", d.shear);
        assert!(d.ok, "{:?} {}", d.notes, d.ratio);
    }
}


// ---------------------------------------------------------------- columns

/// What a column carries per ULS combination: (x, N tension +, My, Mz, Vy, Vz) at its
/// stations (kN, kN·m), its unbraced length and effective length factor.
#[derive(Clone, Debug, Default)]
pub struct ColumnDemand {
    pub lu: f64,
    pub k: f64,
    pub uls: Vec<(String, Vec<(f64, f64, f64, f64, f64, f64)>)>,
    /// Second-order forces from a P-Delta analysis: no moment magnification.
    pub second_order: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ColumnCheck {
    pub name: String,
    pub ratio: f64,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ColumnDesign {
    pub member: crate::model::Id,
    pub bim_id: Option<String>,
    pub b: f64,
    pub h: f64,
    /// e.g. "12-DB20", bars per face along b and along h, steel ratio.
    pub bars: String,
    pub n_b: usize,
    pub n_h: usize,
    pub db: f64,
    pub rho: f64,
    pub ties: String,
    pub checks: Vec<ColumnCheck>,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

/// A rectangular section's bars: (depth from the compression face, area) for bending with
/// `depth` the dimension in the plane of bending, `n_d` bars along the faces normal to it
/// (each of the two extreme rows) and `n_s` bars along the side faces (corners counted).
pub fn bar_rows(depth: f64, cover_c: f64, n_d: usize, n_s: usize, ab: f64) -> Vec<(f64, f64)> {
    let mut out = vec![(cover_c, n_d as f64 * ab), (depth - cover_c, n_d as f64 * ab)];
    for k in 1..n_s.saturating_sub(1) {
        let z = cover_c + (depth - 2.0 * cover_c) * k as f64 / (n_s - 1) as f64;
        out.push((z, 2.0 * ab));
    }
    out
}

/// φ for a section whose extreme tension steel strains εt (ACI 318-19 Table 21.2.2, tied),
/// between the active standard's compression-controlled and tension-controlled values.
pub fn phi_tied(et: f64, fy: f64) -> f64 {
    let ety = fy / ES;
    let f = crate::codes::phi();
    if et >= ety + 0.003 {
        f.flexure
    } else if et <= ety {
        f.tied
    } else {
        f.tied + (f.flexure - f.tied) * (et - ety) / 0.003
    }
}

/// The section's (Pn, Mn, εt) for a neutral-axis depth c (N, N·mm; compression +).
pub fn section_at(c: f64, width: f64, depth: f64, rows: &[(f64, f64)], fc: f64, fy: f64) -> (f64, f64, f64) {
    let b1 = beta1(fc);
    let a = (b1 * c).min(depth);
    let cc = 0.85 * fc * a * width;
    let (mut p, mut m) = (cc, cc * (depth / 2.0 - a / 2.0));
    let mut et: f64 = 0.0;
    for &(d, area) in rows {
        let eps = 0.003 * (c - d) / c;
        let mut fs = (ES * eps).clamp(-fy, fy);
        if d <= a {
            fs -= 0.85 * fc;
        }
        p += area * fs;
        m += area * fs * (depth / 2.0 - d);
        et = et.max(-eps);
    }
    (p, m, et)
}

/// φMn (N·mm) of a rectangular column about one axis at the factored axial force pu (N,
/// compression +), by strain compatibility; φPn,max = 0.80 φ P0 caps the compression.
pub fn column_phi_mn(width: f64, depth: f64, rows: &[(f64, f64)], fc: f64, fy: f64, pu: f64) -> f64 {
    let ast: f64 = rows.iter().map(|r| r.1).sum();
    let (_, cap) = column_p0(width * depth, ast, fc, fy);
    if pu > cap {
        return 0.0;
    }
    let pt = -crate::codes::phi().flexure * fy * ast;
    if pu < pt {
        return 0.0;
    }
    // φPn(c) rises with c: find the c where it equals pu
    let (mut lo, mut hi) = (1e-3 * depth, 5.0 * depth);
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        let (p, _, et) = section_at(mid, width, depth, rows, fc, fy);
        if phi_tied(et, fy) * p < pu {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (_, m, et) = section_at((lo + hi) / 2.0, width, depth, rows, fc, fy);
    phi_tied(et, fy) * m.abs()
}

/// The nominal Mn (N·mm) of a rectangular column about one axis at a nominal axial force pn (N,
/// compression +), by strain compatibility, with the εt of that state.
pub fn column_mn_nominal(width: f64, depth: f64, rows: &[(f64, f64)], fc: f64, fy: f64, pn: f64) -> (f64, f64) {
    let (mut lo, mut hi) = (1e-3 * depth, 5.0 * depth);
    for _ in 0..100 {
        let mid = (lo + hi) / 2.0;
        if section_at(mid, width, depth, rows, fc, fy).0 < pn {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (_, m, et) = section_at((lo + hi) / 2.0, width, depth, rows, fc, fy);
    (m.abs(), et)
}

/// The critical load Pc (N) of 6.6.4.4.2 with (EI)eff = 0.4 Ec Ig / (1 + βdns) (6.6.4.4.4a),
/// for a rectangle with `bend` the dimension in the plane of bending (mm) and klu (mm).
pub fn critical_load(ec: f64, bend: f64, other: f64, klu: f64, beta_dns: f64) -> f64 {
    let ei = 0.4 * ec * other * bend.powi(3) / 12.0 / (1.0 + beta_dns);
    std::f64::consts::PI.powi(2) * ei / klu.powi(2)
}

/// The moment of a braced column after 6.6.4.5 (N·mm): M2 at least Pu(15 + 0.03h), magnified
/// by δ = Cm/(1 − Pu/0.75Pc) ≥ 1 with Cm = 0.6 − 0.4 M1/M2 (M1/M2 negative in single
/// curvature here: `m1`, `m2` the end moments with their signs) unless klu/r with r = 0.3h is
/// within 34 − 12 M1/M2 ≤ 40 (6.2.5.1); infinite when Pu ≥ 0.75 Pc. Returns (Mc, δ).
pub fn moment_magnifier(pu: f64, m1: f64, m2: f64, klu: f64, dim: f64, pc: f64, second_order: bool) -> (f64, f64) {
    let m2a = m2.abs() * 1e6;
    let mmin = pu.max(0.0) * (15.0 + 0.03 * dim);
    if second_order || pu <= 0.0 {
        return (m2a.max(mmin), 1.0);
    }
    // double curvature when the end moments have opposite signs about the axis
    let ratio = if m2.abs() > 1e-9 { -(m1 / m2) } else { 0.0 };
    let lim = (34.0 - 12.0 * ratio).min(40.0);
    if klu / (0.3 * dim) <= lim {
        return (m2a.max(mmin), 1.0);
    }
    if pu >= 0.75 * pc {
        return (f64::INFINITY, f64::INFINITY);
    }
    let cm = (0.6 - 0.4 * ratio).max(0.4);
    let d = (cm / (1.0 - pu / (0.75 * pc))).max(1.0);
    (d * m2a.max(mmin), d)
}

/// Designs a rectangular column b x h (m; h along the member's local z, so My bends over h):
/// the least longitudinal steel (ρ ≥ 1 %) that passes axial and biaxial bending in every
/// combination (load contour, α = 1.15), with the moments of a braced column magnified for
/// slenderness (6.6.4) unless the forces are second-order, the minimum moment, shear with
/// the axial force (22.5.6.1) and the ties (25.7.2).
pub fn design_column(member: crate::model::Id, bim_id: Option<String>, b_m: f64, h_m: f64, dem: &ColumnDemand, mat: &RcMat, set: &crate::model::DesignSettings) -> ColumnDesign {
    let (b, h) = (b_m * 1000.0, h_m * 1000.0);
    let (fc, fy, fyt) = (mat.fc, mat.fy, mat.fyt);
    let cover = set.cover_beam * 1000.0;
    // ties at least No. 10 round bars up to No. 32, No. 13 for larger (25.7.2.2): the
    // first stirrup size that qualifies
    let dt = {
        let need = if set.beam_bars.iter().any(|d| *d >= 36.0) { 12.0 } else { 10.0 };
        set.stirrup_bars.iter().copied().find(|d| *d >= need).unwrap_or(need)
    };
    let ag = b * h;
    let mut notes = vec![];
    // slenderness per axis: klu/r with r = 0.3 x the dimension in the plane of bending
    let klu = dem.k.max(0.5) * dem.lu * 1000.0;
    let ec = mat.ec;
    // βdns: the sustained share of the factored axial load is not known, 0.6 is assumed
    let pc_y = critical_load(ec, h, b, klu, 0.6);
    let pc_z = critical_load(ec, b, h, klu, 0.6);
    // the design forces per combination: Pu and the end moments magnified
    struct Load {
        name: String,
        pu: f64,
        muy: f64,
        muz: f64,
        vy: f64,
        vz: f64,
    }
    let mut loads: Vec<Load> = vec![];
    for (name, st) in &dem.uls {
        if st.is_empty() {
            continue;
        }
        let pu = st.iter().map(|s| -s.1).fold(f64::MIN, f64::max) * 1000.0;
        let ends = |f: fn(&(f64, f64, f64, f64, f64, f64)) -> f64| -> (f64, f64) {
            let (a, z) = (f(&st[0]), f(&st[st.len() - 1]));
            if a.abs() >= z.abs() {
                (z, a)
            } else {
                (a, z)
            }
        };
        let mag = |m1: f64, m2: f64, pc: f64, dim: f64, _r: f64| -> f64 { moment_magnifier(pu, m1, m2, klu, dim, pc, dem.second_order).0 };
        let (y1, y2) = ends(|s| s.2);
        let (z1, z2) = ends(|s| s.3);
        let muy = mag(y1, y2, pc_y, h, 0.0).max(st.iter().map(|s| s.2.abs() * 1e6).fold(0.0, f64::max));
        let muz = mag(z1, z2, pc_z, b, 0.0).max(st.iter().map(|s| s.3.abs() * 1e6).fold(0.0, f64::max));
        let vy = st.iter().map(|s| s.4.abs()).fold(0.0, f64::max) * 1000.0;
        let vz = st.iter().map(|s| s.5.abs()).fold(0.0, f64::max) * 1000.0;
        // tension: the least axial force goes with the moments too
        let pmin = st.iter().map(|s| -s.1).fold(f64::MAX, f64::min) * 1000.0;
        loads.push(Load { name: name.clone(), pu, muy, muz, vy, vz });
        if pmin < 0.0 {
            loads.push(Load { name: format!("{name} (tension)"), pu: pmin, muy, muz, vy, vz });
        }
    }
    if klu / (0.3 * b.min(h)) > 100.0 {
        notes.push(format!("klu/r = {:.0} is over 100: the moment magnifier method does not apply (6.2.6)", klu / (0.3 * b.min(h))));
    }
    // try arrangements from the least steel up
    let mut cands: Vec<(usize, usize, f64)> = vec![];
    for &db in &set.beam_bars {
        let cc = cover + dt + db / 2.0;
        let s_clear = 40f64.max(1.5 * db);
        let max_on = |face: f64| (((face - 2.0 * cc) / (db + s_clear)).floor() as usize + 1).max(2);
        for n_b in 2..=max_on(b) {
            for n_h in 2..=max_on(h) {
                cands.push((n_b, n_h, db));
            }
        }
    }
    cands.sort_by(|x, y| {
        let a = |c: &(usize, usize, f64)| (2 * c.0 + 2 * c.1 - 4) as f64 * bar_area(c.2);
        a(x).total_cmp(&a(y))
    });
    let ratio_for = |n_b: usize, n_h: usize, db: f64| -> (f64, String, Vec<ColumnCheck>) {
        let ab = bar_area(db) * 1e6;
        let cc = cover + dt + db / 2.0;
        let rows_y = bar_rows(h, cc, n_b, n_h, ab);
        let rows_z = bar_rows(b, cc, n_h, n_b, ab);
        let ast = (2 * n_b + 2 * n_h - 4) as f64 * ab;
        let (_, cap) = column_p0(ag, ast, fc, fy);
        let mut worst = (0.0f64, String::new());
        let mut checks: Vec<ColumnCheck> = vec![];
        let mut push = |name: &str, r: f64, detail: String, checks: &mut Vec<ColumnCheck>| match checks.iter_mut().find(|c| c.name == name) {
            Some(c) if c.ratio >= r => {}
            Some(c) => {
                c.ratio = r;
                c.detail = detail;
            }
            None => checks.push(ColumnCheck { name: name.into(), ratio: r, detail }),
        };
        for l in &loads {
            let ra = if l.pu > 0.0 { l.pu / cap } else { -l.pu / (crate::codes::phi().flexure * fy * ast) };
            push("axial", ra, format!("{}: Pu {:.0} kN, φPn,max {:.0} kN", l.name, l.pu / 1e3, cap / 1e3), &mut checks);
            let my_cap = column_phi_mn(b, h, &rows_y, fc, fy, l.pu);
            let mz_cap = column_phi_mn(h, b, &rows_z, fc, fy, l.pu);
            let rb = if my_cap <= 0.0 || mz_cap <= 0.0 || !l.muy.is_finite() || !l.muz.is_finite() { f64::INFINITY } else { ((l.muy / my_cap).powf(1.15) + (l.muz / mz_cap).powf(1.15)).powf(1.0 / 1.15) };
            push("P-M biaxial (load contour α 1.15)", rb, format!("{}: Pu {:.0} kN, Muy {:.1} / φMny {:.1}, Muz {:.1} / φMnz {:.1} kN·m", l.name, l.pu / 1e3, l.muy / 1e6, my_cap / 1e6, l.muz / 1e6, mz_cap / 1e6), &mut checks);
            let r = ra.max(rb);
            if r > worst.0 {
                worst = (r, l.name.clone());
            }
        }
        (worst.0, worst.1, checks)
    };
    let mut pick = None;
    for (n_b, n_h, db) in &cands {
        let ast = (2 * n_b + 2 * n_h - 4) as f64 * bar_area(*db) * 1e6;
        if ast < 0.01 * ag {
            continue;
        }
        if ast > 0.04 * ag {
            break;
        }
        let (r, _, _) = ratio_for(*n_b, *n_h, *db);
        if r <= set.target_ratio {
            pick = Some((*n_b, *n_h, *db));
            break;
        }
    }
    let fallback = cands.iter().rev().find(|c| (2 * c.0 + 2 * c.1 - 4) as f64 * bar_area(c.2) * 1e6 <= 0.04 * ag).copied().or(cands.last().copied()).unwrap_or((2, 2, 20.0));
    let (n_b, n_h, db) = pick.unwrap_or(fallback);
    if pick.is_none() {
        notes.push("no bar arrangement up to ρ = 4 % passes: the section is too small".into());
    }
    let (_, _, mut checks) = ratio_for(n_b, n_h, db);
    let nbar = 2 * n_b + 2 * n_h - 4;
    let ast = nbar as f64 * bar_area(db) * 1e6;
    // shear with the axial force (22.5.6.1) and ties (25.7.2, 10.7.6.5)
    let sq = sqrt_fc(fc);
    let ab_t = bar_area(dt) * 1e6;
    let s_tie_max = (16.0 * db).min(48.0 * dt).min(b.min(h));
    let mut s = (s_tie_max / 25.0).floor() * 25.0;
    for l in &loads {
        for (v, bw, d) in [(l.vz, b, h - cover - dt - db / 2.0), (l.vy, h, b - cover - dt - db / 2.0)] {
            let nu = l.pu.max(0.0);
            let vc = (0.17 * (1.0 + nu / (14.0 * ag)) * sq * bw * d).min(0.42 * sq * bw * d);
            let vs = (v / 0.75 - vc).max(0.0);
            if vs > 0.0 {
                let s_req = 2.0 * ab_t * fyt * d / vs;
                s = s.min((s_req.min(d / 2.0) / 25.0).floor() * 25.0);
            }
            let cap = crate::codes::phi().shear * (vc + 2.0 * ab_t * fyt * d / s.max(25.0));
            let r = v / cap;
            match checks.iter_mut().find(|c| c.name == "shear") {
                Some(c) if c.ratio >= r => {}
                Some(c) => {
                    c.ratio = r;
                    c.detail = format!("{}: Vu {:.1} kN, φVn {:.1} kN", l.name, v / 1e3, cap / 1e3);
                }
                None => checks.push(ColumnCheck { name: "shear".into(), ratio: r, detail: format!("{}: Vu {:.1} kN, φVn {:.1} kN", l.name, v / 1e3, cap / 1e3) }),
            }
        }
    }
    if s < 75.0 {
        notes.push("ties closer than 75 mm: enlarge the section".into());
    }
    let (ratio, governs) = checks.iter().map(|c| (c.ratio, c.name.clone())).fold((0.0, String::new()), |a, x| if x.0 > a.0 { x } else { a });
    if ast > 0.04 * ag {
        notes.push(format!("ρ = {:.1} % is over 4 %: congested at the laps", ast / ag * 100.0));
    }
    if !dem.second_order {
        notes.push("braced (nonsway) column assumed; sway moments need a P-Delta analysis".into());
    }
    ColumnDesign {
        member,
        bim_id,
        b: b_m,
        h: h_m,
        bars: format!("{nbar}-{}", bar_name(db)),
        n_b,
        n_h,
        db,
        rho: ast / ag,
        ties: format!("{} @ {:.0} mm", bar_name(dt), s.max(25.0)),
        ok: ratio <= set.target_ratio + 1e-9 && pick.is_some(),
        checks,
        ratio,
        governs,
        notes,
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;

    #[test]
    fn pm_matches_the_engine_kernel() {
        // 300 x 300, 8-DB16 (3 per face), f'c 25, fy 420, bars 45 mm from the faces: the
        // engine's column kernel (strain compatibility, ACI 318-19 φ) gives φMn at each Pu
        let ab = 2.01e2;
        let rows = vec![(45.0, 3.0 * ab), (150.0, 2.0 * ab), (255.0, 3.0 * ab)];
        let area = 8.0 * 2.01e-4;
        let consts = [0.3, 0.3, 25000.0, 420000.0, 420.0, 0.85, 0.65 * 0.80 * (0.85 * 25000.0 * (0.09 - area) + 420000.0 * area), 0.85];
        let bars: Vec<f64> = vec![0.045, 3.0 * 2.01e-4, 0.150, 2.0 * 2.01e-4, 0.255, 3.0 * 2.01e-4];
        for pu_kn in [0.0, 400.0, 800.0, 1100.0] {
            let eng = dedaliano_engine::column_capacity_kernel::solve_column_capacity_batch(&consts, &bars, &[pu_kn]).unwrap()[0];
            let ours = column_phi_mn(300.0, 300.0, &rows, 25.0, 420.0, pu_kn * 1000.0) / 1e6;
            assert!((ours - eng).abs() / eng.max(1.0) < 0.01, "Pu {pu_kn}: ours {ours} engine {eng}");
        }
    }
}
