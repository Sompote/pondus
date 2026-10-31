// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! `pondus --mcp`: Pondus without a window, as an MCP server (JSON-RPC 2.0 over stdio, one
//! message per line). The chat's agent and other agents (Claude Code, the BIM modeller
//! Aedifex's agent) build or load a structure, analyse and design it, look at it, and take
//! the design back to the BIM model with export_bim.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::analysis::Analysis;
use crate::commands;
use crate::design::{self, Report};
use crate::model::*;
use crate::scene;

const PROTOCOL: &str = "2025-06-18";

pub const INSTRUCTIONS: &str = "Pondus: structural analysis and design of building frames. Units m, kN, kPa, MPa; x and y in plan, z up. \
A model has members (beams, columns, braces; each one straight piece between two points, split for analysis automatically), \
sections (concrete rectangles 'b x h' in mm; steel H shapes 'H 300x150x6.5x9' from list_sections), materials (concrete f'c, steel Fy/Fu), \
supports, load cases (D with self-weight, L, ...), combinations (ULS for strength, SLS for deflection) and rectangular slab panels \
that rest on the beams under their edges (their load goes to the beams by the 45° lines). Analysis is linear static 3D (the Stabileo \
engine). Design: concrete beams and slabs to ACI 318-19 (bars, stirrups, deflection with cracked inertia; two-way slabs by the \
ACI 318-63 Method 2 coefficients used in Thailand), steel members to AISC 360-16 LRFD (with select: the lightest passing H shape). \
Stairs of a house (add_stairs: flights and landings) are designed as one-way slabs with their reactions reported. Concrete columns are designed for P-M (biaxial, slenderness) and spread footings under them. Second order: set_design second_order (P-Delta), buckling. Earthquake: set_seismic (spectrum, R, Ie, Cd) adds modal and response spectrum analysis, storey drifts and the seismic combinations to analyze and design (get_seismic for the results). Typical flow: add_frame (or add_members + add_supports) -> add_slabs / add_loads -> \
check_model -> analyze (compare the reactions with the applied load) -> design (resize / select) -> show_picture -> export_bim. \
A BIM model comes in with import_stabileo (Stabileo / Aedifex export_stabileo JSON; element bimId kept) and goes back with export_bim. \
Shear walls and cores: add_walls (after the floors), each a wall element with its own section, analysed as piers with rigid arms and designed per storey to ACI 318-19 Ch. 11 / 18.10; never model a wall as a column member. \
Foundations and flat plates: with slab_method 'plate' a slab panel rests directly on the supports and columns inside it, with no beams. A pile cap or a mat (raft) foundation is add_mat (a plate on pile supports with their axial stiffness pile_k, or on a Winkler subgrade); a flat plate is add_slabs over the columns with plate true (that panel alone becomes plate elements; the floors on beams keep their method), designed with column and middle strips, punching with the unbalanced moments (ACI 318-19 8.4.4.2) and the Table 8.3.1.1 thickness. Never model a pile cap or a mat as a grillage of beams. Column loads on a mat go with add_loads nodal at [x, y, z]; the pile loads are the reactions (get_results reactions true), the mat's moments per metre are get_slab_results, and design gives the bars of both faces and the punching check at every pile and column. \
Pondus is free software under the GNU AGPL-3.0-only; its complete source is at https://github.com/Sompote/pondus. Mention this link when a user asks about the program or its licence, and remind them that structural results must be checked by a qualified engineer.";

pub struct Server {
    pub model: Model,
    pub autosave: Option<std::path::PathBuf>,
    pub denied: Vec<String>,
    analysis: Option<Analysis>,
    report: Option<Report>,
    pictures: usize,
}

fn tools() -> Vec<Value> {
    let mut t: Vec<Value> = commands::specs().into_iter().map(|s| json!({"name": s.name, "description": s.description, "inputSchema": s.schema})).collect();
    t.push(json!({"name": "analyze", "description": "Analyses the model (linear static, every load case; combinations by superposition). Returns per case the applied load and the reactions (they must balance), the slab load put on the beams against the slabs' total, per combination the largest displacement and member forces, and warnings (e.g. slab edges with no beam).", "inputSchema": {"type": "object", "properties": {}}}));
    t.push(json!({"name": "get_results", "description": "Member forces of a combination or case ('result', a name; default the first ULS): for each member (all, or 'members') the largest sagging and hogging moment, shear, axial force and torsion with where they occur, and the member's own deflection; with reactions true the support reactions.", "inputSchema": {"type": "object", "properties": {"result": {"type": "string"}, "members": {"type": "array", "items": {"type": "integer"}}, "reactions": {"type": "boolean"}}}}));
    t.push(json!({"name": "set_seismic", "description": "Earthquake settings, and switches the seismic analysis on (enabled true) or off. spectrum: {kind: 'code', sds, sd1 (g), tl (s)} — the ASCE 7-22 / DPT 1301/1302-61 design spectrum — or {kind: 'table', points: [[T s, Sa g], ...]} (e.g. a Bangkok basin zone of DPT 1302-61); r, ie, cd, omega0, rho (Table 12.2-1 / DPT 1301/1302 Table 2.4-1; e.g. special RC moment frame R 8, Cd 5.5, Ω0 3; intermediate R 5, Cd 4.5; ordinary R 3, Cd 2.5); live_mass (share of live load in the seismic weight, default 0.25); damping (0.05); combination ('cqc', default, or 'srss'); elf_ratio (the response spectrum base shear is scaled up to this share of the ELF base shear, 1.0 = ASCE 7-22, 0.85 = older editions); ct and x (Ta = Ct·hn^x: RC moment frame 0.0466 / 0.9, steel moment frame 0.0724 / 0.8, other 0.0488 / 0.75), cu (1.4), drift_limit (0.020 of the storey height), live_in_e (live factor in the seismic combinations, 1.0 or 0.5). The seismic combinations (1.2 + 0.2 SDS)D + L ± Ex/Ey and (0.9 − 0.2 SDS)D ± Ex/Ey are then used by design.", "inputSchema": {"type": "object", "properties": {"enabled": {"type": "boolean"}, "spectrum": {"type": "object"}, "r": {"type": "number"}, "ie": {"type": "number"}, "cd": {"type": "number"}, "omega0": {"type": "number"}, "rho": {"type": "number"}, "live_mass": {"type": "number"}, "damping": {"type": "number"}, "combination": {"type": "string", "enum": ["cqc", "srss"]}, "elf_ratio": {"type": "number"}, "ct": {"type": "number"}, "x": {"type": "number"}, "cu": {"type": "number"}, "drift_limit": {"type": "number"}, "live_in_e": {"type": "number"}}}}));
    t.push(json!({"name": "get_seismic", "description": "The earthquake results (set_seismic enabled first): seismic weight, the modes (periods, frequencies, mass ratios in x, y, z and their running sums), and per direction Ex / Ey the fundamental period, the ELF period, Cs and base shear, the response spectrum base shear and its scale factor, and the storey drifts (elastic δe and Cd·δe/Ie against the allowable). Member forces under Ex / Ey: get_results with result 'Ex' or 'Ey' (CQC magnitudes); the seismic combinations are designed by design.", "inputSchema": {"type": "object", "properties": {}}}));
    t.push(json!({"name": "buckling", "description": "Linear buckling of a ULS combination ('combination', a name; default the first ULS): the load factors λ at which it makes the structure unstable (λ < 1: it buckles under the combination itself; λ < 10: second-order effects matter, use set_design second_order) and their mode shapes (show_picture diagram 'buckling', mode n).", "inputSchema": {"type": "object", "properties": {"combination": {"type": "string"}, "modes": {"type": "integer"}}}}));
    t.push(json!({"name": "time_history", "description": "Linear time-history analysis under a ground motion, on the seismic mass (D + live_mass x L): the record from 'path' (PEER .AT2, or two columns time and acceleration in g, or one column with record_dt) or 'record' (the same as text); direction 'x' or 'y', scale (e.g. to a target PGA), dt (analysis step, default 0.01 s), duration, damping (0.05). Returns the PGA, the roof displacement peak and when, each storey's peak drift and drift ratio, and the summed peak reactions (an upper bound of the base shear).", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "record": {"type": "string"}, "record_dt": {"type": "number"}, "direction": {"type": "string", "enum": ["x", "y"]}, "scale": {"type": "number"}, "dt": {"type": "number"}, "duration": {"type": "number"}, "damping": {"type": "number"}}}}));
    t.push(json!({"name": "pushover", "description": "Plastic limit analysis by successive hinges (event-to-event, rigid-plastic hinges at Mp = Fy·Z of steel and Mn of RC members from the last design): either 'combination' (a name: the load factor λ at which that combination becomes a mechanism) or a lateral pattern in 'direction' x or y, 'pattern' 'triangular' (∝ height, default) or 'uniform', lumped at the floors and summing to the seismic weight W, so λ = V/W at collapse. Gravity is not held constant during a lateral push. Returns λ, the base shear, the hinge sequence (member, position 0–1 along it, λ when it formed) and the capacity curve (λ, roof displacement).", "inputSchema": {"type": "object", "properties": {"combination": {"type": "string"}, "direction": {"type": "string", "enum": ["x", "y"]}, "pattern": {"type": "string", "enum": ["triangular", "uniform"]}}}}));
    t.push(json!({"name": "set_moving", "description": "A moving load train (vehicle, crane) along a path of members in order: axles [[offset from the first axle m, weight kN], ...], step (m, default 0.25), impact (dynamic allowance, e.g. 0.33), factor (load factor of the moving-load combinations 1.2 D + factor·M, default 1.6). analyze then envelopes the train ('moving max' / 'moving min' results) and design uses the combinations. 'clear': true removes it.", "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}, "path": {"type": "array", "items": {"type": "integer"}}, "axles": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}}, "step": {"type": "number"}, "impact": {"type": "number"}, "factor": {"type": "number"}, "clear": {"type": "boolean"}}}}));
    t.push(json!({"name": "influence_line", "description": "The influence line of a member force at x (m from node i) — quantity My, Vz, N, T, Mz or Vy, with member — or of a support reaction — quantity Fx, Fy, Fz, Mx, My or Mz, with node — under a unit downward load moved over every member. Returns (member, x, value) points and the largest and least values.", "inputSchema": {"type": "object", "properties": {"quantity": {"type": "string"}, "member": {"type": "integer"}, "x": {"type": "number"}, "node": {"type": "integer"}}, "required": ["quantity"]}}));
    t.push(json!({"name": "set_stages", "description": "Construction stages in order: [{name, members: [ids built in this stage], cases: [load case ids applied in it]}]. A member built later starts free of stress and only takes the loads after it. [] removes them.", "inputSchema": {"type": "object", "properties": {"stages": {"type": "array", "items": {"type": "object"}}}, "required": ["stages"]}}));
    t.push(json!({"name": "staged", "description": "Solves the construction stages (set_stages): per stage the largest displacement and moment so far, and the final state as the result 'staged (final)' for get_results and show_picture.", "inputSchema": {"type": "object", "properties": {}}}));
    t.push(json!({"name": "section_analysis", "description": "Finite element analysis of any solid cross-section: a polygon 'outline' [[y, z], ...] in m, or a section 'name' from the model (rectangle, H or lipped C). Returns the area, centroid, Iy, Iz, Iyz, plastic moduli, the St Venant torsion constant J, the warping constant Cw and the shear centre.", "inputSchema": {"type": "object", "properties": {"outline": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}}, "name": {"type": "string"}, "mesh": {"type": "integer"}}}}));
    t.push(json!({"name": "write_report", "description": "Writes the calculation report as a PDF file: summary, basis of design (codes, materials, settings), model and sections, load cases with their reaction balance, combinations, analysis extremes, the earthquake analysis, the design tables of every member type with failing rows in red, the full calculation of the governing member of each type (detail 'all': of every member) and the changes and warnings. path: the PDF file (default next to the model, <name>-report.pdf); project and engineer for the title block; pictures (default true) adds the model, moment and utilisation pictures. Runs the design first when there is none.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "project": {"type": "string"}, "engineer": {"type": "string"}, "detail": {"type": "string", "enum": ["governing", "all"]}, "pictures": {"type": "boolean"}}}}));
    t.push(json!({"name": "export_file", "description": "Writes the model to a file for other programs: format 'excel' (.xlsx: nodes, members, supports, sections, materials, cases, combinations, loads, slabs, settings, plus results and design sheets when analysed / designed; Pondus reads it back with import_file), 'dxf' (AutoCAD R12: members on layers by kind and section, nodes, supports, slabs, labels and the My diagram; view '3d', 'plan' with z, 'elevation_xz' with y or 'elevation_yz' with x) or 'ifc' (IFC4: storeys, IfcColumn / IfcBeam / IfcMember with extruded rectangle or I profiles, IfcSlab, materials and the design ratio; opens in Revit, BIM viewers and ifcopenshell). path: the file (default next to the model).", "inputSchema": {"type": "object", "properties": {"format": {"type": "string", "enum": ["excel", "dxf", "ifc"]}, "path": {"type": "string"}, "view": {"type": "string", "enum": ["3d", "plan", "elevation_xz", "elevation_yz"]}, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "labels": {"type": "boolean"}, "diagram": {"type": "boolean"}, "result": {"type": "string"}}, "required": ["format"]}}));
    t.push(json!({"name": "import_file", "description": "Reads a structure from a file: format 'excel' (a Pondus workbook: replaces the model), 'dxf' (LINE, LWPOLYLINE and POLYLINE entities become members; a layer such as COLUMN_400x400, BEAM_300x600 or H300x150x6p5x9 gives kind and section, others take default_section; units from $INSUNITS or scale, e.g. 0.001 for mm; a 2D drawing is placed by view 'plan' with z or 'elevation_xz' with y; closed rectangles on SLAB layers become slabs) or 'ifc' (IFC2x3 / IFC4 from Revit or Aedifex: IfcColumn, IfcBeam, IfcMember and IfcSlab with extruded bodies, or the analytical IfcStructuralCurveMember; profiles to sections; GlobalId kept as bim_id; snap (default 0.75 m) moves member ends onto the axes they meet and beams onto their levels, 0 keeps the axes as drawn). replace true starts from an empty model (default for excel), otherwise the members merge into the current model. Supports and loads are not in DXF or IFC: add them after.", "inputSchema": {"type": "object", "properties": {"format": {"type": "string", "enum": ["excel", "dxf", "ifc"]}, "path": {"type": "string"}, "replace": {"type": "boolean"}, "scale": {"type": "number"}, "view": {"type": "string", "enum": ["3d", "plan", "elevation_xz", "elevation_yz"]}, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "default_section": {"type": "string"}, "snap": {"type": "number"}}, "required": ["format", "path"]}}));
    t.push(json!({"name": "get_slab_results", "description": "The slabs' plate results (plate method: set_design slab_method 'plate' first) for a combination or case ('result', default the first ULS): per slab (all, or 'slabs') the largest positive and negative mx, my (kN·m/m, sagging +), mxy, qx, qy (kN/m) with where they occur, the Wood–Armer design moments (bottom and top, x and y) and the largest deflection; with elements true every element's centre and values.", "inputSchema": {"type": "object", "properties": {"result": {"type": "string"}, "slabs": {"type": "array", "items": {"type": "integer"}}, "elements": {"type": "boolean"}}}}));
    t.push(json!({"name": "design", "description": "Designs the structure: concrete beams (bars in thirds of the span, stirrups, deflection) and slabs (bars per direction) to ACI 318-19, steel members to AISC 360-16. resize: deepen failing concrete beams in 50 mm steps and thicken failing slabs; select: give every steel member (or group) the lightest passing H shape. Changed sections are analysed again until stable. Give 'members' / 'slabs' to design only those, or beams/slabs/steel false to skip a kind. slab_method chooses how slabs are analysed and designed (and stays set in the model).", "inputSchema": {"type": "object", "properties": {"members": {"type": "array", "items": {"type": "integer"}}, "slabs": {"type": "array", "items": {"type": "integer"}}, "beams": {"type": "boolean"}, "do_slabs": {"type": "boolean"}, "steel": {"type": "boolean"}, "resize": {"type": "boolean"}, "select": {"type": "boolean"}, "slab_method": {"type": "string", "enum": ["coefficients", "plate"], "description": "coefficients: ACI 318-63 Method 2 / ACI 6.5 with the load to the beams by the 45° lines; plate: the slabs meshed as plate (shell) elements analysed with the frame, bars from the element moments by Wood–Armer"}}}}));
    t.push(json!({"name": "show_picture", "description": "Looks at the model: view '3d' (yaw/pitch degrees), 'plan' (z: one floor) or 'elevation' (y: one frame line); diagram none, moment (My), shear (Vz), axial, momentz (Mz), sheary (Vy), torsion, platemx / platemy / platemxy (slab plate moments) and plateqx / plateqy (slab shears), plate method, deflection, mode (a mode shape of the seismic analysis: mode 1, 2, ...) or utilisation (members and slabs coloured by their design ratio: green < 0.7, yellow < 0.9, orange < 1, red over); result: a combination or case name. The picture comes back to you and appears in the chat; give it a one-line caption.", "inputSchema": {"type": "object", "properties": {"view": {"type": "string", "enum": ["3d", "plan", "elevation"]}, "diagram": {"type": "string", "enum": ["none", "moment", "shear", "axial", "momentz", "sheary", "torsion", "platemx", "platemy", "platemxy", "plateqx", "plateqy", "deflection", "mode", "buckling", "utilisation"]}, "mode": {"type": "integer", "description": "with diagram 'mode' or 'buckling': which mode, 1 = the first"}, "result": {"type": "string"}, "z": {"type": "number"}, "y": {"type": "number"}, "yaw": {"type": "number"}, "pitch": {"type": "number"}, "highlight": {"type": "array", "items": {"type": "integer"}}, "caption": {"type": "string"}, "show_loads": {"type": "boolean"}, "values": {"type": "boolean", "description": "plate diagrams: write the nodal value at every node (the mean of the elements around it)"}}}}));
    t.push(json!({"name": "import_stabileo", "description": "Replaces the model with a Stabileo model (JSON file at 'path', or the object in 'model'), as Aedifex's export_stabileo writes it: collinear pieces of one beam are merged into one member, element bimId (or name) kept for export_bim.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "model": {"type": "object"}}}}));
    t.push(json!({"name": "export_stabileo", "description": "Writes the model as a Stabileo model file (JSON) for the Stabileo editor.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}));
    t.push(json!({"name": "export_bim", "description": "The design for the BIM model (designs first if needed): per member its bim_id, ends, the Aedifex family and type ('Concrete Rectangular Beam' / '300 x 600 mm', 'Steel I Beam (W)' / 'H 300x150x6.5x9') with the type parameters in m, the reinforcement and the ratio; per slab its thickness and bars. Returned as JSON and, with 'path', written to a file.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}}));
    t
}

/// Runs the server until stdin closes. `--open <file>` starts with that model;
/// `--autosave <file>` saves the model there after every change; `--deny a,b` turns tools off.
pub fn serve() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let mut server = Server::new(Model::new("Untitled"));
    server.autosave = arg("--autosave").map(std::path::PathBuf::from);
    server.denied = arg("--deny").map(|d| d.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    if let Some(p) = arg("--open") {
        match Model::load(std::path::Path::new(&p)) {
            Ok(m) => server.model = m,
            Err(e) => eprintln!("pondus --mcp: {e}"),
        }
    }
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}})),
        };
        if let Some(r) = reply {
            let _ = writeln!(out, "{r}");
            let _ = out.flush();
        }
    }
}

fn r(x: f64, n: i32) -> f64 {
    design::round(x, n)
}

impl Server {
    pub fn new(model: Model) -> Server {
        Server { model, autosave: None, denied: vec![], analysis: None, report: None, pictures: 0 }
    }

    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned()?;
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "pondus", "title": "Pondus structural analysis and design (AGPL-3.0-only, source: https://github.com/Sompote/pondus)", "version": env!("CARGO_PKG_VERSION"), "license": "AGPL-3.0-only", "source": crate::about::SOURCE_URL},
                "instructions": INSTRUCTIONS,
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Ok(match self.call(name, &args) {
                    Ok(content) => json!({"content": content, "isError": false}),
                    Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
                })
            }
            _ => Err(json!({"code": -32601, "message": format!("unknown method {method}")})),
        };
        Some(match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
        })
    }

    fn resolve(&self, path: &str) -> std::path::PathBuf {
        let p = std::path::PathBuf::from(path);
        if p.is_absolute() {
            return p;
        }
        self.autosave.as_ref().and_then(|a| a.parent().map(|d| d.join(&p))).unwrap_or(p)
    }

    fn saved(&self) -> Result<(), String> {
        match &self.autosave {
            Some(p) => self.model.save(p),
            None => Ok(()),
        }
    }

    fn analysis(&mut self) -> Result<&Analysis, String> {
        if self.analysis.is_none() {
            self.analysis = Some(crate::analysis::analyze(&self.model)?);
        }
        Ok(self.analysis.as_ref().unwrap())
    }

    pub fn call(&mut self, name: &str, args: &Value) -> Result<Vec<Value>, String> {
        let text = |v: Value| vec![json!({"type": "text", "text": v.to_string()})];
        if self.denied.iter().any(|d| d == name) {
            return Err(format!("{name} is off in this conversation: the engineer did not ask for it. Offer it in your answer instead."));
        }
        match name {
            "analyze" => {
                self.analysis = None;
                let before = dedaliano_engine::linalg::gpu::factorisations();
                let t0 = std::time::Instant::now();
                let a = self.analysis()?.clone();
                let mut v = analysis_summary(&self.model, &a);
                // where the large factorisations ran: the NVIDIA GPU (cuDSS) when there is one
                let on_gpu = dedaliano_engine::linalg::gpu::factorisations() - before;
                v["compute"] = json!({"seconds": (t0.elapsed().as_secs_f64() * 100.0).round() / 100.0, "gpu_factorisations": on_gpu, "device": dedaliano_engine::linalg::gpu::summary()});
                Ok(text(v))
            }
            "get_results" => {
                let a = self.analysis()?.clone();
                let spec = scene::Spec { result: args.get("result").and_then(Value::as_str).unwrap_or("").into(), ..Default::default() };
                if !spec.result.is_empty() && a.result(&self.model, &spec.result).is_none() {
                    return Err(format!("no result '{}': give a case or combination name, 'moving max' / 'moving min', a seismic result or 'staged (final)' after staged", spec.result));
                }
                let res = scene::pick_result(&self.model, &a, &spec).ok_or("no results: the model has no cases")?;
                let want: Vec<Id> = args.get("members").and_then(Value::as_array).map(|v| v.iter().filter_map(|x| x.as_u64().map(|x| x as Id)).collect()).unwrap_or_default();
                let mut out = vec![];
                for mr in res.members.iter().filter(|m| want.is_empty() || want.contains(&m.member)) {
                    let Some(m) = self.model.member(mr.member) else { continue };
                    let ext = |f: &dyn Fn(&crate::analysis::Station) -> f64, max: bool| -> Value {
                        let s = mr.stations.iter().max_by(|a, b| if max { f(a).total_cmp(&f(b)) } else { f(b).total_cmp(&f(a)) });
                        s.map(|s| json!([r(f(s), 2), r(s.x, 2)])).unwrap_or(Value::Null)
                    };
                    let ez = crate::analysis::axes(&self.model, m).map(|x| x.2).unwrap_or([0.0, 0.0, 1.0]);
                    let defl = mr.local_deflection(ez).into_iter().map(f64::abs).fold(0.0, f64::max);
                    out.push(json!({
                        "member": m.id, "kind": format!("{:?}", self.model.kind(m)).to_lowercase(), "section": self.model.section(m.section).map(|s| s.name.clone()), "length": r(mr.length, 3), "bim_id": m.bim_id,
                        "My_sag_max [kNm, x]": ext(&|s| s.my, true), "My_hog_min [kNm, x]": ext(&|s| s.my, false),
                        "Vz_absmax [kN, x]": ext(&|s| s.vz.abs(), true), "N_min [kN, x] (compression -)": ext(&|s| s.n, false), "N_max [kN, x]": ext(&|s| s.n, true),
                        "T_absmax": ext(&|s| s.t.abs(), true), "Mz_absmax": ext(&|s| s.mz.abs(), true), "own_deflection_mm": r(defl * 1000.0, 2),
                    }));
                }
                let mut v = json!({"result": res.name, "members": out});
                if args.get("reactions").and_then(Value::as_bool).unwrap_or(false) {
                    v["reactions"] = json!(res.reactions.iter().map(|(n, f)| json!({"node": n, "at": self.model.node(*n).map(|x| x.p()), "F_kN": [r(f[0], 2), r(f[1], 2), r(f[2], 2)], "M_kNm": [r(f[3], 2), r(f[4], 2), r(f[5], 2)]})).collect::<Vec<_>>());
                }
                Ok(text(v))
            }
            "set_seismic" => {
                let mut v = serde_json::to_value(&self.model.seismic).map_err(|e| e.to_string())?;
                for (k, x) in args.as_object().into_iter().flatten() {
                    if v.get(k).is_none() {
                        return Err(format!("no seismic setting '{k}'"));
                    }
                    v[k] = x.clone();
                }
                self.model.seismic = serde_json::from_value(v).map_err(|e| format!("seismic settings: {e}"))?;
                self.analysis = None;
                self.report = None;
                self.saved()?;
                Ok(text(serde_json::to_value(&self.model.seismic).unwrap_or_default()))
            }
            "get_seismic" => {
                if !self.model.seismic.enabled {
                    return Err("the seismic analysis is off: set_seismic enabled true first".into());
                }
                let a = self.analysis()?.clone();
                let s = a.seismic.as_ref().ok_or("no seismic results")?;
                let mut run = [0.0; 3];
                let modes: Vec<Value> = s.modes.iter().enumerate().map(|(k, m)| {
                    for q in 0..3 {
                        run[q] += m.mass_ratio[q];
                    }
                    json!({"mode": k + 1, "T_s": r(m.period, 4), "f_Hz": r(m.frequency, 3), "mass_ratio_xyz": m.mass_ratio.map(|x| r(x, 4)), "cumulative_xyz": run.map(|x| r(x, 4))})
                }).collect();
                let dirs: Vec<Value> = s.dirs.iter().map(|d| json!({
                    "direction": d.name, "T1_s": r(d.t1, 3), "T_used_ELF_s": r(d.t_used, 3), "Cs": r(d.cs, 4), "V_ELF_kN": r(d.v_elf, 1), "V_RSA_kN": r(d.v_rsa, 1), "scale": r(d.scale, 3),
                    "drifts": d.drifts.iter().map(|x| json!({"storey_z": [r(x.z0, 2), r(x.z1, 2)], "delta_e_mm": r(x.delta_e * 1e3, 2), "delta_mm": r(x.delta * 1e3, 2), "allowed_mm": r(x.allowed * 1e3, 1), "ratio": r(x.ratio, 3)})).collect::<Vec<_>>(),
                })).collect();
                Ok(text(json!({"weight_kN": r(s.weight, 1), "mass_t": r(s.mass, 2), "modes": modes, "directions": dirs, "warnings": s.warnings, "not_included": "accidental torsion, P-Δ, the 100 %/30 % orthogonal combination, slab plate moments and support reactions under E"})))
            }
            "buckling" => {
                let a = self.analysis()?.clone();
                let want = args.get("combination").and_then(Value::as_str).unwrap_or("");
                let c = self.model.combos.iter().find(|c| (want.is_empty() && c.kind == ComboKind::Uls) || c.name.eq_ignore_ascii_case(want)).cloned().ok_or_else(|| format!("no combination '{want}'"))?;
                let n = args.get("modes").and_then(Value::as_u64).unwrap_or(4) as usize;
                let modes = crate::stability::buckling(&self.model, &a, &c, n)?;
                let factors: Vec<f64> = modes.iter().map(|m| r(m.factor, 3)).collect();
                if let Some(an) = self.analysis.as_mut() {
                    an.buckling = Some((c.name.clone(), modes));
                }
                let first = factors.first().copied().unwrap_or(f64::INFINITY);
                Ok(text(json!({"combination": c.name, "load_factors": factors, "reading": if first < 1.0 { "unstable under the combination" } else if first < 10.0 { "second-order effects matter (λ < 10): analyse second order" } else { "first-order analysis is adequate (λ ≥ 10)" }})))
            }
            "pushover" => {
                let a = self.analysis()?.clone();
                let spec = crate::advanced::PushoverSpec {
                    combination: args.get("combination").and_then(Value::as_str).map(String::from),
                    direction: if args.get("direction").and_then(Value::as_str) == Some("y") { 1 } else { 0 },
                    pattern: args.get("pattern").and_then(Value::as_str).unwrap_or("triangular").into(),
                };
                let p = crate::advanced::pushover(&self.model, &a, self.report.as_ref(), &spec)?;
                Ok(text(json!({
                    "collapse_factor": r(p.collapse_factor, 4), "mechanism": p.is_mechanism, "base_shear_kN": (spec.combination.is_none()).then(|| r(p.base_shear, 1)), "weight_kN": r(p.weight, 1),
                    "hinges": p.hinges.iter().map(|h| json!({"member": h.member, "position": r(h.position, 3), "lambda": r(h.at, 4)})).collect::<Vec<_>>(),
                    "curve [lambda, roof m]": p.curve.iter().map(|c| [r(c.0, 4), r(c.1, 5)]).collect::<Vec<_>>(), "warnings": p.warnings,
                })))
            }
            "set_moving" => {
                if args.get("clear").and_then(Value::as_bool).unwrap_or(false) {
                    self.model.moving = None;
                } else {
                    let path: Vec<Id> = args.get("path").and_then(Value::as_array).map(|v| v.iter().filter_map(|x| x.as_u64().map(|x| x as Id)).collect()).unwrap_or_default();
                    let axles: Vec<(f64, f64)> = args.get("axles").and_then(Value::as_array).map(|v| v.iter().filter_map(|x| Some((x.get(0)?.as_f64()?, x.get(1)?.as_f64()?))).collect()).unwrap_or_default();
                    for id in &path {
                        self.model.member(*id).ok_or_else(|| format!("no member {id}"))?;
                    }
                    if path.is_empty() || axles.is_empty() {
                        return Err("give path [member ids] and axles [[offset, kN], ...]".into());
                    }
                    let f = |k: &str, d: f64| args.get(k).and_then(Value::as_f64).unwrap_or(d);
                    self.model.moving = Some(crate::advanced::MovingSpec { name: args.get("name").and_then(Value::as_str).unwrap_or("Train").into(), path, axles, step: f("step", 0.25), impact: f("impact", 0.0), factor: f("factor", 1.6) });
                }
                self.analysis = None;
                self.report = None;
                self.saved()?;
                Ok(text(json!({"moving": self.model.moving})))
            }
            "influence_line" => {
                let a = self.analysis()?.clone();
                let q = args.get("quantity").and_then(Value::as_str).ok_or("quantity")?;
                let member = args.get("member").and_then(Value::as_u64).map(|x| x as Id);
                let node = args.get("node").and_then(Value::as_u64).map(|x| x as Id);
                let il = crate::advanced::influence(&self.model, &a, q, member, args.get("x").and_then(Value::as_f64).unwrap_or(0.0), node)?;
                let hi = il.iter().max_by(|a, b| a.value.total_cmp(&b.value));
                let lo = il.iter().min_by(|a, b| a.value.total_cmp(&b.value));
                let pt = |p: Option<&crate::advanced::InfluencePoint>| p.map(|p| json!({"member": p.member, "x": r(p.x, 3), "value": r(p.value, 5)}));
                Ok(text(json!({"quantity": q, "max": pt(hi), "min": pt(lo), "points": il.iter().map(|p| [p.member as f64, r(p.x, 3), r(p.value, 5)]).collect::<Vec<_>>()})))
            }
            "set_stages" => {
                let st: Vec<crate::advanced::Stage> = serde_json::from_value(args.get("stages").cloned().unwrap_or(json!([]))).map_err(|e| format!("stages: {e}"))?;
                for s in &st {
                    for id in &s.members {
                        self.model.member(*id).ok_or_else(|| format!("stage '{}': no member {id}", s.name))?;
                    }
                    for c in &s.cases {
                        self.model.cases.iter().find(|x| x.id == *c).ok_or_else(|| format!("stage '{}': no case {c}", s.name))?;
                    }
                }
                let built: std::collections::HashSet<Id> = st.iter().flat_map(|s| s.members.iter().copied()).collect();
                let missing: Vec<Id> = self.model.members.iter().map(|m| m.id).filter(|id| !built.contains(id)).collect();
                self.model.stages = st;
                self.saved()?;
                Ok(text(json!({"stages": self.model.stages.len(), "members_never_built": missing})))
            }
            "staged" => {
                let a = self.analysis()?.clone();
                let (st, fin) = crate::advanced::staged(&self.model, &a)?;
                let last = fin.members.iter().flat_map(|m| m.stations.iter().map(move |s| (m.member, s.x, s.my))).fold((0, 0.0f64, 0.0f64), |acc, x| if x.2.abs() > acc.2.abs() { x } else { acc });
                if let Some(an) = self.analysis.as_mut() {
                    an.staged = Some(fin);
                }
                Ok(text(json!({"stages": st.iter().map(|s| json!({"stage": s.name, "members": s.members, "max_displacement_mm": r(s.max_displacement * 1e3, 2), "max_moment [member, kNm]": [s.max_moment.0 as f64, r(s.max_moment.1, 2)]})).collect::<Vec<_>>(), "final_largest_My [member, x, kNm]": [last.0 as f64, r(last.1, 2), r(last.2, 2)], "result": "staged (final)"})))
            }
            "section_analysis" => {
                let outline: Vec<[f64; 2]> = match (args.get("outline").and_then(Value::as_array), args.get("name").and_then(Value::as_str)) {
                    (Some(v), _) => v.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect(),
                    (None, Some(n)) => {
                        let sid = self.model.clone().section_named(n)?;
                        let sec = self.model.clone();
                        let sec = sec.section(sid).ok_or("no section")?;
                        crate::secfe::outline_of(&sec.shape).ok_or("this section has no outline: give one")?
                    }
                    _ => return Err("give outline [[y, z], ...] or a section name".into()),
                };
                if outline.len() < 3 {
                    return Err("the outline needs three points or more".into());
                }
                let fe = crate::secfe::analyze(&outline, args.get("mesh").and_then(Value::as_u64).unwrap_or(600) as usize)?;
                Ok(text(json!({
                    "A_m2": fe.a, "centroid_m": fe.centroid, "Iy_m4 (about horizontal axis)": fe.iy, "Iz_m4": fe.iz, "Iyz_m4": fe.iyz, "Zy_m3 (plastic)": fe.zy, "Zz_m3": fe.zz,
                    "J_m4": fe.j, "Cw_m6": fe.cw, "shear_centre_m (from the centroid)": fe.shear_centre, "triangles": fe.elements,
                })))
            }
            "write_report" => {
                let a = self.analysis()?.clone();
                if self.report.is_none() {
                    self.report = Some(design::design_pass(&self.model, &a, &design::Options::default()));
                }
                let opts = crate::report::ReportOptions {
                    project: args.get("project").and_then(Value::as_str).unwrap_or("").into(),
                    engineer: args.get("engineer").and_then(Value::as_str).unwrap_or("").into(),
                    detail: args.get("detail").and_then(Value::as_str).unwrap_or("governing").into(),
                    pictures: args.get("pictures").and_then(Value::as_bool).unwrap_or(true),
                };
                let default = format!("{}-report.pdf", self.model.name.replace([' ', '/'], "_"));
                let p = self.resolve(args.get("path").and_then(Value::as_str).unwrap_or(&default));
                let r = crate::report::write_pdf(&self.model, &a, self.report.as_ref(), &opts, &p)?;
                Ok(text(serde_json::to_value(&r).unwrap_or_default()))
            }
            "export_file" => {
                let fmt = args.get("format").and_then(Value::as_str).unwrap_or("");
                let ext = match fmt {
                    "excel" => "xlsx",
                    "dxf" => "dxf",
                    "ifc" => "ifc",
                    _ => return Err("format: excel, dxf or ifc".into()),
                };
                let default = format!("{}.{ext}", self.model.name.replace([' ', '/'], "_"));
                let p = self.resolve(args.get("path").and_then(Value::as_str).unwrap_or(&default));
                let a = self.analysis().ok().cloned();
                match fmt {
                    "excel" => crate::excel::export(&self.model, a.as_ref(), self.report.as_ref(), &p)?,
                    "dxf" => {
                        let o: crate::dxf::DxfOptions = serde_json::from_value(with_view(args)).map_err(|e| format!("dxf options: {e}"))?;
                        std::fs::write(&p, crate::dxf::export(&self.model, a.as_ref(), &o)).map_err(|e| format!("{}: {e}", p.display()))?
                    }
                    _ => std::fs::write(&p, crate::ifc::export(&self.model, self.report.as_ref())).map_err(|e| format!("{}: {e}", p.display()))?,
                }
                Ok(text(json!({"written": p.display().to_string(), "format": fmt})))
            }
            "import_file" => {
                let fmt = args.get("format").and_then(Value::as_str).unwrap_or("");
                let p = self.resolve(args.get("path").and_then(Value::as_str).ok_or("give a path")?);
                let replace = args.get("replace").and_then(Value::as_bool).unwrap_or(fmt == "excel");
                let out = if fmt == "excel" {
                    let m = crate::excel::import(&p)?;
                    if replace {
                        self.model = m;
                    } else {
                        return Err("an Excel workbook replaces the model: give replace true".into());
                    }
                    json!({"model": commands::summary(&self.model)})
                } else {
                    let txt = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                    let r = match fmt {
                        "dxf" => crate::dxf::import(&txt, &serde_json::from_value(with_view(args)).map_err(|e| format!("dxf options: {e}"))?)?,
                        "ifc" => crate::ifc::import(&txt, &serde_json::from_value(args.clone()).map_err(|e| format!("ifc options: {e}"))?)?,
                        _ => return Err("format: excel, dxf or ifc".into()),
                    };
                    if replace {
                        let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".into());
                        self.model = Model::new(&name);
                    }
                    let ap = crate::dxf::apply(&mut self.model, &r)?;
                    json!({"members": ap.members.len(), "slabs": ap.slabs.len(), "nodes": ap.nodes, "counts": r.counts, "warnings": r.warnings.iter().chain(&ap.warnings).collect::<Vec<_>>(), "model": commands::summary(&self.model), "next": "add supports and loads, then analyze"})
                };
                self.analysis = None;
                self.report = None;
                self.saved()?;
                Ok(text(out))
            }
            "time_history" => {
                let a = self.analysis()?.clone();
                let text_rec = match (args.get("record").and_then(Value::as_str), args.get("path").and_then(Value::as_str)) {
                    (Some(t), _) => t.to_string(),
                    (None, Some(p)) => {
                        let p = self.resolve(p);
                        std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?
                    }
                    _ => return Err("give path or record".into()),
                };
                let rec = crate::timehistory::parse_record(args.get("path").and_then(Value::as_str).unwrap_or("record"), &text_rec, args.get("record_dt").and_then(Value::as_f64))?;
                let st = crate::timehistory::Settings {
                    direction: if args.get("direction").and_then(Value::as_str) == Some("y") { 1 } else { 0 },
                    scale: args.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
                    dt: args.get("dt").and_then(Value::as_f64).unwrap_or(0.01),
                    duration: args.get("duration").and_then(Value::as_f64),
                    damping: args.get("damping").and_then(Value::as_f64).unwrap_or(0.05),
                    alpha: None,
                };
                let res = crate::timehistory::run(&self.model, &a, &rec, &st)?;
                Ok(text(json!({
                    "record": res.record, "record_dt_s": rec.dt, "record_points": rec.accel.len(), "pga_g": r(res.pga_g, 4), "steps": res.time.len(),
                    "roof_peak_mm": r(res.roof_peak * 1e3, 2), "roof_peak_at_s": r(res.roof_peak_at, 3),
                    "storeys": res.storeys.iter().map(|x| json!({"z": [r(x.z0, 2), r(x.z1, 2)], "peak_drift_mm": r(x.drift * 1e3, 2), "drift_ratio": r(x.ratio, 5), "at_s": r(x.at, 3)})).collect::<Vec<_>>(),
                    "summed_peak_reactions_kN (upper bound of the base shear)": r(res.base_shear_bound, 1), "seismic_weight_kN": r(res.weight, 1), "warnings": res.warnings,
                    "note": "linear elastic: the drifts are elastic (not divided by R)"
                })))
            }
            "get_slab_results" => {
                if !self.model.slabs.iter().any(|s| self.model.plate_slab(s)) {
                    return Err("the slabs are designed by coefficients and have no plate results: set_design slab_method 'plate' first (or give a flat plate plate true)".into());
                }
                let a = self.analysis()?.clone();
                let spec = scene::Spec { result: args.get("result").and_then(Value::as_str).unwrap_or("").into(), ..Default::default() };
                let res = scene::pick_result(&self.model, &a, &spec).ok_or("no results")?;
                let want: Vec<Id> = args.get("slabs").and_then(Value::as_array).map(|v| v.iter().filter_map(|x| x.as_u64().map(|x| x as Id)).collect()).unwrap_or_default();
                let all = args.get("elements").and_then(Value::as_bool).unwrap_or(false);
                let mut out = vec![];
                for s in self.model.slabs.iter().filter(|s| want.is_empty() || want.contains(&s.id)) {
                    let ps: Vec<&crate::slabfe::PlateResult> = res.plates.iter().filter(|p| p.slab == s.id).collect();
                    if ps.is_empty() {
                        continue;
                    }
                    let ext = |f: &dyn Fn(&crate::slabfe::PlateResult) -> f64| {
                        let hi = ps.iter().max_by(|a, b| f(a).total_cmp(&f(b))).unwrap();
                        let lo = ps.iter().min_by(|a, b| f(a).total_cmp(&f(b))).unwrap();
                        json!({"max": [r(f(hi), 2), r(hi.x, 2), r(hi.y, 2)], "min": [r(f(lo), 2), r(lo.x, 2), r(lo.y, 2)]})
                    };
                    let wa: Vec<(f64, f64, f64, f64)> = ps.iter().map(|p| crate::slabfe::wood_armer(p.mx, p.my, p.mxy)).collect();
                    let m = |f: fn(&(f64, f64, f64, f64)) -> f64| r(wa.iter().map(f).fold(0.0, f64::max), 2);
                    let mut v = json!({
                        "slab": s.id, "name": s.name, "elements": ps.len(), "h_mm": r(s.h * 1000.0, 0),
                        "mx [value, x, y]": ext(&|p| p.mx), "my": ext(&|p| p.my), "mxy": ext(&|p| p.mxy), "qx": ext(&|p| p.qx), "qy": ext(&|p| p.qy),
                        "wood_armer_kNm_per_m": {"bottom_x": m(|w| w.0), "bottom_y": m(|w| w.1), "top_x": m(|w| w.2), "top_y": m(|w| w.3)},
                        "max_deflection_mm": r(ps.iter().map(|p| -p.uz).fold(f64::MIN, f64::max) * 1000.0, 2),
                    });
                    if all {
                        v["element_values [x, y, mx, my, mxy, qx, qy]"] = json!(ps.iter().map(|p| [r(p.x, 3), r(p.y, 3), r(p.mx, 2), r(p.my, 2), r(p.mxy, 2), r(p.qx, 2), r(p.qy, 2)]).collect::<Vec<_>>());
                    }
                    out.push(v);
                }
                Ok(text(json!({"result": res.name, "units": "kN·m/m for moments (sagging +), kN/m for shears, m for positions", "slabs": out})))
            }
            "design" => {
                let o: design::Options = serde_json::from_value(args.clone()).map_err(|e| format!("design options: {e}"))?;
                let before = self.model.clone();
                let (rep, a) = design::run(&mut self.model, &o)?;
                if self.model != before {
                    self.saved()?;
                }
                let s = rep.summary();
                self.analysis = Some(a);
                self.report = Some(rep);
                Ok(text(s))
            }
            "show_picture" => self.picture(args),
            "import_stabileo" => {
                let v = match (args.get("model"), args.get("path").and_then(Value::as_str)) {
                    (Some(m), _) => m.clone(),
                    (None, Some(p)) => {
                        let p = self.resolve(p);
                        serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| format!("{}: {e}", p.display()))?
                    }
                    _ => return Err("give path or model".into()),
                };
                let (m, rep) = crate::stabileo::import(&v)?;
                self.model = m;
                self.analysis = None;
                self.report = None;
                self.saved()?;
                Ok(text(json!({"import": rep, "model": commands::summary(&self.model)})))
            }
            "export_stabileo" => {
                let p = self.resolve(args.get("path").and_then(Value::as_str).ok_or("give a path")?);
                std::fs::write(&p, serde_json::to_string_pretty(&crate::stabileo::export(&self.model)).unwrap_or_default()).map_err(|e| format!("{}: {e}", p.display()))?;
                Ok(text(json!({"written": p.display().to_string()})))
            }
            "export_bim" => {
                if self.report.is_none() {
                    let a = self.analysis()?.clone();
                    self.report = Some(design::design_pass(&self.model, &a, &design::Options::default()));
                }
                let v = design::bim_export(&self.model, self.report.as_ref().unwrap());
                if let Some(p) = args.get("path").and_then(Value::as_str) {
                    let p = self.resolve(p);
                    std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| format!("{}: {e}", p.display()))?;
                    return Ok(text(json!({"written": p.display().to_string(), "bim": v})));
                }
                Ok(text(v))
            }
            _ => {
                let spec = commands::specs().into_iter().find(|s| s.name == name).ok_or(format!("unknown tool {name}"))?;
                let v = commands::run(&mut self.model, name, args)?;
                if spec.changes {
                    self.analysis = None;
                    self.report = None;
                    self.saved()?;
                }
                Ok(text(v))
            }
        }
    }

    fn picture(&mut self, args: &Value) -> Result<Vec<Value>, String> {
        let mut spec: scene::Spec = serde_json::from_value(args.clone()).map_err(|e| format!("show_picture: {e}"))?;
        // the agent counts modes from 1
        spec.mode = args.get("mode").and_then(Value::as_u64).map(|m| (m as usize).saturating_sub(1)).unwrap_or(0);
        let needs = !matches!(spec.diagram, scene::Diagram::None);
        let analysis = if needs { Some(self.analysis()?.clone()) } else { self.analysis.clone() };
        if spec.diagram == scene::Diagram::Utilisation && self.report.is_none() {
            self.report = Some(design::design_pass(&self.model, analysis.as_ref().unwrap(), &design::Options::default()));
        }
        let prims = scene::build(&self.model, analysis.as_ref(), self.report.as_ref(), &spec);
        let mut img = crate::picture::render(&prims, &scene::Camera::for_view(&spec), 1024, 704);
        // a plate diagram gets its colour scale, in the picture and in the text
        let mut scale = String::new();
        if spec.diagram.is_plate() {
            if let Some(r) = analysis.as_ref().and_then(|a| scene::pick_result(&self.model, a, &spec)) {
                let vmax = scene::plate_range(&self.model, &r, &spec);
                let (label, units) = spec.diagram.plate_label();
                let short = label.split(':').next().unwrap_or(label);
                img.legend(vmax, &format!("{short} {}", r.name), units);
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                for p in r.plates.iter().filter(|p| spec.z.is_none_or(|z| self.model.slabs.iter().any(|s| s.id == p.slab && (s.z - z).abs() < 0.05))) {
                    let v = spec.diagram.plate_value(p);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                scale = format!(" Colour scale of {label} in {}: symmetric, blue -{vmax:.1} to red +{vmax:.1} {units}; the values themselves run from {lo:.1} to {hi:.1} {units} (quote these, not the scale). The picture shades the nodal values, each node the mean of the elements around it; values true writes them at the nodes. The design uses the element values.", r.name);
            }
        }
        self.pictures += 1;
        let dir = self.autosave.as_ref().and_then(|a| a.parent().map(|d| d.join(".pictures"))).unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(format!("picture-{}-{}.png", std::process::id(), self.pictures));
        crate::picture::write_png(&img, &file)?;
        // the chat redraws the picture from the model and this spec
        if let Some(a) = &self.autosave {
            if let Some(d) = a.parent() {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join(".pictures.jsonl")) {
                    let _ = writeln!(f, "{}", serde_json::to_string(&spec).unwrap_or_default());
                }
            }
        }
        let png = std::fs::read(&file).map_err(|e| e.to_string())?;
        Ok(vec![json!({"type": "image", "data": base64(&png), "mimeType": "image/png"}), json!({"type": "text", "text": format!("Picture saved at {}.{scale} {}", file.display(), spec.caption)})])
    }
}

/// The DXF options with the 3D view when the call names none (the view is a serde tag).
fn with_view(args: &Value) -> Value {
    let mut v = args.clone();
    if v.get("view").is_none() {
        v["view"] = json!("3d");
    }
    v
}

/// What the agent needs to judge an analysis: balance, extremes, warnings.
pub fn analysis_summary(m: &Model, a: &Analysis) -> Value {
    let cases: Vec<Value> = m.cases.iter().filter_map(|c| a.case(c.id).map(|cr| (c, cr))).map(|(c, cr)| {
        let rs = cr.reaction_sum();
        // self-weight is applied inside the analysis: the applied total includes it
        json!({"case": c.name, "applied_kN": [r(cr.applied[0], 2), r(cr.applied[1], 2), r(cr.applied[2], 2)], "reactions_kN": [r(rs[0], 2), r(rs[1], 2), r(rs[2], 2)], "balanced": (0..3).all(|k| (cr.applied[k] + rs[k]).abs() <= 1e-6 * (1.0 + cr.applied.iter().map(|x| x.abs()).fold(0.0, f64::max)))})
    }).collect();
    let combos: Vec<Value> = m.combos.iter().map(|c| {
        let cr = a.combine(&c.name, &c.factors);
        let (dn, dv) = cr.displacements.iter().map(|(n, d)| (*n, (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())).fold((0, 0.0), |b, x| if x.1 > b.1 { x } else { b });
        let mut mmax: (Id, f64, f64) = (0, 0.0, 0.0);
        let mut nmin: (Id, f64) = (0, 0.0);
        for mr in &cr.members {
            for s in &mr.stations {
                if s.my.abs() > mmax.1.abs() {
                    mmax = (mr.member, s.my, s.x);
                }
                if s.n < nmin.1 {
                    nmin = (mr.member, s.n);
                }
            }
        }
        json!({"combination": c.name, "kind": c.kind, "max_displacement_mm": r(dv * 1000.0, 2), "at_node": dn, "max_moment_kNm": {"member": mmax.0, "My": r(mmax.1, 2), "x": r(mmax.2, 2)}, "max_compression_kN": {"member": nmin.0, "N": r(nmin.1, 2)}, "vertical_reaction_kN": r(cr.reaction_sum()[2], 2)})
    }).collect();
    json!({
        "elements": a.elements,
        "cases": cases,
        "combinations": combos,
        "slab_load_on_beams_kN": {"dead": [r(a.slab_applied[0], 1), r(a.slab_expected[0], 1)], "live": [r(a.slab_applied[1], 1), r(a.slab_expected[1], 1)], "note": "[applied to beams, slab total]"},
        "warnings": a.warnings,
        "note": "linear elastic, first order, gross sections (concrete deflections here are lower bounds; design uses cracked inertia)",
    })
}

pub fn base64(b: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}
