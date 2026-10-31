//! A flat plate: a floor slab on columns with no beams, designed as plate finite elements
//! (its own `plate` flag, the rest of the model on the coefficient method), with column and
//! middle strips and punching with the unbalanced moments (ACI 318-19 8.4.4.2).

use pondus_core::analysis::analyze;
use pondus_core::design;
use pondus_core::model::*;

/// 3 x 3 columns at 6 m, 400 x 400, from the ground (fixed) to the floor at 3.2 m and on to a
/// roof at 6.4 m; a 220 mm flat plate on them, its edges at the columns' outer faces; and the
/// floors above as 500 kN (dead) on top of each interior upper column.
fn building() -> Model {
    let mut m = Model::new("flat plate");
    m.materials[0].e = 25_000.0;
    m.materials[0].nu = 0.2;
    let col = m.rect_section(0.4, 0.4);
    let add = |m: &mut Model, p: [f64; 3], q: [f64; 3]| {
        let (a, b) = (m.node_at(p), m.node_at(q));
        let id = m.next_member_id();
        m.members.push(Member { id, i: a, j: b, section: col, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    };
    for x in [0.0, 6.0, 12.0] {
        for y in [0.0, 6.0, 12.0] {
            add(&mut m, [x, y, 0.0], [x, y, 3.2]);
            add(&mut m, [x, y, 3.2], [x, y, 6.4]);
        }
    }
    for n in m.nodes.clone().iter().filter(|n| n.z == 0.0) {
        m.supports.push(Support { node: n.id, fix: [true; 6], springs: [0.0; 6], uplift: false });
    }
    // the floors above, on the centre column
    let top = m.find_node([6.0, 6.0, 6.4]).unwrap();
    m.loads.push(Load { case: 1, kind: LoadKind::Nodal { node: top, f: [0.0, 0.0, -500.0, 0.0, 0.0, 0.0] } });
    m.slabs.push(Slab { id: 1, name: "FP".into(), x0: -0.2, y0: -0.2, x1: 12.2, y1: 12.2, z: 3.2, h: 0.22, dead: 1.5, live: 2.0, bim_id: None, plate: true, ..Default::default() });
    m.design.slab_mesh = 0.5;
    m
}

#[test]
fn flat_plate_on_columns() {
    let mut m = building();
    assert_eq!(m.design.slab_method, pondus_core::slabfe::SlabMethod::Coefficients, "the model keeps the coefficient method");
    let a = analyze(&m).unwrap();
    // balance under D: the plate (24 x 0.22 + 1.5 kPa on 12.4²), the columns and the 500 kN
    let d = a.case(1).unwrap();
    let plate = (24.0 * 0.22 + 1.5) * 12.4 * 12.4;
    let cols = 18.0 * 3.2 * 0.16 * m.materials[0].gamma;
    assert!((d.reaction_sum()[2] - (plate + cols + 500.0)).abs() < 1.0, "{:?} vs {}", d.reaction_sum(), plate + cols + 500.0);
    let (rep, _) = design::run(&mut m, &design::Options::default()).unwrap();
    let sd = rep.slabs.iter().find(|x| x.slab == 1).expect("the flat plate is designed");
    eprintln!("{} · ratio {:.2} ({}) · h_min {:.0}", sd.method, sd.ratio, sd.governs, sd.h_min);
    for st in &sd.strips {
        eprintln!("  {} {}: Mu {:.1} kN·m/m, {} ({:.2})", st.direction, st.location, st.mu, st.bars, st.ratio);
    }
    for n in &sd.notes {
        eprintln!("  {n}");
    }
    assert!(sd.method.starts_with("flat plate FE"), "{}", sd.method);
    for dir in ["x", "y"] {
        for loc in ["column strip (top)", "column strip (bottom)", "middle strip (top)", "middle strip (bottom)"] {
            assert!(sd.strips.iter().any(|s| s.direction == dir && s.location == loc), "{dir} {loc}");
        }
        let get = |loc: &str| sd.strips.iter().find(|s| s.direction == dir && s.location == loc).unwrap().mu;
        // the hogging gathers in the column strips over the columns: the column strip's top
        // moment is well over the middle strip's
        assert!(get("column strip (top)") > 1.5 * get("middle strip (top)"), "{dir}: {} vs {}", get("column strip (top)"), get("middle strip (top)"));
        assert!(sd.strips.iter().any(|s| s.direction == dir && s.location.contains("8.4.2.2.3")), "{dir}: the moment transfer band");
    }
    // nine punching checks; the interior column's shear is the slab's own share (about its
    // 6 x 6 m tributary area under 1.2D + 1.6L, continuity adding some), not the 500 kN of the
    // floors above that the column below also carries
    let punch: Vec<&String> = sd.notes.iter().filter(|n| n.starts_with("punching at column")).collect();
    assert_eq!(punch.len(), 9, "{punch:?}");
    let num_after = |n: &str, key: &str| -> f64 { n.split(key).nth(1).unwrap().trim().split(|c: char| c == ' ' || c == ',').next().unwrap().parse().unwrap() };
    let centre = punch.iter().find(|n| n.contains("(6.00, 6.00)")).unwrap();
    let wu = 1.2 * (24.0 * 0.22 + 1.5) + 1.6 * 2.0;
    let vu = num_after(centre, "Vu");
    assert!(vu > 0.9 * wu * 36.0 && vu < 1.4 * wu * 36.0, "interior Vu {vu} vs tributary {}", wu * 36.0);
    assert!(centre.contains("4 sides") && centre.contains("αs 40"), "{centre}");
    // an edge column takes a large unbalanced moment from the end span, the interior one little
    let edge = punch.iter().find(|n| n.contains("(0.00, 6.00)")).unwrap();
    assert!(edge.contains("3 sides") && edge.contains("αs 30"), "{edge}");
    let corner = punch.iter().find(|n| n.contains("(0.00, 0.00)")).unwrap();
    assert!(corner.contains("2 sides") && corner.contains("αs 20"), "{corner}");
    let m_edge = num_after(edge, "Msc");
    let m_centre = num_after(centre, "Msc");
    assert!(m_edge > 20.0 && m_edge > 3.0 * m_centre, "edge Msc {m_edge}, interior {m_centre}");
    // the moment raises the edge column's stress over its direct shear Vu / (bo d)
    let bo = num_after(edge, "bo");
    let d = (0.22 - 0.02 - 0.009 - 0.0045 + 0.22 - 0.02 - 0.0045) / 2.0;
    let direct = num_after(edge, "Vu") / (bo * d) / 1000.0;
    let vstress = num_after(edge, "vu");
    assert!(vstress > 1.2 * direct, "edge vu {vstress} MPa vs direct {direct:.3}");
    // Table 8.3.1.1 at fy 390: ln = 5.6 m, exterior ln/30.6 = 183 mm
    assert!((sd.h_min - 5600.0 / (33.0 - 3.0 * 110.0 / 140.0)).abs() < 1.0, "h_min {}", sd.h_min);
    assert!(sd.notes.iter().any(|n| n.contains("8.7.4.2")));
}
