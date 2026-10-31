// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Slabs as plates: each panel meshed into MITC4 shell quads (the Stabileo engine's), joined
//! to the beams and columns at shared nodes, and designed from the element moments and shears.
//!
//! Mesh: the grid lines of a panel are its edges, the x and y of every node of the model lying
//! in it (columns, beam ends, crossing beams) and as many lines between as keep the elements
//! within the mesh size. Nodes are ordered anticlockwise in plan, so each quad's local x is the
//! global x and its normal points up: mx, my, mxy are moments per metre about the global axes.
//!
//! Design (Wood 1968, Armer 1968): the bottom moments mx + |mxy|, my + |mxy| and the top
//! moments mx − |mxy|, my − |mxy| (each corrected when one comes out with the wrong sign),
//! enveloped over the ULS combinations, give the bars of each direction and face: the bottom
//! bars from the middle half of the panel and the top bars from the quarter next to each
//! edge. One-way shear: the transverse shears qx, qy against φVc, outside d from the
//! supports. A panel on columns or supports with no beams (a flat plate, a pile cap, a mat on a
//! Winkler subgrade) takes both faces' bars over the whole panel and is checked for punching
//! at every column and support (ACI 318-19 22.6).

use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::rc::{self, RcMat, SlabDesign, SlabStrip};

/// How slabs are analysed and designed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlabMethod {
    /// Load to the beams by the 45° lines; moments by ACI 318-63 Method 2 / ACI 318-19 6.5.
    #[default]
    Coefficients,
    /// Plate finite elements analysed with the frame; bars from the element moments.
    Plate,
}

/// A panel's mesh: its grid lines and z.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub slab: Id,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub z: f64,
}

impl Mesh {
    pub fn point(&self, i: usize, j: usize) -> [f64; 3] {
        [self.xs[i], self.ys[j], self.z]
    }
    pub fn cells(&self) -> usize {
        (self.xs.len() - 1) * (self.ys.len() - 1)
    }
}

fn lines(lo: f64, hi: f64, mut fixed: Vec<f64>, size: f64) -> Vec<f64> {
    fixed.push(lo);
    fixed.push(hi);
    fixed.retain(|x| *x >= lo - 1e-9 && *x <= hi + 1e-9);
    fixed.sort_by(f64::total_cmp);
    fixed.dedup_by(|a, b| (*a - *b).abs() < 0.02);
    let mut out = vec![fixed[0]];
    for w in fixed.windows(2) {
        let n = ((w[1] - w[0]) / size).ceil().max(1.0) as usize;
        for k in 1..=n {
            out.push(w[0] + (w[1] - w[0]) * k as f64 / n as f64);
        }
    }
    out
}

pub fn meshes(m: &Model) -> Vec<Mesh> {
    let size = m.design.slab_mesh.clamp(0.1, 5.0);
    m.slabs
        .iter()
        .filter(|s| m.plate_slab(s))
        .map(|s| {
            let (lo, hi) = (s.lo(), s.hi());
            let inside: Vec<&Node> = m.nodes.iter().filter(|n| (n.z - s.z).abs() < 0.05 && n.x >= lo.0 - 1e-6 && n.x <= hi.0 + 1e-6 && n.y >= lo.1 - 1e-6 && n.y <= hi.1 + 1e-6).collect();
            Mesh { slab: s.id, xs: lines(lo.0, hi.0, inside.iter().map(|n| n.x).collect(), size), ys: lines(lo.1, hi.1, inside.iter().map(|n| n.y).collect(), size), z: s.z }
        })
        .collect()
}

/// The moments and shears of one plate element (kN·m/m, kN/m), at its centre.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PlateResult {
    pub slab: Id,
    /// Centre and size of the element, m.
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub mx: f64,
    pub my: f64,
    pub mxy: f64,
    pub qx: f64,
    pub qy: f64,
    /// Vertical displacement of its centre (mean of its nodes), m.
    pub uz: f64,
}

/// Wood–Armer design moments of (mx, my, mxy): (bottom x, bottom y, top x, top y), each ≥ 0.
pub fn wood_armer(mx: f64, my: f64, mxy: f64) -> (f64, f64, f64, f64) {
    let a = mxy.abs();
    // bottom (sagging positive)
    let (mut bx, mut by) = (mx + a, my + a);
    if bx < 0.0 {
        bx = 0.0;
        by = if mx.abs() > 1e-12 { my + mxy * mxy / mx.abs() } else { my };
    } else if by < 0.0 {
        by = 0.0;
        bx = if my.abs() > 1e-12 { mx + mxy * mxy / my.abs() } else { mx };
    }
    // top (hogging negative)
    let (mut tx, mut ty) = (mx - a, my - a);
    if tx > 0.0 {
        tx = 0.0;
        ty = if mx.abs() > 1e-12 { my - mxy * mxy / mx.abs() } else { my };
    } else if ty > 0.0 {
        ty = 0.0;
        tx = if my.abs() > 1e-12 { mx - mxy * mxy / my.abs() } else { mx };
    }
    (bx.max(0.0), by.max(0.0), (-tx).max(0.0), (-ty).max(0.0))
}

/// A place where a concentrated force passes through a plate: a column standing on or under
/// it, or a support (a pile, a pedestal) inside it, with its largest factored force.
#[derive(Clone, Debug, Serialize)]
pub struct Punch {
    pub x: f64,
    pub y: f64,
    /// Loaded area, m: a column's section, or the slab's `support_size` for a support.
    pub c1: f64,
    pub c2: f64,
    pub what: String,
    /// The largest factored force over the ULS combinations, kN.
    pub vu: f64,
    /// Per ULS combination: the shear the slab hands to the column or support (kN) and the
    /// unbalanced moments it transfers (kN·m), bending in the x–z plane and in the y–z plane.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<[f64; 3]>,
}

/// The punching points of a slab: the columns whose end lies on the panel and the supports
/// inside it (piles under a cap, pedestals on a mat), each with its largest factored force.
pub fn punch_points(model: &Model, s: &Slab, uls: &[(String, crate::analysis::CaseResult)]) -> Vec<Punch> {
    let (lo, hi) = (s.lo(), s.hi());
    let on = |n: &Node| (n.z - s.z).abs() < 0.05 && n.x >= lo.0 - 1e-6 && n.x <= hi.0 + 1e-6 && n.y >= lo.1 - 1e-6 && n.y <= hi.1 + 1e-6;
    // a node with a beam (any member lying at the slab's level) hands its force to the beam,
    // not through the slab: only columns and supports met by the slab alone punch it
    let at_level = |id: Id| model.node(id).is_some_and(|n| (n.z - s.z).abs() < 0.05);
    let has_beam = |id: Id| model.members.iter().any(|x| (x.i == id || x.j == id) && at_level(x.i) && at_level(x.j));
    let mut out = vec![];
    // the columns meeting the panel at each node with no beam: the slab hands each the
    // difference of the axial forces of the columns below and above (not a column's whole
    // force, which carries the floors over it too), and the unbalanced moment the columns
    // above and below take together (their end moments summed, as under gravity, where they
    // bend in double curvature; conservative under sway)
    let mut at_node: Vec<(Id, Vec<(&Member, bool, bool)>)> = vec![]; // node, (column, node is its i end, column is below)
    for m in model.members.iter().filter(|m| model.kind(m) == MemberKind::Column) {
        let (Some(a), Some(b)) = (model.node(m.i), model.node(m.j)) else { continue };
        let at_i = on(a);
        if !at_i && !on(b) {
            continue;
        }
        let (n, other) = if at_i { (a, b) } else { (b, a) };
        if has_beam(n.id) {
            continue;
        }
        let below = other.z < n.z;
        match at_node.iter_mut().find(|(id, _)| *id == n.id) {
            Some((_, v)) => v.push((m, at_i, below)),
            None => at_node.push((n.id, vec![(m, at_i, below)])),
        }
    }
    for (nid, cols) in &at_node {
        let Some(n) = model.node(*nid) else { continue };
        // the loaded area from the column below (else above): c1 along x, c2 along y; a
        // column of roll 0 has its depth h in the x–z plane
        let (m0, _, _) = cols.iter().find(|c| c.2).or(cols.first()).copied().unwrap();
        let turned = m0.roll.to_radians().sin().abs() > std::f64::consts::FRAC_1_SQRT_2;
        let (c1, c2) = match model.section(m0.section).map(|x| x.shape.clone()) {
            Some(Shape::Rect { b, h }) => if turned { (b, h) } else { (h, b) },
            Some(shape) => {
                let a = Section { id: 0, name: String::new(), shape }.props().a.max(1e-6);
                (a.sqrt(), a.sqrt())
            }
            None => (0.3, 0.3),
        };
        let mut cases = vec![];
        for (_, r) in uls {
            let (mut v, mut mxz, mut myz) = (0.0, 0.0, 0.0);
            for (m, at_i, below) in cols {
                let Some(st) = r.member(m.id).and_then(|mr| if *at_i { mr.stations.first() } else { mr.stations.last() }) else { continue };
                // compression of the column below, less that of the column above
                v += if *below { -st.n } else { st.n };
                let (c, sn) = (m.roll.to_radians().cos().abs(), m.roll.to_radians().sin().abs());
                mxz += st.my.abs() * c + st.mz.abs() * sn;
                myz += st.my.abs() * sn + st.mz.abs() * c;
            }
            cases.push([v.abs(), mxz, myz]);
        }
        let vu = cases.iter().map(|c| c[0]).fold(0.0, f64::max);
        let what = format!("column {}", cols.iter().map(|c| c.0.id.to_string()).collect::<Vec<_>>().join("/"));
        out.push(Punch { x: n.x, y: n.y, c1, c2, what, vu, cases });
    }
    for sp in &model.supports {
        let Some(n) = model.node(sp.node) else { continue };
        if !on(n) || has_beam(n.id) {
            continue;
        }
        let c = if s.support_size > 0.0 { s.support_size } else { 0.3 };
        let cases: Vec<[f64; 3]> = uls.iter().map(|(_, r)| [r.reactions.iter().filter(|(id, _)| *id == sp.node).map(|(_, f)| f[2].abs()).sum::<f64>(), 0.0, 0.0]).collect();
        let vu = cases.iter().map(|c| c[0]).fold(0.0, f64::max);
        out.push(Punch { x: n.x, y: n.y, c1: c, c2: c, what: format!("support at node {}", sp.node), vu, cases });
    }
    // a column load given as a nodal load on the panel (no column member): the load punches
    // through a square of the slab's support_size, with the combination's factors
    for n in model.nodes.iter().filter(|n| on(n) && !has_beam(n.id)) {
        if out.iter().any(|p| (p.x - n.x).abs() < 1e-6 && (p.y - n.y).abs() < 1e-6 && p.what.starts_with("column")) {
            continue;
        }
        let per_case = |case: Id| model.loads.iter().filter(|l| l.case == case).filter_map(|l| match l.kind { LoadKind::Nodal { node, f } if node == n.id => Some(f[2]), _ => None }).sum::<f64>();
        let cases: Vec<[f64; 3]> = model.combos.iter().filter(|c| c.kind == ComboKind::Uls).map(|c| [c.factors.iter().map(|(id, f)| f * per_case(*id)).sum::<f64>().abs(), 0.0, 0.0]).collect();
        let vu = cases.iter().map(|c| c[0]).fold(0.0, f64::max);
        if vu <= 1e-6 {
            continue;
        }
        let c = if s.support_size > 0.0 { s.support_size } else { 0.3 };
        out.push(Punch { x: n.x, y: n.y, c1: c, c2: c, what: format!("load at node {}", n.id), vu, cases });
    }
    out
}

/// The critical section of two-way shear around a loaded area (ACI 318-19 22.6.4): the sides
/// at d/2 from its faces that lie inside the panel, each ((x0, y0), (x1, y1)), m. An edge or
/// a corner position loses the sides past the panel's edges, and the others run to the edge.
fn critical_sides(p: &Punch, dm: f64, lo: (f64, f64), hi: (f64, f64)) -> Vec<((f64, f64), (f64, f64))> {
    let (ax, ay) = ((p.c1 + dm) / 2.0, (p.c2 + dm) / 2.0);
    let (x0, x1) = ((p.x - ax).max(lo.0), (p.x + ax).min(hi.0));
    let (y0, y1) = ((p.y - ay).max(lo.1), (p.y + ay).min(hi.1));
    let mut out = vec![];
    for y in [p.y - ay, p.y + ay] {
        if y > lo.1 + 1e-6 && y < hi.1 - 1e-6 && x1 - x0 > 1e-9 {
            out.push(((x0, y), (x1, y)));
        }
    }
    for x in [p.x - ax, p.x + ax] {
        if x > lo.0 + 1e-6 && x < hi.0 - 1e-6 && y1 - y0 > 1e-9 {
            out.push(((x, y0), (x, y1)));
        }
    }
    out
}

/// The section properties for the shear from an unbalanced moment bending in the x–z plane
/// (about an axis along y through the centroid), ACI 318-19 R8.4.4.2.3: the perimeter's
/// centroid x̄, Jc (m⁴, the "polar moment" of the sides, the sides along x with their d³
/// term), the largest distance c from the centroid to the section (m), and b1 (along x) and
/// b2 (across) for γ. For bending in the y–z plane the sides are passed with x and y swapped.
fn moment_props(sides: &[((f64, f64), (f64, f64))], d: f64) -> (f64, f64, f64, f64) {
    let len = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let total: f64 = sides.iter().map(|(a, b)| len(*a, *b)).sum();
    if total <= 1e-12 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let xc = sides.iter().map(|(a, b)| len(*a, *b) * (a.0 + b.0) / 2.0).sum::<f64>() / total;
    let mut jc = 0.0;
    for (a, b) in sides {
        let l = len(*a, *b);
        if (b.1 - a.1).abs() < 1e-9 {
            // a side along x: its own second moment about x̄ and the torsion of its depth
            let (u0, u1) = ((a.0 - xc).min(b.0 - xc), (a.0 - xc).max(b.0 - xc));
            jc += d * (u1.powi(3) - u0.powi(3)) / 3.0 + l * d.powi(3) / 12.0;
        } else {
            jc += l * d * (a.0 - xc).powi(2);
        }
    }
    let xs = sides.iter().flat_map(|(a, b)| [a.0, b.0]);
    let (xmin, xmax) = xs.clone().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)));
    let ys = sides.iter().flat_map(|(a, b)| [a.1, b.1]);
    let (ymin, ymax) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| (lo.min(y), hi.max(y)));
    let c = (xmax - xc).max(xc - xmin);
    // b1 and b2 of γf (8.4.2.2.2): the section's extent along the span and across it
    let gv = {
        let (b1, b2) = ((xmax - xmin).max(1e-6), (ymax - ymin).max(1e-6));
        1.0 - 1.0 / (1.0 + 2.0 / 3.0 * (b1 / b2).sqrt())
    };
    (jc, c, gv, xc)
}

/// The column lines of a flat plate (the x or y of its punching points) and the half width of
/// its column strips, 0.25 min(l1, l2) each side of a line (ACI 318-19 8.4.1.5), with l1 and
/// l2 the median spacings of the lines (the panel's length when there is one line).
fn column_lines(punch: &[Punch], lx: f64, ly: f64) -> (Vec<f64>, Vec<f64>, f64) {
    let distinct = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.05);
        v
    };
    let xs = distinct(punch.iter().map(|p| p.x).collect());
    let ys = distinct(punch.iter().map(|p| p.y).collect());
    let median_gap = |v: &[f64], whole: f64| {
        let mut g: Vec<f64> = v.windows(2).map(|w| w[1] - w[0]).collect();
        if g.is_empty() {
            return whole;
        }
        g.sort_by(f64::total_cmp);
        g[g.len() / 2]
    };
    let hw = 0.25 * median_gap(&xs, lx).min(median_gap(&ys, ly));
    (xs, ys, hw)
}

/// Fy-interpolated divisors of ACI 318-19 Table 8.3.1.1 (slabs without interior beams or drop
/// panels): (exterior panel with no edge beams, interior panel): ln/33, ln/36 at 280 MPa;
/// ln/30, ln/33 at 420 MPa; ln/27, ln/30 at 550 MPa.
fn flat_plate_divisors(fy: f64) -> (f64, f64) {
    let t = [(280.0, 33.0, 36.0), (420.0, 30.0, 33.0), (550.0, 27.0, 30.0)];
    let fy = fy.clamp(280.0, 550.0);
    let k = if fy <= 420.0 { 0 } else { 1 };
    let (a, b) = (t[k], t[k + 1]);
    let f = (fy - a.0) / (b.0 - a.0);
    (a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f)
}

/// Designs a slab from its plate results under every ULS combination (`uls`) and under the
/// service loads (`sls`, for the deflection). `punch` lists the columns and supports that load
/// the panel directly: with any of them, or with the slab marked `mat` or a subgrade under it,
/// the panel is a flat plate or a foundation mat, so the bars of each face are enveloped over
/// the whole panel (the hogging sits over the supports, not at the edges) and punching shear
/// is checked at every point to ACI 318-19 22.6 (two-way shear strength of 22.6.5.2 with the
/// size effect factor, the critical perimeter at d/2 clipped at the panel's edges).
pub fn design(slab: &Slab, uls: &[Vec<&PlateResult>], sls: &[&PlateResult], punch: &[Punch], mat: &RcMat, set: &DesignSettings) -> SlabDesign {
    let (lx, ly) = (slab.lx(), slab.ly());
    let (lo, hi) = (slab.lo(), slab.hi());
    let h = slab.h * 1000.0;
    let cover = set.cover_slab * 1000.0;
    let db0 = set.slab_bars.first().copied().unwrap_or(9.0);
    // the short span's bars in the outer layer
    let short_is_x = lx <= ly;
    let d_outer = h - cover - db0 / 2.0;
    let d_inner = d_outer - db0;
    let (dx, dy) = if short_is_x { (d_outer, d_inner) } else { (d_inner, d_outer) };
    let as_min = 0.0018 * 1000.0 * h;
    let smax = (2.0 * h).min(450.0);
    let direct = slab.mat || slab.subgrade > 0.0 || !punch.is_empty();
    // zones of a floor on beams: the middle half for bottom bars, the quarter next to each
    // edge for top bars; a flat plate or a mat takes both faces over the whole panel
    let mid = |p: &PlateResult| (p.x - lo.0) > 0.25 * lx - 1e-9 && (hi.0 - p.x) > 0.25 * lx - 1e-9 && (p.y - lo.1) > 0.25 * ly - 1e-9 && (hi.1 - p.y) > 0.25 * ly - 1e-9;
    let near_x_edges = |p: &PlateResult| (p.x - lo.0) < 0.25 * lx + 1e-9 || (hi.0 - p.x) < 0.25 * lx + 1e-9;
    let near_y_edges = |p: &PlateResult| (p.y - lo.1) < 0.25 * ly + 1e-9 || (hi.1 - p.y) < 0.25 * ly + 1e-9;
    let (mut bx, mut by, mut tx, mut ty) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut bx_all, mut by_all, mut tx_all, mut ty_all) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut vmax = 0.0f64;
    for combo in uls {
        for p in combo {
            let (a, b, c, d) = wood_armer(p.mx, p.my, p.mxy);
            if mid(p) {
                bx = bx.max(a);
                by = by.max(b);
            }
            bx_all = bx_all.max(a);
            by_all = by_all.max(b);
            tx_all = tx_all.max(c);
            ty_all = ty_all.max(d);
            // top x bars resist mx: they are needed where the x-direction moment hogs, at the
            // edges across x (x near lo/hi); likewise for y
            if near_x_edges(p) {
                tx = tx.max(c);
            }
            if near_y_edges(p) {
                ty = ty.max(d);
            }
            // one-way shear outside d (and outside the edge elements) from the panel edges, and
            // on a flat plate or mat also outside d from the face of every column, support and
            // load point (the element shears next to a point force are its punching shear)
            let dm = (dx.max(dy) / 1000.0).max(p.dx.max(p.dy) / 2.0 + 1e-6);
            let near_point = punch.iter().any(|q| (p.x - q.x).abs() < q.c1 / 2.0 + dm && (p.y - q.y).abs() < q.c2 / 2.0 + dm);
            if p.x - lo.0 > dm && hi.0 - p.x > dm && p.y - lo.1 > dm && hi.1 - p.y > dm && !near_point {
                vmax = vmax.max(p.qx.abs()).max(p.qy.abs());
            }
        }
    }
    let mut strips = vec![];
    let mut ratio: f64 = 0.0;
    let mut governs = String::new();
    let mut notes = vec![];
    let mut push = |dir: &str, loc: &str, mu: f64, d: f64, strips: &mut Vec<SlabStrip>| {
        let (as_req, as_prov, _, bars, fb) = rc::slab_bars(mu, d, as_min, smax, &set.slab_bars, mat.fc, mat.fy, mat.fyt);
        let cap = rc::phi_mn(as_prov, 1000.0, d, mat.fc, fb) / 1e6;
        let r = if mu > 0.0 { mu / cap } else { 0.0 };
        if r > ratio {
            ratio = r;
            governs = format!("flexure, {dir} {loc}");
        }
        strips.push(SlabStrip { direction: dir.into(), location: loc.into(), mu, as_req, as_prov, bars, ratio: r });
    };
    let (col_x, col_y, hw) = column_lines(punch, lx, ly);
    // a flat plate: a floor on columns (not a mat or a cap on supports or soil)
    let flat_plate = direct && !slab.mat && slab.subgrade <= 0.0 && punch.iter().any(|p| p.what.starts_with("column"));
    if flat_plate {
        // a flat plate (or a mat on piles): column strips along the column lines and middle
        // strips between them (8.4.1.5), each designed for its Wood–Armer moment averaged
        // across the strip's width at every section along the span, the largest of them (a
        // strip shares its moment across its width, as the direct design method does)
        let mut best = [[0.0f64; 4]; 2]; // [x bars, y bars][col bottom, col top, mid bottom, mid top]
        for combo in uls {
            for dir in 0..2 {
                // the sections along the span: element centres at one x (x bars) or one y (y bars)
                let along = |p: &PlateResult| if dir == 0 { p.x } else { p.y };
                let across = |p: &PlateResult| if dir == 0 { p.y } else { p.x };
                let width = |p: &PlateResult| if dir == 0 { p.dy } else { p.dx };
                let lines = if dir == 0 { &col_y } else { &col_x };
                let mut sections: Vec<f64> = combo.iter().map(|p| along(p)).collect();
                sections.sort_by(f64::total_cmp);
                sections.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                for sct in sections {
                    // bands across the section: (column strip?, index) → (Σ w·bottom, Σ w·top, Σ w)
                    let mut bands: Vec<((bool, usize), [f64; 3])> = vec![];
                    for p in combo.iter().filter(|p| (along(p) - sct).abs() < 1e-6) {
                        let c = across(p);
                        let (k, dmin) = lines.iter().enumerate().map(|(k, l)| (k, (c - l).abs())).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((0, f64::INFINITY));
                        let key = if dmin <= hw + 1e-9 { (true, k) } else { (false, lines.iter().filter(|l| **l < c).count()) };
                        let (wb, wt) = { let (a, b, c2, d) = wood_armer(p.mx, p.my, p.mxy); if dir == 0 { (a, c2) } else { (b, d) } };
                        let w = width(p);
                        match bands.iter_mut().find(|b| b.0 == key) {
                            Some(b) => { b.1[0] += w * wb; b.1[1] += w * wt; b.1[2] += w; }
                            None => bands.push((key, [w * wb, w * wt, w])),
                        }
                    }
                    for ((col, _), [sb, st, sw]) in bands {
                        let k = if col { 0 } else { 2 };
                        best[dir][k] = best[dir][k].max(sb / sw);
                        best[dir][k + 1] = best[dir][k + 1].max(st / sw);
                    }
                }
            }
        }
        for (dir, name, d) in [(0, "x", dx), (1, "y", dy)] {
            push(name, "column strip (bottom)", best[dir][0], d, &mut strips);
            push(name, "column strip (top)", best[dir][1], d, &mut strips);
            push(name, "middle strip (bottom)", best[dir][2], d, &mut strips);
            push(name, "middle strip (top)", best[dir][3], d, &mut strips);
        }
        notes.push(format!("column strips {:.2} m each side of the column lines (0.25 min(l1, l2), 8.4.1.5), x lines at {:?}, y lines at {:?}; strip moments are the Wood–Armer moments averaged across each strip's width", hw, col_x.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>(), col_y.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>()));
    } else if direct {
        push("x", "between supports (bottom)", bx_all, dx, &mut strips);
        push("y", "between supports (bottom)", by_all, dy, &mut strips);
        push("x", "over supports (top)", tx_all, dx, &mut strips);
        push("y", "over supports (top)", ty_all, dy, &mut strips);
    } else {
        push("x", "middle (bottom)", bx, dx, &mut strips);
        push("y", "middle (bottom)", by, dy, &mut strips);
        if bx_all > bx * 1.05 || by_all > by * 1.05 {
            push("x", "elsewhere (bottom)", bx_all, dx, &mut strips);
            push("y", "elsewhere (bottom)", by_all, dy, &mut strips);
        }
        push("x", "edges across x (top)", tx, dx, &mut strips);
        push("y", "edges across y (top)", ty, dy, &mut strips);
    }
    // moment transfer by flexure (8.4.2.2.3): γf Msc within c2 + 3h across the column (1.5h
    // each side of it) needs top bars of its own; the band's moment per metre gives them
    let hm = slab.h;
    let mut band = [0.0f64; 2];
    let mut band_at = [String::new(), String::new()];
    for p in punch.iter().filter(|p| flat_plate && p.what.starts_with("column")) {
        let dm = (dx + dy) / 2.0 / 1000.0;
        let sides = critical_sides(p, dm, lo, hi);
        let swapped: Vec<_> = sides.iter().map(|(a, b)| ((a.1, a.0), (b.1, b.0))).collect();
        let (_, _, gvx, _) = moment_props(&sides, dm);
        let (_, _, gvy, _) = moment_props(&swapped, dm);
        // the band runs across the moment's span, clipped at the panel's edges
        let wx = (p.x + p.c1 / 2.0 + 1.5 * hm).min(hi.0) - (p.x - p.c1 / 2.0 - 1.5 * hm).max(lo.0);
        let wy = (p.y + p.c2 / 2.0 + 1.5 * hm).min(hi.1) - (p.y - p.c2 / 2.0 - 1.5 * hm).max(lo.1);
        for c in &p.cases {
            for (k, m, g, w) in [(0, c[1], 1.0 - gvx, wy), (1, c[2], 1.0 - gvy, wx)] {
                let q = g * m / w.max(0.1);
                if q > band[k] {
                    band[k] = q;
                    band_at[k] = p.what.clone();
                }
            }
        }
    }
    for (k, name, d) in [(0, "x", dx), (1, "y", dy)] {
        if band[k] > 0.0 {
            push(name, &format!("over the columns within c2 + 3h (top, 8.4.2.2.3, {})", band_at[k]), band[k], d, &mut strips);
        }
    }
    let phi_vc = crate::codes::phi().shear * 0.17 * mat.fc.min(69.0).sqrt() * 1000.0 * dx.min(dy) / 1000.0;
    let rv = vmax / phi_vc;
    if rv > ratio {
        ratio = rv;
        governs = "one-way shear".into();
    }
    // punching (two-way shear) at every column and support: the factored shear stress of
    // 8.4.4.2.3, vu = Vu / (bo d) + γv Msc c / Jc for the unbalanced moments of both
    // directions (added: the critical corner takes both), against φvc of 22.6.5.2 with λs of
    // 22.5.5.1.3, φ 0.75, √f'c at most 8.3 MPa (22.5.3.1); the critical section at d/2 from
    // the loaded area with only the sides inside the panel (an edge or corner position shortens
    // it, moves its centroid and lowers αs: 40 interior, 30 edge, 20 corner). Msc is taken at
    // the column's axis, not moved to the section's centroid (Vu e left out)
    let d_avg = (dx + dy) / 2.0;
    let lam_s = (2.0 / (1.0 + 0.004 * d_avg)).sqrt().min(1.0);
    let sqrt_fc = mat.fc.sqrt().min(8.3);
    let mut punched = 0;
    for p in punch {
        if p.vu <= 1e-6 {
            continue;
        }
        let dm = d_avg / 1000.0;
        let sides = critical_sides(p, dm, lo, hi);
        let bo: f64 = sides.iter().map(|(a, b)| (b.0 - a.0).abs() + (b.1 - a.1).abs()).sum();
        if bo <= 1e-9 {
            continue;
        }
        let n_sides = sides.len();
        let alpha_s = match n_sides {
            4 => 40.0,
            3 => 30.0,
            _ => 20.0,
        };
        let beta = p.c1.max(p.c2) / p.c1.min(p.c2).max(1e-6);
        let bo_mm = bo * 1000.0;
        let vc = 0.33f64.min(0.17 * (1.0 + 2.0 / beta)).min(0.083 * (alpha_s * d_avg / bo_mm + 2.0)) * lam_s * sqrt_fc;
        let swapped: Vec<_> = sides.iter().map(|(a, b)| ((a.1, a.0), (b.1, b.0))).collect();
        let (jx, cx, gvx, _) = moment_props(&sides, dm);
        let (jy, cy, gvy, _) = moment_props(&swapped, dm);
        // the governing combination: the largest stress, MPa
        let (mut vmax_s, mut at) = (0.0f64, [0.0; 3]);
        let cases: Vec<[f64; 3]> = if p.cases.is_empty() { vec![[p.vu, 0.0, 0.0]] } else { p.cases.clone() };
        for c in &cases {
            let direct_v = c[0] / (bo * dm) / 1000.0;
            let mx = if jx > 0.0 { gvx * c[1] * cx / jx / 1000.0 } else { 0.0 };
            let my = if jy > 0.0 { gvy * c[2] * cy / jy / 1000.0 } else { 0.0 };
            let v = direct_v + mx + my;
            if v > vmax_s {
                vmax_s = v;
                at = *c;
            }
        }
        let r = vmax_s / (crate::codes::phi().shear * vc);
        punched += 1;
        notes.push(format!(
            "punching at {} ({:.2}, {:.2}): Vu {:.0} kN, Msc {:.1} / {:.1} kN·m (x–z / y–z), bo {:.2} m ({} sides, αs {:.0}), γv {:.2} / {:.2}, vu {:.2} MPa against φvc {:.2} MPa, ratio {:.2}{}",
            p.what, p.x, p.y, at[0], at[1], at[2], bo, n_sides, alpha_s, gvx, gvy, vmax_s, crate::codes::phi().shear * vc, r,
            if r > 1.0 { " FAILS: thicken the slab, enlarge the column, or add a drop panel or shear reinforcement" } else { "" }
        ));
        if r > ratio {
            ratio = r;
            governs = format!("punching at {}", p.what);
        }
    }
    // minimum thickness: Table 8.3.1.2 (stiff beams) for a floor on beams; Table 8.3.1.1 for a
    // flat plate (no interior beams, no drop panels, no edge beams), ln the clear span between
    // the column lines, the edge bays as exterior panels, at least 125 mm; a mat is governed by
    // punching and its pressure instead
    let (s, l) = (lx.min(ly), lx.max(ly));
    let mut h_min = (l * 1000.0 * (0.8 + mat.fy / 1400.0) / (36.0 + 9.0 * l / s)).max(90.0);
    if flat_plate {
        let (ext, int) = flat_plate_divisors(mat.fy);
        let cmean = punch.iter().map(|p| (p.c1 + p.c2) / 2.0).sum::<f64>() / punch.len() as f64;
        let mut hm = 125.0f64;
        for lines in [&col_x, &col_y] {
            let n = lines.len();
            for (i, w) in lines.windows(2).enumerate() {
                let ln = ((w[1] - w[0]) - cmean).max(0.0) * 1000.0;
                let exterior = i == 0 || i + 2 == n;
                hm = hm.max(ln / if exterior { ext } else { int });
            }
        }
        h_min = hm;
        if h < h_min {
            notes.push(format!("h {h:.0} mm is under the Table 8.3.1.1 minimum {h_min:.0} mm of a flat plate (ln/{ext:.1} exterior, ln/{int:.1} interior, at least 125 mm): check the deflection or thicken it"));
            if h_min / h > ratio {
                ratio = h_min / h;
                governs = "minimum thickness (Table 8.3.1.1)".into();
            }
        }
        notes.push("structural integrity (8.7.4.2): every bottom bar of the column strips runs continuous (Class B laps, or anchored at a discontinuous edge), and at least two of them in each direction pass through the column core".into());
    } else if !direct && h < h_min {
        notes.push(format!("h {h:.0} mm is under the Table 8.3.1.2 minimum {h_min:.0} mm (stiff beams assumed)"));
        if h_min / h > ratio {
            ratio = h_min / h;
            governs = "minimum thickness".into();
        }
    }
    if let (Some(lo_z), Some(hi_z)) = (sls.iter().map(|p| p.uz).reduce(f64::max), sls.iter().map(|p| p.uz).reduce(f64::min)) {
        let what = if slab.subgrade > 0.0 { "differential settlement" } else { "plate deflection" };
        notes.push(format!("{what} under D + L: {:.1} mm between the panel's most and least deflected points (gross section, elastic; cracking and creep increase it)", (lo_z - hi_z) * 1000.0));
        if slab.subgrade > 0.0 {
            notes.push(format!("settlement under D + L: {:.1} mm at most on a subgrade of {:.0} kN/m³ (linear springs: no lift-off, no consolidation)", -hi_z * 1000.0, slab.subgrade));
        }
    }
    if direct && punched == 0 {
        notes.push("no column or support inside the panel carries a force: punching was not checked".into());
    }
    notes.push("bars from the element moments by Wood–Armer, enveloped over the ULS combinations; element centre values (no smoothing across peaks)".into());
    let n_el = uls.first().map(|c| c.len()).unwrap_or(0);
    let kind = if slab.mat { "mat" } else if direct { "flat plate" } else { "plate" };
    SlabDesign {
        slab: slab.id,
        bim_id: slab.bim_id.clone(),
        name: slab.name.clone(),
        lx,
        ly,
        h: slab.h,
        method: if direct { format!("{kind} FE (MITC4, {n_el} elements, Wood–Armer, punching at {punched} points)") } else { format!("plate FE (MITC4, {n_el} elements, Wood–Armer)") },
        discontinuous_edges: 0,
        wu: 0.0,
        strips,
        h_min: if direct && !flat_plate { 0.0 } else { h_min },
        vu: vmax,
        phi_vc,
        ratio,
        governs,
        ok: ratio <= set.target_ratio + 1e-9,
        notes,
        flat: flat_plate.then(|| crate::rc::FlatStrips { lines_x: col_x.clone(), lines_y: col_y.clone(), half_width: hw, columns: punch.iter().filter(|p| p.what.starts_with("column")).map(|p| [p.x, p.y, p.c1, p.c2]).collect() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(x: f64, y: f64, c1: f64, c2: f64) -> Punch {
        Punch { x, y, c1, c2, what: "column".into(), vu: 1.0, cases: vec![] }
    }

    #[test]
    fn interior_column_shear_properties() {
        // ACI R8.4.4.2.3, interior column: Jc = d b1³/6 + b1 d³/6 + d b2 b1²/2, c = b1/2,
        // γv = 1 − 1/(1 + 2/3 √(b1/b2)); 400 x 300 mm column, d = 0.15 m
        let (d, c1, c2) = (0.15, 0.4, 0.3);
        let sides = critical_sides(&col(5.0, 5.0, c1, c2), d, (0.0, 0.0), (10.0, 10.0));
        assert_eq!(sides.len(), 4);
        let (b1, b2) = (c1 + d, c2 + d);
        let (jc, c, gv, xc) = moment_props(&sides, d);
        let jc_aci = d * b1.powi(3) / 6.0 + b1 * d.powi(3) / 6.0 + d * b2 * b1 * b1 / 2.0;
        assert!((jc - jc_aci).abs() < 1e-12, "{jc} {jc_aci}");
        assert!((c - b1 / 2.0).abs() < 1e-12 && (xc - 5.0).abs() < 1e-12);
        assert!((gv - (1.0 - 1.0 / (1.0 + 2.0 / 3.0 * (b1 / b2).sqrt()))).abs() < 1e-12);
    }

    #[test]
    fn edge_column_shear_properties() {
        // an edge column with its outer face on the slab edge x = 0, the moment bending
        // perpendicular to the edge: three sides, b1 = c1 + d/2, b2 = c2 + d; the centroid at
        // x̄ = b1²/(2 b1 + b2) from the edge, Jc = 2 (d b1³/12 + b1 d³/12) + 2 b1 d (b1/2 − x̄)²
        // + b2 d x̄², c = b1 − x̄ (textbook form, e.g. Wight & MacGregor 7e Sect. 13-9)
        let (d, c1, c2) = (0.15, 0.4, 0.4);
        let sides = critical_sides(&col(c1 / 2.0, 5.0, c1, c2), d, (0.0, 0.0), (10.0, 10.0));
        assert_eq!(sides.len(), 3);
        let (b1, b2) = (c1 + d / 2.0, c2 + d);
        let xbar = b1 * b1 / (2.0 * b1 + b2); // from the inner face line of the section
        let (jc, c, gv, xc) = moment_props(&sides, d);
        let jc_t = 2.0 * (d * b1.powi(3) / 12.0 + b1 * d.powi(3) / 12.0) + 2.0 * b1 * d * (b1 / 2.0 - xbar).powi(2) + b2 * d * xbar * xbar;
        assert!((jc - jc_t).abs() < 1e-12, "{jc} {jc_t}");
        // the inner side lies at x = b1: the centroid is x̄ from it
        assert!((xc - (b1 - xbar)).abs() < 1e-12, "{xc}");
        assert!((c - (b1 - xbar).max(xbar)).abs() < 1e-12);
        assert!((gv - (1.0 - 1.0 / (1.0 + 2.0 / 3.0 * (b1 / b2).sqrt()))).abs() < 1e-12);
    }

    #[test]
    fn flat_plate_divisors_follow_table_8_3_1_1() {
        assert_eq!(flat_plate_divisors(420.0), (30.0, 33.0));
        assert_eq!(flat_plate_divisors(280.0), (33.0, 36.0));
        let (e, i) = flat_plate_divisors(390.0);
        assert!((e - (33.0 - 3.0 * 110.0 / 140.0)).abs() < 1e-9 && (i - (36.0 - 3.0 * 110.0 / 140.0)).abs() < 1e-9);
    }

    #[test]
    fn wood_armer_cases() {
        // pure bending: no twist, the moments themselves
        assert_eq!(wood_armer(10.0, 5.0, 0.0), (10.0, 5.0, 0.0, 0.0));
        // pure twist: |mxy| on both faces and both directions
        assert_eq!(wood_armer(0.0, 0.0, 4.0), (4.0, 4.0, 4.0, 4.0));
        // bottom x would be negative: zero, and y takes my + mxy²/|mx|
        let (bx, by, _, _) = wood_armer(-10.0, 6.0, 2.0);
        assert_eq!(bx, 0.0);
        assert!((by - (6.0 + 4.0 / 10.0)).abs() < 1e-12);
    }
}
