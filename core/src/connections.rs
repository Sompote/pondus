// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Simple shear connections of steel beams (AISC 360-16, the conventional single-plate shear
//! tab of AISC Manual Part 10): one vertical line of bolts through the beam web and a plate
//! fillet-welded to the support, for the beam's end shear Vu with the eccentricity e from the
//! bolt line to the weld. The bolt group (elastic method, V and M = Vu·e) and the welds are
//! checked by the Stabileo engine (`postprocess::connection_check`); bolt bearing on the plate
//! and the web (J3.10, 2.4 d t Fu) and the plate's shear yielding and rupture (J4.2) by Pondus.
//! The bolts are added until everything passes or the plate no longer fits the web.
//! Also the floor vibration screen of the beams: f = 0.18 √(g/Δ) (AISC Design Guide 11, 3.3)
//! with Δ the beam's own deflection under D + its live share, against 3 Hz.

use dedaliano_engine::postprocess::connection_check::*;
use dedaliano_engine::postprocess::serviceability::*;
use serde::{Deserialize, Serialize};

use crate::model::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnSettings {
    /// Bolt diameter, mm; nominal shear stress Fnv, MPa (A325-N / F10T threads included: 372).
    pub bolt_d: f64,
    pub fnv: f64,
    /// Plate thickness, mm; plate steel Fy, Fu, MPa; pitch, edge distance, eccentricity, mm.
    pub plate_t: f64,
    pub plate_fy: f64,
    pub plate_fu: f64,
    pub pitch: f64,
    pub edge: f64,
    pub e: f64,
    /// Fillet weld size, mm, and electrode strength FEXX, MPa (E70: 482).
    pub weld: f64,
    pub fexx: f64,
}

impl Default for ConnSettings {
    fn default() -> Self {
        ConnSettings { bolt_d: 20.0, fnv: 372.0, plate_t: 10.0, plate_fy: 245.0, plate_fu: 400.0, pitch: 75.0, edge: 40.0, e: 75.0, weld: 6.0, fexx: 482.0 }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ConnectionDesign {
    pub member: Id,
    pub end: String,
    pub vu: f64,
    pub bolts: String,
    pub plate: String,
    pub weld: String,
    pub bolt_ratio: f64,
    pub bearing_ratio: f64,
    pub plate_ratio: f64,
    pub weld_ratio: f64,
    pub ratio: f64,
    pub ok: bool,
    pub notes: Vec<String>,
}

/// Designs the shear tab at an end of a steel H beam for its end shear vu (kN).
pub fn shear_tab(member: Id, end: &str, vu: f64, beam: (f64, f64, f64, f64, f64), beam_fu: f64, cs: &ConnSettings) -> ConnectionDesign {
    let (d, _bf, tw, tf, r) = beam;
    let clear = (d - 2.0 * tf - 2.0 * r) * 1000.0;
    let ab = std::f64::consts::PI * (cs.bolt_d / 1000.0).powi(2) / 4.0;
    let rn_bolt = cs.fnv * 1e6 * ab;
    let mut best: Option<ConnectionDesign> = None;
    // layouts: one bolt and a horizontal pair for shallow members (purlins, rafters), then
    // vertical rows of 2 to 12; each needs its depth inside the clear web
    // the edge distance shrinks for a shallow web, never below 1.5 d (AISC J3.4 / AISI E3.1)
    let edge = cs.edge.min(((clear - 2.0) / 2.0).max(1.5 * cs.bolt_d));
    let mut layouts: Vec<(usize, bool, f64, Vec<BoltPosition>)> = vec![
        (1, false, 2.0 * edge, vec![BoltPosition { x: 0.0, y: 0.0 }]),
        (2, true, 2.0 * edge, vec![BoltPosition { x: -cs.pitch / 2000.0, y: 0.0 }, BoltPosition { x: cs.pitch / 2000.0, y: 0.0 }]),
    ];
    for n in 2..=12usize {
        layouts.push((n, false, (n - 1) as f64 * cs.pitch + 2.0 * edge, (0..n).map(|k| BoltPosition { x: 0.0, y: (k as f64 - (n - 1) as f64 / 2.0) * cs.pitch / 1000.0 }).collect()));
    }
    for (n, row, depth, bolts) in layouts {
        if depth > clear {
            continue;
        }
        let g = BoltGroupData { connection_id: 1, bolts, rn_shear: rn_bolt, rn_bearing: None, phi: Some(0.75) };
        let f = BoltGroupForces { connection_id: 1, vx: 0.0, vy: vu * 1e3, m: Some(vu * 1e3 * cs.e / 1000.0) };
        let br = check_bolt_groups(&BoltGroupInput { groups: vec![g], forces: vec![f] }).remove(0);
        // bearing per bolt on the thinner of plate and web (J3-6a, deformation a concern)
        let t_min = (cs.plate_t / 1000.0).min(tw);
        let fu_min = if cs.plate_t / 1000.0 <= tw { cs.plate_fu } else { beam_fu };
        let bearing = br.max_bolt_force / (0.75 * 2.4 * cs.bolt_d / 1000.0 * t_min * fu_min * 1e6);
        // plate shear: gross yielding φ 1.0 x 0.6 Fy Agv, net rupture φ 0.75 x 0.6 Fu Anv
        let agv = depth / 1000.0 * cs.plate_t / 1000.0;
        let anv = (depth - n as f64 * (cs.bolt_d + 2.0)) / 1000.0 * cs.plate_t / 1000.0;
        let plate = (vu * 1e3 / (crate::codes::phi().conn_shear_yield * 0.6 * cs.plate_fy * 1e6 * agv)).max(vu * 1e3 / (0.75 * 0.6 * cs.plate_fu * 1e6 * anv));
        // the welds: two fillets down the plate's height, V and M = V e
        let h = depth / 1000.0;
        let seg = |x: f64| WeldSegment { x1: x, y1: -h / 2.0, x2: x, y2: h / 2.0, size: cs.weld / 1000.0, end_loaded: Some(false) };
        let wg = WeldGroupData { connection_id: 1, segments: vec![seg(-cs.plate_t / 2000.0), seg(cs.plate_t / 2000.0)], fexx: cs.fexx * 1e6, phi: Some(0.75) };
        let wf = WeldGroupForces { connection_id: 1, vx: 0.0, vy: vu * 1e3, m: Some(vu * 1e3 * cs.e / 1000.0) };
        let wr = check_weld_groups(&WeldGroupInput { groups: vec![wg], forces: vec![wf] }).remove(0);
        let ratio = br.unity_ratio.max(bearing).max(plate).max(wr.unity_ratio);
        let d = ConnectionDesign {
            member,
            end: end.into(),
            vu,
            bolts: if n == 1 { format!("1-M{:.0} (Fnv {:.0} MPa)", cs.bolt_d, cs.fnv) } else if row { format!("2-M{:.0} in a row (Fnv {:.0} MPa) at {:.0} mm", cs.bolt_d, cs.fnv, cs.pitch) } else { format!("{n}-M{:.0} (Fnv {:.0} MPa) at {:.0} mm", cs.bolt_d, cs.fnv, cs.pitch) },
            plate: format!("PL {:.0} x {:.0} mm", cs.plate_t, depth),
            weld: format!("2 x {:.0} mm fillet, E{:.0}", cs.weld, cs.fexx / 6.895),
            bolt_ratio: br.unity_ratio,
            bearing_ratio: bearing,
            plate_ratio: plate,
            weld_ratio: wr.unity_ratio,
            ratio,
            ok: ratio <= 1.0,
            notes: if edge < cs.edge - 1e-9 { vec![format!("edge distance {edge:.0} mm (the web's clear depth is {clear:.0} mm)")] } else { vec![] },
        };
        let done = d.ok;
        best = Some(d);
        if done {
            break;
        }
    }
    best.unwrap_or(ConnectionDesign { member, end: end.into(), vu, bolts: "—".into(), plate: "—".into(), weld: "—".into(), bolt_ratio: f64::INFINITY, bearing_ratio: 0.0, plate_ratio: 0.0, weld_ratio: 0.0, ratio: f64::INFINITY, ok: false, notes: vec![format!("the web's clear depth {clear:.0} mm takes no two bolts at this pitch")] })
}

#[derive(Clone, Debug, Serialize)]
pub struct Vibration {
    pub member: Id,
    pub deflection: f64,
    pub frequency: f64,
    pub ratio: f64,
    pub ok: bool,
}

/// The vibration screen of beams: (member, span, own deflection under D + live share, m).
pub fn vibration(beams: &[(Id, f64, f64)], min_hz: f64) -> Vec<Vibration> {
    let members: Vec<ServiceabilityMember> = beams
        .iter()
        .map(|(id, span, d)| ServiceabilityMember { element_id: *id, span: *span, max_deflection: d.abs(), criterion: DeflectionCriterion::Absolute(1e9), natural_frequency: Some(0.18 * (9.81 / d.abs().max(1e-6)).sqrt()), min_frequency: Some(min_hz), description: None })
        .collect();
    check_serviceability(&ServiceabilityInput { members })
        .into_iter()
        .zip(beams)
        .map(|(r, (id, _, d))| {
            let f = 0.18 * (9.81 / d.abs().max(1e-6)).sqrt();
            Vibration { member: *id, deflection: *d, frequency: f, ratio: r.vibration_ratio.unwrap_or(0.0), ok: r.vibration_ok.unwrap_or(true) }
        })
        .collect()
}
