// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The structural model: nodes, members (the physical beams, columns and braces, each split
//! into analysis elements only inside the analysis), sections, materials, supports, load
//! cases, loads, combinations and slab panels. Units: m, kN, kPa, MPa; x and y in plan,
//! z upwards. A member is one straight piece from node i to node j; members that frame into
//! it part-way along its length split it in the analysis, not in the model.

use serde::{Deserialize, Serialize};

pub type Id = usize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: Id,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Node {
    pub fn p(&self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MaterialKind {
    #[default]
    Concrete,
    Steel,
    /// Sawn timber (NDS): fb, ft, fc, fv, E and Emin are its reference design values.
    Timber,
    /// Reinforced masonry (TMS 402): fc is f'm.
    Masonry,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: Id,
    pub name: String,
    pub kind: MaterialKind,
    /// Young's modulus, MPa.
    pub e: f64,
    pub nu: f64,
    /// Unit weight, kN/m³.
    pub gamma: f64,
    /// Concrete: f'c (MPa). Steel: 0.
    #[serde(default)]
    pub fc: f64,
    /// Steel: Fy and Fu (MPa). Concrete: 0 (the rebar grades are in the design settings).
    #[serde(default)]
    pub fy: f64,
    #[serde(default)]
    pub fu: f64,
    /// Timber reference design values, MPa: bending, tension, shear, E for stability.
    #[serde(default)]
    pub fb: f64,
    #[serde(default)]
    pub ft: f64,
    #[serde(default)]
    pub fv: f64,
    #[serde(default)]
    pub e_min: f64,
    /// Coefficient of thermal expansion, 1/°C (0: by kind, see `alpha()`).
    #[serde(default)]
    pub alpha: f64,
}

impl Material {
    /// The coefficient of thermal expansion: the material's own, else 10e-6 for concrete
    /// (EN 1992-1-1 3.1.3(5)), 12e-6 for steel (EN 1993-1-1 3.2.6), 8.1e-6 for concrete masonry
    /// (TMS 402-16 4.2.4: 4.5e-6 /°F), 5e-6 for timber along the grain and 12e-6 otherwise.
    pub fn alpha(&self) -> f64 {
        if self.alpha > 0.0 {
            return self.alpha;
        }
        match self.kind {
            MaterialKind::Concrete => 10e-6,
            MaterialKind::Steel => 12e-6,
            MaterialKind::Masonry => 8.1e-6,
            MaterialKind::Timber => 5e-6,
            MaterialKind::Other => 12e-6,
        }
    }

    /// Concrete of f'c (MPa), E = 4700 √f'c (ACI 318-19 19.2.2.1b), 24 kN/m³.
    pub fn concrete(id: Id, fc: f64) -> Material {
        Material { id, name: format!("Concrete f'c {fc:.0} MPa"), kind: MaterialKind::Concrete, e: (4700.0 * fc.sqrt()).round(), nu: 0.2, gamma: 24.0, fc, fy: 0.0, fu: 0.0, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 }
    }
    /// Structural steel of grade `name` (e.g. SS400: Fy 245, Fu 400 MPa), E = 200 GPa, 77 kN/m³.
    pub fn steel(id: Id, name: &str, fy: f64, fu: f64) -> Material {
        Material { id, name: name.into(), kind: MaterialKind::Steel, e: 200_000.0, nu: 0.3, gamma: 77.0, fc: 0.0, fy, fu, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 }
    }
    /// Sawn timber, Douglas Fir-Larch No. 2 by default (NDS Supplement Table 4A: Fb 900, Ft 575,
    /// Fc 1350, Fv 180 psi, E 1.6e6, Emin 0.58e6 psi), 5 kN/m³.
    pub fn timber(id: Id) -> Material {
        Material { id, name: "Timber DF-L No.2".into(), kind: MaterialKind::Timber, e: 11_032.0, nu: 0.3, gamma: 5.0, fc: 9.31, fy: 0.0, fu: 0.0, fb: 6.21, ft: 3.96, fv: 1.24, e_min: 3_999.0, alpha: 0.0 }
    }
    /// Grouted concrete masonry, f'm 13.8 MPa (2000 psi), Em = 900 f'm, 20 kN/m³.
    pub fn masonry(id: Id, fm: f64) -> Material {
        Material { id, name: format!("Masonry f'm {fm:.1} MPa"), kind: MaterialKind::Masonry, e: 900.0 * fm, nu: 0.25, gamma: 20.0, fc: fm, fy: 0.0, fu: 0.0, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 }
    }
}

/// The cross-section shape. Rectangles are concrete members (b wide, h deep: h along the
/// member's local z, vertical for a beam); H shapes are rolled steel; General carries only
/// the stiffness (no design).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
pub enum Shape {
    Rect { b: f64, h: f64 },
    /// Rolled H/I shape: depth d, flange width bf, web tw, flange tf, root radius r (m).
    H { d: f64, bf: f64, tw: f64, tf: f64, r: f64 },
    /// Cold-formed lipped channel: outer depth h, flange b, lip d, thickness t (m).
    #[serde(rename = "lippedc")]
    LippedC { h: f64, b: f64, d: f64, t: f64 },
    General {
        a: f64,
        iy: f64,
        iz: f64,
        j: f64,
        /// Shear areas [Asy, Asz] (m²) for shear along local y and local z, used when the
        /// analysis includes shear deformation (`DesignSettings::shear_deformation`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shear: Option<[f64; 2]>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: Id,
    pub name: String,
    #[serde(flatten)]
    pub shape: Shape,
}

/// Section properties for analysis and design (m², m⁴, m³, m⁶).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Props {
    pub a: f64,
    /// Major axis (bending under gravity of a beam: about local y).
    pub iy: f64,
    pub iz: f64,
    pub j: f64,
    pub cw: f64,
    pub sy: f64,
    pub sz: f64,
    pub zy: f64,
    pub zz: f64,
    pub ry: f64,
    pub rz: f64,
}

impl Section {
    pub fn props(&self) -> Props {
        match self.shape {
            Shape::Rect { b, h } => {
                let (long, short) = if h > b { (h, b) } else { (b, h) };
                // St Venant torsion of a rectangle (Roark): β long short³
                let beta = 1.0 / 3.0 - 0.21 * short / long * (1.0 - short.powi(4) / (12.0 * long.powi(4)));
                let a = b * h;
                let iy = b * h.powi(3) / 12.0;
                let iz = h * b.powi(3) / 12.0;
                Props { a, iy, iz, j: beta * long * short.powi(3), cw: 0.0, sy: b * h * h / 6.0, sz: h * b * b / 6.0, zy: b * h * h / 4.0, zz: h * b * b / 4.0, ry: (iy / a).sqrt(), rz: (iz / a).sqrt() }
            }
            Shape::H { d, bf, tw, tf, r } => crate::sections::h_props(d, bf, tw, tf, r),
            Shape::LippedC { h, b, d, t } => {
                // polygon integrals (no mesh) and the thin-walled J; Cw from the FE when designed
                let poly = dedaliano_engine::section::SectionPolygon { vertices: crate::secfe::lipped_c(h, b, d, t), material_id: 0, is_void: false };
                match dedaliano_engine::section::analyze_section(&dedaliano_engine::section::SectionInput { polygons: vec![poly], modular_ratios: Default::default() }) {
                    Ok(p) => Props { a: p.a, iy: p.iy, iz: p.iz, j: (h + 2.0 * b + 2.0 * d) * t.powi(3) / 3.0, cw: 0.0, sy: p.iy / (h / 2.0), sz: p.sz_left.min(p.sz_right), zy: p.zy, zz: p.zz, ry: p.ry, rz: p.rz },
                    Err(_) => Props::default(),
                }
            }
            Shape::General { a, iy, iz, j, .. } => Props { a, iy, iz, j, ry: (iy / a).sqrt(), rz: (iz / a).sqrt(), ..Default::default() },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MemberKind {
    /// From the direction: horizontal = beam, vertical = column, otherwise brace.
    #[default]
    Auto,
    Beam,
    Column,
    Brace,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: Id,
    pub i: Id,
    pub j: Id,
    pub section: Id,
    pub material: Id,
    #[serde(default)]
    pub kind: MemberKind,
    /// Moment releases (pins) at the start and the end, about both bending axes.
    #[serde(default)]
    pub hinge_i: bool,
    #[serde(default)]
    pub hinge_j: bool,
    /// Unbraced length of the compression flange for lateral-torsional buckling (m);
    /// None: the member's length.
    #[serde(default)]
    pub lb: Option<f64>,
    /// Effective length factor K for column buckling (both axes); default 1.0.
    #[serde(default)]
    pub k: Option<f64>,
    /// Roll of the section about the member axis, degrees. A column with roll 0 has its
    /// depth (strong axis) resisting bending in the x-z plane; roll 90 turns it to the y-z
    /// plane (checked in the tests).
    #[serde(default)]
    pub roll: f64,
    /// A truss member: axial force only (pinned both ends, no bending).
    #[serde(default)]
    pub truss: bool,
    /// Normal, tension-only (a brace that buckles out), compression-only (a strut, a gap) or
    /// a cable (tension only, with its sag by Ernst's modulus).
    #[serde(default)]
    pub behaviour: Behaviour,
    /// A beam on an elastic foundation: subgrade stiffness per metre of member, kN/m/m,
    /// [kz, ky]: along its local z (vertical for a beam, the usual subgrade) and local y
    /// (ground beams, strip footings). The order is the MCP tool's and the Excel sheet's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winkler: Option<[f64; 2]>,
    /// Bars provided, mm² (masonry members).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebar: Option<f64>,
    /// The element of the BIM model (e.g. Aedifex) this member stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bim_id: Option<String>,
    /// The end connection of a steel member in tension (None: the design settings' one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tension: Option<TensionConnection>,
    /// A frame member, or a pier or rigid arm generated for a wall.
    #[serde(default, skip_serializing_if = "Role::is_frame")]
    pub role: Role,
    /// Members of one group get one section when steel is selected (e.g. "B-L2").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behaviour {
    #[default]
    Normal,
    TensionOnly,
    CompressionOnly,
    Cable,
}

/// Fixed degrees of freedom: ux, uy, uz, rx, ry, rz.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Support {
    pub node: Id,
    pub fix: [bool; 6],
    /// Springs on the free DOFs: kN/m for translations, kN·m/rad for rotations (0: none).
    #[serde(default)]
    pub springs: [f64; 6],
    /// The support lifts off (holds the node only when pushed down).
    #[serde(default)]
    pub uplift: bool,
}

impl Member {
    /// Axial force only: a truss, one-way member or cable (one element end to end).
    pub fn axial_only(&self) -> bool {
        self.truss || self.behaviour != Behaviour::Normal
    }
}

impl Support {
    pub fn kind(&self) -> &'static str {
        match self.fix {
            [true, true, true, true, true, true] => "fixed",
            [true, true, true, false, false, false] => "pinned",
            [false, false, true, false, false, false] => "roller",
            _ => "custom",
        }
    }
    pub fn of_kind(node: Id, kind: &str) -> Option<Support> {
        let fix = match kind {
            "fixed" | "fixed3d" => [true; 6],
            "pinned" | "pinned3d" => [true, true, true, false, false, false],
            "roller" | "rollerZ" | "rollerz" => [false, false, true, false, false, false],
            _ => return None,
        };
        Some(Support { node, fix, springs: [0.0; 6], uplift: false })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaseKind {
    /// Dead load (self-weight, slabs, finishes).
    #[default]
    D,
    /// Live load.
    L,
    /// Roof live load.
    Lr,
    /// Wind.
    W,
    /// Earthquake.
    E,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadCase {
    pub id: Id,
    pub name: String,
    pub kind: CaseKind,
    /// The members' own weight is added to this case.
    #[serde(default)]
    pub self_weight: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LoadKind {
    /// Forces fx, fy, fz (kN) and moments mx, my, mz (kN·m) at a node, global axes.
    Nodal { node: Id, f: [f64; 6] },
    /// A line load on a member in global axes, kN/m: `wi` at distance `a` from node i
    /// varying linearly to `wj` at distance `b` (None: the whole member).
    Line {
        member: Id,
        wi: [f64; 3],
        wj: [f64; 3],
        #[serde(default)]
        a: Option<f64>,
        #[serde(default)]
        b: Option<f64>,
    },
    /// A point load on a member, kN in global axes, `a` m from node i.
    Point { member: Id, a: f64, p: [f64; 3] },
    /// An imposed displacement of a support (settlement), m and rad, in the DOFs it holds.
    Settlement { node: Id, d: [f64; 6] },
    /// A temperature change of a member: uniform ΔT and gradients across its local y and z
    /// (°C; α = 12e-6 /°C, the engine's).
    Thermal { member: Id, dt: f64, #[serde(default)] dt_y: f64, #[serde(default)] dt_z: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Load {
    pub case: Id,
    #[serde(flatten)]
    pub kind: LoadKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ComboKind {
    /// Strength (factored): used to design.
    #[default]
    Uls,
    /// Service: used for deflections.
    Sls,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Combination {
    pub id: Id,
    pub name: String,
    pub kind: ComboKind,
    pub factors: Vec<(Id, f64)>,
}

/// A rectangular floor panel in plan, x0..x1 by y0..y1 at height z (its top), carried by the
/// beams on its edges. Its own weight and `dead` go to the dead case, `live` to the live
/// case, both by the 45° lines of a two-way slab (one-way past a 2:1 aspect).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Slab {
    pub id: Id,
    #[serde(default)]
    pub name: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub z: f64,
    /// Thickness, m.
    pub h: f64,
    /// Superimposed dead load (finishes, services, partitions), kPa.
    pub dead: f64,
    /// Live load, kPa.
    pub live: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bim_id: Option<String>,
    /// Winkler subgrade modulus of the soil under the panel, kN/m³ (0: none). With the plate
    /// method a spring of ks × tributary area goes under every plate node (and the same in
    /// plan, the soil's friction), so the panel is a mat foundation on soil.
    #[serde(default)]
    pub subgrade: f64,
    /// A foundation mat or pile cap: designed with the bars of each face enveloped over the
    /// whole panel and punching checked at every support and column inside it (a flat plate
    /// on columns gets the same treatment by itself).
    #[serde(default)]
    pub mat: bool,
    /// Bearing width of the supports inside the panel (piles, pedestals) for punching, m
    /// (0: 0.3 m).
    #[serde(default)]
    pub support_size: f64,
    /// Analysed as plate finite elements whatever the slab method: a flat plate (a floor on
    /// columns with no beams), which the coefficient method cannot carry (it hands the load
    /// to beams by the 45° lines). The other slabs keep the model's method.
    #[serde(default)]
    pub plate: bool,
}

impl Model {
    /// Whether a slab is analysed and designed as plate finite elements: every slab under the
    /// plate method, and under the coefficient method a flat plate, a mat or a slab on soil.
    pub fn plate_slab(&self, s: &Slab) -> bool {
        self.design.slab_method == crate::slabfe::SlabMethod::Plate || s.plate || s.mat || s.subgrade > 0.0
    }
}

impl Slab {
    /// Whether the point lies on the panel (in plan, within its edges) at its level.
    pub fn contains(&self, x: f64, y: f64, z: f64) -> bool {
        let (lo, hi) = (self.lo(), self.hi());
        (z - self.z).abs() < 0.05 && x >= lo.0 - 1e-6 && x <= hi.0 + 1e-6 && y >= lo.1 - 1e-6 && y <= hi.1 + 1e-6
    }

    pub fn lx(&self) -> f64 {
        (self.x1 - self.x0).abs()
    }
    pub fn ly(&self) -> f64 {
        (self.y1 - self.y0).abs()
    }
    pub fn lo(&self) -> (f64, f64) {
        (self.x0.min(self.x1), self.y0.min(self.y1))
    }
    pub fn hi(&self) -> (f64, f64) {
        (self.x0.max(self.x1), self.y0.max(self.y1))
    }
}

/// Design settings: the codes (ACI 318-19 for concrete, AISC 360-16 LRFD for steel), the
/// reinforcement grades and the bars to choose from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesignSettings {
    /// Main bars (deformed, e.g. SD40) and stirrups (e.g. SR24) yield strengths, MPa.
    pub fy: f64,
    pub fyt: f64,
    /// Clear cover to the stirrups of beams and to the bars of slabs, m.
    pub cover_beam: f64,
    pub cover_slab: f64,
    /// Bar diameters to choose from, mm.
    pub beam_bars: Vec<f64>,
    pub stirrup_bars: Vec<f64>,
    pub slab_bars: Vec<f64>,
    /// Nominal maximum aggregate size, mm (bar spacing).
    pub aggregate: f64,
    /// Deflection limits as span / limit: live-load immediate and total long-term.
    pub defl_live: f64,
    pub defl_total: f64,
    /// Steel shape names to choose from (empty: the whole catalogue).
    pub steel_shapes: Vec<String>,
    /// Largest utilisation accepted when choosing sizes (1.0 = the code limit).
    pub target_ratio: f64,
    /// Slabs by moment coefficients (load to the beams by the 45° lines) or as plate finite
    /// elements analysed with the frame; the plate mesh size, m.
    pub slab_method: crate::slabfe::SlabMethod,
    pub slab_mesh: f64,
    /// Analyse every ULS combination second order (P-Delta): column moments are then not
    /// magnified for slenderness.
    pub second_order: bool,
    /// Include the shear deformation of frame members in the analysis (Timoshenko beams), as
    /// SAP2000 and ETABS do by default: shear areas 5A/6 for rectangles, d·tw (major) and
    /// 5/3 bf tf (minor) for H shapes, and the given ones for general sections. Off by default
    /// (Euler-Bernoulli frames); wall piers always deform in shear.
    pub shear_deformation: bool,
    /// Spread footings under the concrete columns: soil and geometry.
    pub footing: crate::footing::FootingSettings,
    /// The steel code (AISC 360-16 or EN 1993-1-1), and whether concrete beams are also
    /// checked to EN 1992-1-1 with their ACI bars.
    pub steel_code: crate::codes::SteelCode,
    pub ec2_check: bool,
    /// Depth limits for the steel shapes select chooses, mm (0: none).
    pub steel_min_depth: f64,
    pub steel_max_depth: f64,
    /// Steel beam end connections (shear tabs) and the floor vibration limit, Hz (0: off).
    pub connection: crate::connections::ConnSettings,
    pub vibration_hz: f64,
    /// The end connection of steel members in tension (unless a member gives its own), from
    /// which the net area An and the shear lag factor U of tensile rupture follow.
    pub tension_connection: TensionConnection,
    /// The load and strength reduction factors: ACI 318-19 / AISC 360-16 / ASCE 7-22, or the
    /// Thai Ministerial Regulation on structural design B.E. 2566 (with or without specified
    /// quality control).
    pub standard: crate::codes::DesignStandard,
}

/// How a steel H member's end is connected, for its tensile rupture: the net area An (holes
/// through the connected elements in one cross-section) and the shear lag factor U of AISC
/// 360-16 Table D3.1, and the net area of EN 1993-1-1 6.2.3(2)b.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TensionConnection {
    /// "bolted" or "welded"; "none": no rupture check beyond yielding (An = Ae = Ag).
    pub kind: String,
    /// The elements connected: "flanges", "web" or "all".
    pub connected: String,
    /// Bolt diameter, mm.
    pub bolt_d: f64,
    /// Holes across each connected flange, and across the web, in one cross-section.
    pub holes_flange: usize,
    pub holes_web: usize,
    /// Bolts per line in the direction of the load (Table D3.1 case 7), and the connection
    /// length l, m (case 2, U = 1 − x̄/l; 0: not used).
    pub bolts_per_line: usize,
    pub length: f64,
}

impl Default for TensionConnection {
    fn default() -> Self {
        TensionConnection { kind: "bolted".into(), connected: "flanges".into(), bolt_d: 20.0, holes_flange: 2, holes_web: 0, bolts_per_line: 3, length: 0.0 }
    }
}

impl TensionConnection {
    /// An and Ae (mm²) of an H shape of gross area ag (mm²), and the notes saying how. The hole
    /// width is d + 1/8 in (3.2 mm: the standard hole d + 1/16 in, plus 1/16 in, B4.3b) for
    /// AISC, d0 = d + 2 mm (EN 1090-2, M16–M24; +1 below, +3 above) for EC3.
    pub fn net(&self, ag: f64, h: crate::steel::HDims, ec3: bool) -> (f64, f64, String) {
        let (flanges, web) = match self.connected.as_str() {
            "web" => (false, true),
            "all" => (true, true),
            _ => (true, false),
        };
        if self.kind == "none" {
            return (ag, ag, "no end connection given: An = Ae = Ag".into());
        }
        let width = if ec3 { self.bolt_d + if self.bolt_d <= 14.0 { 1.0 } else if self.bolt_d <= 24.0 { 2.0 } else { 3.0 } } else { self.bolt_d + 3.175 };
        let holes = if self.kind == "bolted" { (if flanges { 2.0 * self.holes_flange as f64 * h.tf } else { 0.0 }) + if web { self.holes_web as f64 * h.tw } else { 0.0 } } else { 0.0 };
        let an = (ag - holes * width).max(0.0);
        // Table D3.1
        let mut how = vec![];
        let u = if flanges && web {
            how.push("case 1: all elements connected, U = 1".to_string());
            1.0
        } else {
            // case 2: U = 1 − x̄/l, x̄ of the half section (a tee for the flanges, a channel for
            // the web) from the connected face
            let tee = {
                let (af, aw) = (h.bf * h.tf, (h.d / 2.0 - h.tf) * h.tw);
                (af * h.tf / 2.0 + aw * (h.tf + (h.d / 2.0 - h.tf) / 2.0)) / (af + aw)
            };
            let chan = {
                let (af, aw) = (2.0 * (h.bf / 2.0 - h.tw / 2.0) * h.tf, (h.d - 2.0 * h.tf) * h.tw / 2.0);
                (af * (h.tw / 2.0 + (h.bf / 2.0 - h.tw / 2.0) / 2.0) + aw * h.tw / 4.0) / (af + aw) - h.tw / 2.0
            };
            let xbar = if flanges { tee } else { chan.max(0.0) };
            let l = self.length * 1000.0;
            let u2 = if l > 0.0 { (1.0 - xbar / l).max(0.0) } else { 0.0 };
            if l > 0.0 {
                how.push(format!("case 2: x̄ = {xbar:.1} mm, l = {l:.0} mm, U = {u2:.3}"));
            }
            let u7 = if self.kind != "bolted" {
                0.0
            } else if flanges && self.bolts_per_line >= 3 {
                if h.bf >= 2.0 / 3.0 * h.d { 0.90 } else { 0.85 }
            } else if web && self.bolts_per_line >= 4 {
                0.70
            } else {
                0.0
            };
            if u7 > 0.0 {
                how.push(format!("case 7: U = {u7:.2}"));
            }
            let u = u2.max(u7);
            if u == 0.0 {
                how.push("no case of Table D3.1 applies (give the connection length): U = 0.60, conservative".into());
                0.60
            } else {
                u
            }
        };
        let ae = if ec3 { an } else { u * an };
        (an, ae, format!("{} {}: An = {an:.0} mm² ({:.0} mm holes), {}", self.kind, self.connected, width, how.join("; ")))
    }
}

impl Default for DesignSettings {
    fn default() -> Self {
        DesignSettings {
            fy: 390.0,
            fyt: 235.0,
            cover_beam: 0.030,
            cover_slab: 0.020,
            beam_bars: vec![12.0, 16.0, 20.0, 25.0, 28.0, 32.0],
            stirrup_bars: vec![9.0, 10.0, 12.0],
            slab_bars: vec![9.0, 12.0, 16.0],
            aggregate: 20.0,
            defl_live: 360.0,
            defl_total: 240.0,
            steel_shapes: vec![],
            target_ratio: 1.0,
            slab_method: Default::default(),
            slab_mesh: 0.5,
            second_order: false,
            shear_deformation: false,
            footing: Default::default(),
            steel_code: Default::default(),
            ec2_check: false,
            steel_min_depth: 0.0,
            steel_max_depth: 0.0,
            connection: Default::default(),
            vibration_hz: 3.0,
            tension_connection: Default::default(),
            standard: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Model {
    pub name: String,
    pub materials: Vec<Material>,
    pub sections: Vec<Section>,
    pub nodes: Vec<Node>,
    pub members: Vec<Member>,
    pub supports: Vec<Support>,
    pub cases: Vec<LoadCase>,
    pub loads: Vec<Load>,
    pub combos: Vec<Combination>,
    pub slabs: Vec<Slab>,
    /// Rigid floor diaphragms: the levels (z, m) whose nodes move together in plan.
    pub diaphragms: Vec<f64>,
    /// A moving load (an axle train along a path of members), designed with the combinations
    /// 1.2D + factor x M max / min.
    pub moving: Option<crate::advanced::MovingSpec>,
    /// Construction stages (staged analysis).
    pub stages: Vec<crate::advanced::Stage>,
    pub design: DesignSettings,
    /// Earthquake: modal and response spectrum analysis (off unless enabled).
    pub seismic: crate::seismic::Seismic,
    /// Structural walls (shear walls), each analysed as piers with rigid arms (`walls.rs`).
    pub walls: Vec<Wall>,
    /// Stair flights and landings, designed as one-way slabs (`stairs.rs`).
    #[serde(default)]
    pub stairs: Vec<crate::stairs::Stair>,
    /// Lumped masses at nodes, for the modal, response spectrum and seismic weight; a node
    /// that carries only a mass (a floor's centre of mass) must lie on a rigid diaphragm.
    pub masses: Vec<NodalMass>,
}

/// A lumped mass at a node: translational mass in x, y and z (t) and rotational inertia about
/// x, y and z (t·m²), in global axes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodalMass {
    pub node: Id,
    pub m: [f64; 3],
    #[serde(default)]
    pub r: [f64; 3],
}

/// A structural wall: a straight line in plan from `a` to `b`, from `z0` to `z1`, of thickness
/// `t` (m). It is analysed by the wide-column analogy: a pier member at its centre line between
/// consecutive levels, with the wall's section, and rigid arms at each level out to its ends,
/// so beams and slabs along the wall connect to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub id: Id,
    pub name: String,
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub z0: f64,
    pub z1: f64,
    pub t: f64,
    pub material: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bim_id: Option<String>,
    /// The levels its piers were built between (kept so that new floors rebuild them).
    #[serde(default)]
    pub levels: Vec<f64>,
}

impl Wall {
    pub fn length(&self) -> f64 {
        ((self.b[0] - self.a[0]).powi(2) + (self.b[1] - self.a[1]).powi(2)).sqrt()
    }
}

/// What a member stands for: an ordinary frame member, a pier of a wall, or a rigid arm of a
/// wall (analysed, never designed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Frame,
    WallPier(Id),
    WallLink(Id),
}

impl Role {
    pub fn is_frame(&self) -> bool {
        *self == Role::Frame
    }
    pub fn wall(&self) -> Option<Id> {
        match *self {
            Role::WallPier(w) | Role::WallLink(w) => Some(w),
            Role::Frame => None,
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Model::new("Untitled")
    }
}

pub const TOL: f64 = 0.005;

impl Model {
    /// An empty model with concrete f'c 24 MPa and steel SS400, cases D (with self-weight)
    /// and L, and the ACI 318-19 / ASCE 7 combinations 1.4D, 1.2D + 1.6L and D + L.
    pub fn new(name: &str) -> Model {
        Model {
            name: name.into(),
            materials: vec![Material::concrete(1, 24.0), Material::steel(2, "Steel SS400", 245.0, 400.0)],
            sections: vec![],
            nodes: vec![],
            members: vec![],
            supports: vec![],
            cases: vec![LoadCase { id: 1, name: "D".into(), kind: CaseKind::D, self_weight: true }, LoadCase { id: 2, name: "L".into(), kind: CaseKind::L, self_weight: false }],
            loads: vec![],
            combos: vec![
                Combination { id: 1, name: "ULS 1.4D".into(), kind: ComboKind::Uls, factors: vec![(1, 1.4)] },
                Combination { id: 2, name: "ULS 1.2D + 1.6L".into(), kind: ComboKind::Uls, factors: vec![(1, 1.2), (2, 1.6)] },
                Combination { id: 3, name: "SLS D + L".into(), kind: ComboKind::Sls, factors: vec![(1, 1.0), (2, 1.0)] },
            ],
            slabs: vec![],
            diaphragms: vec![],
            moving: None,
            stages: vec![],
            design: DesignSettings::default(),
            seismic: Default::default(),
            walls: vec![], stairs: vec![],
            masses: vec![],
        }
    }

    pub fn load(path: &std::path::Path) -> Result<Model, String> {
        let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if path.extension().is_some_and(|e| e == "json") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                if crate::stabileo::is_stabileo(&v) {
                    return crate::stabileo::import(&v).map(|(m, _)| m);
                }
            }
        }
        serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let t = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("tmp~");
        std::fs::write(&tmp, t).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn node(&self, id: Id) -> Option<&Node> {
        by_id(&self.nodes, id, |n| n.id)
    }
    pub fn member(&self, id: Id) -> Option<&Member> {
        by_id(&self.members, id, |m| m.id)
    }
    pub fn section(&self, id: Id) -> Option<&Section> {
        by_id(&self.sections, id, |s| s.id)
    }
    pub fn material(&self, id: Id) -> Option<&Material> {
        by_id(&self.materials, id, |s| s.id)
    }
    pub fn case(&self, id: Id) -> Option<&LoadCase> {
        self.cases.iter().find(|s| s.id == id)
    }

    pub fn next_node_id(&self) -> Id {
        self.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }
    pub fn next_member_id(&self) -> Id {
        self.members.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }
    pub fn next_section_id(&self) -> Id {
        self.sections.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }
    pub fn next_material_id(&self) -> Id {
        self.materials.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }
    pub fn next_case_id(&self) -> Id {
        self.cases.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }
    pub fn next_combo_id(&self) -> Id {
        self.combos.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }
    pub fn next_slab_id(&self) -> Id {
        self.slabs.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }

    /// The node at `p` (within 5 mm), made if there is none.
    pub fn node_at(&mut self, p: [f64; 3]) -> Id {
        if let Some(n) = self.find_node(p) {
            return n;
        }
        let id = self.next_node_id();
        self.nodes.push(Node { id, x: p[0], y: p[1], z: p[2] });
        id
    }

    pub fn find_node(&self, p: [f64; 3]) -> Option<Id> {
        self.nodes.iter().find(|n| (n.x - p[0]).abs() < TOL && (n.y - p[1]).abs() < TOL && (n.z - p[2]).abs() < TOL).map(|n| n.id)
    }

    /// The section named `name`; a catalogue steel shape ("H 300x150x6.5x9") or a
    /// rectangle written "300x600" / "300 x 600 mm" (b x h in mm) is added when missing.
    pub fn section_named(&mut self, name: &str) -> Result<Id, String> {
        let key = |s: &str| s.to_lowercase().replace([' ', '-'], "").replace("mm", "");
        if let Some(s) = self.sections.iter().find(|s| key(&s.name) == key(name)) {
            return Ok(s.id);
        }
        let id = self.next_section_id();
        if let Some(sh) = crate::sections::find(name) {
            self.sections.push(Section { id, name: sh.name.clone(), shape: sh.shape() });
            return Ok(id);
        }
        if let Some((h, b, d, t)) = parse_lipped(name) {
            self.sections.push(Section { id, name: format!("C {:.0}x{:.0}x{:.0}x{}", h * 1e3, b * 1e3, d * 1e3, t * 1e3), shape: Shape::LippedC { h, b, d, t } });
            return Ok(id);
        }
        if let Some((b, h)) = parse_rect(name) {
            self.sections.push(Section { id, name: rect_name(b, h), shape: Shape::Rect { b, h } });
            return Ok(id);
        }
        Err(format!("no section '{name}': give a rectangle as 'b x h' in mm (e.g. '300 x 600') or a steel shape from list_sections (e.g. 'H 300x150x6.5x9'); the model has {:?}", self.sections.iter().map(|s| &s.name).collect::<Vec<_>>()))
    }

    pub fn rect_section(&mut self, b: f64, h: f64) -> Id {
        self.section_named(&rect_name(b, h)).expect("a rectangle always parses")
    }

    pub fn length(&self, m: &Member) -> f64 {
        match (self.node(m.i), self.node(m.j)) {
            (Some(a), Some(b)) => dist(a.p(), b.p()),
            _ => 0.0,
        }
    }

    /// The member's kind, with Auto resolved from its direction.
    pub fn kind(&self, m: &Member) -> MemberKind {
        if m.kind != MemberKind::Auto {
            return m.kind;
        }
        let (Some(a), Some(b)) = (self.node(m.i), self.node(m.j)) else { return MemberKind::Brace };
        let l = dist(a.p(), b.p()).max(1e-9);
        let dz = (b.z - a.z).abs() / l;
        if dz < 0.02 {
            MemberKind::Beam
        } else if dz > 0.999 {
            MemberKind::Column
        } else {
            MemberKind::Brace
        }
    }

    pub fn is_steel(&self, m: &Member) -> bool {
        self.material(m.material).is_some_and(|x| x.kind == MaterialKind::Steel)
    }
    pub fn is_concrete(&self, m: &Member) -> bool {
        self.material(m.material).is_some_and(|x| x.kind == MaterialKind::Concrete)
    }

    pub fn concrete_id(&self) -> Id {
        self.materials.iter().find(|m| m.kind == MaterialKind::Concrete).map(|m| m.id).unwrap_or(1)
    }
    pub fn steel_id(&self) -> Id {
        self.materials.iter().find(|m| m.kind == MaterialKind::Steel).map(|m| m.id).unwrap_or(2)
    }

    /// The dead and live cases slab loads go to: the first of each kind.
    pub fn slab_cases(&self) -> (Option<Id>, Option<Id>) {
        (self.cases.iter().find(|c| c.kind == CaseKind::D).map(|c| c.id), self.cases.iter().find(|c| c.kind == CaseKind::L).map(|c| c.id))
    }

    /// Deletes members (and their loads), nodes no member or support uses, and so on.
    pub fn delete_members(&mut self, ids: &[Id]) {
        self.members.retain(|m| !ids.contains(&m.id));
        self.loads.retain(|l| match &l.kind {
            LoadKind::Line { member, .. } | LoadKind::Point { member, .. } => !ids.contains(member),
            _ => true,
        });
    }

    pub fn delete_nodes(&mut self, ids: &[Id]) {
        let gone: Vec<Id> = self.members.iter().filter(|m| ids.contains(&m.i) || ids.contains(&m.j)).map(|m| m.id).collect();
        self.delete_members(&gone);
        self.nodes.retain(|n| !ids.contains(&n.id));
        self.supports.retain(|s| !ids.contains(&s.node));
        self.loads.retain(|l| !matches!(&l.kind, LoadKind::Nodal { node, .. } if ids.contains(node)));
        self.masses.retain(|m| !ids.contains(&m.node));
    }

    /// Nodes not used by any member, support, load or mass.
    pub fn purge_nodes(&mut self) {
        let used: std::collections::HashSet<Id> = self.members.iter().flat_map(|m| [m.i, m.j]).chain(self.supports.iter().map(|s| s.node)).chain(self.loads.iter().filter_map(|l| if let LoadKind::Nodal { node, .. } = l.kind { Some(node) } else { None })).chain(self.masses.iter().map(|m| m.node)).collect();
        self.nodes.retain(|n| used.contains(&n.id));
    }
}

pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub fn rect_name(b: f64, h: f64) -> String {
    format!("{:.0} x {:.0} mm", b * 1000.0, h * 1000.0)
}

/// "C 200x75x20x2.0" (lipped channel h x b x lip x t, mm) → m.
pub fn parse_lipped(s: &str) -> Option<(f64, f64, f64, f64)> {
    let t = s.trim();
    let rest = t.strip_prefix('C').or_else(|| t.strip_prefix('c'))?;
    let v: Vec<f64> = rest.split(['x', 'X', '×']).map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
    (v.len() == 4 && v[3] < 10.0 && v.iter().all(|x| *x > 0.0)).then(|| (v[0] / 1e3, v[1] / 1e3, v[2] / 1e3, v[3] / 1e3))
}

/// "300x600", "300 x 600 mm", "RC 300x600" → (0.3, 0.6) m.
pub fn parse_rect(s: &str) -> Option<(f64, f64)> {
    let t = s.to_lowercase().replace("mm", "").replace("rc", "");
    let parts: Vec<&str> = t.split(['x', '×']).map(str::trim).collect();
    if parts.len() != 2 {
        return None;
    }
    let b: f64 = parts[0].parse().ok()?;
    let h: f64 = parts[1].parse().ok()?;
    (b > 50.0 && h > 50.0 && b < 5000.0 && h < 5000.0).then_some((b / 1000.0, h / 1000.0))
}

/// An item by its id. Ids are handed out in increasing order and lists keep that order, so the
/// item is usually at index id - 1, or else found by bisection; a list out of order falls back to
/// a scan. On a 7000-member tower the linear search alone made every redraw take about 2 s.
fn by_id<T>(v: &[T], id: Id, key: impl Fn(&T) -> Id) -> Option<&T> {
    if let Some(x) = id.checked_sub(1).and_then(|k| v.get(k)).filter(|x| key(x) == id) {
        return Some(x);
    }
    if let Ok(k) = v.binary_search_by_key(&id, &key) {
        return Some(&v[k]);
    }
    v.iter().find(|x| key(x) == id)
}
