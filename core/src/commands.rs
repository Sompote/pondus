// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The commands that build and change a model, with their JSON schemas: the agent calls
//! them through the MCP server and the app through the same functions. Each returns what it
//! did as JSON, and errors say what to give instead.

use serde_json::{json, Value};

use crate::model::*;

pub struct Spec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
    /// Whether it changes the model (the server saves after it).
    pub changes: bool,
}

fn s(name: &'static str, description: &'static str, changes: bool, schema: Value) -> Spec {
    Spec { name, description, schema, changes }
}

pub fn specs() -> Vec<Spec> {
    let p3 = json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3});
    let ids = json!({"type": "array", "items": {"type": "integer"}});
    vec![
        s("get_model", "The model: a summary (counts, sections, materials, cases, combinations, slabs, design settings), or with detail true every node, member, support and load.", false, json!({"type": "object", "properties": {"detail": {"type": "boolean"}}})),
        s("new_model", "Starts an empty model (concrete f'c 24 MPa, steel SS400, cases D with self-weight and L, combinations 1.4D, 1.2D + 1.6L and SLS D + L).", true, json!({"type": "object", "properties": {"name": {"type": "string"}}})),
        s("add_frame", "Adds a regular building frame: columns at every grid intersection for each storey, beams on the grid lines at every floor, supports under the columns, and optionally a slab panel in every bay of every floor. x_spans / y_spans are bay widths (m) from the origin; without y_spans it is a plane frame in x-z. Sections are names: concrete 'b x h' in mm ('300 x 600'), steel from list_sections ('H 300x150x6.5x9').", true, json!({"type": "object", "properties": {
            "x_spans": {"type": "array", "items": {"type": "number"}}, "y_spans": {"type": "array", "items": {"type": "number"}}, "storeys": {"type": "array", "items": {"type": "number"}, "description": "storey heights, m, from the bottom"},
            "material": {"type": "string", "enum": ["concrete", "steel"]}, "column": {"type": "string"}, "beam": {"type": "string"}, "beam_y": {"type": "string", "description": "section of the beams along y (default: beam)"},
            "base": {"type": "string", "enum": ["fixed", "pinned"]}, "origin": p3, "column_roll": {"type": "number", "description": "degrees; default turns the columns' strong axis into the direction of the longer spans"},
            "slab": {"type": "object", "properties": {"h": {"type": "number", "description": "thickness, m"}, "dead": {"type": "number", "description": "superimposed dead, kPa"}, "live": {"type": "number", "description": "kPa"}, "roof_live": {"type": "number", "description": "live on the top floor, kPa (default: live)"}}}
        }, "required": ["x_spans", "storeys", "column", "beam"]})),
        s("add_members", "Adds members between points (nodes are made or reused at the points). section: a name as in add_frame; material 'concrete' or 'steel' (default from the section: H shapes steel); kind beam/column/brace (default from the direction); hinge_i/hinge_j pin the ends; lb the unbraced length of the compression flange and k the effective length factor (steel); roll turns the section about the member axis (degrees; a column with roll 0 bends about its strong axis in the x-z plane, roll 90 in the y-z plane); bim_id the BIM element it stands for; group shares one section when steel is selected.", true, json!({"type": "object", "properties": {"members": {"type": "array", "items": {"type": "object", "properties": {"from": p3, "to": p3, "section": {"type": "string"}, "material": {"type": "string"}, "kind": {"type": "string", "enum": ["beam", "column", "brace"]}, "hinge_i": {"type": "boolean"}, "hinge_j": {"type": "boolean"}, "lb": {"type": "number"}, "k": {"type": "number"}, "roll": {"type": "number"}, "bim_id": {"type": "string"}, "group": {"type": "string"}}, "required": ["from", "to", "section"]}}}, "required": ["members"]})),
        s("set_members", "Changes members: section (a name), material, lb, k, hinges, kind, behaviour, winkler (elastic foundation), bim_id or group. Give the member ids, or 'where': {kind, z (beams at that height), section (by name)} to select them.", true, json!({"type": "object", "properties": {"members": ids, "where": {"type": "object", "properties": {"kind": {"type": "string"}, "z": {"type": "number"}, "section": {"type": "string"}}}, "section": {"type": "string"}, "material": {"type": "string"}, "lb": {"type": "number"}, "k": {"type": "number"}, "roll": {"type": "number"}, "truss": {"type": "boolean", "description": "axial force only"}, "behaviour": {"type": "string", "enum": ["normal", "tension_only", "compression_only", "cable"], "description": "tension_only (bracing that buckles out), compression_only (struts, contact), cable (tension only with sag, solved nonlinearly); the model is then solved combination by combination"}, "winkler": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "a beam on elastic foundation: [kz, ky] subgrade reaction per length, kN/m per m (k_s x width), 0 = none; [0, 0] removes it"}, "tension": {"type": "object", "description": "this member's end connection for tensile rupture, as set_design tension_connection; null removes it"}, "hinge_i": {"type": "boolean"}, "hinge_j": {"type": "boolean"}, "kind": {"type": "string"}, "bim_id": {"type": "string"}, "group": {"type": "string"}}})),
        s("add_supports", "Supports at points or nodes: fixed, pinned (translations held), roller (vertical held) or spring: springs [kx, ky, kz, krx, kry, krz] (kN/m, kN·m/rad; 0 = free) on the DOFs a type leaves free (type 'spring' holds none, e.g. a footing on soil: kz = subgrade modulus x area).", true, json!({"type": "object", "properties": {"points": {"type": "array", "items": p3}, "nodes": ids, "type": {"type": "string", "enum": ["fixed", "pinned", "roller", "spring"]}, "springs": {"type": "array", "items": {"type": "number"}, "minItems": 6, "maxItems": 6}, "uplift": {"type": "boolean", "description": "the support takes compression only and lifts off under tension (solved nonlinearly)"}}, "required": ["type"]})),
        s("add_masses", "Lumped masses for the earthquake (modal and response spectrum) analysis, at nodes or points: mass (t, the same in x, y and z) or masses [mx, my, mz] (t), and rotational [Ixx, Iyy, Izz] (t·m², e.g. a floor's mass moment of inertia about z at its centre of mass). A point where no node exists gets a new node, which must lie on a rigid diaphragm level (set_diaphragms) unless a member ends there. They add to the mass of the load cases (D + live_mass·L) and to the seismic weight; replace true replaces the masses already at those nodes.", true, json!({"type": "object", "properties": {"nodes": ids, "points": {"type": "array", "items": p3}, "mass": {"type": "number"}, "masses": {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3}, "rotational": {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3}, "replace": {"type": "boolean"}}})),
        s("set_diaphragms", "Rigid floor diaphragms: the nodes of each level (z, m) move together in plan (in-plane rigid). 'levels': [z, ...], or 'all': true for every level above the supports, or 'levels': [] to remove them. Use with coefficient slabs for lateral (earthquake, wind) analysis; plate slabs are diaphragms by themselves.", true, json!({"type": "object", "properties": {"levels": {"type": "array", "items": {"type": "number"}}, "all": {"type": "boolean"}}})),
        s("add_material", "Adds a material: concrete with fc (MPa); steel with fy and fu (MPa); timber (NDS reference design values fb, ft, fc, fv, e, e_min in MPa; default Douglas Fir-Larch No. 2) or masonry (fm, MPa; default 13.8). Returns its id; use its name in add_members / set_members. Timber and masonry members are rectangles 'b x h'; masonry members take their bars as rebar (mm²) in set_members. Cold-formed lipped channels are steel sections named 'C 200x75x20x2.0' (h x b x lip x t, mm).", true, json!({"type": "object", "properties": {"kind": {"type": "string", "enum": ["concrete", "steel", "timber", "masonry"]}, "name": {"type": "string"}, "fc": {"type": "number"}, "fy": {"type": "number"}, "fu": {"type": "number"}, "fb": {"type": "number"}, "ft": {"type": "number"}, "fv": {"type": "number"}, "e": {"type": "number"}, "e_min": {"type": "number"}, "fm": {"type": "number"}}, "required": ["kind"]})),
        s("set_material", "Changes a material's strengths: concrete fc (E follows as 4700 √fc), steel fy / fu.", true, json!({"type": "object", "properties": {"material": {"type": "string", "description": "name or id"}, "fc": {"type": "number"}, "fy": {"type": "number"}, "fu": {"type": "number"}}, "required": ["material"]})),
        s("add_load_case", "Adds a load case: kind D (dead), L (live), Lr (roof live), W, E or Other; self_weight adds the members' weight.", true, json!({"type": "object", "properties": {"name": {"type": "string"}, "kind": {"type": "string", "enum": ["D", "L", "Lr", "W", "E", "Other"]}, "self_weight": {"type": "boolean"}}, "required": ["name", "kind"]})),
        s("code_combinations", "Replaces the model's load combinations with those of a design standard, from the load case kinds (D, L, Lr, W): 'aci' (ASCE 7-22: 1.4D, 1.2D + 1.6L + 0.5Lr, 1.2D + 1.6Lr + L, 1.2D + L + 0.5Lr + 1.0W, 0.9D + 1.0W) or 'mr2566' / 'mr2566_noqc' (Thai Ministerial Regulation on structural design B.E. 2566, clause 7: 1.4D + 1.7L, 0.75(1.4D + 1.7L) + 1.6W, 0.9D + 1.6W), with SLS D + L. Also sets the design standard (its strength reduction factors); earthquake combinations are added at design from the seismic analysis. Without a standard it uses the current one.", true, json!({"type": "object", "properties": {"standard": {"type": "string", "enum": ["aci", "mr2566", "mr2566_noqc"]}}})),
        s("add_combination", "Adds a load combination: factors by case name, e.g. {\"D\": 1.2, \"L\": 1.6}; kind uls (strength, used to design) or sls (service, used for deflections).", true, json!({"type": "object", "properties": {"name": {"type": "string"}, "kind": {"type": "string", "enum": ["uls", "sls"]}, "factors": {"type": "object", "additionalProperties": {"type": "number"}}}, "required": ["name", "factors"]})),
        s("add_loads", "Adds loads to a case (name or id). Each load: {type: 'nodal', at: [x,y,z] or node, f: [fx,fy,fz,mx,my,mz] kN, kN·m}, {type: 'line', members: [ids] or z (all beams at that height), w: [wx,wy,wz] kN/m global (down is negative z), w_end for a varying load, a/b m from node i for part of the member}, {type: 'point', member, a (m from node i), p: [px,py,pz] kN}, {type: 'settlement', at or node (a support), d: [dx,dy,dz,rx,ry,rz] m, rad (a settlement of 10 mm down: dz -0.01)} or {type: 'thermal', members, dt (uniform °C), dt_y, dt_z (gradients across the section, °C)}. Slab loads come from the slabs; do not add them again.", true, json!({"type": "object", "properties": {"case": {"type": "string", "description": "case name (or id)"}, "loads": {"type": "array", "items": {"type": "object"}}}, "required": ["case", "loads"]})),
        s("add_mat", "Adds a pile cap or a mat (raft) foundation as a plate panel resting on its supports, with no beams: the panel x0, y0, x1, y1 (m) at z (its top, default 0) and thickness h (m, default 0.6); the piles as points [[x, y], ...] or as pile_grid {nx, ny, sx, sy (m), edge (m, pile centre to the mat edge, default half the spacing)} centred on the panel (without x0..y1 the panel is made around the grid); pile_k (kN/m, the pile's axial stiffness EA/L or from a load test, as a spring support: give it, because the cap then shares a column load between the piles by stiffness, whereas a pinned pile right under a column load takes all of it and the cap does not bend; 0 = pinned), pile_size (m, pile or pedestal width for punching, default 0.3), uplift (piles take compression only); subgrade (kN/m³, soil under the mat as Winkler springs: a mat on soil needs no piles). Sets slab_method 'plate' and a mesh of a quarter of the pile spacing. Then put the column loads with add_loads nodal at [x, y, z] (a node is made on the mat) or add_members for the columns, analyze (get_results reactions true = the pile loads; node 0 = the soil), get_slab_results (moments) and design (bars of both faces, punching at every pile and column).", true, json!({"type": "object", "properties": {"x0": {"type": "number"}, "y0": {"type": "number"}, "x1": {"type": "number"}, "y1": {"type": "number"}, "z": {"type": "number"}, "h": {"type": "number"}, "piles": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}}, "pile_grid": {"type": "object", "properties": {"nx": {"type": "integer"}, "ny": {"type": "integer"}, "sx": {"type": "number"}, "sy": {"type": "number"}, "edge": {"type": "number"}}}, "pile_k": {"type": "number"}, "pile_size": {"type": "number"}, "uplift": {"type": "boolean"}, "subgrade": {"type": "number"}, "dead": {"type": "number"}, "live": {"type": "number"}, "name": {"type": "string"}}})),
        s("add_slabs", "Adds rectangular slab panels on the beams under their edges: {x0, y0, x1, y1, z (top, at the beams' level), h (m), dead (superimposed, kPa), live (kPa), name, bim_id, plate (true: a flat plate, a floor on columns with no beams, analysed as plate finite elements whatever the slab method and designed with column and middle strips, punching with the unbalanced moments and the Table 8.3.1.1 thickness), and for a panel on supports with no beams (a pile cap, a mat): subgrade (kN/m³, Winkler soil under it), mat (true: foundation design, bars of each face over the whole panel, punching at every support and column), support_size (m, the pile or pedestal width for punching)}. Their weight (24 kN/m³ x h + dead) goes to the first D case and live to the first L case, by the 45° lines (one-way past 2:1). Or 'bays': {z, h, dead, live} makes one panel in every closed bay of the beams at height z.", true, json!({"type": "object", "properties": {"panels": {"type": "array", "items": {"type": "object"}}, "bays": {"type": "object", "properties": {"z": {"type": "number"}, "h": {"type": "number"}, "dead": {"type": "number"}, "live": {"type": "number"}}}}})),
        s("add_stairs", "Adds reinforced concrete stairs of a house to design as one-way slabs (ACI 318-19): each {kind 'flight' | 'landing', width (m), waist (m: the flight's waist, or the landing's thickness), riser, tread, risers (flight), span (m, horizontal, between the supports), fixity 'continuous' (monolithic with the beams at both ends: wL²/10 at midspan and over the supports, default) | 'simple' (wL²/8), dead (finishes, kPa), live (kPa, default 3), flight_dead / flight_live (landing: the flights' unfactored reactions landing on it, kN per m of its width), name, bim_id, z0, z1}. A straight stair is one flight; a half-turn stair is two flights and a landing: give each flight its span to the landing and the landing the two flights' reactions (design once to read them). The design gives the main bars along the span, the top bars at the supports, the distribution bars, the shear check, the least thickness for deflection and the reactions on the supports.", true, json!({"type": "object", "properties": {"stairs": {"type": "array", "items": {"type": "object"}}}, "required": ["stairs"]})),
        s("delete", "Deletes members, nodes (with their members), slabs, walls, supports (by node), lumped masses (by node) or all loads of a case.", true, json!({"type": "object", "properties": {"members": ids, "nodes": ids, "slabs": ids, "walls": ids, "supports": ids, "masses": ids, "loads_of_case": {"type": "string", "description": "case name (or id)"}}})),
        s("set_design", "Design settings: fy and fyt (MPa, main bars and stirrups, default SD40 390 and SR24 235), cover_beam / cover_slab (m), beam_bars / stirrup_bars / slab_bars (diameters, mm), defl_live / defl_total (span / limit), steel_shapes (names to choose from), target_ratio (largest utilisation accepted when sizing), shear_deformation (true: frame members deform in shear too, Timoshenko beams as SAP2000 and ETABS use; off by default), second_order (true: every ULS combination is solved second order with P-Delta, and column slenderness magnifiers are then not applied), footing ({q_allow kPa, depth m (base below ground), gamma_soil kN/m³, mu, min_h m, bars mm, cover m}: spread footings are designed under the concrete columns), steel_code ('aisc' AISC 360-16 or 'ec3' EN 1993-1-1, for checks and for select), ec2_check (also check concrete beams to EN 1992-1-1 with their bars), steel_min_depth / steel_max_depth (mm, limits for select), connection ({bolt_d mm, fnv MPa, plate_t mm, plate_fy, plate_fu MPa, pitch, edge, e mm, weld mm, fexx MPa}: shear tabs designed at steel beam ends), vibration_hz (floor beams' f = 0.18 √(g/Δ) limit, 0 = off), tension_connection ({kind 'bolted' | 'welded' | 'none', connected 'flanges' | 'web' | 'all', bolt_d mm, holes_flange, holes_web (holes across each flange and the web in one section), bolts_per_line, length m}: the end connection of steel members in tension, from which the net area An and shear lag U of AISC Table D3.1 / the net area of EC3 6.2.3 follow; default bolted flanges, M20, 2 holes per flange, 3 bolts per line), slab_method ('coefficients' or 'plate': slabs as plate elements with the frame), slab_mesh (plate element size, m, default 0.5) and standard (the load and strength reduction factors: 'aci' ACI 318-19 / AISC 360-16 / ASCE 7-22, default; 'mr2566' the Thai Ministerial Regulation on structural design B.E. 2566 with specified construction standards and quality control, φ 0.90 flexure, 0.70 tied columns, 0.85 shear and torsion, and steel shear 0.90; 'mr2566_noqc' the same without them, φ 0.75, 0.60, 0.70; slab and stair loads 1.4D + 1.7L and the earthquake combinations 0.75(1.4D + 1.7L) ± 1.0E and 0.9D ± 1.0E follow it; use code_combinations to replace the model's combinations too).", true, json!({"type": "object", "properties": {"fy": {"type": "number"}, "fyt": {"type": "number"}, "cover_beam": {"type": "number"}, "cover_slab": {"type": "number"}, "beam_bars": {"type": "array", "items": {"type": "number"}}, "stirrup_bars": {"type": "array", "items": {"type": "number"}}, "slab_bars": {"type": "array", "items": {"type": "number"}}, "defl_live": {"type": "number"}, "defl_total": {"type": "number"}, "steel_shapes": {"type": "array", "items": {"type": "string"}}, "target_ratio": {"type": "number"}, "slab_method": {"type": "string", "enum": ["coefficients", "plate"]}, "slab_mesh": {"type": "number"}, "second_order": {"type": "boolean"}, "footing": {"type": "object"}, "steel_code": {"type": "string", "enum": ["aisc", "ec3"]}, "ec2_check": {"type": "boolean"}, "steel_min_depth": {"type": "number"}, "steel_max_depth": {"type": "number"}, "connection": {"type": "object"}, "vibration_hz": {"type": "number"}, "tension_connection": {"type": "object"}, "standard": {"type": "string", "enum": ["aci", "mr2566", "mr2566_noqc"]}}})),
        s("list_sections", "The steel H shapes of the catalogue (JIS G 3192 / TIS 1227), lightest first, with mass, depth and Ix / Zx; and the model's sections.", false, json!({"type": "object", "properties": {}})),
        s("add_walls", "Adds structural (shear) walls: each a straight wall in plan from 'from' [x, y] to 'to' [x, y] (m), thickness t (m, default 0.25), from z0 to z1 (default the lowest and highest level of the model), concrete by default. A wall is analysed as piers on its centre line with the wall's section (shear deformation included) and rigid arms at every level out to its ends, so beams and slab edges along it connect to it; it is designed per storey to ACI 318-19 Ch. 11 (and 18.10 special walls when set_seismic is on): distributed vertical and horizontal bars, in-plane shear, axial with in-plane and out-of-plane bending, boundary elements. Never model a wall as a column. Add the floors (add_frame) first so the walls meet them; 'levels' [z, ...] or 'storey_h' give the levels of a wall that has no floors yet. base 'fixed' (default) fixes the wall's base, 'none' leaves it to rest on what is below (a transfer).", true, json!({"type": "object", "properties": {"walls": {"type": "array", "items": {"type": "object", "properties": {"from": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "to": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "t": {"type": "number"}, "z0": {"type": "number"}, "z1": {"type": "number"}, "name": {"type": "string"}, "material": {"type": "string"}, "bim_id": {"type": "string"}}, "required": ["from", "to"]}}, "base": {"type": "string", "enum": ["fixed", "none"]}, "levels": {"type": "array", "items": {"type": "number"}}, "storey_h": {"type": "number"}}, "required": ["walls"]})),
        s("set_walls", "Changes walls (ids, or all with 'all': true): thickness t (m) or material.", true, json!({"type": "object", "properties": {"walls": ids, "all": {"type": "boolean"}, "t": {"type": "number"}, "material": {"type": "string"}}})),
        s("move_node", "Moves a node to a point (its members follow); landing on another node (within 1 mm) merges the two, and members left with no length are deleted.", true, json!({"type": "object", "properties": {"node": {"type": "integer"}, "to": p3}, "required": ["node", "to"]})),
        s("transform", "Moves, copies, rotates or mirrors members (ids, or all with 'all': true). action 'move' (d [dx,dy,dz] m; copy true with n copies makes an array at d, 2d, ...), 'rotate' (about a vertical axis through centre [x,y,z], angle degrees), or 'mirror' (about the plane through point with normal [nx,ny,nz], e.g. [1,0,0] for the plane x = const). copy keeps the originals. Copies merge with nodes already there, take their member loads (loads), supports and nodal loads at nodes they create (supports), and slabs whose four edges are all copied (slabs); a member that would land on an existing one is skipped. A moved member drags the members attached to it.", true, json!({"type": "object", "properties": {"members": ids, "all": {"type": "boolean"}, "action": {"type": "string", "enum": ["move", "rotate", "mirror"]}, "d": p3, "copy": {"type": "boolean"}, "n": {"type": "integer"}, "centre": p3, "angle": {"type": "number"}, "point": p3, "normal": p3, "loads": {"type": "boolean"}, "supports": {"type": "boolean"}, "slabs": {"type": "boolean"}}, "required": ["action"]})),
        s("split_member", "Splits a member into pieces at fractions 'at' of its length (e.g. [0.25, 0.5]) or into 'parts' equal pieces, keeping section, material, kind and behaviour; its line and point loads go to the pieces with the intensities interpolated, its end hinges stay at the outer ends.", true, json!({"type": "object", "properties": {"member": {"type": "integer"}, "at": {"type": "array", "items": {"type": "number"}}, "parts": {"type": "integer"}}, "required": ["member"]})),
        s("merge_members", "Merges two collinear members that share a node where nothing else connects (no other member, support or nodal load), with the same section and behaviour; their loads are remapped.", true, json!({"type": "object", "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}}, "required": ["a", "b"]})),
        s("check_model", "Problems in the model (missing sections, unconnected supports, members on each other, combinations of missing cases).", false, json!({"type": "object", "properties": {}})),
    ]
}

/// The node at `p`, made when the point lies on a slab panel (a plate on supports carries
/// supports and loads at any point of it), otherwise an error with `why`.
fn node_on_slab_or_member(m: &mut Model, p: [f64; 3], why: &str) -> Result<Id, String> {
    if let Some(n) = m.find_node(p) {
        return Ok(n);
    }
    if m.slabs.iter().any(|s| s.contains(p[0], p[1], p[2])) {
        return Ok(m.node_at(p));
    }
    Err(format!("no node at {p:?}: {why}"))
}

/// A pile cap or a mat: one plate panel with pile supports under it and / or a subgrade.
fn add_mat(m: &mut Model, a: &Value) -> Result<Value, String> {
    let z = num(a, "z").unwrap_or(0.0);
    let h = num(a, "h").unwrap_or(0.6);
    if !(0.1..=5.0).contains(&h) {
        return Err("h: the mat's thickness in m (0.1 to 5)".into());
    }
    let mut piles: Vec<[f64; 2]> = vec![];
    for p in a.get("piles").and_then(Value::as_array).into_iter().flatten() {
        let q = p.as_array().filter(|q| q.len() >= 2).ok_or("piles: [[x, y], ...] in m")?;
        piles.push([q[0].as_f64().ok_or("pile x")?, q[1].as_f64().ok_or("pile y")?]);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (num(a, "x0"), num(a, "y0"), num(a, "x1"), num(a, "y1"));
    let mut spacing = f64::MAX;
    if let Some(g) = a.get("pile_grid") {
        let nx = g.get("nx").and_then(Value::as_u64).unwrap_or(3).max(1) as usize;
        let ny = g.get("ny").and_then(Value::as_u64).unwrap_or(nx as u64).max(1) as usize;
        let sx = num(g, "sx").or(num(g, "s")).unwrap_or(1.2);
        let sy = num(g, "sy").unwrap_or(sx);
        if sx <= 0.05 || sy <= 0.05 {
            return Err("pile_grid: sx and sy are the pile spacings in m".into());
        }
        spacing = sx.min(sy);
        let edge = num(g, "edge").unwrap_or(sx.max(sy) / 2.0);
        let (cx, cy) = match (x0, x1, y0, y1) {
            (Some(a0), Some(a1), Some(b0), Some(b1)) => ((a0 + a1) / 2.0, (b0 + b1) / 2.0),
            _ => {
                let c = a.get("centre").or(a.get("center")).and_then(Value::as_array);
                (c.and_then(|c| c.first()).and_then(Value::as_f64).unwrap_or(0.0), c.and_then(|c| c.get(1)).and_then(Value::as_f64).unwrap_or(0.0))
            }
        };
        for i in 0..nx {
            for j in 0..ny {
                piles.push([cx + (i as f64 - (nx - 1) as f64 / 2.0) * sx, cy + (j as f64 - (ny - 1) as f64 / 2.0) * sy]);
            }
        }
        if x0.is_none() || x1.is_none() || y0.is_none() || y1.is_none() {
            x0 = Some(cx - (nx - 1) as f64 / 2.0 * sx - edge);
            x1 = Some(cx + (nx - 1) as f64 / 2.0 * sx + edge);
            y0 = Some(cy - (ny - 1) as f64 / 2.0 * sy - edge);
            y1 = Some(cy + (ny - 1) as f64 / 2.0 * sy + edge);
        }
    }
    let (x0, y0, x1, y1) = match (x0, y0, x1, y1) {
        (Some(a0), Some(b0), Some(a1), Some(b1)) => (a0, b0, a1, b1),
        _ if !piles.is_empty() => {
            let edge = num(a, "edge").unwrap_or(0.6);
            let (xs, ys): (Vec<f64>, Vec<f64>) = piles.iter().map(|p| (p[0], p[1])).unzip();
            (xs.iter().copied().fold(f64::MAX, f64::min) - edge, ys.iter().copied().fold(f64::MAX, f64::min) - edge, xs.iter().copied().fold(f64::MIN, f64::max) + edge, ys.iter().copied().fold(f64::MIN, f64::max) + edge)
        }
        _ => return Err("give the panel x0, y0, x1, y1 (m), or piles / a pile_grid to make it around".into()),
    };
    if (x1 - x0).abs() < 0.2 || (y1 - y0).abs() < 0.2 {
        return Err("the mat is smaller than 0.2 m on a side".into());
    }
    let subgrade = num(a, "subgrade").unwrap_or(0.0).max(0.0);
    if piles.is_empty() && subgrade <= 0.0 {
        return Err("a mat needs piles (piles or pile_grid) or a subgrade modulus (kN/m³) under it".into());
    }
    for (i, p) in piles.iter().enumerate() {
        if p[0] < x0.min(x1) - 1e-6 || p[0] > x0.max(x1) + 1e-6 || p[1] < y0.min(y1) - 1e-6 || p[1] > y0.max(y1) + 1e-6 {
            return Err(format!("pile {} at {p:?} lies outside the mat", i + 1));
        }
        for q in &piles[i + 1..] {
            spacing = spacing.min(((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt());
        }
    }
    let id = m.next_slab_id();
    let name = a.get("name").and_then(Value::as_str).map(String::from).unwrap_or(format!("MAT{id}"));
    m.slabs.push(Slab { id, name: name.clone(), x0, y0, x1, y1, z, h, dead: num(a, "dead").unwrap_or(0.0), live: num(a, "live").unwrap_or(0.0), bim_id: None, subgrade, mat: true, support_size: num(a, "pile_size").unwrap_or(0.3), plate: false });
    let k = num(a, "pile_k").unwrap_or(0.0);
    let uplift = a.get("uplift").and_then(Value::as_bool).unwrap_or(false);
    let mut nodes = vec![];
    for p in &piles {
        let n = m.node_at([p[0], p[1], z]);
        m.supports.retain(|s| s.node != n);
        m.supports.push(if k > 0.0 { Support { node: n, fix: [true, true, false, false, false, false], springs: [0.0, 0.0, k, 0.0, 0.0, 0.0], uplift } } else { Support { node: n, fix: [true, true, true, false, false, false], springs: [0.0; 6], uplift } });
        nodes.push(n);
    }
    m.design.slab_method = crate::slabfe::SlabMethod::Plate;
    let side = (x1 - x0).abs().min((y1 - y0).abs());
    let want = if spacing < f64::MAX { spacing / 4.0 } else { side / 8.0 };
    m.design.slab_mesh = m.design.slab_mesh.min(want).clamp(0.1, 1.0);
    let plan = [x0, y0, x1, y1].map(|v| (v * 1000.0).round() / 1000.0);
    Ok(json!({
        "slab": id, "name": name, "plan": plan, "z": z, "h": h, "piles": nodes, "pile_support": if k > 0.0 { format!("spring kz {k} kN/m, held in plan") } else { "pinned".into() },
        "subgrade": subgrade, "slab_method": "plate", "slab_mesh": m.design.slab_mesh,
        "next": "column loads: add_loads nodal at [x, y, z] (a node is made on the mat) or add_members for the columns; then analyze (reactions balance), get_results reactions true (pile loads), get_slab_results (mx, my per metre), design (bars of both faces, punching at every pile and column)"
    }))
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn p3(v: &Value) -> Result<[f64; 3], String> {
    let a = v.as_array().ok_or("a point is [x, y, z]")?;
    if a.len() != 3 {
        return Err("a point is [x, y, z]".into());
    }
    Ok([a[0].as_f64().ok_or("x")?, a[1].as_f64().ok_or("y")?, a[2].as_f64().ok_or("z")?])
}

fn ids(v: &Value, k: &str) -> Vec<Id> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as Id)).collect()).unwrap_or_default()
}

pub fn case_id(m: &Model, v: &Value) -> Result<Id, String> {
    if let Some(i) = v.as_u64() {
        if m.case(i as Id).is_some() {
            return Ok(i as Id);
        }
    }
    let s = v.as_str().unwrap_or("");
    m.cases.iter().find(|c| c.name.eq_ignore_ascii_case(s) || c.id.to_string() == s).map(|c| c.id).ok_or_else(|| format!("no load case '{v}'; the cases are {:?}", m.cases.iter().map(|c| &c.name).collect::<Vec<_>>()))
}

fn material_id(m: &Model, v: Option<&Value>, section: &Section) -> Result<Id, String> {
    match v {
        None => Ok(if matches!(section.shape, Shape::H { .. } | Shape::LippedC { .. }) { m.steel_id() } else { m.concrete_id() }),
        Some(v) => {
            if let Some(i) = v.as_u64() {
                return m.material(i as Id).map(|x| x.id).ok_or_else(|| format!("no material {i}"));
            }
            let s = v.as_str().unwrap_or("").to_lowercase();
            if s == "concrete" {
                return Ok(m.concrete_id());
            }
            if s == "steel" {
                return Ok(m.steel_id());
            }
            m.materials.iter().find(|x| x.name.to_lowercase() == s).map(|x| x.id).ok_or_else(|| format!("no material '{s}': 'concrete', 'steel' or one of {:?}", m.materials.iter().map(|x| &x.name).collect::<Vec<_>>()))
        }
    }
}

fn kind_of(s: &str) -> MemberKind {
    match s {
        "beam" => MemberKind::Beam,
        "column" => MemberKind::Column,
        "brace" => MemberKind::Brace,
        _ => MemberKind::Auto,
    }
}

/// A short description of the model, for the agent's prompt and get_model.
pub fn summary(m: &Model) -> Value {
    let count = |k: MemberKind| m.members.iter().filter(|x| x.role.is_frame() && m.kind(x) == k).count();
    let ext = crate::scene::extent(m);
    let mut levels: Vec<f64> = m.nodes.iter().map(|n| (n.z * 1000.0).round() / 1000.0).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    json!({
        "name": m.name,
        "nodes": m.nodes.len(),
        "members": {"beams": count(MemberKind::Beam), "columns": count(MemberKind::Column), "braces": count(MemberKind::Brace)},
        "extent_m": ext.map(|(lo, hi)| json!({"min": lo, "max": hi})),
        "levels_z": levels,
        "materials": m.materials.iter().map(|x| json!({"id": x.id, "name": x.name, "kind": x.kind, "fc": x.fc, "fy": x.fy, "E": x.e})).collect::<Vec<_>>(),
        "sections": m.sections.iter().map(|s| json!({"id": s.id, "name": s.name, "used_by": m.members.iter().filter(|x| x.section == s.id).count()})).collect::<Vec<_>>(),
        "supports": m.supports.len(),
        "cases": m.cases.iter().map(|c| json!({"id": c.id, "name": c.name, "kind": c.kind, "self_weight": c.self_weight, "loads": m.loads.iter().filter(|l| l.case == c.id).count()})).collect::<Vec<_>>(),
        "combinations": m.combos.iter().map(|c| json!({"id": c.id, "name": c.name, "kind": c.kind, "factors": c.factors.iter().map(|(id, f)| (m.case(*id).map(|c| c.name.clone()).unwrap_or(id.to_string()), *f)).collect::<std::collections::BTreeMap<String, f64>>()})).collect::<Vec<_>>(),
        "slabs": m.slabs.len(),
        "walls": m.walls.iter().map(|w| json!({"id": w.id, "name": w.name, "from": w.a, "to": w.b, "t_mm": (w.t * 1e3).round(), "length_m": (w.length() * 1000.0).round() / 1000.0, "z": [w.z0, w.z1], "piers": m.members.iter().filter(|x| x.role == Role::WallPier(w.id)).count()})).collect::<Vec<_>>(),
        "design": m.design,
    })
}

/// Runs a command; afterwards the walls are rebuilt where the model's levels changed (a new
/// floor gives every wall through it a pier joint there).
pub fn run(m: &mut Model, name: &str, a: &Value) -> Result<Value, String> {
    let out = run_inner(m, name, a)?;
    if !m.walls.is_empty() && !matches!(name, "get_model" | "list_sections" | "check_model") {
        crate::walls::sync(m);
    }
    Ok(out)
}

fn run_inner(m: &mut Model, name: &str, a: &Value) -> Result<Value, String> {
    match name {
        "get_model" => {
            if a.get("detail").and_then(Value::as_bool).unwrap_or(false) {
                serde_json::to_value(&*m).map_err(|e| e.to_string())
            } else {
                Ok(summary(m))
            }
        }
        "new_model" => {
            *m = Model::new(a.get("name").and_then(Value::as_str).unwrap_or("Untitled"));
            Ok(json!({"ok": true}))
        }
        "add_frame" => add_frame(m, a),
        "add_mat" => add_mat(m, a),
        "add_members" => {
            let list = a.get("members").and_then(Value::as_array).ok_or("give members: [{from, to, section}]")?;
            let mut made = vec![];
            let mut skipped: Vec<String> = vec![];
            for e in list {
                let from = p3(e.get("from").ok_or("from")?)?;
                let to = p3(e.get("to").ok_or("to")?)?;
                if dist(from, to) < 1e-3 {
                    return Err(format!("a member from {from:?} to {to:?} has no length"));
                }
                let sid = m.section_named(e.get("section").and_then(Value::as_str).ok_or("each member needs a section")?)?;
                let sec = m.section(sid).cloned().unwrap();
                let mat = material_id(m, e.get("material"), &sec)?;
                let (i, j) = (m.node_at(from), m.node_at(to));
                // a member on top of an existing one (the same two nodes) is not added again
                if let Some(x) = m.members.iter().find(|x| (x.i == i && x.j == j) || (x.i == j && x.j == i)) {
                    skipped.push(format!("member {} already joins {from:?} and {to:?}", x.id));
                    continue;
                }
                let id = m.next_member_id();
                m.members.push(Member {
                    id,
                    i,
                    j,
                    section: sid,
                    material: mat,
                    kind: kind_of(e.get("kind").and_then(Value::as_str).unwrap_or("")),
                    hinge_i: e.get("hinge_i").and_then(Value::as_bool).unwrap_or(false),
                    hinge_j: e.get("hinge_j").and_then(Value::as_bool).unwrap_or(false),
                    lb: num(e, "lb"),
                    k: num(e, "k"),
                    roll: num(e, "roll").unwrap_or(0.0),
                    truss: e.get("truss").and_then(Value::as_bool).unwrap_or(false),
                    behaviour: Default::default(),
                    winkler: None,
                    rebar: num(e, "rebar"),
                    bim_id: e.get("bim_id").and_then(Value::as_str).map(String::from),
                    tension: e.get("tension").and_then(|t| serde_json::from_value(t.clone()).ok()), role: Default::default(),
                    group: e.get("group").and_then(Value::as_str).map(String::from),
                });
                made.push(id);
            }
            if skipped.is_empty() {
                Ok(json!({"members": made}))
            } else {
                Ok(json!({"members": made, "skipped": skipped}))
            }
        }
        "set_members" => {
            let mut sel = ids(a, "members");
            if let Some(w) = a.get("where") {
                let kind = w.get("kind").and_then(Value::as_str).map(kind_of);
                let z = num(w, "z");
                let sec = w.get("section").and_then(Value::as_str).map(|s| s.to_lowercase().replace(' ', ""));
                for x in &m.members {
                    let ok_k = kind.is_none_or(|k| m.kind(x) == k);
                    let ok_z = z.is_none_or(|z| [x.i, x.j].iter().all(|n| m.node(*n).is_some_and(|n| (n.z - z).abs() < 0.05)));
                    let ok_s = sec.as_ref().is_none_or(|s| m.section(x.section).is_some_and(|q| q.name.to_lowercase().replace(' ', "") == *s));
                    // a selection by kind, height or section is of frame members, not the walls' piers and arms
                    if ok_k && ok_z && ok_s && x.role.is_frame() {
                        sel.push(x.id);
                    }
                }
            }
            if sel.is_empty() {
                return Err("no members selected: give members [ids] or where {kind, z, section}".into());
            }
            let sid = match a.get("section").and_then(Value::as_str) {
                Some(s) => Some(m.section_named(s)?),
                None => None,
            };
            let mat = match a.get("material") {
                Some(v) => Some(material_id(m, Some(v), &Section { id: 0, name: String::new(), shape: Shape::Rect { b: 1.0, h: 1.0 } })?),
                None => None,
            };
            let mut n = 0;
            for x in m.members.iter_mut().filter(|x| sel.contains(&x.id)) {
                if let Some(s) = sid {
                    x.section = s;
                }
                if let Some(t) = mat {
                    x.material = t;
                }
                if let Some(v) = num(a, "lb") {
                    x.lb = Some(v);
                }
                if let Some(v) = num(a, "k") {
                    x.k = Some(v);
                }
                if let Some(v) = num(a, "roll") {
                    x.roll = v;
                }
                if let Some(v) = a.get("truss").and_then(Value::as_bool) {
                    x.truss = v;
                }
                if let Some(v) = num(a, "rebar") {
                    x.rebar = Some(v);
                }
                if let Some(v) = a.get("behaviour").and_then(Value::as_str) {
                    x.behaviour = serde_json::from_value(json!(v)).map_err(|_| format!("behaviour '{v}': normal, tension_only, compression_only or cable"))?;
                }
                if let Some(v) = a.get("tension") {
                    x.tension = if v.is_null() { None } else { Some(serde_json::from_value(v.clone()).map_err(|e| format!("tension: {e}"))?) };
                }
                if let Some(v) = a.get("winkler").and_then(Value::as_array) {
                    let k = [v.first().and_then(Value::as_f64).unwrap_or(0.0).max(0.0), v.get(1).and_then(Value::as_f64).unwrap_or(0.0).max(0.0)];
                    x.winkler = (k[0] > 0.0 || k[1] > 0.0).then_some(k);
                }
                if let Some(v) = a.get("hinge_i").and_then(Value::as_bool) {
                    x.hinge_i = v;
                }
                if let Some(v) = a.get("hinge_j").and_then(Value::as_bool) {
                    x.hinge_j = v;
                }
                if let Some(v) = a.get("kind").and_then(Value::as_str) {
                    x.kind = kind_of(v);
                }
                if let Some(v) = a.get("bim_id").and_then(Value::as_str) {
                    x.bim_id = Some(v.into());
                }
                if let Some(v) = a.get("group").and_then(Value::as_str) {
                    x.group = Some(v.into());
                }
                n += 1;
            }
            let used: Vec<Id> = m.members.iter().map(|x| x.section).collect();
            m.sections.retain(|s| used.contains(&s.id));
            Ok(json!({"changed": n}))
        }
        "add_supports" => {
            let kind = a.get("type").and_then(Value::as_str).unwrap_or("fixed");
            let mut nodes = ids(a, "nodes");
            for p in a.get("points").and_then(Value::as_array).into_iter().flatten() {
                let p = p3(p)?;
                nodes.push(node_on_slab_or_member(m, p, "supports go where members end, or on a slab panel with slab_method 'plate' (a pile under a cap, a pedestal under a mat)")?);
            }
            let springs: [f64; 6] = match a.get("springs").and_then(Value::as_array) {
                Some(v) if v.len() == 6 => [0, 1, 2, 3, 4, 5].map(|k| v[k].as_f64().unwrap_or(0.0)),
                Some(_) => return Err("springs: six values [kx, ky, kz, krx, kry, krz]".into()),
                None => [0.0; 6],
            };
            for n in &nodes {
                m.supports.retain(|s| s.node != *n);
                let mut sp = if kind == "spring" { Support { node: *n, fix: [false; 6], springs: [0.0; 6], uplift: false } } else { Support::of_kind(*n, kind).ok_or("type is fixed, pinned, roller or spring")? };
                for q in 0..6 {
                    if !sp.fix[q] {
                        sp.springs[q] = springs[q].max(0.0);
                    }
                }
                sp.springs = sp.springs.map(|k| k.max(0.0));
                sp.uplift = a.get("uplift").and_then(Value::as_bool).unwrap_or(false);
                m.supports.push(sp);
            }
            Ok(json!({"supports": nodes.len()}))
        }
        "add_masses" => {
            let mut nodes = ids(a, "nodes");
            for p in a.get("points").and_then(Value::as_array).into_iter().flatten() {
                let p = p3(p)?;
                nodes.push(m.node_at(p));
            }
            if nodes.is_empty() {
                return Err("give nodes or points".into());
            }
            let three = |k: &str| -> Option<[f64; 3]> { a.get(k).and_then(Value::as_array).filter(|v| v.len() == 3).map(|v| [0, 1, 2].map(|i| v[i].as_f64().unwrap_or(0.0))) };
            let mass = three("masses").or_else(|| num(a, "mass").map(|x| [x; 3])).unwrap_or([0.0; 3]);
            let rot = three("rotational").unwrap_or([0.0; 3]);
            if mass.iter().chain(rot.iter()).any(|x| *x < 0.0) || mass.iter().chain(rot.iter()).all(|x| *x == 0.0) {
                return Err("give a positive mass (t) or masses [mx, my, mz], and rotational [Ixx, Iyy, Izz] (t·m²) if needed".into());
            }
            let replace = a.get("replace").and_then(Value::as_bool).unwrap_or(false);
            for n in &nodes {
                if replace {
                    m.masses.retain(|x| x.node != *n);
                }
                m.masses.push(NodalMass { node: *n, m: mass, r: rot });
            }
            let total: f64 = m.masses.iter().map(|x| x.m[0]).sum();
            Ok(json!({"masses": nodes.len(), "nodes": nodes, "total_mass_x_t": total}))
        }
        "add_material" => {
            let id = m.next_material_id();
            let mt = match a.get("kind").and_then(Value::as_str) {
                Some("steel") => Material::steel(id, a.get("name").and_then(Value::as_str).unwrap_or("Steel"), num(a, "fy").unwrap_or(245.0), num(a, "fu").unwrap_or(400.0)),
                Some("timber") => {
                    let mut t = Material::timber(id);
                    for (k, f) in [("fb", &mut t.fb), ("ft", &mut t.ft), ("fc", &mut t.fc), ("fv", &mut t.fv), ("e", &mut t.e), ("e_min", &mut t.e_min)] {
                        if let Some(v) = num(a, k) {
                            *f = v;
                        }
                    }
                    if let Some(n) = a.get("name").and_then(Value::as_str) {
                        t.name = n.into();
                    }
                    t
                }
                Some("masonry") => Material::masonry(id, num(a, "fm").or(num(a, "fc")).unwrap_or(13.8)),
                _ => {
                    let mut c = Material::concrete(id, num(a, "fc").unwrap_or(24.0));
                    if let Some(n) = a.get("name").and_then(Value::as_str) {
                        c.name = n.into();
                    }
                    c
                }
            };
            let name = mt.name.clone();
            m.materials.push(mt);
            Ok(json!({"id": id, "name": name}))
        }
        "set_material" => {
            let key = a.get("material").ok_or("give the material")?;
            let k = key.as_u64().map(|x| x.to_string()).unwrap_or_else(|| key.as_str().unwrap_or("").to_lowercase());
            let mt = m.materials.iter_mut().find(|x| x.id.to_string() == k || x.name.to_lowercase() == k || (k == "concrete" && x.kind == MaterialKind::Concrete) || (k == "steel" && x.kind == MaterialKind::Steel)).ok_or("no such material")?;
            if let Some(fc) = num(a, "fc") {
                mt.fc = fc;
                mt.e = (4700.0 * fc.sqrt()).round();
                mt.name = format!("Concrete f'c {fc:.0} MPa");
            }
            if let Some(v) = num(a, "fy") {
                mt.fy = v;
            }
            if let Some(v) = num(a, "fu") {
                mt.fu = v;
            }
            Ok(serde_json::to_value(&*mt).unwrap_or_default())
        }
        "add_load_case" => {
            let id = m.next_case_id();
            let kind = match a.get("kind").and_then(Value::as_str).unwrap_or("D") {
                "L" => CaseKind::L,
                "Lr" => CaseKind::Lr,
                "W" => CaseKind::W,
                "E" => CaseKind::E,
                "Other" => CaseKind::Other,
                _ => CaseKind::D,
            };
            m.cases.push(LoadCase { id, name: a.get("name").and_then(Value::as_str).unwrap_or("case").into(), kind, self_weight: a.get("self_weight").and_then(Value::as_bool).unwrap_or(false) });
            Ok(json!({"id": id}))
        }
        "code_combinations" => {
            if let Some(v) = a.get("standard") {
                m.design.standard = serde_json::from_value(v.clone()).map_err(|_| "standard: 'aci', 'mr2566' or 'mr2566_noqc'")?;
            }
            m.combos = m.design.standard.combinations(&m.cases);
            Ok(json!({"standard": m.design.standard.label(), "combinations": m.combos.iter().map(|c| c.name.clone()).collect::<Vec<_>>()}))
        }
        "add_combination" => {
            let f = a.get("factors").and_then(Value::as_object).ok_or("factors: {case name: factor}")?;
            let mut factors = vec![];
            for (k, v) in f {
                factors.push((case_id(m, &json!(k))?, v.as_f64().ok_or("a factor is a number")?));
            }
            let id = m.next_combo_id();
            let kind = if a.get("kind").and_then(Value::as_str) == Some("sls") { ComboKind::Sls } else { ComboKind::Uls };
            m.combos.push(Combination { id, name: a.get("name").and_then(Value::as_str).unwrap_or("combination").into(), kind, factors });
            Ok(json!({"id": id}))
        }
        "add_loads" => {
            let case = case_id(m, a.get("case").ok_or("give the case")?)?;
            let mut n = 0;
            for l in a.get("loads").and_then(Value::as_array).ok_or("loads: [...]")? {
                match l.get("type").and_then(Value::as_str).unwrap_or("") {
                    "nodal" => {
                        let node = match l.get("at") {
                            Some(p) => {
                                let p = p3(p)?;
                                node_on_slab_or_member(m, p, "nodal loads go where members end, or on a slab panel with slab_method 'plate' (a column load on a mat)")?
                            }
                            None => l.get("node").and_then(Value::as_u64).ok_or("a nodal load needs 'at' or 'node'")? as Id,
                        };
                        let f: Vec<f64> = l.get("f").and_then(Value::as_array).ok_or("f: [fx,fy,fz,mx,my,mz]")?.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                        let mut ff = [0.0; 6];
                        for (k, v) in f.iter().take(6).enumerate() {
                            ff[k] = *v;
                        }
                        m.loads.push(Load { case, kind: LoadKind::Nodal { node, f: ff } });
                        n += 1;
                    }
                    "line" => {
                        let w = p3(l.get("w").ok_or("w: [wx, wy, wz] kN/m")?)?;
                        let wj = match l.get("w_end") {
                            Some(v) => p3(v)?,
                            None => w,
                        };
                        let mut targets = ids(l, "members");
                        if let Some(z) = num(l, "z") {
                            // the walls' rigid arms are no floor beams: a load by height goes to frame beams only
                            targets.extend(m.members.iter().filter(|x| x.role.is_frame() && m.kind(x) == MemberKind::Beam && [x.i, x.j].iter().all(|n| m.node(*n).is_some_and(|n| (n.z - z).abs() < 0.05))).map(|x| x.id));
                        }
                        if targets.is_empty() {
                            return Err("a line load needs members [ids] or z (all beams at that height)".into());
                        }
                        for t in targets {
                            if m.member(t).is_none() {
                                return Err(format!("no member {t}"));
                            }
                            m.loads.push(Load { case, kind: LoadKind::Line { member: t, wi: w, wj, a: num(l, "a"), b: num(l, "b") } });
                            n += 1;
                        }
                    }
                    "point" => {
                        let t = l.get("member").and_then(Value::as_u64).ok_or("a point load needs member")? as Id;
                        let mm = m.member(t).ok_or(format!("no member {t}"))?;
                        let pos = num(l, "a").ok_or("a: distance from node i, m")?;
                        if pos < 0.0 || pos > m.length(mm) {
                            return Err(format!("a = {pos} is off member {t} (length {:.3} m)", m.length(mm)));
                        }
                        m.loads.push(Load { case, kind: LoadKind::Point { member: t, a: pos, p: p3(l.get("p").ok_or("p: [px,py,pz] kN")?)? } });
                        n += 1;
                    }
                    "settlement" => {
                        let node = match l.get("at") {
                            Some(p) => m.find_node(p3(p)?).ok_or("no node there")?,
                            None => l.get("node").and_then(Value::as_u64).ok_or("a settlement needs 'at' or 'node'")? as Id,
                        };
                        if !m.supports.iter().any(|s| s.node == node) {
                            return Err(format!("node {node} has no support to settle"));
                        }
                        let d: Vec<f64> = l.get("d").and_then(Value::as_array).ok_or("d: [dx, dy, dz, rx, ry, rz] m, rad")?.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                        let mut dd = [0.0; 6];
                        for (k, v) in d.iter().take(6).enumerate() {
                            dd[k] = *v;
                        }
                        m.loads.push(Load { case, kind: LoadKind::Settlement { node, d: dd } });
                        n += 1;
                    }
                    "thermal" => {
                        let dt = num(l, "dt").ok_or("dt: the uniform temperature change, °C")?;
                        for t in ids(l, "members") {
                            m.loads.push(Load { case, kind: LoadKind::Thermal { member: t, dt, dt_y: num(l, "dt_y").unwrap_or(0.0), dt_z: num(l, "dt_z").unwrap_or(0.0) } });
                            n += 1;
                        }
                    }
                    other => return Err(format!("load type '{other}': nodal, line, point, settlement or thermal")),
                }
            }
            Ok(json!({"loads": n}))
        }
        "add_slabs" => {
            let mut made = vec![];
            for p in a.get("panels").and_then(Value::as_array).into_iter().flatten() {
                let id = m.next_slab_id();
                let g = |k: &str| num(p, k).ok_or(format!("a panel needs {k}"));
                let s = Slab { id, name: p.get("name").and_then(Value::as_str).map(String::from).unwrap_or(format!("S{id}")), x0: g("x0")?, y0: g("y0")?, x1: g("x1")?, y1: g("y1")?, z: g("z")?, h: num(p, "h").unwrap_or(0.15), dead: num(p, "dead").unwrap_or(1.5), live: num(p, "live").unwrap_or(2.0), bim_id: p.get("bim_id").and_then(Value::as_str).map(String::from), subgrade: num(p, "subgrade").unwrap_or(0.0), mat: p.get("mat").and_then(Value::as_bool).unwrap_or(false), support_size: num(p, "support_size").unwrap_or(0.0), plate: p.get("plate").and_then(Value::as_bool).unwrap_or(false) };
                m.slabs.push(s);
                made.push(id);
            }
            if let Some(b) = a.get("bays") {
                let z = num(b, "z").ok_or("bays needs z")?;
                for (x0, y0, x1, y1) in bays(m, z) {
                    let id = m.next_slab_id();
                    m.slabs.push(Slab { id, name: format!("S{id}"), x0, y0, x1, y1, z, h: num(b, "h").unwrap_or(0.15), dead: num(b, "dead").unwrap_or(1.5), live: num(b, "live").unwrap_or(2.0), bim_id: None, ..Default::default() });
                    made.push(id);
                }
            }
            if made.is_empty() {
                return Err("no panels made: give panels, or bays with the z of a floor of beams".into());
            }
            Ok(json!({"slabs": made}))
        }
        "add_stairs" => {
            let list = a.get("stairs").and_then(Value::as_array).ok_or("give stairs [{kind, width, waist, span, ...}]")?;
            let mut made = vec![];
            for e in list {
                let id = m.stairs.iter().map(|s| s.id).max().unwrap_or(0) + 1;
                let kind = match e.get("kind").and_then(Value::as_str).unwrap_or("flight") { "landing" => crate::stairs::StairKind::Landing, _ => crate::stairs::StairKind::Flight };
                let g = |k: &str| num(e, k).ok_or(format!("a stair needs {k}"));
                let st = crate::stairs::Stair {
                    id, name: e.get("name").and_then(Value::as_str).map(String::from).unwrap_or(format!("ST{id}")), bim_id: e.get("bim_id").and_then(Value::as_str).map(String::from), kind,
                    width: g("width")?, waist: g("waist")?, riser: num(e, "riser").unwrap_or(0.0), tread: num(e, "tread").unwrap_or(0.0), risers: e.get("risers").and_then(Value::as_u64).unwrap_or(0) as usize,
                    span: g("span")?, fixity: e.get("fixity").and_then(Value::as_str).unwrap_or("continuous").into(), dead: num(e, "dead").unwrap_or(1.0), live: num(e, "live").unwrap_or(3.0),
                    flight_dead: num(e, "flight_dead").unwrap_or(0.0), flight_live: num(e, "flight_live").unwrap_or(0.0), z0: num(e, "z0").unwrap_or(0.0), z1: num(e, "z1").unwrap_or(0.0),
                };
                if kind == crate::stairs::StairKind::Flight && (st.riser <= 0.0 || st.tread <= 0.0) {
                    return Err(format!("stair {}: a flight needs riser and tread", st.name));
                }
                m.stairs.push(st);
                made.push(id);
            }
            Ok(json!({"stairs": made}))
        }
        "add_walls" => {
            let list = a.get("walls").and_then(Value::as_array).ok_or("give walls [{from, to, t}]")?;
            let zs: Vec<f64> = m.nodes.iter().map(|n| n.z).collect();
            let (zmin, zmax) = (zs.iter().copied().fold(f64::MAX, f64::min), zs.iter().copied().fold(f64::MIN, f64::max));
            let conc = m.materials.iter().find(|x| x.kind == MaterialKind::Concrete).map(|x| x.id);
            let base_fixed = a.get("base").and_then(Value::as_str).unwrap_or("fixed") == "fixed";
            let mut made = vec![];
            for e in list {
                let xy = |k: &str| -> Result<[f64; 2], String> {
                    let v = e.get(k).and_then(Value::as_array).ok_or_else(|| format!("each wall needs '{k}' [x, y]"))?;
                    Ok([v.first().and_then(Value::as_f64).ok_or("x")?, v.get(1).and_then(Value::as_f64).ok_or("y")?])
                };
                let (pa, pb) = (xy("from")?, xy("to")?);
                let z0 = num(e, "z0").unwrap_or(if zmin.is_finite() { zmin } else { 0.0 });
                let mut z1 = num(e, "z1").unwrap_or(if zmax.is_finite() { zmax } else { 0.0 });
                let mut levels: Vec<f64> = a.get("levels").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                if let Some(h) = num(a, "storey_h").filter(|h| *h > 0.1) {
                    if num(e, "z1").is_none() && z1 <= z0 + 1e-6 {
                        return Err("storey_h needs z1 (the wall's top) when the model has no levels yet".into());
                    }
                    let n = ((z1 - z0) / h).round().max(1.0) as usize;
                    levels.extend((0..=n).map(|k| z0 + (z1 - z0) * k as f64 / n as f64));
                }
                if levels.iter().any(|z| *z > z1) && num(e, "z1").is_none() {
                    z1 = levels.iter().copied().fold(z1, f64::max);
                }
                if z1 <= z0 + 0.1 {
                    return Err(format!("wall from {pa:?} to {pb:?}: give z1 above z0 = {z0} (or add the floors first)"));
                }
                let t = num(e, "t").unwrap_or(0.25);
                if !(0.1..=1.5).contains(&t) {
                    return Err(format!("wall thickness {t} m: give t in m, e.g. 0.25"));
                }
                let material = match e.get("material") {
                    Some(v) => material_id(m, Some(v), &Section { id: 0, name: String::new(), shape: Shape::Rect { b: t, h: 1.0 } })?,
                    None => conc.ok_or("no concrete material")?,
                };
                let id = m.walls.iter().map(|w| w.id).max().unwrap_or(0) + 1;
                let name = e.get("name").and_then(Value::as_str).map(String::from).unwrap_or(format!("W{id}"));
                m.walls.push(Wall { id, name, a: pa, b: pb, z0, z1, t, material, bim_id: e.get("bim_id").and_then(Value::as_str).map(String::from), levels: levels.into_iter().filter(|z| *z >= z0 - 1e-6 && *z <= z1 + 1e-6).collect() });
                made.push(id);
            }
            crate::walls::sync(m);
            if base_fixed {
                for id in &made {
                    let w = m.walls.iter().find(|w| w.id == *id).cloned().unwrap();
                    let c = [(w.a[0] + w.b[0]) / 2.0, (w.a[1] + w.b[1]) / 2.0, w.z0];
                    if let Some(n) = m.find_node(c) {
                        if !m.supports.iter().any(|s| s.node == n) {
                            m.supports.push(Support::of_kind(n, "fixed").unwrap());
                        }
                    }
                }
            }
            let piers = m.members.iter().filter(|x| matches!(x.role, Role::WallPier(w) if made.contains(&w))).count();
            Ok(json!({"walls": made, "piers": piers, "next": "analyze, then design: each wall is designed per storey"}))
        }
        "set_walls" => {
            let sel: Vec<Id> = if a.get("all").and_then(Value::as_bool).unwrap_or(false) { m.walls.iter().map(|w| w.id).collect() } else { ids(a, "walls") };
            if sel.is_empty() {
                return Err("give walls [ids] or all true".into());
            }
            let mat = match a.get("material") {
                Some(v) => Some(material_id(m, Some(v), &Section { id: 0, name: String::new(), shape: Shape::Rect { b: 0.25, h: 1.0 } })?),
                None => None,
            };
            for w in m.walls.iter_mut().filter(|w| sel.contains(&w.id)) {
                if let Some(t) = num(a, "t") {
                    w.t = t;
                }
                if let Some(x) = mat {
                    w.material = x;
                }
                w.levels.clear();
            }
            // rebuilt with the new section (their levels are found again)
            for id in &sel {
                let old: Vec<Id> = m.members.iter().filter(|x| x.role.wall() == Some(*id)).map(|x| x.id).collect();
                let lv: Vec<f64> = old.iter().filter_map(|x| m.member(*x)).flat_map(|x| [x.i, x.j]).filter_map(|n| m.node(n).map(|n| n.z)).collect();
                if let Some(w) = m.walls.iter_mut().find(|w| w.id == *id) {
                    w.levels = lv;
                }
            }
            crate::walls::sync(m);
            Ok(json!({"changed": sel}))
        }
        "move_node" => {
            let id = a.get("node").and_then(Value::as_u64).ok_or("node")? as Id;
            let to = p3(a.get("to").ok_or("to")?)?;
            m.node(id).ok_or_else(|| format!("no node {id}"))?;
            let other = m.nodes.iter().find(|n| n.id != id && dist(n.p(), to) < 1e-3).map(|n| n.id);
            match other {
                None => {
                    if let Some(n) = m.nodes.iter_mut().find(|n| n.id == id) {
                        (n.x, n.y, n.z) = (to[0], to[1], to[2]);
                    }
                    Ok(json!({"moved": id, "to": to}))
                }
                Some(t) => {
                    for x in m.members.iter_mut() {
                        if x.i == id {
                            x.i = t;
                        }
                        if x.j == id {
                            x.j = t;
                        }
                    }
                    let gone: Vec<Id> = m.members.iter().filter(|x| x.i == x.j).map(|x| x.id).collect();
                    if !gone.is_empty() {
                        m.delete_members(&gone);
                    }
                    // the merged node keeps the target's support, or takes the moved one's
                    if !m.supports.iter().any(|s| s.node == t) {
                        for s in m.supports.iter_mut().filter(|s| s.node == id) {
                            s.node = t;
                        }
                    }
                    m.supports.retain(|s| s.node != id);
                    for l in m.loads.iter_mut() {
                        match &mut l.kind {
                            LoadKind::Nodal { node, .. } | LoadKind::Settlement { node, .. } if *node == id => *node = t,
                            _ => {}
                        }
                    }
                    m.nodes.retain(|n| n.id != id);
                    Ok(json!({"merged": id, "into": t, "deleted_members": gone}))
                }
            }
        }
        "transform" => {
            let sel: Vec<Id> = if a.get("all").and_then(Value::as_bool).unwrap_or(false) { m.members.iter().map(|x| x.id).collect() } else { ids(a, "members") };
            if sel.is_empty() {
                return Err("give members [ids] or all true".into());
            }
            let get3 = |k: &str, d: [f64; 3]| a.get(k).map(p3).transpose().map(|v| v.unwrap_or(d));
            let x = match a.get("action").and_then(Value::as_str).unwrap_or("") {
                "move" => crate::transform::Xform::translate(get3("d", [0.0; 3])?),
                "rotate" => crate::transform::Xform::rotate_z(get3("centre", [0.0; 3])?, num(a, "angle").ok_or("rotate: give angle (degrees)")?),
                "mirror" => crate::transform::Xform::mirror(get3("point", [0.0; 3])?, get3("normal", [1.0, 0.0, 0.0])?)?,
                other => return Err(format!("action '{other}': move, rotate or mirror")),
            };
            let flag = |k: &str| a.get(k).and_then(Value::as_bool).unwrap_or(true);
            let o = crate::transform::Opts { copy: a.get("copy").and_then(Value::as_bool).unwrap_or(false), n: a.get("n").and_then(Value::as_u64).unwrap_or(1).max(1) as usize, loads: flag("loads"), supports: flag("supports"), slabs: flag("slabs") };
            let e = crate::transform::transform_members(m, &sel, &x, &o)?;
            Ok(serde_json::to_value(&e).unwrap_or_default())
        }
        "split_member" => {
            let id = a.get("member").and_then(Value::as_u64).ok_or("member")? as Id;
            let at: Vec<f64> = a.get("at").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let parts = a.get("parts").and_then(Value::as_u64).unwrap_or(0) as usize;
            let e = crate::transform::split_member(m, id, &at, parts)?;
            Ok(serde_json::to_value(&e).unwrap_or_default())
        }
        "merge_members" => {
            let (x, y) = (a.get("a").and_then(Value::as_u64).ok_or("a")? as Id, a.get("b").and_then(Value::as_u64).ok_or("b")? as Id);
            let e = crate::transform::merge_members(m, x, y)?;
            Ok(serde_json::to_value(&e).unwrap_or_default())
        }
        "delete" => {
            let (mem, nodes, slabs, sups) = (ids(a, "members"), ids(a, "nodes"), ids(a, "slabs"), ids(a, "supports"));
            let walls = ids(a, "walls");
            if !walls.is_empty() {
                let base: Vec<Id> = m.members.iter().filter(|x| x.role.wall().is_some_and(|w| walls.contains(&w))).flat_map(|x| [x.i]).collect();
                m.walls.retain(|w| !walls.contains(&w.id));
                crate::walls::sync(m);
                // the base supports of the walls that went
                m.supports.retain(|s| !base.contains(&s.node) || m.members.iter().any(|x| x.i == s.node || x.j == s.node));
            }
            m.delete_members(&mem);
            m.delete_nodes(&nodes);
            m.slabs.retain(|s| !slabs.contains(&s.id));
            m.supports.retain(|s| !sups.contains(&s.node));
            let masses = ids(a, "masses");
            m.masses.retain(|x| !masses.contains(&x.node));
            if let Some(c) = a.get("loads_of_case") {
                let c = case_id(m, c)?;
                m.loads.retain(|l| l.case != c);
            }
            m.purge_nodes();
            Ok(json!({"ok": true}))
        }
        "set_design" => {
            let mut v = serde_json::to_value(&m.design).map_err(|e| e.to_string())?;
            for (k, x) in a.as_object().into_iter().flatten() {
                if v.get(k).is_none() {
                    return Err(format!("no design setting '{k}'"));
                }
                v[k] = x.clone();
            }
            m.design = serde_json::from_value(v).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(&m.design).unwrap_or_default())
        }
        "list_sections" => Ok(json!({
            "steel_catalogue": crate::sections::catalogue().iter().map(|s| {
                let p = crate::sections::h_props(s.d / 1e3, s.bf / 1e3, s.tw / 1e3, s.tf / 1e3, s.r / 1e3);
                json!({"name": s.name, "kg_m": (s.mass() * 10.0).round() / 10.0, "d_mm": s.d, "Ix_cm4": (p.iy * 1e8).round(), "Zx_cm3": (p.zy * 1e6).round()})
            }).collect::<Vec<_>>(),
            "concrete": "any rectangle, written 'b x h' in mm, e.g. '300 x 600'",
            "model": m.sections.iter().map(|s| json!({"id": s.id, "name": s.name})).collect::<Vec<_>>(),
        })),
        "check_model" => Ok(json!({"problems": crate::check::problems(m)})),
        "set_diaphragms" => {
            if a.get("all").and_then(Value::as_bool).unwrap_or(false) {
                let base: Vec<f64> = m.supports.iter().filter_map(|s| m.node(s.node).map(|n| n.z)).collect();
                let zmin = base.iter().copied().fold(f64::MAX, f64::min);
                let mut z: Vec<f64> = m.nodes.iter().map(|n| (n.z * 1000.0).round() / 1000.0).filter(|z| *z > zmin + 0.01).collect();
                z.sort_by(f64::total_cmp);
                z.dedup();
                m.diaphragms = z;
            } else {
                m.diaphragms = a.get("levels").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            }
            Ok(json!({"diaphragms_z": m.diaphragms}))
        }
        _ => Err(format!("unknown command {name}")),
    }
}

/// Closed rectangular bays of beams at height z: consecutive grid lines in x and y with beams
/// along all four edges.
pub fn bays(m: &Model, z: f64) -> Vec<(f64, f64, f64, f64)> {
    let pts: Vec<&Node> = m.nodes.iter().filter(|n| (n.z - z).abs() < 0.05).collect();
    let mut xs: Vec<f64> = pts.iter().map(|n| (n.x * 1000.0).round() / 1000.0).collect();
    let mut ys: Vec<f64> = pts.iter().map(|n| (n.y * 1000.0).round() / 1000.0).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let covered = |a: [f64; 2], b: [f64; 2]| -> bool {
        let le = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let total: f64 = crate::slabload::beams_on(m, a, b, z).iter().map(|(_, si, sj)| si.max(*sj).min(le) - si.min(*sj).max(0.0)).sum();
        total > le - 0.02
    };
    let mut out = vec![];
    for wx in xs.windows(2) {
        for wy in ys.windows(2) {
            let (x0, x1, y0, y1) = (wx[0], wx[1], wy[0], wy[1]);
            if covered([x0, y0], [x1, y0]) && covered([x1, y0], [x1, y1]) && covered([x1, y1], [x0, y1]) && covered([x0, y1], [x0, y0]) {
                out.push((x0, y0, x1, y1));
            }
        }
    }
    out
}

fn add_frame(m: &mut Model, a: &Value) -> Result<Value, String> {
    let arr = |k: &str| -> Vec<f64> { a.get(k).and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_f64).collect()).unwrap_or_default() };
    let (xs, ys, hs) = (arr("x_spans"), arr("y_spans"), arr("storeys"));
    if xs.is_empty() || hs.is_empty() {
        return Err("give x_spans and storeys (m)".into());
    }
    let o = match a.get("origin") {
        Some(v) => p3(v)?,
        None => [0.0; 3],
    };
    let cum = |v: &[f64], start: f64| -> Vec<f64> {
        let mut out = vec![start];
        for s in v {
            out.push(out.last().unwrap() + s);
        }
        out
    };
    let gx = cum(&xs, o[0]);
    let gy = if ys.is_empty() { vec![o[1]] } else { cum(&ys, o[1]) };
    let gz = cum(&hs, o[2]);
    let col = m.section_named(a.get("column").and_then(Value::as_str).ok_or("column section")?)?;
    let beam = m.section_named(a.get("beam").and_then(Value::as_str).ok_or("beam section")?)?;
    let beam_y = match a.get("beam_y").and_then(Value::as_str) {
        Some(s) => m.section_named(s)?,
        None => beam,
    };
    let steel = a.get("material").and_then(Value::as_str) == Some("steel") || matches!(m.section(beam).map(|s| &s.shape), Some(Shape::H { .. }));
    let mat = if steel { m.steel_id() } else { m.concrete_id() };
    let base = a.get("base").and_then(Value::as_str).unwrap_or("fixed");
    // the columns' strong axis in the frames of the longer spans (x-z frames: roll 0)
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    let col_roll = num(a, "column_roll").unwrap_or(if mean(&xs) >= mean(&ys) { 0.0 } else { 90.0 });
    let mut made = 0;
    let mut add = |m: &mut Model, p: [f64; 3], q: [f64; 3], s: Id, kind: MemberKind, group: String| {
        let (i, j) = (m.node_at(p), m.node_at(q));
        if m.members.iter().any(|x| (x.i == i && x.j == j) || (x.i == j && x.j == i)) {
            return;
        }
        let id = m.next_member_id();
        let roll = if kind == MemberKind::Column { col_roll } else { 0.0 };
        m.members.push(Member { id, i, j, section: s, material: mat, kind, hinge_i: false, hinge_j: false, lb: None, k: None, roll, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: steel.then_some(group) });
        made += 1;
    };
    for (lvl, w) in gz.windows(2).enumerate() {
        for &x in &gx {
            for &y in &gy {
                add(m, [x, y, w[0]], [x, y, w[1]], col, MemberKind::Column, format!("C{}", lvl + 1));
            }
        }
        for &y in &gy {
            for wx in gx.windows(2) {
                add(m, [wx[0], y, w[1]], [wx[1], y, w[1]], beam, MemberKind::Beam, format!("BX{}", lvl + 1));
            }
        }
        for &x in &gx {
            for wy in gy.windows(2) {
                add(m, [x, wy[0], w[1]], [x, wy[1], w[1]], beam_y, MemberKind::Beam, format!("BY{}", lvl + 1));
            }
        }
    }
    let mut sups = 0;
    for &x in &gx {
        for &y in &gy {
            let n = m.node_at([x, y, gz[0]]);
            m.supports.retain(|s| s.node != n);
            let mut s = Support::of_kind(n, base).ok_or("base is fixed or pinned")?;
            if ys.is_empty() && base == "pinned" {
                // a plane frame: hold it out of its plane
                s.fix[3] = true;
                s.fix[5] = true;
            }
            m.supports.push(s);
            sups += 1;
        }
    }
    let mut slabs = 0;
    if let (Some(sl), false) = (a.get("slab"), ys.is_empty()) {
        let h = num(sl, "h").unwrap_or(0.15);
        let dead = num(sl, "dead").unwrap_or(1.5);
        let live = num(sl, "live").unwrap_or(2.0);
        let roof = num(sl, "roof_live").unwrap_or(live);
        for (k, z) in gz.iter().enumerate().skip(1) {
            for wx in gx.windows(2) {
                for wy in gy.windows(2) {
                    let id = m.next_slab_id();
                    m.slabs.push(Slab { id, name: format!("S{k}-{id}"), x0: wx[0], y0: wy[0], x1: wx[1], y1: wy[1], z: *z, h, dead, live: if k == gz.len() - 1 { roof } else { live }, bim_id: None, ..Default::default() });
                    slabs += 1;
                }
            }
        }
    }
    Ok(json!({"members": made, "supports": sups, "slabs": slabs, "levels_z": gz}))
}
