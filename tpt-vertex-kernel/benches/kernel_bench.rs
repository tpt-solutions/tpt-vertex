//! Dependency-free performance benchmarks for the geometry kernel.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Run with `cargo bench -p tpt-vertex-kernel`. This uses `std::time::Instant`
//! rather than an external harness (e.g. criterion) to avoid adding dependencies
//! to the workspace; it prints median/mean timings for representative workloads
//! on complex assemblies so regressions are visible in CI logs.

use std::time::Instant;

use tpt_vertex_kernel::assembly::{Assembly, Part};
use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
use tpt_vertex_kernel::geometry::sketch::Sketch;
use tpt_vertex_kernel::math::Vec2;

fn box_tree(w: f64, h: f64, depth: f64) -> FeatureTree {
    let mut s = Sketch::new();
    s.line(Vec2::new(0.0, 0.0), Vec2::new(w, 0.0));
    s.line(Vec2::new(w, 0.0), Vec2::new(w, h));
    s.line(Vec2::new(w, h), Vec2::new(0.0, h));
    s.line(Vec2::new(0.0, h), Vec2::new(0.0, 0.0));
    let mut tree = FeatureTree::new();
    tree.add(
        Feature::Extrude {
            sketch: s,
            height: depth,
        },
        None,
    );
    tree
}

/// Build an assembly of `n` extruded parts.
fn build_assembly(n: usize) -> Assembly {
    let mut asm = Assembly::new();
    for i in 0..n {
        let w = 1.0 + (i % 7) as f64 * 0.3;
        asm.add_part(Part::new(format!("part{i}"), box_tree(w, w, w)));
    }
    asm
}

fn bench<F: FnMut()>(label: &str, iters: usize, mut f: F) {
    // Warm up.
    f();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_secs_f64() * 1e3); // ms
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[samples.len() / 2];
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let min = samples[0];
    let max = *samples.last().unwrap();
    println!(
        "{label:<40} median={median:8.3}ms  mean={mean:8.3}ms  min={min:8.3}ms  max={max:8.3}ms  (n={iters})"
    );
}

fn main() {
    println!("TPT Vertex kernel benchmarks\n");

    // Feature-tree evaluation for a single moderately complex part.
    let tree = box_tree(2.0, 3.0, 4.0);
    bench("evaluate single extrude", 200, || {
        let _ = std::hint::black_box(tree.evaluate().unwrap());
    });

    // Rebuild cost across assembly sizes (complex assemblies).
    for &n in &[50usize, 200, 1000] {
        let asm = build_assembly(n);
        bench(&format!("assembly total_triangles (n={n})"), 50, || {
            let _ = std::hint::black_box(asm.total_triangles());
        });
        bench(&format!("assembly solids eval (n={n})"), 20, || {
            let mut tris = 0usize;
            for (_, part) in asm.parts() {
                tris += std::hint::black_box(part.solid_in_assembly()).triangle_count();
            }
            let _ = std::hint::black_box(tris);
        });
    }
}
