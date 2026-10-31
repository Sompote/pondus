// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Structural walls (shear walls). A wall is analysed by the wide-column analogy (MacLeod
//! 1971; the "pier" of building programs): a member on its centre line between consecutive
//! levels carries the wall's section (t × lw, strong axis in the plane of the wall, shear
//! deformation with 5/6 of the area), and at every level two rigid arms run from the centre
//! line to the wall's ends. Beams and slabs that meet the wall anywhere along its length meet
//! an arm (members are split at nodes lying on them), so they connect to the wall as they do in
//! the building. The arms carry no weight and are never designed.
//!
//! Each pier (one wall, one storey) is designed to ACI 318-19 Chapter 11, and to 18.10 when
//! the model has an earthquake analysis (the walls are then taken as special structural walls):
//! - in-plane shear, Vn = Acv(αc λ √f'c + ρt fy) ≤ 0.66 √f'c Acv (11.5.4.3, 11.5.4.4),
//!   αc = 0.25 for hw/lw ≤ 1.5 and 0.17 for hw/lw ≥ 2.0, φ = 0.75;
//! - the minimum distributed steel of Table 11.6.1, or 11.6.2 when Vu > 0.5 φ αc λ √f'c Acv
//!   (and 18.10.2.1 for special walls), spacing ≤ min(3t, 450 mm), two curtains when
//!   t > 250 mm (11.7.2.3), when Vu > 0.17 λ √f'c Acv or when hw/lw ≥ 2 in a special wall
//!   (18.10.2.2);
//! - axial force with in-plane bending by strain compatibility on the wall section with its
//!   vertical bars distributed along the length (no concentrated boundary bars: conservative);
//! - axial force with out-of-plane bending, the same on a strip through the thickness;
//! - for special walls, the stress test of 18.10.6.3 (σ = Pu/Ag + Mu lw/2Ig > 0.2 f'c) for
//!   special boundary elements, with the length of 18.10.6.4(a); their confinement is listed
//!   for the engineer to detail, not designed.

use serde::Serialize;

use crate::model::*;
use crate::rc;
use crate::sections::bar_area;

/// How much stiffer the rigid arms are than the wall they belong to. The arms' section is the
/// wall's own (t × lw) and their modulus this many times the wall's, so they are rigid against
/// the wall without the absolute, model-independent stiffness (E 2.5e7 MPa on I = 1 m⁴) they
/// had before, which put a stiffness contrast of order 10⁹ into K and lost precision silently
/// when a slender wall was coupled to soft frames.
pub const ARM_FACTOR: f64 = 1000.0;

fn arm_material(m: &mut Model, wall_material: Id) -> Id {
    let base = m.material(wall_material).map(|x| x.e).unwrap_or(25_000.0);
    let e = ARM_FACTOR * base;
    let name = format!("Rigid arm (wall, E {e:.0} MPa)");
    if let Some(x) = m.materials.iter().find(|x| x.name == name) {
        return x.id;
    }
    let id = m.next_material_id();
    m.materials.push(Material { id, name, kind: MaterialKind::Other, e, nu: 0.2, gamma: 0.0, fc: 0.0, fy: 0.0, fu: 0.0, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 });
    id
}

/// The arms' section: the wall's own in-plane inertia about both axes, so that they are
/// rigid in and out of the wall's plane and in torsion.
fn arm_section(m: &mut Model, t: f64, lw: f64) -> Id {
    let name = format!("Rigid arm (wall {:.0} x {:.0} mm)", t * 1e3, lw * 1e3);
    if let Some(x) = m.sections.iter().find(|x| x.name == name) {
        return x.id;
    }
    let i = t * lw.powi(3) / 12.0;
    let id = m.sections.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    m.sections.push(Section { id, name, shape: Shape::General { a: t * lw, iy: i, iz: i, j: i, shear: None } });
    id
}

/// Whether a wall's arms are of the current kind (models saved before ARM_FACTOR carry the
/// old absolute arms, which `sync` then rebuilds).
fn arms_ok(m: &Model, w: &Wall) -> bool {
    m.members.iter().filter(|x| x.role == Role::WallLink(w.id)).all(|x| m.section(x.section).is_some_and(|s| s.name.starts_with("Rigid arm (wall ")) && m.material(x.material).is_some_and(|q| q.name.starts_with("Rigid arm (wall, E")))
}

/// The section of a wall's piers: a rectangle t × lw named after the wall.
fn pier_section(m: &mut Model, t: f64, lw: f64) -> Id {
    let name = format!("Wall {:.0} x {:.0} mm", t * 1e3, lw * 1e3);
    if let Some(x) = m.sections.iter().find(|x| x.name == name) {
        return x.id;
    }
    let id = m.sections.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    m.sections.push(Section { id, name, shape: Shape::Rect { b: t, h: lw } });
    id
}

/// The levels a wall's piers run between: its base and top, the levels it was built with, and
/// every level of the model (a node's height) in between that is not one of its own nodes.
pub fn levels_for(m: &Model, w: &Wall) -> Vec<f64> {
    let own: std::collections::HashSet<Id> = m.members.iter().filter(|x| x.role.wall() == Some(w.id)).flat_map(|x| [x.i, x.j]).collect();
    let mut z: Vec<f64> = m.nodes.iter().filter(|n| !own.contains(&n.id)).map(|n| (n.z * 1000.0).round() / 1000.0).filter(|z| *z > w.z0 + 1e-3 && *z < w.z1 - 1e-3).collect();
    z.push(w.z0);
    z.push(w.z1);
    // the levels already built stay (a wall alone has no other nodes to find them by)
    z.extend(w.levels.iter().copied().filter(|z| *z >= w.z0 - 1e-3 && *z <= w.z1 + 1e-3));
    z.sort_by(f64::total_cmp);
    // levels closer than the node tolerance would share a node and make a pier of zero length
    z.dedup_by(|a, b| (*a - *b).abs() < crate::model::TOL);
    z
}

/// Builds (or rebuilds when the model's levels changed) the piers and arms of every wall.
/// Returns whether anything changed.
pub fn sync(m: &mut Model) -> bool {
    let mut changed = false;
    let walls = m.walls.clone();
    for w in &walls {
        let lv = levels_for(m, w);
        let piers = m.members.iter().filter(|x| x.role == Role::WallPier(w.id)).count();
        let has = piers > 0 && piers + 1 == lv.len();
        let sec_ok = m.members.iter().filter(|x| x.role == Role::WallPier(w.id)).all(|x| m.section(x.section).is_some_and(|s| matches!(s.shape, Shape::Rect { b, h } if (b - w.t).abs() < 1e-6 && (h - w.length()).abs() < 1e-6)));
        if has && lv == w.levels && sec_ok && arms_ok(m, w) {
            continue;
        }
        build(m, w, &lv);
        if let Some(x) = m.walls.iter_mut().find(|x| x.id == w.id) {
            x.levels = lv;
        }
        changed = true;
    }
    // the members of walls that no longer exist
    let gone: Vec<Id> = m.members.iter().filter(|x| x.role.wall().is_some_and(|w| !m.walls.iter().any(|y| y.id == w))).map(|x| x.id).collect();
    if !gone.is_empty() {
        m.delete_members(&gone);
        changed = true;
    }
    // arm sections and materials no member uses any more (older arms, removed walls)
    let used_s: std::collections::HashSet<Id> = m.members.iter().map(|x| x.section).collect();
    let used_m: std::collections::HashSet<Id> = m.members.iter().map(|x| x.material).collect();
    let (ns, nm) = (m.sections.len(), m.materials.len());
    m.sections.retain(|x| !x.name.starts_with("Rigid arm (wall") || used_s.contains(&x.id));
    m.materials.retain(|x| !x.name.starts_with("Rigid arm (wall") || used_m.contains(&x.id));
    changed |= ns != m.sections.len() || nm != m.materials.len();
    changed
}

fn build(m: &mut Model, w: &Wall, levels: &[f64]) {
    let old: Vec<Id> = m.members.iter().filter(|x| x.role.wall() == Some(w.id)).map(|x| x.id).collect();
    if !old.is_empty() {
        m.delete_members(&old);
    }
    let lw = w.length();
    if lw < 0.1 || levels.len() < 2 {
        return;
    }
    // beams lying in the wall (on its line, within its length and height) are replaced by it
    let on_wall = |p: [f64; 3]| -> bool {
        let (dx, dy) = (w.b[0] - w.a[0], w.b[1] - w.a[1]);
        let s = ((p[0] - w.a[0]) * dx + (p[1] - w.a[1]) * dy) / (lw * lw);
        let off = ((p[0] - w.a[0]) * dy - (p[1] - w.a[1]) * dx).abs() / lw;
        s > -1e-6 && s < 1.0 + 1e-6 && off < 1e-3 && p[2] > w.z0 - 1e-3 && p[2] < w.z1 + 1e-3
    };
    let inside: Vec<Id> = m.members.iter().filter(|x| x.role.is_frame() && m.kind(x) == MemberKind::Beam).filter(|x| [x.i, x.j].iter().all(|n| m.node(*n).is_some_and(|n| on_wall(n.p())))).map(|x| x.id).collect();
    if !inside.is_empty() {
        m.delete_members(&inside);
    }
    let sec = pier_section(m, w.t, lw);
    let (arm_sec, arm_mat) = (arm_section(m, w.t, lw), arm_material(m, w.material));
    let c = [(w.a[0] + w.b[0]) / 2.0, (w.a[1] + w.b[1]) / 2.0];
    // a column of roll 0 bends about its strong axis in the x-z plane: the wall's strong axis
    // turns with the wall
    let roll = (w.b[1] - w.a[1]).atan2(w.b[0] - w.a[0]).to_degrees();
    let push = |m: &mut Model, i: Id, j: Id, section: Id, material: Id, kind: MemberKind, role: Role, roll: f64| {
        let id = m.next_member_id();
        m.members.push(Member { id, i, j, section, material, kind, hinge_i: false, hinge_j: false, lb: None, k: None, roll, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: w.bim_id.clone(), tension: None, role, group: None });
    };
    for (k, z) in levels.iter().enumerate() {
        let nc = m.node_at([c[0], c[1], *z]);
        if k + 1 < levels.len() {
            let nt = m.node_at([c[0], c[1], levels[k + 1]]);
            if nt != nc {
                push(m, nc, nt, sec, w.material, MemberKind::Column, Role::WallPier(w.id), roll);
            }
        }
        for e in [w.a, w.b] {
            let ne = m.node_at([e[0], e[1], *z]);
            if ne != nc {
                push(m, nc, ne, arm_sec, arm_mat, MemberKind::Beam, Role::WallLink(w.id), 0.0);
            }
        }
    }
}

/// The sections of wall piers (for the shear deformation the analysis gives them).
pub fn pier_sections(m: &Model) -> std::collections::HashSet<Id> {
    m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).map(|x| x.section).collect()
}

// ------------------------------------------------------------------ design

#[derive(Clone, Debug, Serialize)]
pub struct WallCheck {
    pub name: String,
    pub ratio: f64,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WallDesign {
    pub wall: Id,
    pub name: String,
    pub member: Id,
    pub bim_id: Option<String>,
    pub z0: f64,
    pub z1: f64,
    pub t: f64,
    pub lw: f64,
    pub curtains: usize,
    pub vertical: String,
    pub horizontal: String,
    pub rho_l: f64,
    pub rho_t: f64,
    /// Largest factored compression, in-plane moment and in-plane shear (kN, kN·m).
    pub pu: f64,
    pub mu: f64,
    pub vu: f64,
    pub phi_vn: f64,
    pub special: bool,
    pub boundary: Option<String>,
    pub checks: Vec<WallCheck>,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

/// The demand on one pier: per ULS combination its stations (x, N tension +, Vy, Vz, My, Mz).
pub type Stations = Vec<(String, Vec<(f64, f64, f64, f64, f64, f64)>)>;

/// Designs one pier of a wall.
#[allow(clippy::too_many_arguments)]
pub fn design_pier(w: &Wall, member: &Member, z0: f64, z1: f64, fc: f64, fy: f64, uls: &Stations, special: bool, set: &DesignSettings) -> WallDesign {
    let (t, lw) = (w.t * 1e3, w.length() * 1e3);
    let hw = (w.z1 - w.z0) * 1e3;
    let acv = t * lw;
    let sq = fc.sqrt();
    let ratio_hl = hw / lw.max(1.0);
    let alpha_c = if ratio_hl <= 1.5 { 0.25 } else if ratio_hl >= 2.0 { 0.17 } else { 0.25 - (ratio_hl - 1.5) / 0.5 * 0.08 };
    let phi_v = crate::codes::phi().shear;
    // the demands (N, N·mm), compression +
    let st: Vec<(String, f64, f64, f64, f64, f64)> = uls.iter().flat_map(|(c, s)| s.iter().map(move |s| (c.clone(), -s.1 * 1e3, s.3.abs() * 1e3, s.2.abs() * 1e3, s.4.abs() * 1e6, s.5.abs() * 1e6))).collect();
    let vu = st.iter().map(|s| s.2).fold(0.0, f64::max);
    let pu_max = st.iter().map(|s| s.1).fold(0.0, f64::max);
    let mu_max = st.iter().map(|s| s.4).fold(0.0, f64::max);
    let mut notes = vec![];
    let high_shear = vu > 0.5 * phi_v * alpha_c * sq * acv;
    let curtains = if t > 250.0 || vu > 0.17 * sq * acv || (special && ratio_hl >= 2.0) { 2 } else { 1 };
    let cover = set.cover_beam.max(0.02) * 1e3;
    let s_cap = (3.0 * t).min(450.0);
    // horizontal steel: shear first, then the minimum
    let small_bar = |db: f64| db <= 16.0 && fy >= 420.0;
    let rho_t_min = |db: f64| if high_shear || special { 0.0025 } else if small_bar(db) { 0.0020 } else { 0.0025 };
    let vn_max = 0.66 * sq * acv;
    let mut checks: Vec<WallCheck> = vec![];
    let mut horizontal = String::new();
    let mut rho_t = 0.0;
    let mut phi_vn = 0.0;
    for db in [10.0, 12.0, 16.0, 20.0, 25.0] {
        let need = ((vu / phi_v - alpha_c * sq * acv) / (fy * acv)).max(rho_t_min(db));
        let ab = bar_area(db) * 1e6;
        let s = ((curtains as f64 * ab / (need * t)) / 25.0).floor() * 25.0;
        if s >= 100.0 || db >= 25.0 {
            let s = s.clamp(75.0, s_cap);
            rho_t = curtains as f64 * ab / (s * t);
            horizontal = format!("{} × DB{:.0} @ {:.0} mm", curtains, db, s);
            break;
        }
    }
    let vn = (alpha_c * sq + rho_t * fy) * acv;
    phi_vn = phi_v * vn.min(vn_max).max(phi_vn);
    checks.push(WallCheck { name: "in-plane shear (11.5.4)".into(), ratio: vu / phi_vn.max(1.0), detail: format!("Vu {:.0} kN, φVn = {phi_v:.2} Acv(αc √f'c + ρt fy) = {:.0} kN (αc {alpha_c:.2}, ρt {:.4}; limit 0.66 √f'c Acv = {:.0} kN)", vu / 1e3, phi_vn / 1e3, rho_t, phi_v * vn_max / 1e3) });
    // vertical steel: the least that passes axial with in-plane and out-of-plane bending
    let rho_l_min = |db: f64| {
        let base: f64 = if special || high_shear { 0.0025 } else if small_bar(db) { 0.0012 } else { 0.0015 };
        if high_shear { base.max(0.0025 + 0.5 * (2.5 - ratio_hl) * (rho_t - 0.0025)) } else { base }
    };
    let rows_of = |db: f64, s: f64| -> Vec<(f64, f64)> {
        let ab = bar_area(db) * 1e6 * curtains as f64;
        let n = (((lw - 2.0 * cover) / s).floor() as usize + 1).max(2);
        (0..n).map(|k| (cover + (lw - 2.0 * cover) * k as f64 / (n - 1) as f64, ab)).collect()
    };
    let out_rows = |db: f64, as_total: f64| -> Vec<(f64, f64)> {
        let d = cover + db / 2.0;
        if curtains == 2 { vec![(d, as_total / 2.0), (t - d, as_total / 2.0)] } else { vec![(t / 2.0, as_total)] }
    };
    let eval = |db: f64, s: f64| -> (f64, f64, String, String) {
        let rows = rows_of(db, s);
        let ast: f64 = rows.iter().map(|r| r.1).sum();
        let (mut r_in, mut r_out) = (0.0f64, 0.0f64);
        let (mut d_in, mut d_out) = (String::new(), String::new());
        for (c, pu, _, _, my, mz) in &st {
            let phi_mn = rc::column_phi_mn(t, lw, &rows, fc, fy, *pu);
            let r = if phi_mn > 0.0 { my / phi_mn } else { f64::INFINITY };
            if r > r_in {
                r_in = r;
                d_in = format!("{c}: Pu {:.0} kN, Mu {:.0} kN·m, φMn {:.0} kN·m", pu / 1e3, my / 1e6, phi_mn / 1e6);
            }
            let phi_mo = rc::column_phi_mn(lw, t, &out_rows(db, ast), fc, fy, *pu);
            let ro = if phi_mo > 0.0 { mz / phi_mo } else { f64::INFINITY };
            if ro > r_out {
                r_out = ro;
                d_out = format!("{c}: Pu {:.0} kN, Mu {:.1} kN·m, φMn {:.0} kN·m", pu / 1e3, mz / 1e6, phi_mo / 1e6);
            }
        }
        (r_in, r_out, d_in, d_out)
    };
    let mut best: Option<(f64, f64, f64, f64, String, String)> = None;
    'search: for db in [12.0, 16.0, 20.0, 25.0, 28.0, 32.0] {
        let ab = bar_area(db) * 1e6;
        let s_min_rho = (curtains as f64 * ab / (rho_l_min(db) * t) / 25.0).floor() * 25.0;
        let mut s = s_min_rho.min(s_cap);
        while s >= 100.0 {
            let (ri, ro, di, dout) = eval(db, s);
            let ok = ri <= set.target_ratio && ro <= set.target_ratio;
            if best.is_none() || ok {
                best = Some((db, s, ri, ro, di, dout));
            }
            if ok {
                break 'search;
            }
            s -= 25.0;
        }
    }
    let (db_v, s_v, r_in, r_out, d_in, d_out) = best.unwrap_or((12.0, s_cap, f64::INFINITY, f64::INFINITY, String::new(), String::new()));
    let rho_l = curtains as f64 * bar_area(db_v) * 1e6 / (s_v * t);
    let vertical = format!("{} × DB{:.0} @ {:.0} mm", curtains, db_v, s_v);
    checks.push(WallCheck { name: "axial + in-plane bending (22.4, strain compatibility)".into(), ratio: r_in, detail: d_in });
    checks.push(WallCheck { name: "axial + out-of-plane bending".into(), ratio: r_out, detail: d_out });
    let (pc, _) = rc::column_p0(t * lw, rows_of(db_v, s_v).iter().map(|r| r.1).sum(), fc, fy);
    let _ = pc;
    // special boundary elements by the stress method (18.10.6.3)
    let mut boundary = None;
    if special {
        let ig = t * lw.powi(3) / 12.0;
        let sigma = st.iter().map(|s| s.1 / acv + s.4 * (lw / 2.0) / ig).fold(0.0, f64::max);
        if sigma > 0.2 * fc {
            // c at the factored axial force with the nominal moment strength
            let rows = rows_of(db_v, s_v);
            let (mut lo, mut hi) = (1e-3 * lw, 1.5 * lw);
            for _ in 0..80 {
                let mid = (lo + hi) / 2.0;
                if rc::section_at(mid, t, lw, &rows, fc, fy).0 < pu_max {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let c = (lo + hi) / 2.0;
            let len = (c - 0.1 * lw).max(c / 2.0);
            boundary = Some(format!("special boundary elements required (σ = {:.1} MPa > 0.2 f'c = {:.1} MPa): {:.0} mm from each end (c = {:.0} mm), confinement to 18.10.6.4, until σ < 0.15 f'c", sigma, 0.2 * fc, len, c));
        }
    }
    if special {
        notes.push("designed as a special structural wall (18.10) because the model has an earthquake analysis; ties of the boundary elements and the capacity-design shear amplification of 18.10.3.1 are for the engineer to detail".into());
    }
    notes.push("vertical bars are distributed evenly along the length; concentrating bars at the ends (boundary bars) raises the moment capacity".into());
    if curtains == 2 {
        notes.push("two curtains of reinforcement".into());
    }
    let (ratio, governs) = checks.iter().map(|c| (c.ratio, c.name.clone())).fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a });
    if vu / phi_v > vn_max {
        notes.push(format!("the shear exceeds 0.66 √f'c Acv: the wall must be thicker (Vu/φ = {:.0} kN > {:.0} kN)", vu / phi_v / 1e3, vn_max / 1e3));
    }
    WallDesign {
        wall: w.id,
        name: w.name.clone(),
        member: member.id,
        bim_id: w.bim_id.clone(),
        z0,
        z1,
        t: w.t,
        lw: w.length(),
        curtains,
        vertical,
        horizontal,
        rho_l,
        rho_t,
        pu: pu_max / 1e3,
        mu: mu_max / 1e6,
        vu: vu / 1e3,
        phi_vn: phi_vn / 1e3,
        special,
        boundary,
        checks,
        ratio,
        governs,
        ok: ratio <= set.target_ratio + 1e-9,
        notes,
    }
}
