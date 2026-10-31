<p align="center"><img src="docs/logo.png" alt="Pondus" width="420"></p>

<h1 align="center">Pondus</h1>

<p align="center"><b>Structural analysis and design that you can talk to.</b><br>
Ask an AI agent to build, analyse and design a building, then check every number yourself in the same model.</p>

<p align="center">
<a href="https://doi.org/10.5281/zenodo.23581167"><img src="https://zenodo.org/badge/DOI/10.5281/zenodo.23581167.svg" alt="DOI 10.5281/zenodo.23581167"></a>
</p>

<p align="center">
<a href="https://github.com/Sompote/pondus/releases/latest"><b>⬇ Download for macOS or Windows</b></a> ·
<a href="https://strpondus.github.io/">Website and manual</a> ·
<a href="https://doi.org/10.5281/zenodo.23581167">Paper (Zenodo)</a> ·
<a href="paper/paper.pdf">Calibration report</a> ·
<a href="docs/mcp.md">Use with Claude or ChatGPT</a>
</p>

<p align="center"><img src="docs/burj-khalifa-modes.png" alt="Pondus with the first mode shape of a simplified Burj Khalifa model" width="900"></p>
<p align="center"><sub>The earthquake analysis of a simplified Burj Khalifa model (586 m of concrete, 156 storeys, the buttressed core with three wings and setbacks), built in Pondus from published dimensions: the first mode shape in the view, and the periods, mass shares, base shear and storey drifts in the Seismic table below it.</sub></p>

## What Pondus does for a civil engineer

- **Tell it what to build.** Type a request such as *"a 3-storey concrete frame, 3 bays of 6 m by 2 bays of 5 m, 150 mm slabs, office live load, design the beams and slabs"*. The agent builds the model, runs the analysis and designs it while you watch.
- **Or draw it yourself.** Beams, columns, bays, slabs, walls, supports and loads are drawn and edited with the mouse, and you can switch between the agent and your own hand at any time.
- **Analysis:** 3D frames with slabs, shear walls, earthquake (response spectrum with CQC or SRSS, storey masses at the centre of mass), P-Delta and buckling, pushover, moving loads for bridges, construction stages and time history, with optional shear deformation of the members as in SAP2000 and ETABS.
- **Design:** reinforced concrete beams, slabs, columns, walls and footings to **ACI 318-19**, and steel to **AISC 360-16** or **Eurocode 3**, with the bars and sizes chosen for you.
- **Thai regulation:** the load factors and strength reduction factors can follow the **Thai Ministerial Regulation on structural design B.E. 2566** (กฎกระทรวง กำหนดการออกแบบโครงสร้างอาคาร พ.ศ. 2566, which replaced Regulation No. 6 of B.E. 2527), with or without specified quality control. The design then uses U = 1.4D + 1.7L, 0.75(1.4D + 1.7L) + 1.6W or ± 1.0E and 0.9D + 1.6W or ± 1.0E, and φ of 0.90 / 0.70 / 0.85 for flexure, tied columns and shear (0.75 / 0.60 / 0.70 without quality control). Choose it in the settings window or with `set_design` `standard`, and use `code_combinations` to rebuild the model's combinations.
- **Report:** a complete calculation report as a PDF, ready to check and submit.
- **Exchange:** Excel, DXF (AutoCAD) and IFC (Revit and BIM).
- **Fast on large models:** a 156-storey tower with 41,000 elements and 60 earthquake modes analyses in about a minute on a 16-core computer, and Windows and Linux machines with an NVIDIA GPU factorise the large stiffness matrices on the card, with the same answers.
- **Checked against published results:** 470 comparisons with textbooks and design-code examples, 430 of them within 0.5 %, plus a check against a full-scale building tested on a shake table ([calibration report](paper/paper.pdf)), and 199 comparisons with the verification problems of SAP2000 and ETABS, 177 of them within 0.5 % ([paper](https://doi.org/10.5281/zenodo.23581167), [notes](calibration/NOTES_csi.md)).

## The paper

**[Pondus: an agentic structural analysis and design program](https://doi.org/10.5281/zenodo.23581167)** (S. Youwai, October 2026, Zenodo, DOI [10.5281/zenodo.23581167](https://doi.org/10.5281/zenodo.23581167); also in this repository as [paper/pondus-agentic.pdf](paper/pondus-agentic.pdf)) describes:
- **The agent:** how it is set up, in the built-in chat and from Claude Desktop, Claude Code and ChatGPT through the MCP server.
- **The analysis and design:** how the agent builds, analyses and designs a structure.
- **The engine:** how Pondus connects to the Stabileo engine and to the Stabileo and Aedifex model files.
- **Two recorded agent runs:** a two-storey frame in 105 s, and a five-storey wall-frame building with an earthquake analysis and a calculation report in 382 s. The agent's answers are audited against the program and hand checks: every number was right, but four descriptions were wrong.
- **Both calibrations:** against 470 published benchmarks, and against 199 verification problems of SAP2000 and ETABS.

The detailed report of the first calibration, with every comparison listed, is [paper/paper.pdf](paper/paper.pdf).

If you use Pondus in your work, please cite:

```bibtex
@misc{youwai2026pondus,
  author    = {Youwai, Sompote},
  title     = {Pondus: an agentic structural analysis and design program},
  year      = {2026},
  publisher = {Zenodo},
  version   = {v1},
  doi       = {10.5281/zenodo.23581167},
  url       = {https://doi.org/10.5281/zenodo.23581167}
}
```

## Features

### Bending moments and forces
Every member's moment, shear and axial force is drawn on the model, and the design table below lists the bars chosen for every beam.

<img src="docs/features/moments.png" alt="Bending moment diagram of a 3-storey frame" width="900">

### Design ratios at a glance
After design, every member and slab is coloured by how hard it works: green below 0.7, yellow below 0.9, orange below 1.0 and red when it fails.

<img src="docs/features/design-ratios.png" alt="Design ratio colours" width="900">

### Earthquake analysis
The response spectrum after ASCE 7-22 and the Thai DPT 1301/1302-61, with the modes, the base shear and the storey drift check. Click any member to see its forces and its full ACI design on the right.

<img src="docs/features/earthquake.png" alt="Earthquake settings, modes and drift check" width="900">

### Slabs as plates
Slabs can be designed by the code coefficient tables or meshed as plates with the frame, with the moments shown in colour and the bars computed from them.

<img src="docs/features/slab-plate.png" alt="Slab plate moments in plan" width="900">

### Shear walls
Draw a wall, and Pondus connects it to the floors, analyses it and designs it storey by storey to ACI 318-19 Chapters 11 and 18.

<img src="docs/features/shear-wall.png" alt="A shear wall in a frame" width="900">

### Moving loads and bridges
Run a truck or any train of axles along the beams. Pondus finds the worst position at every point and designs the members for it.

<img src="docs/features/moving-load.png" alt="Moving load envelope" width="900">

### Pushover
Push the building sideways until it forms a mechanism, and read the collapse load, the hinge sequence and the capacity curve.

<img src="docs/features/pushover.png" alt="Pushover analysis" width="900">

### Drawing by hand and the agent side by side
Draw with the toolbar and the palette, edit in the tables, or ask the agent in the chat on the right. Both work on the same model, and Undo takes back either.

<img src="docs/features/draw-by-hand.png" alt="A portal drawn by hand with the agent panel open" width="900">

### The calculation report
One click writes the PDF: basis of design, loads and their balance, diagrams, every design table and the full calculation of the governing members.

<img src="docs/features/report.png" alt="Pages of the calculation report" width="900">

## Download and install

| Computer | Download |
|---|---|
| macOS 11 or later (Apple Silicon and Intel) | [Pondus-0.1.5-macos-universal.zip](https://github.com/Sompote/pondus/releases/download/v0.1.5/Pondus-0.1.5-macos-universal.zip) |
| Windows 10 or later | [Pondus-0.1.5-windows-x64.zip](https://github.com/Sompote/pondus/releases/download/v0.1.5/Pondus-0.1.5-windows-x64.zip) |

- **macOS:** unzip, move `Pondus.app` to Applications, and the first time right-click it and choose **Open**.
- **Windows:** unzip and run `pondus.exe`. If Windows warns about an unknown app, choose **More info › Run anyway**.
- **The agent** needs an AI provider (for example OpenRouter): enter its address, model and key under **Agent › Settings**. Everything else works without it.

You can also let **Claude Desktop, Claude Code or ChatGPT** drive Pondus directly. The setup takes a few lines and is explained in [docs/mcp.md](docs/mcp.md).

> Structural results from any software must be checked by a qualified engineer before they are used for design or construction.

---

## For developers and reviewers

The sections below hold the technical details. Click a heading to open it.

<details>
<summary><b>Verification and the papers</b></summary>

**[Pondus: an agentic structural analysis and design program](https://doi.org/10.5281/zenodo.23581167)** (S. Youwai, October 2026, DOI 10.5281/zenodo.23581167, [PDF](paper/pondus-agentic.pdf)) is the main paper. It covers the agent, the link to Stabileo, two audited agent runs, and both calibrations: Section 8.2 summarises the benchmark calibration, and Section 8.4 the SAP2000 and ETABS calibration with its eight defects.

**[Calibrating an agentic structural analysis and design program against published benchmarks](paper/paper.pdf)** (S. Youwai, October 2026) reports how Pondus was checked against reference values from textbooks, software verification manuals and the published worked examples of the design codes. Every comparison goes through Pondus's own model, analysis wrapper and design code. Of 470 comparisons over 229 cases, 430 agree within 0.5 % and 449 within 1 %, and each of the other 21 is explained by mesh convergence, the conservative load contour for biaxial bending, the rounded SI constants of ACI 318 or the shear deformation of walls that Euler–Bernoulli references leave out. Shear walls under earthquake and dynamic loading are checked against the cantilever frequencies of Clough and Penzien, the wall-frame continuum of Heidebrecht and Stafford Smith, a one-storey system with torsional coupling and a step response by modal superposition. Moving loads are checked against the HL-93 envelopes of the FHWA two-span continuous steel girder bridge (Design Example 2A), within 0.16 % for the vehicles and 0.65 % with the lane load. The calibration and the closed-form tests before it found fourteen defects, six of them unconservative, and thirteen of the fourteen lay in the code that prepares the inputs of a kernel or a check rather than in the kernels. A separate validation against the full-scale 7-storey wall building tested on the UCSD–NEES shake table (`core/tests/validation_ucsd.rs`) gives the measured first period within 6.2 % and the second-mode frequency within 0.9 % with gross sections, and the ACI 318 effective stiffnesses bracket the periods measured after the earthquake tests. The paper's tables and figures are generated from the test output (`calibration/*.csv`, `calibration/validation/`), and `paper/build.sh` rebuilds the PDF in arXiv format (pandoc and tectonic) together with the arXiv submission package `paper/arxiv/submission.tar.gz`.

**Calibration against the SAP2000 and ETABS verification manuals.** `core/tests/calib_csi_analysis.rs` and `calib_csi_design.rs` rebuild 28 problems of CSI's *SAP2000 Software Verification* and *Software Verification Examples for ETABS 2016* (frame statics, releases, temperature, settlements, no-tension members, Winkler beams, P-Delta, buckling, staged construction, modal and response spectrum analysis of 2D and 3D frames with storey masses, planar shear walls, and the ACI 318, AISC 360, EN 1992 and EN 1993 design examples) and compare Pondus with each problem's independent value; the SAP2000 or ETABS value is recorded beside it in `calibration/programs.csv`. Of 199 comparisons over 64 cases, 177 agree within 0.5 % and 180 within 1 %. The calibration found eight defects, among them a Eurocode 3 moment resistance not reduced for high shear (8 % unconservative) and Cm taken as 1.0 in the Annex B interaction (up to 41 % conservative), and it led to lumped nodal masses (`add_masses`), the SRSS combination and an optional frame shear deformation (`set_design shear_deformation`). The PDFs, the extracted data and the notes are in `calibration/sources/csi/` and `calibration/NOTES_csi.md`.

**Calibration against published benchmarks.** `core/tests/calib_analysis.rs`, `calib_steel.rs`, `calib_concrete.rs`, `calib_walls.rs` and `calib_bridge.rs` compare Pondus with 470 reference values over 229 cases. The references are textbooks, the ANSYS Verification Manual, the AISC Design Examples v16, the SCI/CTICM, P364 and JRC Eurocode examples, the INTI-CIRSOC examples, StructurePoint's ACI 318 examples, a scan of the ACI 318-63 Method 2 table and, for walls, the wall-frame continuum and one-storey torsional coupling, and for moving loads the FHWA Steel Bridge Design Handbook Design Example 2A. Every comparison goes through Pondus's own model, analysis and design functions. 430 agree within 0.5 % and 449 within 1 %, and the remaining 21 are mesh convergence, Bresler's conservative load contour (−8 %), the rounded SI constants of ACI 318 and the shear deformation of walls in their third mode (about −1 %). Fourteen defects were found and fixed, six of them unconservative. Two of the last were rigid wall arms of absolute stiffness that lost precision in a 40-storey wall-frame model, which is why the arms are now scaled to their own wall, and a moving-load envelope read one element away from the point asked for, 0.8 % high on the bridge example. The rows are written to `calibration/*.csv` with `PONDUS_CALIB=$PWD/calibration`, the sources, pages and fixes are in `calibration/NOTES_*.md`, and the paper is built by `paper/build.sh` into `paper/paper.pdf`.


The tests run with `cargo test --release`.

- **Analysis against closed-form results** (`core/tests/analysis.rs`):
  - A simply supported beam gives wL²/8, V = wL/2 and 5wL⁴/384EI, the last to within 0.1 %.
  - Fixed ends give −wL²/12 and +wL²/24.
  - A column under a downward load is in compression.
  - A cantilever with a point load gives Pa²(3L − a)/6EI exactly.
  - Section roll turns a column's strong axis as documented.
  - The slab load reaching the beams equals the slab total.
- **Design against the engine's own, independently written checkers** (`core/tests/engine_crosscheck.rs`):
  - ACI φMn and φVn are identical (357.93 kN·m, 242.89 kN).
  - For four H shapes, AISC φPn (compression and tension), φMnx, φMny and Vn agree within 2 %.
  - Two differences were traced and settled. The engine takes φv = 0.9 for every web, whereas G2.1(a) gives 1.0 for rolled shapes. The engine loads beams on the top flange by default, which Pondus now does too.
- **Sections:** the computed H-shape area and Ix match the JIS tables for five sizes, within 0.2 % for area and 1 % for Ix.
- **End-to-end MCP flows** (`core/tests/flow.rs`):
  - A 2 × 2 bay, two-storey concrete building is designed with resize.
  - A steel frame selects its shapes.
  - An Aedifex-style import with four-piece beams merges back to one beam and survives a Stabileo round trip.
- **Other codes** (`core/tests/othercodes.rs`, `engine_crosscheck.rs`): the timber, masonry and cold-formed checks and the shear tab bolt group match hand calculations exactly (0.6184, 0.0974, Ae = 516.9 mm², 0.668), and the EC3 buckling reduction χ matches the hand value (0.2915).
- **Nonlinear supports and members** (`core/tests/nonlinear.rs`): a tension-only brace pair takes the whole shear in one brace (37.500 kN exactly), a lift-off support releases (50/50/0 kN), a beam on a uniform Winkler foundation settles uniformly (1.500 mm), and a guyed pole's moment is within 0.3 % of statics.
- **Advanced analyses** (`core/tests/advanced.rs`):
  - A fixed-ended steel beam collapses at 16Mp/(wL²) exactly (λ = 5.9030) with hinges at both ends and midspan.
  - A pinned portal sways at 2Mp/(hW) exactly (λ = 13.1686), and the capacity curve rises at every event.
  - One axle over a simple span gives PL/4 = 150.000 kN·m, and the reaction influence line is 1 − x/L to 1e-14.
  - Two stages with everything built in the first equal the linear D + L solve (−31.081 kN·m).
  - On the 3 × 2 bay, three-storey test building the lateral pushover forms a first-storey sway mechanism (24 column hinges, λ = 0.0674, V = 323.1 kN = 6Mp with Mp = 53.85 kN·m), in about 4 s.
- **Editing and interchange** (`core/tests/editing.rs`, `core/tests/interchange.rs`): splitting a loaded beam in three and merging it back keeps wL²/8 = 45 kN·m and the reactions to 1e-9; a portal copied three times merges its shared nodes and balances; mirrored and rotated members keep their section axes; Excel, DXF and IFC round trips return the same members (ends within 1 mm), sections and slabs; a hand-written IFC2x3 file in mm with nested placements lands at the hand-computed coordinates; and an IFC file from Aedifex imports onto its 6 m grid and analyses. ezdxf 1.4.5 audits the DXF with no errors and ifcopenshell 0.9.0 validates the IFC4 with no issues.
- **The calculation report** (`core/tests/flow.rs`): a designed two-storey building writes a seven-page PDF whose deflection numbers match L/360 and L/240 by hand.
- **The window** is rendered offscreen with egui_kittest: `PONDUS_SNAPSHOTS=dir cargo test -p pondus --release ui_`.

</details>

<details>
<summary><b>How the analysis and design work</b></summary>

The analysis is linear static and three-dimensional. Each member is split at every node lying on it, at every load break point and into at least eight elements, so each element carries a load that is linear over its whole length. The stiffness is factored once, every load case is solved on it, and combinations are factored sums of cases, which is exact for a linear analysis. Slab panels pass their load to the beams under their edges along the 45° lines (triangles and trapezoids), or one way when the long-to-short ratio exceeds 2, and the analysis reports the slab load that reached the beams against the slab total, so lost load is visible rather than silent.

Concrete beam design works per third of the span. Top steel comes from hogging and bottom steel from sagging, compression steel is added when the section cannot stay tension-controlled (εt ≥ 0.005), and bars are chosen to fit in one or two layers. Stirrups follow 22.5 and 9.7.6.2.2 for the end quarters and the middle half. Deflection uses the effective inertia of Table 24.2.3.5 with the long-term multiplier of 24.2.4.1, checked against L/360 (live) and L/240 (long-term), alongside the minimum depth of Table 9.3.1.1. Significant axial force is flagged but not designed.

Slabs are analysed and designed in one of two ways, chosen with `slab_method` (ribbon › Analyse, the settings window, `set_design` or the `design` tool). The default, **coefficients**, passes the slab load to the beams along the 45° lines and takes the slab moments from the code tables described below. The alternative, **plate**, meshes every panel into the engine's MITC4 shell elements (0.5 m by default, `slab_mesh`), joined to the beams and columns at shared nodes, so slab and frame are analysed together, and a panel can rest directly on columns or supports with no beams at all: a flat plate on its columns, or a pile cap or mat foundation on pile supports and / or a Winkler subgrade (`add_mat`, or a slab with `mat`, `subgrade` and `support_size`; the Slab properties in the app). Such a panel takes the bars of each face over the whole panel and is checked for punching at every column and support (ACI 318-19 22.6, the critical perimeter at d/2 clipped at the panel's edges, with the size effect factor). Worked examples, with the build script, the models, the pictures and the reports, are in [examples/piled-raft](examples/piled-raft): a nine-pile cap, a 16-pile raft under four columns and the same raft on soil. The bars then come from the element moments by Wood–Armer, enveloped over the ULS combinations (bottom bars from the middle half of the panel, top bars from the quarter next to each edge), and one-way shear comes from the element shears qx and qy outside d from the edges. Punching shear is checked at every column and support inside a panel; shear reinforcement and drop panels are left to the engineer. The slab results can be read three ways. In the view, the ribbon's Slab group colours mx, my, mxy, qx or qy on a diverging scale with a legend; the values toggle writes each element's number, and hovering shows all of an element's values with its Wood–Armer moments and deflection. A selected slab's Properties list each result's extremes and where they occur. For agents, the MCP tool `get_slab_results` returns the same per slab, optionally element by element. On Navier's simply supported square plate (ν = 0.2) the mesh gives the centre moment within 0.8 % and the deflection within 0.8 % (`core/tests/plate.rs`). On a 6 × 5 m panel of the test building its moments are 10–35 % below the Method 2 values, because the plate rests on deflecting beams and the tables assume rigid supports.

With plate slabs the edge moments put compatibility torsion into the beams, and torsion is designed for both methods: above the threshold of 22.7.4.1 Pondus provides closed stirrups (At/s combined with the shear stirrups), longitudinal steel Al (a quarter added to the top and bottom bars, half as side bars) and checks the section limit of 22.7.7.1. A hand calculation for a 300 × 600 beam under 20 kN·m (At/s = 0.544 mm²/mm, Al = 500 mm²) is in the tests. Round bars (RB6, RB9) take the stirrup grade fyt (SR24) and deformed bars the main grade fy (SD40), in slabs as in beams.

Two-way slabs on beams use the ACI 318-63 Method 2 coefficients (the method of EIT 1008), and one-way slabs use the coefficients of ACI 318-19 6.5. Both apply As,min = 0.0018bh, the spacing limits, the minimum thickness of Table 8.3.1.2 (stiff edge beams assumed) or Table 7.3.1.1, and one-way shear. Edge continuity is found from neighbouring panels at the same level.

Structural (shear) walls are real wall objects (`add_walls`, ribbon › Wall), not columns. Each wall is analysed by the wide-column analogy: a pier member on its centre line between consecutive levels carries the wall's section t × lw, with shear deformation (5/6 of the area), and rigid weightless arms at every level run from the centre line out to its ends. Beams and slab edges that meet the wall anywhere along its length therefore connect to it, as in the building, and beams lying inside a wall are replaced by it. A wall is rebuilt when new floors pass through it. On a cantilever wall the top deflection equals PH³/3EI + PH/κGA exactly, in plane and out of plane (`core/tests/walls.rs`). Each pier is designed per storey to ACI 318-19 Chapter 11, and to 18.10 when the earthquake analysis is on: distributed vertical and horizontal bars (Table 11.6.1 or 11.6.2, spacing ≤ min(3t, 450 mm), two curtains where 11.7.2.3 or 18.10.2.2 asks), in-plane shear Vn = Acv(αc√f'c + ρt fy) ≤ 0.66√f'c Acv, axial force with in-plane and with out-of-plane bending by strain compatibility, and the stress test for special boundary elements (18.10.6.3) with their length (18.10.6.4a). Walls go to Aedifex as Basic Walls, to IFC as IfcWall and to DXF as WALL layers. The vertical bars are distributed evenly (boundary-bar concentrations are not optimised), boundary-element confinement and the capacity-design shear amplification of 18.10.3.1 are left to the engineer, and wall footings are not designed.

Concrete columns are designed to ACI 318-19. Pondus chooses the least bar arrangement (ρ from 1 % to 4 %, bars around the perimeter) that passes axial load and biaxial bending in every combination. Bending is checked by strain compatibility per axis and combined by Bresler's load contour (α = 1.15). Braced-frame slenderness magnification (6.6.4) and the minimum moment apply unless the forces are second order. Shear includes the benefit of axial compression (22.5.6.1), and ties follow 25.7.2. The uniaxial capacity matches the engine's column kernel within 1 % at four axial loads. Spread footings are designed under every supported concrete column with the engine's ACI 318-19 footing check: plan size from the allowable bearing pressure under service loads, thickness for one-way and punching shear under factored loads, and bars for the cantilever moment at the column face, in both directions.

Second-order analysis (P-Delta, `set_design second_order`) solves every ULS combination with the engine's P-Delta solver, and linear buckling (the `buckling` tool, or ribbon › Buckling) gives the load factors and mode shapes of a combination. On a 6 m cantilever column the buckling factor matches Euler within 0.05 % (10.658 vs 10.652), and the P-Delta sway amplification matches the exact 3(tan u − u)/u³ within 0.02 % (1.6574 vs 1.6576). The model also takes rigid floor diaphragms (`set_diaphragms`), truss members, spring supports, support settlements and temperature changes. All of them are checked against hand results in `core/tests/modelling.rs`: 6EIδ/L² for a settlement, EAαΔT for a restrained bar, the spring share of a propped cantilever, two columns sharing a load through a diaphragm, and a two-bar truss. A linear time-history analysis under a recorded ground motion (`time_history`, ribbon › History; PEER .AT2 or a table) uses the seismic mass, and its peak tip displacement of a cantilever matches an independent single-mode Newmark integration within 1 %.

Steel members are rolled H shapes from JIS G 3192 / TIS 1227, whose properties are computed from the plate dimensions and fillets. The checks are D2 tension, E3 compression with E7 effective area, F2/F3 lateral-torsional buckling with Cb from the moment diagram, F6 minor-axis flexure, G2 shear and H1-1 interaction with B1 amplification. A beam's load is taken on its top flange, so Cb is divided by 1.4 (AISC Commentary F1). Service deflections are checked against L/360 and L/240.

Earthquake loading is analysed by modal and response spectrum analysis after ASCE 7-22 Chapter 12 and DPT 1301/1302-61, which follows it (ribbon › Seismic, or the MCP tools `set_seismic` and `get_seismic`). Following Stabileo's editor, the mass is the seismic weight W = D + 0.25L of the model's own load cases (the live share can be set), turned into a mass density per element so the engine's consistent mass carries it. The modes are found until 90 % of the mass takes part in x and y. Each mode's displacement field Γ·Sd·φ gives its member forces, which are combined by CQC at every station, and the base shear is scaled up to the equivalent lateral force value: Cs from the spectrum at T ≤ Cu·Ta, with the 0.044·SDS·Ie and 0.01 minimums. Storey drifts Cd·δe/Ie are checked against the allowable drift. The design then takes the combinations (1.2 + 0.2SDS)D + L ± Ex/Ey and (0.9 − 0.2SDS)D ± Ex/Ey as well. The spectrum is either the two-parameter code shape (SDS, SD1, TL) or a table of T and Sa, for example a Bangkok basin zone of DPT 1302-61. On a 4 m cantilever column the first frequency matches 1.8751²/2π·√(EI/mL⁴) within 0.05 % (12.395 vs 12.389 Hz). On a three-storey RC frame, the seismic weight equals D + 0.25L within 1 %, and Ta, Cs and the ELF base shear match a hand calculation (Ta = 0.403 s, Cs = 0.075, V = 380.9 kN). Not included are accidental torsion, P-Δ, the 100 %/30 % orthogonal combination, slab plate moments and support reactions under E, and the special detailing of seismic frames.

Beyond ACI 318 and AISC 360, steel H members can be checked to EN 1993-1-1 instead (`set_design steel_code ec3`), with the section class of Table 5.2, the buckling curves of Table 6.2 and the Annex B interaction, and concrete beams can be cross-checked to EN 1992-1-1 with the bars ACI chose. Timber members follow the NDS (ASD under the service combinations), masonry TMS 402 and cold-formed lipped channels AISI S100 with effective widths. Steel beam ends (H shapes and lipped channels, sloping rafters included) get single-plate shear tabs (bolt group and welds by the elastic method, bearing on the thinner of plate and web, plate shear; one bolt or a horizontal pair when the web is too shallow for a row), and steel beams are screened for floor vibration by f = 0.18√(g/Δ) against 3 Hz (AISC Design Guide 11). Members can be tension-only, compression-only or cables, supports can lift off, and beams can rest on a Winkler foundation; such models are solved combination by combination with the engine's contact, cable and Winkler solvers.

The advanced analyses run on the engine of the last analysis (ribbon › Advanced, or the MCP tools `pushover`, `set_moving`, `influence_line`, `set_stages` and `staged`). The pushover adds rigid-plastic hinges event by event, at Mp = Fy·Z for steel and at Mn of the designed bars for concrete, until the structure is a mechanism, either under a combination scaled to collapse or under a lateral pattern (triangular or uniform) lumped at the floors and summing to the seismic weight, so λ = V/W. It runs on a coarse model of its own (beams in two elements, columns in one, no load-break nodes), because hinges form only at element ends. Gravity is not held constant during a lateral push, axial force does not reduce Mp, and a concrete column's Mp counts only the bars of its two extreme faces, so the result is a lower bound. A moving load train runs along a path of members, and its envelope becomes the results `moving max` and `moving min` and the design combinations 1.2D + 1.6M. Influence lines of member forces and reactions move a unit load over the non-vertical members. Construction stages are solved as linear increments on the members built so far, so a member built later starts free of stress; Pondus solves each increment itself because the engine's staged solver leaves the fixed-end forces out of the member forces. The section analysis (ribbon › Section FE, MCP `section_analysis`) gives the area, inertias, plastic moduli, St Venant J, warping Cw and shear centre of any solid outline by finite elements.

The calculation report (ribbon › PDF report, File › Calculation report, MCP `write_report`) is an A4 PDF written next to the model file. It holds the summary, the basis of design, the materials, sections and supports, each load case with its reaction balance, the combinations, the analysis extremes with figures, the earthquake tables, every design table with failing rows in red, the full calculation of the governing member of each type (or of every member) and the changes and warnings. Models move in and out as Excel workbooks (an exact round trip, with result and design sheets), as DXF R12 drawings (members on layers by kind and section, read back by layer name) and as IFC4 (IfcColumn, IfcBeam, IfcMember and IfcSlab with extruded profiles and the design ratio; IFC2x3 and IFC4 from Revit or Aedifex are read, with member ends snapped onto the axes they meet). The Modify group and the MCP tools `transform`, `split_member` and `merge_members` move, copy (arrays), rotate, mirror, split and merge members, carrying their loads, supports and slabs.

With `resize`, failing concrete beams are deepened in 50 mm steps and failing slabs are thickened, and with `select`, each steel member or group gets the lightest passing shape. The model is analysed again after every change until no section changes, for at most six rounds.

</details>

<details>
<summary><b>Speed and the NVIDIA GPU</b></summary>

Pondus factorises a large stiffness matrix on an NVIDIA GPU when the computer has one, using cuDSS, NVIDIA's sparse direct solver, and on the CPU otherwise. The CUDA runtime and cuDSS are loaded when the program starts rather than linked in, so the same binary runs everywhere, and on macOS, or on a machine without the libraries, everything stays on the CPU with unchanged results. The GPU takes systems of 20,000 unknowns or more (`PONDUS_GPU_MIN`), because smaller ones are faster on the CPU than the copy to the card, and it serves the linear static solve, the solve with rigid diaphragms and the shift-invert step of the modal and buckling analyses. `PONDUS_GPU=0` switches it off, `pondus --gpu` reports whether it is used, the status line and the MCP `analyze` result count the factorisations done on the card, and `pondus bench model.pondus` (or `cargo run --release -p pondus-core --example bench -- model.pondus` on a headless server) prints the time with the periods, base shears, drifts and reactions for comparing the two.

To use it, install the CUDA 12 runtime and cuDSS 0.8 or later, for example with `pip install nvidia-cudss-cu12`, and put their library folders on `LD_LIBRARY_PATH` (Linux) or `PATH` (Windows), or name the files with `PONDUS_CUDART` and `PONDUS_CUDSS`. cuDSS runs in its deterministic mode, so a model gives identical results on every run.

The GPU was tested on an NVIDIA A100 (40 GB) with a 16-core CPU. With every factorisation forced onto the card (`PONDUS_GPU_MIN=0`) the whole test suite passes, and the GPU agrees with the CPU Cholesky to 1e-8 or better on the calibration frames and the bridge. On models with shear walls the two differ by up to 2e-5, and on the Burj Khalifa example by 8e-4 in the second period, because the rigid wall arms (1000 times the wall's modulus) make the stiffness matrix ill-conditioned, so any two direct solvers round differently there. On the wall calibration cases the GPU result is the closer one to the closed-form reference in 23 of 33 (`core/tests/gpu.rs` checks the GPU against the CPU where a GPU is present).

| Model | CPU before | CPU now | GPU now |
|---|---|---|---|
| Burj Khalifa example: 41,056 elements, 216,588 unknowns, static and 60-mode response spectrum | 290 s | 60 s | 54 s |
| 12 × 12-bay, 20-storey frame without diaphragms: 56,680 elements, static | – | 30 s | 14.5 s |

Most of the speed came from the CPU rather than the card. Profiling the tower showed that the factorisations took about 2 s of the original 290 s, whereas the Lanczos reorthogonalisation ran on one core (71 s), the Ritz vectors were assembled with a cache-hostile loop order (71 s) and the response spectrum recovered every mode's member forces once per direction, in sequence (58 s). The reorthogonalisation now uses classical Gram–Schmidt applied twice in parallel, the Ritz vectors are summed by blocks of rows, and each mode's forces are found once in parallel and scaled by Γ·Sd for each direction, which leaves the results unchanged to 6e-11 and speeds up every computer, with or without a GPU. On the tower the GPU now saves only about 10 %, because the remaining time is in assembly and the vector work of the modal analysis, which still runs on the CPU, whereas a large static model is about twice as fast on the card.

</details>

<details>
<summary><b>Limits</b></summary>

These limits matter for the results:

- **Method 2 table, confirmed against a scan of ACI 318-63.** All 91 coefficients of Method 2 Table I (five cases, three moments, six values of m and the long span) equal the 1963 table as reproduced in H. K. Hussain's University of Basrah notes, and two of their worked examples are reproduced to 0.1 % (`core/tests/calib_concrete.rs`, `calibration/NOTES_concrete.md`). EIT 1008 itself was not available for comparison. The column strips are given the middle-strip bars, which is conservative.
- **Column design** assumes braced (nonsway) frames unless P-Delta is on. Spread footings under earthquake (the response envelope has no signed reactions) are not designed; a mat or pile cap is a plate on its supports (`add_mat`) with linear soil springs (no lift-off, no consolidation) and its punching checked, but without shear reinforcement.
- **No wind load generator.** W cases can be added by hand; earthquake loads are generated (see above).
- **Rectangular sections only for concrete beams,** so T-beam action is ignored, which is conservative for sagging moments.
- **Steel is H shapes and cold-formed lipped channels.** Hollow sections are analysed but not designed. With `select`, every steel member gets the lightest catalogue section that passes from `steel_shapes` (a name the catalogue does not know is reported; the lipped channels are the twelve JIS SSC400 sizes from C 75x45x15x2.3 to C 250x75x25x4.5); the pieces of one BIM member (the same `bim_id`, a rafter split at its purlins) take one section, the heaviest any piece needs, and a member no listed section carries goes to the heaviest listed.
- **Large frames.** A BIM roof of a thousand cold-formed pieces solves in seconds: members shorter than 4 m get a station every 0.5 m instead of 8 elements, the lipped-channel section FE is cached per size, and when the sparse Cholesky fails the dense fallback is refused above 12 000 free degrees of freedom (the error names the likely mechanism) instead of filling memory.
- **Pushover.** Rigid-plastic hinges without P–M interaction, gravity not held during a lateral push, and beam Mp from the larger of the top and bottom steel for both signs, so treat a building's pushover as indicative.
- **Interchange.** DXF and IFC carry geometry and sections only: supports and loads are added after an import. IFC import reads extruded bodies and analytical members, not breps (footings are skipped with a warning). Aedifex has no IFC reader, so designs go back to Aedifex through `export_bim`.
- **Section orientation.** A column with roll 0 has its strong axis bending in the x–z plane, and `add_frame` turns the columns' strong axis into the direction of the longer spans.

</details>

<details>
<summary><b>Drawing by hand: all the controls</b></summary>

The structure can be built without the agent. The ribbon's Draw group and the palette down the left of the view hold the creation tools: Node, Beam, Column, Brace, Member, Bay and Slab, the fixed, pinned and roller supports, Load, Hinge and Delete. A click arms a tool, and an item dragged from the palette onto the model is placed where it is dropped: a column on a point, a support on a node, a load on a member, or a beam or bay of the size set in the tool options.

With a drawing tool armed, a left drag draws instead of orbiting the view (Shift-drag still pans and right-drag orbits). A beam, brace or member is dragged from its start to its end, or clicked at both ends. A bay is dragged as a rectangle on a floor and gets four edge beams, a slab and, if chosen, columns under its corners. A column is placed with a click, or with a rectangle drag that puts one at each corner. Points snap to the grid (0.5 m by default) and to nodes. They lie on the work plane: the floor chosen in the plan view, the frame line in an elevation, or the plane height in 3D. With the Select tool, dragging a node moves it, and a node dropped on another node merges with it. Dragging a member moves it, and Ctrl or Cmd-dragging copies it. A member that would lie on an existing one is not added twice. Every change is one undo step, and the agent sees the same model.

</details>

<details>
<summary><b>Using Pondus from Claude Desktop, ChatGPT and other agents (MCP)</b></summary>

Pondus runs as an MCP server (`pondus --mcp`), so any agent that speaks the Model Context Protocol can build, analyse and design structures with it. Claude Desktop and Claude Code start it directly from a few lines of configuration, ChatGPT reaches it through an HTTP bridge and a tunnel, and every change is saved to a `.pondus` file that you then open in the app to check. The steps for each are in **[docs/mcp.md](docs/mcp.md)**. For Claude Desktop on macOS the whole configuration is:

```json
{
  "mcpServers": {
    "pondus": {
      "command": "/Applications/Pondus.app/Contents/MacOS/pondus",
      "args": ["--mcp", "--autosave", "/Users/you/Pondus/model.pondus"]
    }
  }
}
```

</details>

<details>
<summary><b>The link to Aedifex (BIM)</b></summary>

Aedifex's `export_stabileo` writes Stabileo's fixture JSON, which Pondus reads with `import_stabileo` (or File › Import). Collinear pieces of one beam are merged back into one design member, and an element's `bimId` (or `name`) is kept. `export_bim` returns the design per element: its `bim_id`, its ends, the Aedifex family and type (for example `Concrete Rectangular Beam` / `300 x 600 mm`, or `Steel I Beam (W)` / `H 300x150x6.5x9`) with the type parameters in metres, the reinforcement as text, and the ratio. An agent applies it in Aedifex with `duplicate_type` and `change_type`. Aedifex does not yet write `bimId` on its exported elements, so until it does, the calling agent matches members by their `start` and `end` points.

</details>

<details>
<summary><b>Build from source and workspace</b></summary>

The workspace is self-contained, because the Stabileo engine and the TigrimOS agent harness are vendored inside it, so one command builds everything:

```sh
cargo build --release          # target/release/pondus and target/release/tigrim (12 min cold)
target/release/pondus          # the app
target/release/pondus --mcp [--open model.pondus] [--autosave model.pondus]
target/release/pondus design in.(pondus|json) out-bim.json
target/release/pondus bench model.pondus     # analysis time, CPU or GPU, periods and reactions
target/release/pondus --gpu  # whether an NVIDIA GPU (CUDA, cuDSS) is used
target/release/pondus --about  # the licence notice and the source link
```

The chat needs an OpenAI-compatible provider (OpenRouter by default), whose URL, model and key go under Agent › Settings. The key is stored in `~/Library/Application Support/Pondus/tigrimos_api_key`, and `tigrim` is found next to the `pondus` binary.

| Crate | What it holds |
|---|---|
| `engine/` | The Stabileo engine (`dedaliano-engine`), copied from `lambdaclass/stabileo` commit `7985e8c` with its manifest trimmed to a native library and its source changes marked `[Pondus]`. The first fixes the sign of the w–θy coupling of the vertical Winkler foundation in `solver/winkler.rs`, without which a uniformly loaded beam on a uniform foundation settled unevenly. The second, in `solver/plastic_3d.rs`, tries a sparse Cholesky factorisation before the dense rank test that checks for a mechanism at every hinge event; it gives the same answer and cut the test building's pushover from 28 s to 3.7 s. The third, in `solver/moving_loads.rs`, keeps the moving-load envelope at each end of an element as well as over the element, so that an envelope read at a point does not carry the value one element away. The rest add the GPU factorisation (`linalg/gpu.rs`, used from `solver/linear.rs`, `solver/constraints.rs` and `linalg/lanczos.rs`) and make the Lanczos reorthogonalisation and Ritz vectors parallel, with rayon now a plain dependency (AGPL-3.0). |
| `core/` | The model, the analysis wrapper, the design codes, the advanced analyses, the scene for the views, the PDF report, the Stabileo, BIM, Excel, DXF and IFC exchange, the editing transforms and the MCP server. |
| `app/` | The egui window in PhotoCraft's Pro theme through `craft-ui` (storytold/craft-libs), with the chat, the 3D view, diagrams and the design table. |
| `tigrimos/` | The TigrimOS agent harness vendored from TigrimOSR, plus the MCP-timeout and vision patches that Aedifex also carries. |

</details>

<details>
<summary><b>Licence and source</b></summary>

Pondus is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License, version 3 only (AGPL-3.0-only), whose full text is in [LICENSE](LICENSE). It is distributed in the hope that it will be useful, but without any warranty, without even the implied warranty of merchantability or fitness for a particular purpose. Structural results produced by Pondus must be checked by a qualified engineer before they are used for design or construction.

The complete source of every version is at **https://github.com/Sompote/pondus**. The program shows the licence and this link in Help › About, in `pondus --about`, in the `serverInfo` and instructions of its MCP server and in the footer of every calculation report, as AGPL-3.0 section 13 asks of a program that users may reach over a network.

The whole program is AGPL-3.0 because it compiles the AGPL-3.0 Stabileo engine (`engine/`, © the Stabileo authors and LambdaClass, [lambdaclass/stabileo](https://github.com/lambdaclass/stabileo) at commit `7985e8c`) into the same binary as its own code. The code in `core/` and `app/` was written for Pondus and contains no Stabileo code, and every Pondus source file carries an `SPDX-License-Identifier: AGPL-3.0-only` header. The three changes to the engine are marked `[Pondus]` in the source and listed in [NOTICE.md](NOTICE.md) with their dates, as section 5(a) asks. TigrimOS (`tigrimos/`) is Apache-2.0, craft-ui and the Rust crates are MIT or Apache-2.0, the Lucide icons are ISC and the fonts are SIL OFL 1.1, all of which can be distributed under AGPL-3.0. [NOTICE.md](NOTICE.md) lists each component with its copyright and licence.

In practice, using Pondus yourself or inside an organisation creates no obligation. If you distribute Pondus, or a modified version, or let other people use a modified version over a network (for example as an MCP server that another agent calls), you must offer them its complete source under AGPL-3.0, keep these notices, and mark your changes. Programs that only exchange data with Pondus through MCP, JSON, Excel, DXF or IFC, such as Aedifex, stay under their own licences, and the models, results, drawings and reports that Pondus produces belong to their user.

</details>

**Licence:** [GNU AGPL-3.0-only](LICENSE) · **Source:** https://github.com/Sompote/pondus · Copyright © 2026 Sompote Youwai · includes the [Stabileo](https://github.com/lambdaclass/stabileo) engine (AGPL-3.0), see [NOTICE.md](NOTICE.md)
