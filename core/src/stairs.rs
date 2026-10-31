// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Reinforced concrete stairs of a house, designed as one-way slabs (ACI 318-19 Ch. 7):
//! a flight is an inclined waist slab spanning horizontally between its supports (a floor
//! beam, a landing beam or the landing slab), carrying its waist along the slope, the
//! steps, the finishes and the live load; a landing is a level slab spanning between its
//! supports, carrying its own load and the flights' reactions spread over it. Main bars
//! along the span in the bottom, top bars at the supports, distribution bars across.

use serde::{Deserialize, Serialize};

use crate::model::{DesignSettings, Id};
use crate::rc::{phi_mn, slab_bars, RcMat};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StairKind {
    #[default]
    Flight,
    Landing,
}

/// A stair flight or landing to design.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Stair {
    pub id: Id,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bim_id: Option<String>,
    #[serde(default)]
    pub kind: StairKind,
    /// Width of the flight or landing, m (across the span).
    pub width: f64,
    /// Waist (flight) or thickness (landing), m.
    pub waist: f64,
    /// Riser and tread, m (flight).
    #[serde(default)]
    pub riser: f64,
    #[serde(default)]
    pub tread: f64,
    #[serde(default)]
    pub risers: usize,
    /// Horizontal span between the supports, m.
    pub span: f64,
    /// "simple" (M = wL²/8) or "continuous" (monolithic with beams at both ends: wL²/10 at
    /// midspan and over the supports).
    #[serde(default = "continuous")]
    pub fixity: String,
    /// Finishes, kPa; live load, kPa.
    #[serde(default)]
    pub dead: f64,
    #[serde(default = "three")]
    pub live: f64,
    /// Landing: unfactored dead and live reactions of the flights landing on it, kN per m of
    /// landing width (0 for a flight).
    #[serde(default)]
    pub flight_dead: f64,
    #[serde(default)]
    pub flight_live: f64,
    /// Bottom and top heights, m (for the picture and the report).
    #[serde(default)]
    pub z0: f64,
    #[serde(default)]
    pub z1: f64,
}
fn continuous() -> String {
    "continuous".into()
}
fn three() -> f64 {
    3.0
}

#[derive(Clone, Debug, Serialize)]
pub struct StairBars {
    pub location: String,
    /// kN·m per m, mm² per m.
    pub mu: f64,
    pub as_req: f64,
    pub as_prov: f64,
    pub bars: String,
    pub ratio: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StairDesign {
    pub stair: Id,
    pub bim_id: Option<String>,
    pub name: String,
    pub kind: StairKind,
    pub width: f64,
    pub waist: f64,
    pub span: f64,
    /// Dead (with self-weight), live and factored loads, kPa of plan.
    pub wd: f64,
    pub wl: f64,
    pub wu: f64,
    pub bars: Vec<StairBars>,
    pub vu: f64,
    pub phi_vc: f64,
    /// Least thickness for deflection (ACI 318-19 Table 7.3.1.1), mm.
    pub h_min: f64,
    /// Unfactored reactions at each support, kN per m of width: dead, live.
    pub reaction: [f64; 2],
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

/// Designs one flight or landing.
pub fn design(s: &Stair, mat: &RcMat, set: &DesignSettings, gamma: f64) -> StairDesign {
    let h = s.waist * 1000.0;
    let fc = mat.fc;
    let (fy, fy_round) = (mat.fy, mat.fyt);
    let cover = set.cover_slab * 1000.0;
    let db0 = set.slab_bars.first().copied().unwrap_or(9.0);
    let d = h - cover - db0 / 2.0;
    let mut notes = vec![];
    // loads per m² of plan
    let (wd, wl) = match s.kind {
        StairKind::Flight => {
            let pitch = if s.tread > 1e-6 { s.riser / s.tread } else { 0.0 };
            let waist = gamma * s.waist * (1.0 + pitch * pitch).sqrt();
            let steps = gamma * s.riser / 2.0;
            notes.push(format!("waist {:.2} kPa along the slope ({:.0}°), steps {:.2} kPa, finishes {:.2} kPa", waist, pitch.atan().to_degrees(), steps, s.dead));
            (waist + steps + s.dead, s.live)
        }
        StairKind::Landing => {
            // the flights' reactions spread over the landing's span
            let spread = if s.span > 1e-6 { 1.0 / s.span } else { 0.0 };
            notes.push(format!("own weight {:.2} kPa, finishes {:.2} kPa, flights {:.2} kN/m dead + {:.2} kN/m live spread over the {:.2} m span", gamma * s.waist, s.dead, s.flight_dead, s.flight_live, s.span));
            (gamma * s.waist + s.dead + s.flight_dead * spread, s.live + s.flight_live * spread)
        }
    };
    let wu = set.standard.gravity(wd, wl);
    let l = s.span;
    let cont = s.fixity != "simple";
    let (m_pos, m_neg) = if cont { (wu * l * l / 10.0, wu * l * l / 10.0) } else { (wu * l * l / 8.0, 0.0) };
    let as_min = 0.0018 * 1000.0 * h;
    let s_max_main = (3.0 * h).min(450.0);
    let s_max_dist = (5.0 * h).min(450.0);
    let mut bars = vec![];
    let (mut ratio, mut governs) = (0.0f64, String::new());
    let mut push = |loc: &str, mu: f64, s_max: f64, bars: &mut Vec<StairBars>| {
        let (as_req, as_prov, _, text, fb) = slab_bars(mu, d, as_min, s_max, &set.slab_bars, fc, fy, fy_round);
        let cap = phi_mn(as_prov, 1000.0, d, fc, fb) / 1e6;
        let r = if mu > 0.0 { mu / cap } else { 0.0 };
        if r > ratio {
            ratio = r;
            governs = format!("flexure, {loc}");
        }
        bars.push(StairBars { location: loc.into(), mu, as_req, as_prov, bars: text, ratio: r });
    };
    push("main (bottom, along the span)", m_pos, s_max_main, &mut bars);
    push("top at the supports", m_neg, s_max_main, &mut bars);
    push("distribution (across)", 0.0, s_max_dist, &mut bars);
    // one-way shear at the support, per m: φVc = 0.75 · 0.17 √f'c b d (22.5.5.1)
    let vu = wu * l / 2.0;
    let phi_vc = crate::codes::phi().shear * 0.17 * fc.sqrt() * 1000.0 * d / 1000.0;
    let rv = vu / phi_vc;
    if rv > ratio {
        ratio = rv;
        governs = "one-way shear".into();
    }
    // deflection: least thickness of a one-way slab, simply supported L/20, continuous L/24
    let h_min = l * 1000.0 / if cont { 24.0 } else { 20.0 };
    if h < h_min - 1e-9 {
        notes.push(format!("waist {h:.0} mm is under the L/{} = {h_min:.0} mm of Table 7.3.1.1: check the deflection or thicken it", if cont { 24 } else { 20 }));
    }
    if cont {
        notes.push("top bars run 0.25 L into the span from each support; at the kink where the flight meets the landing, the bottom bars are anchored past it (not bent round the inside corner)".into());
    } else {
        notes.push("top bars at the supports are the minimum steel over 0.25 L (simply supported)".into());
    }
    let reaction = [wd * l / 2.0, wl * l / 2.0];
    StairDesign {
        stair: s.id, bim_id: s.bim_id.clone(), name: s.name.clone(), kind: s.kind, width: s.width, waist: s.waist, span: l,
        wd, wl, wu, bars, vu, phi_vc, h_min, reaction, ratio, governs, ok: ratio <= 1.0 && h >= h_min - 1e-9, notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn house_flight_and_landing() {
        let set = DesignSettings::default();
        let mat = RcMat { fc: 24.0, ec: 23_000.0, fy: 390.0, fyt: 235.0 };
        let f = Stair { id: 1, name: "flight".into(), kind: StairKind::Flight, width: 1.0, waist: 0.15, riser: 0.17, tread: 0.28, risers: 10, span: 2.8, fixity: "continuous".into(), dead: 1.0, live: 3.0, ..Default::default() };
        let d = design(&f, &mat, &set, 24.0);
        // waist 24 × 0.15 × 1.17 = 4.21, steps 2.04, finishes 1.0 → wd ≈ 7.25 kPa
        assert!((d.wd - 7.25).abs() < 0.05, "{}", d.wd);
        assert!(d.ok, "{:?}", d);
        assert!(d.bars[0].bars.contains("@"), "{:?}", d.bars);
        assert!(d.reaction[0] > 9.0 && d.reaction[1] > 4.0);
        // a landing carrying two flights' reactions over a 2.2 m span, 150 thick
        let l = Stair { id: 2, kind: StairKind::Landing, width: 2.2, waist: 0.15, span: 2.2, fixity: "simple".into(), dead: 1.0, live: 3.0, flight_dead: d.reaction[0], flight_live: d.reaction[1], ..Default::default() };
        let dl = design(&l, &mat, &set, 24.0);
        assert!(dl.ok, "{:?}", dl);
        assert!(dl.wd > 4.6 + d.reaction[0] / 2.2 - 0.1);
        // a thin long flight fails on thickness
        let thin = Stair { waist: 0.10, span: 3.6, fixity: "simple".into(), ..f.clone() };
        let dt = design(&thin, &mat, &set, 24.0);
        assert!(!dt.ok && dt.h_min > 100.0);
    }
}
