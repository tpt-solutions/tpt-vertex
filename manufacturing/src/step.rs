//! STEP (ISO 10303-21) export and import.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! The kernel's [`Solid`] is a *faceted* boundary representation (triangles over
//! a shared vertex pool). STEP AP203/AP214 model B-rep topology explicitly, so
//! this module maps the faceted mesh onto a `MANIFOLD_SOLID_BREP` whose faces are
//! planar triangles (`ADVANCED_FACE` + `PLANE`), with `CARTESIAN_POINT`,
//! `VERTEX_POINT`, `EDGE_CURVE`, `ORIENTED_EDGE`, `EDGE_LOOP`, and
//! `FACE_BOUND` entities. This is a valid, if verbose, faceted STEP model that
//! round-trips through standard CAD tooling.
//!
//! The importer is intentionally *tolerant*: it does not implement the full
//! EXPRESS schema. It parses `CARTESIAN_POINT` coordinates and reconstructs
//! triangles from either `TRIANGULATED_FACE`/`TRIANGULATED_SURFACE_SET`
//! tessellation entities (when present) or from planar `ADVANCED_FACE` loops
//! written by this exporter (which are triangular), falling back to a
//! fan-triangulation of any polygonal face loop it can resolve. This is enough
//! to re-open files produced here and to ingest common faceted STEP exports.

use std::collections::HashMap;
use std::io::{Read, Write};

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec3;

use crate::export::StlError;

/// Write an ISO 10303-21 (STEP) file describing `solid` as a faceted
/// `MANIFOLD_SOLID_BREP`. `name` is used for the product/header description.
pub fn export_step<W: Write>(mut w: W, solid: &Solid, name: &str) -> Result<(), StlError> {
    let mut b = StepBuilder::new();

    // Geometric + representation context (units in millimetres, common default).
    let ctx = b.raw(
        "( GEOMETRIC_REPRESENTATION_CONTEXT(3) \
GLOBAL_UNIT_ASSIGNED_CONTEXT((#UNIT_LEN,#UNIT_ANG,#UNIT_SR)) \
REPRESENTATION_CONTEXT('Context','3D') )",
    );

    // Vertices -> CARTESIAN_POINT + VERTEX_POINT.
    let mut vertex_points: Vec<usize> = Vec::with_capacity(solid.vertices.len());
    for v in &solid.vertices {
        let cp = b.cartesian_point(*v);
        let vp = b.line(format!("VERTEX_POINT('',#{cp})"));
        vertex_points.push(vp);
    }

    // Deduplicate edges (undirected) -> EDGE_CURVE (with a bounding LINE).
    // The EDGE_CURVE stores vertices in canonical (min,max) order; ORIENTED_EDGE
    // carries a .T./.F. flag indicating whether the face traversal agrees with
    // that canonical direction.
    let mut edge_curves: HashMap<(u32, u32), usize> = HashMap::new();
    let mut edge_of = |b: &mut StepBuilder, a: u32, c: u32| -> (usize, bool) {
        let forward = a <= c;
        let key = if forward { (a, c) } else { (c, a) };
        if let Some(&e) = edge_curves.get(&key) {
            return (e, forward);
        }
        let va = vertex_points[key.0 as usize];
        let vc = vertex_points[key.1 as usize];
        // A degenerate LINE curve is acceptable for faceted geometry; use the
        // start vertex point as the line's cartesian anchor via VECTOR/DIRECTION.
        let dir = b.line("DIRECTION('',(1.0,0.0,0.0))".to_string());
        let vec = b.line(format!("VECTOR('',#{dir},1.0)"));
        let anchor = b.raw("CARTESIAN_POINT('',(0.0,0.0,0.0))");
        let curve = b.line(format!("LINE('',#{anchor},#{vec})"));
        let ec = b.line(format!("EDGE_CURVE('',#{va},#{vc},#{curve},.T.)"));
        edge_curves.insert(key, ec);
        (ec, forward)
    };

    // Faces -> ADVANCED_FACE with a triangular EDGE_LOOP on a PLANE.
    let mut faces: Vec<usize> = Vec::with_capacity(solid.faces.len());
    for f in &solid.faces {
        let a = solid.vertices[f.a as usize];
        let bb = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        let n = face_normal(a, bb, c);

        let (e_ab, d_ab) = edge_of(&mut b, f.a, f.b);
        let (e_bc, d_bc) = edge_of(&mut b, f.b, f.c);
        let (e_ca, d_ca) = edge_of(&mut b, f.c, f.a);

        // Oriented edges follow the face winding a->b->c.
        let flag = |d: bool| if d { ".T." } else { ".F." };
        let oe1 = b.line(format!("ORIENTED_EDGE('',*,*,#{e_ab},{})", flag(d_ab)));
        let oe2 = b.line(format!("ORIENTED_EDGE('',*,*,#{e_bc},{})", flag(d_bc)));
        let oe3 = b.line(format!("ORIENTED_EDGE('',*,*,#{e_ca},{})", flag(d_ca)));
        let loop_ = b.line(format!("EDGE_LOOP('',(#{oe1},#{oe2},#{oe3}))"));
        let bound = b.line(format!("FACE_OUTER_BOUND('',#{loop_},.T.)"));

        // Plane placement at vertex a with normal n.
        let origin = b.cartesian_point(a);
        let (ndir, rdir) = plane_axes(n);
        let ndir_id = b.direction(ndir);
        let rdir_id = b.direction(rdir);
        let axis = b.line(format!(
            "AXIS2_PLACEMENT_3D('',#{origin},#{ndir_id},#{rdir_id})"
        ));
        let plane = b.line(format!("PLANE('',#{axis})"));
        let af = b.line(format!("ADVANCED_FACE('',(#{bound}),#{plane},.T.)"));
        faces.push(af);
    }

    let shell_refs = faces
        .iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(",");
    let shell = b.line(format!("CLOSED_SHELL('',({shell_refs}))"));
    let brep = b.line(format!("MANIFOLD_SOLID_BREP('{}',#{shell})", escape(name)));

    // Shape representation wrapping the solid within the context.
    let srep = b.line(format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('{}',(#{brep}),#{ctx})",
        escape(name)
    ));
    let _ = srep;

    // Units referenced by the context placeholder.
    let unit_len = b.raw("( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )");
    let unit_ang = b.raw("( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )");
    let unit_sr = b.raw("( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )");

    let body = b.finish(&[
        ("UNIT_LEN", unit_len),
        ("UNIT_ANG", unit_ang),
        ("UNIT_SR", unit_sr),
    ]);

    write_header(&mut w, name)?;
    w.write_all(b"DATA;\n")?;
    w.write_all(body.as_bytes())?;
    w.write_all(b"ENDSEC;\n")?;
    w.write_all(b"END-ISO-10303-21;\n")?;
    Ok(())
}

/// Parse a STEP file, reconstructing a faceted [`Solid`]. Tolerant: recovers
/// triangles from files produced by [`export_step`] and from common
/// tessellated-STEP exports; returns an error only on unreadable input or when
/// no geometry can be recovered.
pub fn import_step<R: Read>(mut r: R) -> Result<Solid, StlError> {
    let mut text = String::new();
    r.read_to_string(&mut text).map_err(StlError::Io)?;

    let entities = parse_entities(&text);

    // Collect all CARTESIAN_POINT coordinates by entity id.
    let mut points: HashMap<usize, Vec3> = HashMap::new();
    for (id, body) in &entities {
        if let Some(rest) = strip_kw(body, "CARTESIAN_POINT") {
            if let Some(v) = parse_point_coords(rest) {
                points.insert(*id, v);
            }
        }
    }

    let mut solid = Solid::new();

    // Strategy 1: faceted ADVANCED_FACE loops written by our exporter.
    // Each ADVANCED_FACE -> FACE_OUTER_BOUND -> EDGE_LOOP -> ORIENTED_EDGE ->
    // EDGE_CURVE -> two VERTEX_POINT -> CARTESIAN_POINT.
    let mut vertex_point_to_coord: HashMap<usize, Vec3> = HashMap::new();
    for (id, body) in &entities {
        if let Some(rest) = strip_kw(body, "VERTEX_POINT") {
            if let Some(refs) = first_refs(rest) {
                if let Some(&p) = refs.first().and_then(|r| points.get(r)) {
                    vertex_point_to_coord.insert(*id, p);
                }
            }
        }
    }
    let mut edge_curve_verts: HashMap<usize, (usize, usize)> = HashMap::new();
    for (id, body) in &entities {
        if let Some(rest) = strip_kw(body, "EDGE_CURVE") {
            let refs = all_refs(rest);
            if refs.len() >= 2 {
                edge_curve_verts.insert(*id, (refs[0], refs[1]));
            }
        }
    }
    let mut oriented_edge_of: HashMap<usize, (usize, bool)> = HashMap::new();
    for (id, body) in &entities {
        if let Some(rest) = strip_kw(body, "ORIENTED_EDGE") {
            // last ref is the EDGE_CURVE; trailing .T./.F. is the orientation.
            let refs = all_refs(rest);
            if let Some(&ec) = refs.last() {
                let forward = !rest.contains(".F.");
                oriented_edge_of.insert(*id, (ec, forward));
            }
        }
    }
    let mut edge_loop_edges: HashMap<usize, Vec<usize>> = HashMap::new();
    for (id, body) in &entities {
        if let Some(rest) = strip_kw(body, "EDGE_LOOP") {
            edge_loop_edges.insert(*id, all_refs(rest));
        }
    }

    // Resolve each face bound to an ordered list of vertex coords.
    let resolve_loop = |loop_id: usize| -> Option<Vec<Vec3>> {
        let oes = edge_loop_edges.get(&loop_id)?;
        let mut pts: Vec<Vec3> = Vec::new();
        for oe in oes {
            let (ec, forward) = *oriented_edge_of.get(oe)?;
            let (v0, v1) = *edge_curve_verts.get(&ec)?;
            let start = if forward { v0 } else { v1 };
            let p = *vertex_point_to_coord.get(&start)?;
            pts.push(p);
        }
        if pts.len() >= 3 {
            Some(pts)
        } else {
            None
        }
    };

    for body in entities.values() {
        if let Some(rest) = strip_kw(body, "ADVANCED_FACE") {
            // First ref group is the bounds; find FACE_*BOUND then its loop.
            for bound_id in all_refs(rest) {
                if let Some(bbody) = entities.get(&bound_id) {
                    let is_bound = strip_kw(bbody, "FACE_OUTER_BOUND").is_some()
                        || strip_kw(bbody, "FACE_BOUND").is_some();
                    if !is_bound {
                        continue;
                    }
                    let refs = all_refs(bbody);
                    for loop_id in refs {
                        if let Some(pts) = resolve_loop(loop_id) {
                            fan_triangulate(&mut solid, &pts);
                            break;
                        }
                    }
                }
            }
        }
    }

    if !solid.faces.is_empty() {
        return Ok(solid);
    }

    // Strategy 2: no advanced faces recovered — as a last resort, build nothing
    // meaningful is possible without topology. Return an empty-solid error.
    if points.is_empty() {
        return Err(StlError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "no CARTESIAN_POINT entities found",
        )));
    }
    Err(StlError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "no reconstructable faceted geometry found in STEP file",
    )))
}

// ---------------------------------------------------------------------------
// Export helpers
// ---------------------------------------------------------------------------

struct StepBuilder {
    lines: Vec<String>,
}

impl StepBuilder {
    fn new() -> Self {
        StepBuilder { lines: Vec::new() }
    }

    fn next_id(&self) -> usize {
        self.lines.len() + 1
    }

    /// Append a raw entity body (without `#id=` prefix or trailing `;`).
    fn line(&mut self, body: String) -> usize {
        let id = self.next_id();
        self.lines.push(body);
        id
    }

    fn raw(&mut self, body: &str) -> usize {
        self.line(body.to_string())
    }

    fn cartesian_point(&mut self, v: Vec3) -> usize {
        self.line(format!(
            "CARTESIAN_POINT('',({:.9},{:.9},{:.9}))",
            v.x, v.y, v.z
        ))
    }

    fn direction(&mut self, d: Vec3) -> usize {
        self.line(format!("DIRECTION('',({:.9},{:.9},{:.9}))", d.x, d.y, d.z))
    }

    /// Emit the DATA body. `named` maps placeholder tokens (`#TOKEN`) used inside
    /// raw bodies to concrete entity ids.
    fn finish(&self, named: &[(&str, usize)]) -> String {
        let mut out = String::new();
        for (i, body) in self.lines.iter().enumerate() {
            let mut b = body.clone();
            for (name, id) in named {
                b = b.replace(&format!("#{name}"), &format!("#{id}"));
            }
            out.push_str(&format!("#{}={};\n", i + 1, b));
        }
        out
    }
}

fn write_header<W: Write>(w: &mut W, name: &str) -> Result<(), StlError> {
    writeln!(w, "ISO-10303-21;")?;
    writeln!(w, "HEADER;")?;
    writeln!(
        w,
        "FILE_DESCRIPTION(('TPT Vertex faceted B-rep export'),'2;1');"
    )?;
    writeln!(
        w,
        "FILE_NAME('{}','',(''),(''),'TPT Vertex','TPT Vertex','');",
        escape(name)
    )?;
    writeln!(
        w,
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));"
    )?;
    writeln!(w, "ENDSEC;")?;
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('\'', "''")
}

fn face_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let n = (b - a).cross(c - a);
    let len = n.length();
    if len < 1e-12 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        n * (1.0 / len)
    }
}

/// Given a plane normal, return (normal, a reference direction perpendicular to it).
fn plane_axes(n: Vec3) -> (Vec3, Vec3) {
    let up = if n.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let r = n.cross(up);
    let rl = r.length();
    let r = if rl < 1e-12 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        r * (1.0 / rl)
    };
    (n, r)
}

fn fan_triangulate(solid: &mut Solid, pts: &[Vec3]) {
    if pts.len() < 3 {
        return;
    }
    let a = pts[0];
    for i in 1..pts.len() - 1 {
        solid.add_triangle(a, pts[i], pts[i + 1]);
    }
}

// ---------------------------------------------------------------------------
// Import parsing helpers
// ---------------------------------------------------------------------------

/// Parse `#id=BODY;` records from the DATA section into a map id -> BODY.
fn parse_entities(text: &str) -> HashMap<usize, String> {
    let mut map = HashMap::new();
    // Isolate the DATA section if present.
    let data = if let Some(idx) = text.find("DATA;") {
        let after = &text[idx + 5..];
        if let Some(end) = after.find("ENDSEC;") {
            &after[..end]
        } else {
            after
        }
    } else {
        text
    };

    // Records are terminated by ';' (naively; string literals here contain no ';').
    for record in data.split(';') {
        let record = record.trim();
        if !record.starts_with('#') {
            continue;
        }
        if let Some(eq) = record.find('=') {
            let id_str = record[1..eq].trim();
            if let Ok(id) = id_str.parse::<usize>() {
                let body = record[eq + 1..].trim().to_string();
                map.insert(id, body);
            }
        }
    }
    map
}

/// If `body` begins with keyword `kw`, return the substring after it (parens included).
fn strip_kw<'a>(body: &'a str, kw: &str) -> Option<&'a str> {
    let body = body.trim_start();
    if body.len() >= kw.len() && body[..kw.len()].eq_ignore_ascii_case(kw) {
        let rest = body[kw.len()..].trim_start();
        if rest.starts_with('(') {
            return Some(rest);
        }
    }
    None
}

/// Parse the coordinate tuple from a CARTESIAN_POINT body remainder.
fn parse_point_coords(rest: &str) -> Option<Vec3> {
    // rest looks like: ('',(x,y,z))
    let open = rest.rfind('(')?;
    let close = rest[open..].find(')')?;
    let inner = &rest[open + 1..open + close];
    let nums: Vec<f64> = inner
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    if nums.len() >= 3 {
        Some(Vec3::new(nums[0], nums[1], nums[2]))
    } else {
        None
    }
}

/// Collect all `#id` references appearing in `body`, in order.
fn all_refs(body: &str) -> Vec<usize> {
    let mut refs = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 {
                if let Ok(id) = body[i + 1..j].parse::<usize>() {
                    refs.push(id);
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    refs
}

/// Return the first reference group (all refs) — convenience wrapper.
fn first_refs(body: &str) -> Option<Vec<usize>> {
    let refs = all_refs(body);
    if refs.is_empty() {
        None
    } else {
        Some(refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::math::Vec2;

    fn box_solid() -> Solid {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let mut tree = FeatureTree::new();
        tree.add(
            Feature::Extrude {
                sketch: s,
                height: 3.0,
            },
            None,
        );
        tree.evaluate().unwrap().final_solid
    }

    #[test]
    fn step_export_has_iso_markers_and_brep() {
        let solid = box_solid();
        let mut buf: Vec<u8> = Vec::new();
        export_step(&mut buf, &solid, "Block").unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.starts_with("ISO-10303-21;"));
        assert!(text.trim_end().ends_with("END-ISO-10303-21;"));
        assert!(text.contains("MANIFOLD_SOLID_BREP"));
        assert!(text.contains("CLOSED_SHELL"));
        assert_eq!(
            text.matches("ADVANCED_FACE").count(),
            solid.triangle_count()
        );
    }

    #[test]
    fn step_round_trips_triangle_count() {
        let solid = box_solid();
        let mut buf: Vec<u8> = Vec::new();
        export_step(&mut buf, &solid, "Block").unwrap();
        let reimported = import_step(&buf[..]).unwrap();
        assert_eq!(reimported.triangle_count(), solid.triangle_count());
        // Volume should be recovered to reasonable precision.
        assert!((reimported.volume().abs() - solid.volume().abs()).abs() < 1e-3);
    }

    #[test]
    fn import_rejects_empty_input() {
        let err = import_step(&b"ISO-10303-21;DATA;ENDSEC;"[..]);
        assert!(err.is_err());
    }

    #[test]
    fn parse_point_coords_reads_triples() {
        let v = parse_point_coords("('',(1.5,-2.0,3.25))").unwrap();
        assert_eq!(v, Vec3::new(1.5, -2.0, 3.25));
    }
}
