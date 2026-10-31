// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Model checks before analysis: what is missing or suspicious, in words an engineer acts on.

use crate::model::*;

pub fn problems(m: &Model) -> Vec<String> {
    let mut out = vec![];
    // a beam inside a wall doubles the slab load the wall's arms already take
    for w in &m.walls {
        let lw = w.length().max(1e-9);
        let (dx, dy) = (w.b[0] - w.a[0], w.b[1] - w.a[1]);
        let on = |p: [f64; 3]| {
            let s = ((p[0] - w.a[0]) * dx + (p[1] - w.a[1]) * dy) / (lw * lw);
            let off = ((p[0] - w.a[0]) * dy - (p[1] - w.a[1]) * dx).abs() / lw;
            s > -1e-6 && s < 1.0 + 1e-6 && off < 1e-3 && p[2] > w.z0 - 1e-3 && p[2] < w.z1 + 1e-3
        };
        for x in m.members.iter().filter(|x| x.role.is_frame() && m.kind(x) == MemberKind::Beam) {
            if [x.i, x.j].iter().all(|n| m.node(*n).is_some_and(|n| on(n.p()))) {
                out.push(format!("beam {} lies inside wall {}: delete it (the wall carries the slab edge)", x.id, w.name));
            }
        }
    }
    for mm in &m.members {
        if m.node(mm.i).is_none() || m.node(mm.j).is_none() {
            out.push(format!("member {} uses a node that does not exist", mm.id));
        } else if m.length(mm) < 1e-6 {
            out.push(format!("member {} has no length", mm.id));
        }
        if m.section(mm.section).is_none() {
            out.push(format!("member {} has no section (id {})", mm.id, mm.section));
        }
        if m.material(mm.material).is_none() {
            out.push(format!("member {} has no material (id {})", mm.id, mm.material));
        }
        if let (Some(s), Some(mt)) = (m.section(mm.section), m.material(mm.material)) {
            match (&s.shape, mt.kind) {
                (Shape::H { .. }, MaterialKind::Concrete) => out.push(format!("member {} is concrete with the steel shape {}", mm.id, s.name)),
                (Shape::Rect { .. }, MaterialKind::Steel) => out.push(format!("member {} is steel with the rectangle {} (solid steel?)", mm.id, s.name)),
                _ => {}
            }
        }
    }
    for (k, a) in m.members.iter().enumerate() {
        for b in &m.members[k + 1..] {
            if (a.i == b.i && a.j == b.j) || (a.i == b.j && a.j == b.i) {
                out.push(format!("members {} and {} lie on each other", a.id, b.id));
            }
        }
    }
    let used: std::collections::HashSet<Id> = m.members.iter().flat_map(|x| [x.i, x.j]).collect();
    for s in &m.supports {
        if used.contains(&s.node) {
            continue;
        }
        // a support under a plate slab (a pile under a cap, a pedestal under a mat) holds the slab
        let on_slab = m.node(s.node).and_then(|n| m.slabs.iter().find(|sl| sl.contains(n.x, n.y, n.z)));
        match on_slab {
            Some(sl) if m.plate_slab(sl) => {}
            Some(sl) => out.push(format!("the support at node {} lies in slab {} but holds no member: set_design slab_method 'plate' so the slab rests on it", s.node, sl.id)),
            None => out.push(format!("the support at node {} holds no member", s.node)),
        }
    }
    for c in &m.combos {
        for (id, _) in &c.factors {
            if m.case(*id).is_none() {
                out.push(format!("combination '{}' uses case {id}, which does not exist", c.name));
            }
        }
    }
    if !m.combos.iter().any(|c| c.kind == ComboKind::Uls) {
        out.push("no strength (ULS) combination: nothing can be designed".into());
    }
    for s in &m.slabs {
        if s.lx() < 0.1 || s.ly() < 0.1 {
            out.push(format!("slab {} is too small ({:.2} x {:.2} m)", s.id, s.lx(), s.ly()));
        }
    }
    out
}
