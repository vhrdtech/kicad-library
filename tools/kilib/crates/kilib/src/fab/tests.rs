use super::*;

/// A footprint with a fab body `w` x `h` (stroke 0.1), the given pads (x, y, w, h), the Reference on
/// `ref_layer`, `${REFERENCE}` user texts on `user_layers` and `extra` fab drawing.
pub(crate) fn footprint(
    w: f64,
    h: f64,
    pads: &[(f64, f64, f64, f64)],
    ref_layer: &str,
    user_layers: &[&str],
) -> String {
    with_drawing(w, h, pads, ref_layer, user_layers, "")
}

fn with_drawing(
    w: f64,
    h: f64,
    pads: &[(f64, f64, f64, f64)],
    ref_layer: &str,
    user_layers: &[&str],
    extra: &str,
) -> String {
    let mut s = String::from("(footprint \"T\"\n\t(version 20260206)\n\t(layer \"F.Cu\")\n");
    s += &format!(
        "\t(property \"Reference\" \"REF**\"\n\t\t(at 0 -2 0)\n\t\t(layer \"{ref_layer}\")\n\t\t(uuid \"u-ref\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n"
    );
    s += "\t(property \"Value\" \"T\"\n\t\t(at 0 2 0)\n\t\t(layer \"F.Fab\")\n\t\t(uuid \"u-val\")\n\t)\n";
    for l in user_layers {
        s += &format!(
            "\t(fp_text user \"${{REFERENCE}}\"\n\t\t(at 0 0 0)\n\t\t(layer \"{l}\")\n\t\t(uuid \"u-{l}\")\n\t)\n"
        );
    }
    s += &format!(
        "\t(fp_rect\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n",
        -w / 2.0,
        -h / 2.0,
        w / 2.0,
        h / 2.0
    );
    s += extra;
    for (n, (x, y, pw, ph)) in pads.iter().enumerate() {
        s += &format!(
            "\t(pad \"{}\" smd rect\n\t\t(at {x} {y})\n\t\t(size {pw} {ph})\n\t\t(layers \"F.Cu\")\n\t)\n",
            n + 1
        );
    }
    s + ")\n"
}

#[test]
fn a_reference_layer_drawing_is_the_body() {
    let circle = |r: f64| {
        format!(
            "\t(fp_circle\n\t\t(center 0.5 0)\n\t\t(end {} 0)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"User.9\")\n\t)\n",
            0.5 + r
        )
    };
    // A large test point circle beside the fab outline: the designator sits in the circle, the fab drawing ignored.
    let text = with_drawing(1.0, 1.0, &[(0.5, 0.0, 1.0, 1.0)], "User.9", &[], &circle(2.0));
    let Outcome::Change { placement: p, new, body_from, .. } = fix_as(&text, "VHRD_TestPoint:TestPoint_X") else {
        panic!()
    };
    assert_eq!(body_from, Some(BodyFrom::RefLayer));
    let fp = read_as(&new, "VHRD_TestPoint:TestPoint_X").unwrap();
    let shape = fp.shape.unwrap();
    let (cx, _) = shape.text_rect(p.x, p.y, p.angle, p.size).center();
    assert!(p.clean && (cx - 0.5).abs() < 2e-3 && p.size > 0.5, "{p:?}");
    assert_eq!(fix_as(&new, "VHRD_TestPoint:TestPoint_X"), Outcome::Fine);
    // A small one holds no text: the designator goes beside it, clear of the circle.
    let text = with_drawing(0.5, 0.5, &[(0.5, 0.0, 0.5, 0.5)], "User.9", &[], &circle(0.3));
    let Outcome::Change { placement: p, new, .. } = fix_as(&text, "VHRD_TestPoint:TestPoint_X") else { panic!() };
    let shape = read_as(&new, "VHRD_TestPoint:TestPoint_X").unwrap().shape.unwrap();
    let r = shape.text_rect(p.x, p.y, p.angle, p.size);
    let b = shape.body;
    assert!(p.clean && OUTSIDE_SIZES.contains(&p.size), "{p:?}");
    assert!(r.x1 <= b.x0 + 1e-9 || r.x0 >= b.x1 - 1e-9 || r.y1 <= b.y0 + 1e-9 || r.y0 >= b.y1 - 1e-9, "{r:?} {b:?}");
    assert_eq!(fix_as(&new, "VHRD_TestPoint:TestPoint_X"), Outcome::Fine);
}

fn line(a: (f64, f64), b: (f64, f64)) -> String {
    format!(
        "\t(fp_line\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.12)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n",
        a.0, a.1, b.0, b.1
    )
}

/// The ink's middle sits on the body's centre.
fn centred(new: &str, p: &Placement) {
    let shape = read(new).unwrap().shape.unwrap();
    let (cx, cy) = shape.text_rect(p.x, p.y, p.angle, p.size).center();
    let (bx, by) = shape.centre();
    assert!((cx - bx).abs() < 1.1e-3 && (cy - by).abs() < 1.1e-3, "{p:?}: ink at ({cx}, {cy})");
}

fn changed(text: &str) -> (Placement, String) {
    match fix(text) {
        Outcome::Change { placement, new, .. } => (placement, new),
        o => panic!("expected a change, got {o:?}"),
    }
}

/// The rewritten footprint follows every rule, a second run changes nothing, and the text is clear of the
/// drawing when the placement is clean.
fn settled(new: &str, p: &Placement) -> Footprint {
    let fp = read(new).unwrap();
    assert_eq!(check(&fp), Vec::<String>::new(), "{new}");
    assert_eq!(fix(new), Outcome::Fine, "a second run changes nothing");
    let shape = fp.shape.as_ref().unwrap();
    if p.clean {
        let r = shape.text_rect(p.x, p.y, p.angle, p.size);
        assert!(shape.region.contains(&r), "{p:?} leaves {:?}", shape.region);
        assert!(!shape.graphics.iter().any(|g| g.hits(&r)), "{p:?} touches the drawing");
    }
    fp
}

#[test]
fn soic_like_gets_1mm_turned_on_the_centre_with_silk_texts_gone() {
    // A 3.9 x 4.9 body, pads outside it left and right: 1 mm only fits along the 4.9 mm side.
    let pads: Vec<_> = (0..4)
        .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
        .collect();
    let text = footprint(3.9, 4.9, &pads, "F.SilkS", &["F.SilkS", "F.Fab"]);
    let (p, new) = changed(&text);
    assert_eq!((p.angle, p.size, p.clean), (270.0, 1.0, true), "{p:?}");
    centred(&new, &p);
    assert!(!new.contains("${REFERENCE}"), "{new}");
    assert!(new.contains("(layer \"User.23\")\n\t\t(uuid \"u-ref\")"), "{new}");
    assert!(new.contains("(property \"Value\" \"T\"\n\t\t(at "), "{new}");
    assert!(new.contains("(unlocked yes)\n\t\t(layer \"User.17\")\n\t\t(uuid \"u-val\")"), "{new}");
    assert!(new.contains("(width 0.05)") && !new.contains("(width 0.1)"), "{new}");
    settled(&new, &p);
}

#[test]
fn pads_do_not_push_the_text_off_the_centre() {
    // 1206: the body lies within the pads; the text sits on its centre over them, level, 0.65 mm.
    let pads = [(-1.475, 0.0, 1.05, 1.75), (1.475, 0.0, 1.05, 1.75)];
    let (p, new) = changed(&footprint(3.2, 1.6, &pads, "F.SilkS", &["F.Fab"]));
    assert_eq!((p.angle, p.size, p.clean), (0.0, 0.65, true), "{p:?}");
    centred(&new, &p);
    settled(&new, &p);
}

#[test]
fn a_mark_on_the_centre_moves_the_text_to_the_largest_free_spot() {
    // An 8 x 4 body with a diode bar across its middle: the text goes beside it, as large as it gets.
    let bar = line((0.0, -1.0), (0.0, 1.0));
    let (p, new) = changed(&with_drawing(8.0, 4.0, &[], "F.Fab", &[], &bar));
    assert!(p.clean && p.x != 0.0, "{p:?}");
    assert!(p.size >= 0.75, "{p:?}");
    let fp = settled(&new, &p);
    // Every line on the fab layer is 0.05 mm now; the obstacle was measured that way already.
    assert!(fp.fab_widths.is_empty());
    // Nothing larger fits anywhere, and nothing as large nearer the centre.
    let shape = fp.shape.unwrap();
    let bigger = p.size + SIZE_STEP;
    assert!(
        grid(shape.region, shape.centre())
            .iter()
            .all(|&(x, y)| !shape.fits(x, y, 0.0, bigger) && !shape.fits(x, y, 90.0, bigger))
    );
}

#[test]
fn a_mark_shrinks_or_turns_the_text_before_it_moves() {
    // A band line near the left end of a 6 x 3 body leaves the centre free at a smaller size.
    let mark = line((-2.0, -1.5), (-2.0, 1.5));
    let (p, new) = changed(&with_drawing(6.0, 3.0, &[], "F.Fab", &[], &mark));
    assert_eq!((p.angle, p.size, p.clean), (0.0, 0.8, true), "{p:?}");
    centred(&with_drawing(6.0, 3.0, &[], "F.Fab", &[], &mark), &p);
    settled(&new, &p);
    // A tall body turns it.
    let (p, new) = changed(&footprint(1.4, 4.0, &[], "F.Fab", &[]));
    assert_eq!(p.angle, 270.0, "{p:?}");
    centred(&new, &p);
    settled(&new, &p);
}

#[test]
fn circles_arcs_and_filled_polygons_count() {
    let circle = "\t(fp_circle\n\t\t(center 0 0)\n\t\t(end 0.5 0)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill yes)\n\t\t(layer \"F.Fab\")\n\t)\n";
    let fp = read(&with_drawing(6.0, 6.0, &[], "F.Fab", &[], circle)).unwrap();
    let shape = fp.shape.unwrap();
    assert!(!shape.fits(0.0, 0.0, 0.0, MIN_SIZE), "a filled dot on the centre");
    let p = layout(&shape);
    assert!(p.clean && (p.x, p.y) != (0.0, 0.0), "{p:?}");

    let arc = "\t(fp_arc\n\t\t(start -1 0)\n\t\t(mid 0 -1)\n\t\t(end 1 0)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n";
    let shape = read(&with_drawing(6.0, 6.0, &[], "F.Fab", &[], arc)).unwrap().shape.unwrap();
    // The arc's top at (0, -1): text there is hit, the empty lower half of its circle is free.
    assert!(!shape.fits(0.0, -1.0, 0.0, MIN_SIZE));
    assert!(shape.fits(0.0, 0.5, 0.0, MIN_SIZE));

    let tri = "\t(fp_poly\n\t\t(pts\n\t\t\t(xy -1 -1) (xy 1 -1) (xy 0 1)\n\t\t)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill yes)\n\t\t(layer \"F.Fab\")\n\t)\n";
    let shape = read(&with_drawing(6.0, 6.0, &[], "F.Fab", &[], tri)).unwrap().shape.unwrap();
    assert!(Prim::Area(vec![(-1.0, -1.0), (1.0, -1.0), (0.0, 1.0)]).hits(&Rect::around(0.0, -0.2, 0.05, 0.05)));
    assert!(!shape.fits(0.0, -0.3, 0.0, MIN_SIZE), "inside a filled triangle");
}

#[test]
fn small_bodies_get_small_text_and_too_small_ones_stay_centred_not_clean() {
    // 0402 (1 x 0.5): R1004 fits inside the outline at 0.19, below the usual floor.
    let pads = [(-0.48, 0.0, 0.56, 0.62), (0.48, 0.0, 0.56, 0.62)];
    let r0402 = footprint(1.0, 0.5, &pads, "F.SilkS", &["F.Fab"]).replace("\"T\"", "\"R_0402\"");
    let (p, new) = changed(&r0402);
    assert_eq!((p.angle, p.size, p.clean, p.tiny()), (0.0, 0.19, true, true), "{p:?}");
    centred(&new, &p);
    settled(&new, &p);
    assert!(new.contains("(size 0.19 0.19)\n\t\t\t\t(thickness 0.028)"), "{new}");
    // A 0.6 x 0.3 body (0201): nothing fits even at the tiny floor.
    let (p, new) = changed(&footprint(0.6, 0.3, &[], "F.SilkS", &[]));
    assert_eq!((p.angle, p.size, p.clean), (0.0, MIN_SIZE, false), "{p:?}");
    centred(&new, &p);
    settled(&new, &p);
}

#[test]
fn a_layout_note_fits_much_smaller_than_a_designator_would() {
    // The same 0.6 x 0.3 body, no courtyard drawn: the note falls back to the centre, like a designator.
    let shape = read(&footprint(0.6, 0.3, &[], "F.SilkS", &[])).unwrap().shape.unwrap();
    assert!(shape.courtyard.is_none());
    let designator = layout(&shape);
    assert_eq!((designator.size, designator.clean), (MIN_SIZE, false), "{designator:?}");
    // ...but a short note reads fine well below that floor, so it gets a clean fit instead.
    let (note, _, edge) = layout_note(&shape, &["FB U301".to_string()], Edge::Top, false, &[]);
    assert!(note.clean && note.size < TINY_SIZE && edge.is_none(), "{note:?} {edge:?}");
    assert!(note.size >= NOTE_TINY_SIZE - EPS, "{note:?}");
    // A note too long even for the note floor still ends up not clean, like a designator.
    let (huge, _, _) = layout_note(
        &shape,
        &["a very much longer note than any body this small could ever hold".to_string()],
        Edge::Top,
        false,
        &[],
    );
    assert_eq!((huge.size, huge.clean), (MIN_SIZE, false), "{huge:?}");
}

/// A courtyard rect round `body` with `margin` on every side, 0.05 mm stroke, on F.CrtYd (B.CrtYd when `back`).
fn crtyd_rect(w: f64, h: f64, margin: f64, back: bool) -> String {
    let layer = if back { "B.CrtYd" } else { "F.CrtYd" };
    format!(
        "\t(fp_rect\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"{layer}\")\n\t)\n",
        -w / 2.0 - margin,
        -h / 2.0 - margin,
        w / 2.0 + margin,
        h / 2.0 + margin
    )
}

#[test]
fn a_layout_note_hugs_the_courtyard_clear_of_the_designator() {
    // A 4 x 6 body (taller than wide: left/right are the longer pair) with a 0.25 mm courtyard margin.
    let extra = crtyd_rect(4.0, 6.0, 0.25, false);
    let text = with_drawing(4.0, 6.0, &[], "F.Fab", &[], &extra);
    let shape = read(&text).unwrap().shape.unwrap();
    let courtyard = shape.courtyard.expect("a courtyard was drawn");
    assert_eq!(note_edges(courtyard), [Edge::Left, Edge::Right]);
    let designator = layout(&shape);
    let designator_ink = shape.text_rect(designator.x, designator.y, designator.angle, designator.size);

    let (critical, crit_ink, edge) =
        layout_note(&shape, &["U101.5 VDD".to_string()], Edge::Left, false, &[designator_ink]);
    assert_eq!(edge, Some(Edge::Left), "{critical:?}");
    assert!(critical.clean, "{critical:?}");
    // Left of the body, inside the courtyard, clear of the designator's ink.
    assert!(crit_ink.x1 <= shape.body.x0 + 1e-9, "{crit_ink:?} body {:?}", shape.body);
    assert!(crit_ink.x0 >= courtyard.x0 - 1e-9, "{crit_ink:?} courtyard {courtyard:?}");
    assert!(!crit_ink.overlaps(&designator_ink), "{crit_ink:?} {designator_ink:?}");

    // Important avoids both the designator and Critical's ink: it lands on the opposite (Right) edge.
    let (important, imp_ink, edge) =
        layout_note(&shape, &["FB U301".to_string()], Edge::Right, true, &[designator_ink, crit_ink]);
    assert_eq!(edge, Some(Edge::Right), "{important:?}");
    assert!(important.clean, "{important:?}");
    assert!(imp_ink.x0 >= shape.body.x1 - 1e-9, "{imp_ink:?} body {:?}", shape.body);
    assert!(!imp_ink.overlaps(&crit_ink) && !imp_ink.overlaps(&designator_ink), "{imp_ink:?} {crit_ink:?}");
}

#[test]
fn a_passives_notes_turn_to_the_ends_where_there_is_more_room() {
    // 0402: a 1.0 x 0.5 body, courtyard 0.43 beyond it at the ends and 0.22 along the long edges.
    let extra = "\t(fp_rect\n\t\t(start -0.93 -0.47)\n\t\t(end 0.93 0.47)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"F.CrtYd\")\n\t)\n";
    let text = with_drawing(1.0, 0.5, &[], "F.Fab", &[], extra);
    let shape = read(&text).unwrap().shape.unwrap();
    let note = ["U1001.48".to_string()];
    let (crit, crit_ink, edge) = layout_note(&shape, &note, Edge::Top, false, &[]);
    assert!(matches!(edge, Some(Edge::Left | Edge::Right)), "{crit:?} {edge:?}");
    assert_eq!(crit.angle, 270.0, "{crit:?}");
    // Along the long edge the same note got only 0.08 mm.
    assert!(crit.size > 0.11, "{crit:?}");
    // Important goes to the opposite end and never touches Critical.
    let (prefer, keep) = note_prefer([Edge::Top, Edge::Bottom], 1, edge);
    let (imp, imp_ink, imp_edge) = layout_note(&shape, &note, prefer, keep, &[crit_ink]);
    assert_eq!(imp_edge, edge.map(opposite), "{imp:?}");
    assert!(!imp_ink.overlaps(&crit_ink), "{imp_ink:?} {crit_ink:?}");
    assert!((imp.size - crit.size).abs() < 1e-9, "{imp:?} {crit:?}");
}

#[test]
fn a_courtyard_with_no_room_beside_the_body_falls_back_to_the_centre() {
    // The courtyard clears the body by less than a floor-sized note needs on any side.
    let extra = crtyd_rect(4.0, 4.0, 0.01, false);
    let text = with_drawing(4.0, 4.0, &[], "F.Fab", &[], &extra);
    let shape = read(&text).unwrap().shape.unwrap();
    assert!(shape.courtyard.is_some(), "a courtyard was drawn, just too tight to use");
    let (p, ink, edge) = layout_note(&shape, &["VDDA33".to_string()], Edge::Top, false, &[]);
    assert!(edge.is_none(), "{p:?}");
    let (cx, cy) = ink.center();
    let (bx, by) = shape.centre();
    assert!((cx - bx).abs() < 1.1e-3 && (cy - by).abs() < 1.1e-3, "{p:?}: ink at ({cx}, {cy})");
}

#[test]
fn the_library_text_is_sized_for_the_longest_designator_of_the_kind() {
    assert_eq!(library_refs(Class::Part, "R_0402_1005Metric"), ["R1004"]);
    assert_eq!(library_refs(Class::Part, "VHRD_Capacitor_SMD:C_0603_1608Metric"), ["C1001"]);
    assert_eq!(library_refs(Class::Test, "TestPoint_Pad_D1.0mm"), ["TP526"]);
    assert_eq!(library_refs(Class::Tie, "SPARK_GAP_011"), ["JP104", "NT104"]);
    assert_eq!(library_refs(Class::Part, "SOIC-8_3.9x4.9mm_P1.27mm"), ["U1004", "Q1004"]);
}

#[test]
fn test_points_net_ties_and_mechanical_parts_get_their_own_layers() {
    // A test point: its designator on Ref.Test (User.9), fitted like any other.
    let (p, new) = changed(&footprint(3.0, 3.0, &[], "F.SilkS", &["F.Fab"]).replace("\"T\"", "\"TestPoint_X\""));
    assert!(new.contains("(unlocked yes)\n\t\t(layer \"User.9\")") && !new.contains("${REFERENCE}"), "{new}");
    assert!(p.clean && p.size > 0.5, "{p:?}");
    assert_eq!(fix_as(&new, "VHRD_TestPoint:TestPoint_X"), Outcome::Fine);
    // A net tie of the VHRD_NetTie library, named otherwise: on Ref.Tie (User.11), the fab drawing in the way.
    let text = with_drawing(3.0, 1.0, &[], "F.SilkS", &[], &line((0.0, -0.5), (0.0, 0.5)));
    let Outcome::Change { placement: p, new, .. } = fix_as(&text, "VHRD_NetTie:T") else { panic!() };
    assert!(new.contains("(layer \"User.11\")"), "{new}");
    let fp = read_as(&new, "VHRD_NetTie:T").unwrap();
    assert!(check(&fp).is_empty(), "{:?}", check(&fp));
    let shape = fp.shape.unwrap();
    assert!(!shape.graphics.iter().any(|g| g.hits(&shape.text_rect(p.x, p.y, p.angle, p.size))));
    // A logo: on Ref.Mech (User.13), hidden, where it was.
    let text = footprint(3.0, 3.0, &[], "F.SilkS", &[]);
    let Outcome::Change { new, .. } = fix_as(&text, "VHRD_Symbol:LOGO") else { panic!() };
    assert!(new.contains("(at 0 -2 0)\n\t\t(unlocked yes)\n\t\t(layer \"User.13\")\n\t\t(hide yes)\n"), "{new}");
    assert_eq!(fix_as(&new, "VHRD_Symbol:LOGO"), Outcome::Fine);
    // A logo drawn on the silkscreen alone has no body: its texts still move, hidden, where they were.
    let bare = format!("{})\n", text.split("\t(fp_rect").next().unwrap());
    let Outcome::Change { new, body_from: None, .. } = fix_as(&bare, "VHRD_Symbol:LOGO") else { panic!() };
    assert!(new.contains("(layer \"User.13\")\n\t\t(hide yes)") && new.contains("(layer \"F.Fab\")\n\t\t(hide yes)"));
    assert_eq!(fix_as(&new, "VHRD_Symbol:LOGO"), Outcome::Fine);
    // On the back, the back layer of the pair.
    let back = footprint(3.0, 3.0, &[], "F.SilkS", &[])
        .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
        .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
    let o = fix_as(&back, "VHRD_TestPoint:T");
    let Outcome::Change { new, .. } = &o else { panic!("{o:?}") };
    assert!(new.contains("(layer \"User.10\")") && new.contains("(justify mirror)"), "{new}");
}

#[test]
fn a_reference_off_the_centre_is_moved_back() {
    let pads = [(-3.0, 0.0, 1.2, 1.2), (3.0, 0.0, 1.2, 1.2)];
    let (p, new) = changed(&footprint(4.0, 2.0, &pads, "F.Fab", &[]));
    let at = format!("(at {} {} {})", fmt(p.x), fmt(p.y), fmt(p.angle));
    assert!(new.contains(&at), "{new}");
    let moved = new.replace(&at, "(at 0.2 0.3 0)");
    let Outcome::Change { why, before, .. } = fix(&moved) else { panic!("{moved}") };
    assert!(why.iter().any(|w| w.contains(&format!("not ({}, {})", fmt(p.x), fmt(p.y)))), "{why:?}");
    assert_eq!((before.x, before.y), (0.2, 0.3));
    let o = fix(&moved);
    assert_eq!(o.designator_changes(), (true, false, false));
}

#[test]
fn a_bottom_footprint_gets_ref_des_b_and_mirrored_text() {
    let text = footprint(4.0, 2.0, &[], "B.SilkS", &[])
        .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
        .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
    let (p, new) = changed(&text);
    assert!(new.contains("(layer \"User.24\")") && new.contains("(justify mirror)"), "{new}");
    settled(&new, &p);
}

#[test]
fn a_pin_1_dot_alone_is_not_the_body() {
    let pads = [(-2.0, 0.0, 1.0, 0.6), (2.0, 0.0, 1.0, 0.6)];
    let text = footprint(0.2, 0.2, &pads, "F.SilkS", &[])
        .replace("(start -0.1 -0.1)\n\t\t(end 0.1 0.1)", "(start -2.6 -0.6)\n\t\t(end -2.4 -0.4)");
    let fp = read(&text).unwrap();
    assert_eq!(fp.shape.as_ref().unwrap().body_from, BodyFrom::Pads);
}

#[test]
fn turned_pads_and_formatting() {
    let n = sexpr::parse("(pad \"1\" smd rect (at 1 2 90) (size 2 0.5))").unwrap();
    let r = pad_rect(&n).unwrap();
    assert!((r.w() - 0.5).abs() < 1e-9 && (r.h() - 2.0).abs() < 1e-9, "{r:?}");
    assert_eq!(
        (fmt(0.30000001), fmt(-0.0), fmt(1.0), fmt(-1.25)),
        ("0.3".into(), "0".into(), "1".into(), "-1.25".into())
    );
    assert_eq!((thickness(1.0), thickness(0.3), thickness(0.22), thickness(0.15)), (0.15, 0.045, 0.033, 0.025));
    let s = sizes();
    assert_eq!((s.len(), s[13], s[14], s[15], s.last()), (25, 0.35, 0.3, 0.29, Some(&MIN_SIZE)));
    assert_eq!(tiny_sizes(TINY_SIZE), vec![0.19, 0.18, 0.17, 0.16, 0.15]);
    assert_eq!(tiny_sizes(NOTE_TINY_SIZE).len(), 15);
    assert_eq!(tiny_sizes(NOTE_TINY_SIZE).last(), Some(&NOTE_TINY_SIZE));
}

#[test]
fn a_real_parts_value_is_centred_on_its_own_layer_and_the_fab_layer_keeps_no_text() {
    // A SOIC-like body: the designator on Ref.Des, the Value on Value, each centred, each clear of the drawing.
    let pads: Vec<_> = (0..4)
        .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
        .collect();
    let (p, new) = changed(&footprint(3.9, 4.9, &pads, "F.Fab", &["F.Fab"]));
    let fp = settled(&new, &p);
    assert_eq!(fp.reference.as_ref().unwrap().layer, "User.23");
    let v = fp.value.as_ref().unwrap();
    assert!(v.layer == "User.17" && !v.hidden && v.unlocked, "{v:?}");
    assert!(!new.contains("(layer \"F.Fab\")\n\t\t(uuid") && !new.contains("${REFERENCE}"), "{new}");
    assert_eq!(fp.values, ["TPS54202"]);
    let shape = fp.shape.as_ref().unwrap();
    let vp = fp.value_placement().unwrap();
    let mut vs = shape.clone();
    vs.texts = fp.values.clone();
    let vr = vs.text_rect(vp.x, vp.y, vp.angle, vp.size);
    let ((cx, cy), (bx, by)) = (vr.center(), shape.centre());
    assert!(vp.clean && (cx - bx).abs() < 1.1e-3 && (cy - by).abs() < 1.1e-3, "{vp:?}");
    assert!(shape.region.contains(&vr) && !shape.graphics.iter().any(|g| g.hits(&vr)));
    // A value longer than the designator gets smaller text, at the same angle on this body.
    assert!(vp.size < p.size && vp.angle == p.angle, "{p:?} {vp:?}");
    // On the back: Ref.Des.B and Value.B, mirrored.
    let back = footprint(4.0, 2.0, &[], "F.Fab", &[])
        .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
        .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
    let (_, new) = changed(&back);
    let fp = read(&new).unwrap();
    let (r, v) = (fp.reference.unwrap(), fp.value.unwrap());
    assert!(r.layer == "User.24" && v.layer == "User.18" && v.justify == ["mirror"], "{v:?}");
    // A logo's and a test point's Value stay hidden on the fab layer, where they were.
    for id in ["VHRD_Symbol:LOGO", "VHRD_TestPoint:TP"] {
        let Outcome::Change { new, .. } = fix_as(&footprint(3.0, 3.0, &[], "F.SilkS", &[]), id) else { panic!() };
        assert!(
            new.contains(
                "(property \"Value\" \"T\"\n\t\t(at 0 2 0)\n\t\t(unlocked yes)\n\t\t(layer \"F.Fab\")\n\t\t(hide yes)"
            ),
            "{new}"
        );
        assert_eq!(fix_as(&new, id), Outcome::Fine);
    }
}

#[test]
fn a_board_fits_each_value_to_its_own_text() {
    let pads: Vec<_> = (0..4)
        .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
        .collect();
    let (_, lib) = changed(&footprint(3.9, 4.9, &pads, "F.Fab", &[]));
    let placed = lib
        .replacen("(footprint \"T\"", "(footprint \"Lib:T\"\n\t(at 10 10 90)", 1)
        .replace("\"REF**\"", "\"U1\"")
        .replace("(property \"Value\" \"T\"", "(property \"Value\" \"TPS54202DDCR\"");
    let board = format!("(kicad_pcb\n{placed})\n");
    let plan = plan_board(&board).unwrap();
    let item = &plan.items[0];
    let (BoardOutcome::Change { placement: p, .. }, BoardOutcome::Change { placement: vp, angle, .. }) =
        (&item.outcome, &item.value)
    else {
        panic!("{item:?}")
    };
    // Both centred on the body, each as large as its own text allows.
    assert!(p.clean && vp.clean && vp.size < p.size, "{p:?} {vp:?}");
    assert!(*angle == 0.0 || *angle == 270.0, "{angle}");
    let again = plan_board(&plan.new).unwrap();
    assert_eq!(again.new, plan.new, "a second run changes nothing");
    assert!(matches!(again.items[0].value, BoardOutcome::Fine(_)));
    // A footprint not updated from the library: its value on F.Fab, hidden, is left alone.
    let old = board.replace("(layer \"User.17\")", "(layer \"F.Fab\")");
    let plan = plan_board(&old).unwrap();
    assert!(matches!(&plan.items[0].value, BoardOutcome::Skipped(w) if w.contains("update the footprint")));
    // A test point's Value is not fitted.
    let (_, tp) = changed(&footprint(3.0, 3.0, &[], "F.Fab", &[]).replace("\"T\"", "\"TestPoint_X\""));
    let tp = tp.replacen("(footprint \"TestPoint_X\"", "(footprint \"VHRD_TestPoint:TestPoint_X\"", 1);
    let plan = plan_board(&format!("(kicad_pcb\n{})\n", tp.replace("\"REF**\"", "\"TP1\""))).unwrap();
    assert!(matches!(&plan.items[0].outcome, BoardOutcome::Change { .. }), "{:?}", plan.items[0]);
    assert!(matches!(&plan.items[0].value, BoardOutcome::Skipped(w) if w.contains("not a real part")));
}

#[test]
fn a_board_moves_a_set_layout_note_kicad_left_on_the_fab_layer() {
    let pads = [(-1.0, 0.0, 0.6, 0.6), (1.0, 0.0, 0.6, 0.6)];
    let (_, lib) = changed(&footprint(3.0, 1.6, &pads, "F.Fab", &[]));
    // As Update PCB from Schematic adds a field the footprint lacks: hidden on F.Fab at the origin.
    let note = "\t(property \"Layout_Critical\" \"U1.5\"\n\t\t(at 10 10 90)\n\t\t(layer \"F.Fab\")\n\t\t(hide yes)\n\t\t(uuid \"u-crit\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1.27 1.27)\n\t\t\t)\n\t\t)\n\t)\n";
    let placed = lib
        .replacen("(footprint \"T\"", "(footprint \"Lib:T\"\n\t(at 10 10 90)", 1)
        .replace("\"REF**\"", "\"C1\"")
        .replacen("\t(fp_rect", &format!("{note}\t(fp_rect"), 1);
    let board = format!("(kicad_pcb\n{placed})\n");
    let plan = plan_board(&board).unwrap();
    let [(name, BoardOutcome::Change { placement: p, angle, .. })] = plan.items[0].notes.as_slice() else {
        panic!("{:?}", plan.items[0])
    };
    assert_eq!((*name, *angle), ("Layout_Critical", 270.0));
    assert!(
        plan.new.contains(&format!(
            "(property \"Layout_Critical\" \"U1.5\"\n\t\t(at {} {} 270)\n\t\t(unlocked yes)\n\t\t(layer \"User.25\")\n\t\t(uuid \"u-crit\")",
            fmt(p.x),
            fmt(p.y)
        )),
        "{}",
        plan.new
    );
    let again = plan_board(&plan.new).unwrap();
    assert_eq!(again.new, plan.new, "a second run changes nothing");
    // An empty note is not looked at.
    let empty = plan_board(&board.replace("\"U1.5\"", "\"\"")).unwrap();
    assert!(empty.items[0].notes.is_empty());
}

#[test]
fn every_designator_reads_left_to_right_or_top_to_bottom_on_each_side() {
    let (_, lib) = changed(&footprint(3.9, 4.9, &[], "F.Fab", &[]));
    for (side, vertical) in [("F.Cu", 270.0), ("B.Cu", 90.0)] {
        for phi in [0.0, 90.0, 180.0, 270.0, -90.0] {
            let placed = lib
                .replacen(
                    "(footprint \"T\"",
                    &format!("(footprint \"Lib:T\"\n\t(layer \"{side}\")\n\t(at 10 10 {phi})"),
                    1,
                )
                .replace("\"REF**\"", "\"U1\"");
            let placed = if side == "B.Cu" {
                placed.replace("User.23", "User.24").replace("User.17", "User.18").replace("F.Fab", "B.Fab")
            } else {
                placed
            };
            let plan = plan_board(&format!("(kicad_pcb\n{placed})\n")).unwrap();
            let (BoardOutcome::Change { angle, .. } | BoardOutcome::Fine(Placement { angle, .. })) =
                plan.items[0].outcome
            else {
                panic!("{:?}", plan.items[0])
            };
            assert!(angle == 0.0 || angle == vertical, "{side} part at {phi}: {angle}");
        }
    }
}
