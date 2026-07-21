fn main() {
    use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::geometry::solid::Solid;
    use tpt_vertex_kernel::math::Vec2;
    use tpt_vertex_kernel::math::Vec3;

    let [x0, y0, x1, y1] = [0.0f64, 0.0, 2.0, 2.0];
    let mut s = Sketch::new();
    s.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
    s.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
    s.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
    s.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
    let mut tree = FeatureTree::new();
    tree.add(Feature::Extrude { sketch: s, height: 2.0 }, None);
    let solid: Solid = tree.evaluate().map(|e| e.final_solid).unwrap_or_default();

    // Möller–Trumbore ray-triangle intersection against the +X ray through q.
    fn ray_hits(q: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let e1 = b - a;
        let e2 = c - a;
        let pvec = cross(dir, e2);
        let det = dot(e1, pvec);
        if det.abs() < 1e-12 { return false; }
        let inv = 1.0 / det;
        let tvec = q - a;
        let u = dot(tvec, pvec) * inv;
        if u < -1e-9 || u > 1.0 + 1e-9 { return false; }
        let qvec = cross(tvec, e1);
        let v = dot(dir, qvec) * inv;
        if v < -1e-9 || u + v > 1.0 + 1e-9 { return false; }
        // t > 0 means intersection is in +X direction (in front of q).
        let t = dot(e2, qvec) * inv;
        t > 0.0
    }
    fn cross(a: Vec3, b: Vec3) -> Vec3 { a.cross(b) }
    fn dot(a: Vec3, b: Vec3) -> f64 { a.dot(b) }

    fn pis(solid: &Solid, q: Vec3) -> bool {
        let mut count = 0i32;
        for f in &solid.faces {
            let a = solid.vertices[f.a as usize];
            let b = solid.vertices[f.b as usize];
            let c = solid.vertices[f.c as usize];
            if ray_hits(q, a, b, c) { count += 1; }
        }
        count % 2 == 1
    }

    for pt in [Vec3::new(0.25,0.25,0.25), Vec3::new(3.0,1.0,1.0), Vec3::new(1.0,1.0,1.0)] {
        println!("inside {:?} = {}", pt, pis(&solid, pt));
    }
}
