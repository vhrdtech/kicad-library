//! A small STEP writer (ISO 10303-21, AP214 `AUTOMOTIVE_DESIGN`) for models made of straight prisms: each
//! [`Solid`] is a convex polygon in the x-y plane extruded from one height to another, written as a closed
//! manifold B-rep (planar faces, straight edges) with a colour of its own.
//!
//! That is enough for chip passives and moulded parts (boxes, chamfered and round-cornered bodies), and KiCad
//! (through OpenCascade) reads the colours the way it reads them from its own models: a `STYLED_ITEM` per solid in
//! a `MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION`. Units are millimetres; z = 0 is the board surface.

use std::fmt::Write as _;

/// A colour, each channel 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub f64, pub f64, pub f64);

/// A convex polygon in the x-y plane (corners counter-clockwise seen from +z), extruded from `z0` up to `z1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Solid {
    pub name: String,
    pub outline: Vec<[f64; 2]>,
    pub z0: f64,
    pub z1: f64,
    pub colour: Rgb,
}

impl Solid {
    /// An axis-aligned box from `min` to `max`.
    pub fn cuboid(name: &str, min: [f64; 3], max: [f64; 3], colour: Rgb) -> Solid {
        let outline = vec![[min[0], min[1]], [max[0], min[1]], [max[0], max[1]], [min[0], max[1]]];
        Solid { name: name.into(), outline, z0: min[2], z1: max[2], colour }
    }

    /// Why the solid cannot be written: fewer than three corners, no height, a corner repeated, or an outline
    /// that is not convex and counter-clockwise.
    pub fn check(&self) -> Result<(), String> {
        let n = self.outline.len();
        if n < 3 {
            return Err(format!("{}: {n} corners", self.name));
        }
        if self.z1 - self.z0 <= 1e-9 {
            return Err(format!("{}: no height ({} to {})", self.name, self.z0, self.z1));
        }
        for i in 0..n {
            let [a, b, c] = [self.outline[i], self.outline[(i + 1) % n], self.outline[(i + 2) % n]];
            if (b[0] - a[0]).hypot(b[1] - a[1]) < 1e-9 {
                return Err(format!("{}: corner {i} repeated", self.name));
            }
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if cross <= 1e-12 {
                return Err(format!("{}: not convex and counter-clockwise at corner {}", self.name, (i + 1) % n));
            }
        }
        Ok(())
    }
}

/// A STEP real: always with a decimal point (`1.`, `0.25`, `-1.5`), never `-0.`.
fn real(v: f64) -> String {
    let v = if v.abs() < 5e-10 { 0.0 } else { v };
    let mut s = format!("{v:.9}");
    while s.ends_with('0') {
        s.pop();
    }
    s
}

/// A STEP string: `'` doubled, only printable ASCII kept.
fn string(s: &str) -> String {
    let clean: String = s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).collect();
    format!("'{}'", clean.replace('\'', "''"))
}

struct Writer {
    data: String,
    next: usize,
}

impl Writer {
    /// Adds one entity instance (the text after `#n=`, without the `;`) and answers its number.
    fn add(&mut self, entity: impl AsRef<str>) -> usize {
        let id = self.next;
        self.next += 1;
        let _ = writeln!(self.data, "#{id}={};", entity.as_ref());
        id
    }

    fn point(&mut self, p: [f64; 3]) -> usize {
        self.add(format!("CARTESIAN_POINT('',({},{},{}))", real(p[0]), real(p[1]), real(p[2])))
    }

    fn direction(&mut self, d: [f64; 3]) -> usize {
        self.add(format!("DIRECTION('',({},{},{}))", real(d[0]), real(d[1]), real(d[2])))
    }

    fn placement(&mut self, at: [f64; 3], axis: [f64; 3], reference: [f64; 3]) -> usize {
        let p = self.point(at);
        let a = self.direction(axis);
        let r = self.direction(reference);
        self.add(format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
    }
}

/// A planar face: its outward normal, a direction in its plane, and its loop as (edge, same sense).
type Face = ([f64; 3], [f64; 3], Vec<(usize, bool)>);

/// Faces, edges and vertices of one prism, by index: what [`Solid`] becomes before it is written.
#[derive(Debug)]
struct Topology {
    /// Corner `i` at the bottom is vertex `i`, at the top `n + i`.
    vertices: Vec<[f64; 3]>,
    /// (from, to) vertex indices.
    edges: Vec<(usize, usize)>,
    faces: Vec<Face>,
}

fn topology(s: &Solid) -> Topology {
    let n = s.outline.len();
    let mut vertices = Vec::with_capacity(2 * n);
    for z in [s.z0, s.z1] {
        vertices.extend(s.outline.iter().map(|c| [c[0], c[1], z]));
    }
    // Edges: bottom i (b_i -> b_i+1) is i, top i (t_i -> t_i+1) is n + i, vertical i (b_i -> t_i) is 2n + i.
    let mut edges = Vec::with_capacity(3 * n);
    for i in 0..n {
        edges.push((i, (i + 1) % n));
    }
    for i in 0..n {
        edges.push((n + i, n + (i + 1) % n));
    }
    for i in 0..n {
        edges.push((i, n + i));
    }
    let mut faces = Vec::with_capacity(n + 2);
    // The bottom, seen from below: the outline backwards.
    faces.push(([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], (0..n).rev().map(|i| (i, false)).collect()));
    faces.push(([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], (0..n).map(|i| (n + i, true)).collect()));
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b) = (s.outline[i], s.outline[j]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let d = [(b[0] - a[0]) / len, (b[1] - a[1]) / len, 0.0];
        // Seen from outside: along the bottom edge, up the next corner, back along the top, down this corner.
        let lp = vec![(i, true), (2 * n + j, true), (n + i, false), (2 * n + i, false)];
        faces.push(([d[1], -d[0], 0.0], d, lp));
    }
    Topology { vertices, edges, faces }
}

/// The STEP text of a part named `name` made of `solids`. Fails on a solid [`Solid::check`] refuses.
pub fn write(name: &str, solids: &[Solid]) -> Result<String, String> {
    for s in solids {
        s.check()?;
    }
    if solids.is_empty() {
        return Err(format!("{name}: no solids"));
    }
    let mut w = Writer { data: String::new(), next: 1 };
    let app = w.add("APPLICATION_CONTEXT('core data for automotive mechanical design processes')");
    w.add(format!("APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app})"));
    let pctx = w.add(format!("PRODUCT_CONTEXT('',#{app},'mechanical')"));
    let product = w.add(format!("PRODUCT({0},{0},'',(#{pctx}))", string(name)));
    w.add(format!("PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{product}))"));
    let pdf = w.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{product})"));
    let pdctx = w.add(format!("PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"));
    let pd = w.add(format!("PRODUCT_DEFINITION('design','',#{pdf},#{pdctx})"));
    let pds = w.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));
    let mm = w.add("( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )");
    let rad = w.add("( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )");
    let sr = w.add("( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )");
    let unc = w.add(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#{mm},'distance_accuracy_value','confusion accuracy')"
    ));
    let ctx = w.add(format!(
        "( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc})) \
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{mm},#{rad},#{sr})) REPRESENTATION_CONTEXT('','') )"
    ));
    let origin = w.placement([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);

    let mut breps = Vec::new();
    let mut styled = Vec::new();
    for s in solids {
        let t = topology(s);
        let pts: Vec<usize> = t.vertices.iter().map(|&p| w.point(p)).collect();
        let verts: Vec<usize> = pts.iter().map(|p| w.add(format!("VERTEX_POINT('',#{p})"))).collect();
        let mut edges = Vec::with_capacity(t.edges.len());
        for &(a, b) in &t.edges {
            let (pa, pb) = (t.vertices[a], t.vertices[b]);
            let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let dir = w.direction([d[0] / len, d[1] / len, d[2] / len]);
            let vec = w.add(format!("VECTOR('',#{dir},{})", real(len)));
            let line = w.add(format!("LINE('',#{},#{vec})", pts[a]));
            edges.push(w.add(format!("EDGE_CURVE('',#{},#{},#{line},.T.)", verts[a], verts[b])));
        }
        let mut faces = Vec::with_capacity(t.faces.len());
        for (normal, reference, lp) in &t.faces {
            let oriented: Vec<String> = lp
                .iter()
                .map(|&(e, sense)| {
                    format!(
                        "#{}",
                        w.add(format!("ORIENTED_EDGE('',*,*,#{},{})", edges[e], if sense { ".T." } else { ".F." }))
                    )
                })
                .collect();
            let edge_loop = w.add(format!("EDGE_LOOP('',({}))", oriented.join(",")));
            let bound = w.add(format!("FACE_OUTER_BOUND('',#{edge_loop},.T.)"));
            let at = t.vertices[t.edges[lp[0].0].0];
            let place = w.placement(at, *normal, *reference);
            let plane = w.add(format!("PLANE('',#{place})"));
            faces.push(w.add(format!("ADVANCED_FACE('',(#{bound}),#{plane},.T.)")));
        }
        let face_list: Vec<String> = faces.iter().map(|f| format!("#{f}")).collect();
        let shell = w.add(format!("CLOSED_SHELL('',({}))", face_list.join(",")));
        let brep = w.add(format!("MANIFOLD_SOLID_BREP({},#{shell})", string(&s.name)));
        breps.push(brep);

        let Rgb(r, g, b) = s.colour;
        let colour = w.add(format!("COLOUR_RGB('',{},{},{})", real(r), real(g), real(b)));
        let fill_colour = w.add(format!("FILL_AREA_STYLE_COLOUR('',#{colour})"));
        let fill = w.add(format!("FILL_AREA_STYLE('',(#{fill_colour}))"));
        let surface_fill = w.add(format!("SURFACE_STYLE_FILL_AREA(#{fill})"));
        let side = w.add(format!("SURFACE_SIDE_STYLE('',(#{surface_fill}))"));
        let usage = w.add(format!("SURFACE_STYLE_USAGE(.BOTH.,#{side})"));
        let assign = w.add(format!("PRESENTATION_STYLE_ASSIGNMENT((#{usage}))"));
        styled.push(w.add(format!("STYLED_ITEM('color',(#{assign}),#{brep})")));
    }
    let items: Vec<String> = std::iter::once(origin).chain(breps.iter().copied()).map(|i| format!("#{i}")).collect();
    let rep = w.add(format!("ADVANCED_BREP_SHAPE_REPRESENTATION({},({}),#{ctx})", string(name), items.join(",")));
    w.add(format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));
    let styles: Vec<String> = styled.iter().map(|i| format!("#{i}")).collect();
    w.add(format!("MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',({}),#{ctx})", styles.join(",")));

    let mut out = String::new();
    out.push_str("ISO-10303-21;\nHEADER;\n");
    let _ = writeln!(out, "FILE_DESCRIPTION(({}),'2;1');", string(&format!("{name}, made by kilib")));
    let _ = writeln!(
        out,
        "FILE_NAME({},'2026-01-01T00:00:00',(''),(''),'kilib {}','kilib','');",
        string(&format!("{name}.step")),
        env!("CARGO_PKG_VERSION")
    );
    out.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n");
    out.push_str(&w.data);
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: Rgb = Rgb(0.5, 0.5, 0.5);

    fn octagon() -> Solid {
        let outline = vec![
            [-1.0, -2.0],
            [1.0, -2.0],
            [2.0, -1.0],
            [2.0, 1.0],
            [1.0, 2.0],
            [-1.0, 2.0],
            [-2.0, 1.0],
            [-2.0, -1.0],
        ];
        Solid { name: "oct".into(), outline, z0: 0.1, z1: 1.0, colour: GREY }
    }

    #[test]
    fn reals_have_a_decimal_point() {
        assert_eq!(real(1.0), "1.");
        assert_eq!(real(0.25), "0.25");
        assert_eq!(real(-1.5), "-1.5");
        assert_eq!(real(-0.0), "0.");
        assert_eq!(real(1e-12), "0.");
        assert_eq!(real(0.1 + 0.2), "0.3");
    }

    #[test]
    fn strings_are_quoted() {
        assert_eq!(string("it's"), "'it''s'");
        assert_eq!(string("Ω 1.0"), "' 1.0'");
    }

    /// Every edge is used by two faces, once in each direction: a closed, consistently oriented shell; and
    /// V - E + F = 2.
    #[test]
    fn prisms_are_closed_and_oriented() {
        for s in [Solid::cuboid("box", [-1.0, -0.5, 0.0], [1.0, 0.5, 0.5], GREY), octagon()] {
            let t = topology(&s);
            let mut uses = vec![(0, 0); t.edges.len()];
            for (_, _, lp) in &t.faces {
                for (k, &(e, sense)) in lp.iter().enumerate() {
                    if sense {
                        uses[e].0 += 1
                    } else {
                        uses[e].1 += 1
                    }
                    // The loop is connected: this edge ends where the next one starts.
                    let end = if sense { t.edges[e].1 } else { t.edges[e].0 };
                    let (n, ns) = lp[(k + 1) % lp.len()];
                    let start = if ns { t.edges[n].0 } else { t.edges[n].1 };
                    assert_eq!(end, start, "{}: loop broken", s.name);
                }
            }
            assert!(uses.iter().all(|&u| u == (1, 1)), "{}: {uses:?}", s.name);
            assert_eq!(t.vertices.len() as i64 - t.edges.len() as i64 + t.faces.len() as i64, 2);
        }
    }

    /// Each face's loop runs counter-clockwise around its outward normal.
    #[test]
    fn loops_turn_around_the_outward_normal() {
        let s = octagon();
        let t = topology(&s);
        for (normal, _, lp) in &t.faces {
            let pts: Vec<[f64; 3]> =
                lp.iter().map(|&(e, sense)| t.vertices[if sense { t.edges[e].0 } else { t.edges[e].1 }]).collect();
            let mut area = [0.0; 3];
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                area[0] += a[1] * b[2] - a[2] * b[1];
                area[1] += a[2] * b[0] - a[0] * b[2];
                area[2] += a[0] * b[1] - a[1] * b[0];
            }
            let dot = area[0] * normal[0] + area[1] * normal[1] + area[2] * normal[2];
            assert!(dot > 0.0, "normal {normal:?}");
        }
    }

    #[test]
    fn every_reference_is_defined_once() {
        let text = write("part", &[octagon(), Solid::cuboid("pad", [0.0, 0.0, 0.0], [1.0, 1.0, 0.1], GREY)]).unwrap();
        let data = &text[text.find("DATA;").unwrap()..text.find("ENDSEC;\nEND-ISO").unwrap()];
        let mut defined = std::collections::BTreeSet::new();
        for line in data.lines().skip(1) {
            let id: usize = line[1..line.find('=').unwrap()].parse().unwrap();
            assert!(defined.insert(id), "#{id} twice");
            assert!(line.ends_with(';'), "{line}");
        }
        for (i, _) in data.match_indices('#') {
            let digits: String = data[i + 1..].chars().take_while(char::is_ascii_digit).collect();
            assert!(defined.contains(&digits.parse::<usize>().unwrap()), "#{digits} not defined");
        }
        assert_eq!(text.matches("MANIFOLD_SOLID_BREP(").count(), 2);
        assert_eq!(text.matches("STYLED_ITEM(").count(), 2);
        assert!(text.starts_with("ISO-10303-21;") && text.ends_with("END-ISO-10303-21;\n"));
    }

    #[test]
    fn bad_solids_are_refused() {
        let mut s = octagon();
        s.outline.reverse();
        assert!(write("x", &[s]).unwrap_err().contains("counter-clockwise"));
        let flat = Solid::cuboid("flat", [0.0; 3], [1.0, 1.0, 0.0], GREY);
        assert!(write("x", &[flat]).unwrap_err().contains("no height"));
        assert!(write("x", &[]).is_err());
    }
}
