//! Tree / organic support generation.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Tree supports grow from the build plate (and optionally from the model
//! surface) toward overhanging regions, branching and tapering as they rise.
//! Compared to the basic grid/pillar strategy in [`crate::support`], tree
//! supports use less material and are easier to remove.
//!
//! Algorithm overview:
//! 1. Detect overhang points on each layer by comparing solid footprints
//!    against the grown layer below.
//! 2. Cluster nearby overhang points into *tip groups*.
//! 3. For each cluster, trace a trunk from the build plate upward through the
//!    centroid of successive layers' support points.  Where trunks from
//!    different clusters are close, they merge organically.
//! 4. Where a single trunk serves widely separated tip clusters, it branches
//!    into separate limbs.
//! 5. Trunk paths are smoothed with Catmull-Rom interpolation for organic
//!    curvature.
//! 6. Generate circular cross-section toolpaths along each trunk/branch,
//!    tapering from `base_radius` at the build plate to `tip_radius` at the
//!    overhang.

use crate::infill::point_in_polygon;
use crate::layers::{Contour, Layer, P2};
use crate::offset::offset_contour;
use crate::path::ExtrusionPath;

/// Tunables for tree support generation.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeSupportSettings {
    /// Maximum overhang angle (degrees from vertical) that prints cleanly
    /// without support.
    pub overhang_angle_deg: f64,
    /// Radius (mm) of the trunk at the build plate.
    pub base_radius: f64,
    /// Radius (mm) of the trunk at the overhang tip.
    pub tip_radius: f64,
    /// Minimum distance (mm) between trunk centre-lines below which two
    /// branches merge.
    pub merge_distance: f64,
    /// Vertical air gap (in layers) left between the tip and the overhang.
    pub z_gap_layers: usize,
    /// Number of segments used to approximate the circular cross-section.
    pub circle_segments: usize,
    /// Distance threshold (mm) at which a trunk should split into branches
    /// when serving separated tip clusters.  When the convex hull of a
    /// trunk's active tip cluster exceeds this diameter, a branch is forked.
    pub branch_split_distance: f64,
    /// Whether to anchor trunks from the model surface (true) or only from
    /// the build plate (false, default).
    pub anchor_from_model: bool,
    /// Whether to smooth trunk paths with Catmull-Rom interpolation for
    /// organic shaping.
    pub smooth_trunks: bool,
}

impl Default for TreeSupportSettings {
    fn default() -> Self {
        TreeSupportSettings {
            overhang_angle_deg: 45.0,
            base_radius: 1.5,
            tip_radius: 0.4,
            merge_distance: 3.0,
            z_gap_layers: 1,
            circle_segments: 8,
            branch_split_distance: 10.0,
            anchor_from_model: false,
            smooth_trunks: true,
        }
    }
}

/// A tree support trunk: a polyline from the build plate to an overhang tip,
/// carrying a radius at each control point.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeTrunk {
    /// Control points from bottom (build plate) to top (overhang).
    pub points: Vec<P2>,
    /// Radius at each control point (tapers from base to tip).
    pub radii: Vec<f64>,
}

/// Per-layer tree support toolpaths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeSupportLayer {
    /// Circular cross-section paths to print at this layer's Z.
    pub paths: Vec<ExtrusionPath>,
}

impl TreeSupportLayer {
    /// Merge in paths from another layer (used when combining trunks).
    pub fn extend(&mut self, other: TreeSupportLayer) {
        self.paths.extend(other.paths);
    }
}

/// Generate tree supports for a stack of layers.
pub fn generate_tree_supports(
    layers: &[Layer],
    settings: &TreeSupportSettings,
) -> Vec<TreeSupportLayer> {
    let n = layers.len();
    let mut result = vec![TreeSupportLayer::default(); n];

    let tan_angle = settings.overhang_angle_deg.to_radians().tan();
    let z_gap = settings.z_gap_layers.max(1);

    // Pass 1: collect overhang sample points per layer.
    let overhang_points = detect_overhang_points(layers, tan_angle);

    // Pass 2: cluster overhang points and build trunks.
    let mut trunks = build_trunks(&overhang_points, layers, settings, z_gap);

    // Pass 3: apply branching where trunks serve widely separated clusters.
    apply_branching(&mut trunks, &overhang_points, layers, settings);

    // Pass 4: optionally smooth trunk paths with Catmull-Rom interpolation.
    if settings.smooth_trunks {
        for trunk in &mut trunks {
            smooth_trunk_path(trunk);
        }
    }

    // Pass 5: rasterise trunks into per-layer circular cross-section paths.
    for trunk in &trunks {
        let tip_idx = trunk.points.len().saturating_sub(1);
        for (li, layer) in layers.iter().enumerate() {
            let Some(radius) = interpolate_trunk_at_z(trunk, layer.z, layers) else {
                continue;
            };
            if radius < 1e-3 {
                continue;
            }
            // Don't print support inside the part itself.
            if point_in_any(&layer.contours, trunk.points[li.min(tip_idx)]) {
                continue;
            }
            let center = trunk.points[li.min(tip_idx)];
            let path = circle_path(center, radius, settings.circle_segments);
            result[li].paths.push(path);
        }
    }

    result
}

/// Detect overhang sample points on each layer by comparing footprints.
fn detect_overhang_points(layers: &[Layer], tan_angle: f64) -> Vec<Vec<P2>> {
    let n = layers.len();
    let mut overhangs: Vec<Vec<P2>> = vec![Vec::new(); n];

    for i in 1..n {
        let prev = &layers[i - 1];
        let cur = &layers[i];

        let dz = (cur.z - prev.z).max(1e-6);
        let allowance = dz * tan_angle;

        let grown: Vec<Contour> = prev
            .contours
            .iter()
            .map(|c| offset_contour(c, allowance))
            .collect();

        // Sample a grid inside each current contour, keep points that are
        // outside all grown contours (i.e. unsupported).
        if let Some(grid) = sample_grid_for_layer(cur, 2.0) {
            for pt in grid {
                let solid_here = point_in_any(&cur.contours, pt);
                let backed = point_in_any(&grown, pt);
                if solid_here && !backed {
                    overhangs[i].push(pt);
                }
            }
        }
    }

    overhangs
}

/// Build tree trunks from overhang points using a greedy bottom-up approach.
fn build_trunks(
    overhang_points: &[Vec<P2>],
    layers: &[Layer],
    settings: &TreeSupportSettings,
    z_gap: usize,
) -> Vec<TreeTrunk> {
    let n = layers.len();

    // For each layer, cluster nearby overhang points.
    let clusters: Vec<Vec<Vec<P2>>> = overhang_points
        .iter()
        .map(|pts| cluster_points(pts, settings.merge_distance))
        .collect();

    // Greedily connect clusters across layers: for each cluster at layer i,
    // find the nearest cluster at layer i-1 and merge their centroids.
    // Unmatched clusters start a new trunk at the build plate.

    let mut trunks: Vec<TreeTrunk> = Vec::new();
    // Active trunk endpoints: (centroid, trunk_index) for the previous layer.
    let mut active: Vec<(P2, usize)> = Vec::new();

    for (i, cluster) in clusters.iter().enumerate().take(n) {
        if cluster.is_empty() {
            // No overhang here — carry active trunks forward.
            for (_, ti) in &mut active {
                if let Some(trunk) = trunks.get_mut(*ti) {
                    let center = trunk.points.last().copied().unwrap_or(P2::new(0.0, 0.0));
                    trunk.points.push(center);
                    let last_r = *trunk.radii.last().unwrap_or(&settings.base_radius);
                    trunk.radii.push(last_r);
                }
            }
            continue;
        }

        let mut matched_active: Vec<bool> = vec![false; active.len()];
        let mut new_active: Vec<(P2, usize)> = Vec::new();

        for sub in cluster {
            let centroid = medoid_of(sub);

            // Find nearest active trunk.
            let mut best: Option<(f64, usize)> = None;
            for (ai, &(pt, _ti)) in active.iter().enumerate() {
                if matched_active[ai] {
                    continue;
                }
                let d = pt.dist(centroid);
                if d < settings.merge_distance * 2.0 {
                    match &best {
                        Some((bd, _)) if d < *bd => best = Some((d, ai)),
                        None => best = Some((d, ai)),
                        _ => {}
                    }
                }
            }

            if let Some((_, ai)) = best {
                matched_active[ai] = true;
                let ti = active[ai].1;
                let _top = (n - 1).saturating_sub(z_gap).min(i);
                if let Some(trunk) = trunks.get_mut(ti) {
                    trunk.points.push(centroid);
                    let progress = (i as f64) / (n as f64).max(1.0);
                    let r = settings.base_radius
                        + (settings.tip_radius - settings.base_radius) * progress;
                    trunk.radii.push(r);
                }
                new_active.push((centroid, ti));
            } else {
                // Start a new trunk from the build plate.
                let mut trunk = TreeTrunk {
                    points: vec![centroid],
                    radii: vec![settings.base_radius],
                };
                // Add build-plate anchor point at z=0 (layer 0 centroid).
                trunk.points.insert(0, centroid);
                trunk.radii.insert(0, settings.base_radius);
                let ti = trunks.len();
                trunks.push(trunk);
                new_active.push((centroid, ti));
            }
        }

        // Carry unmatched old trunks forward.
        for (ai, &(pt, ti)) in active.iter().enumerate() {
            if !matched_active[ai] {
                if let Some(trunk) = trunks.get_mut(ti) {
                    let center = trunk.points.last().copied().unwrap_or(pt);
                    trunk.points.push(center);
                    let last_r = *trunk.radii.last().unwrap_or(&settings.tip_radius);
                    trunk.radii.push(last_r);
                }
                new_active.push((pt, ti));
            }
        }

        active = new_active;
    }

    trunks
}

/// Apply branching: when a single trunk serves widely separated tip clusters,
/// fork it into separate branches.
fn apply_branching(
    trunks: &mut Vec<TreeTrunk>,
    overhang_points: &[Vec<P2>],
    layers: &[Layer],
    settings: &TreeSupportSettings,
) {
    let n = layers.len();
    let split_dist = settings.branch_split_distance;

    // For each trunk, collect the tip points it serves at each layer.
    // If at any layer the tip points span a diameter > split_distance,
    // fork the trunk at that layer.
    let mut new_trunks: Vec<TreeTrunk> = Vec::new();

    let trunk_ids: Vec<usize> = (0..trunks.len()).collect();
    for &ti in &trunk_ids {
        let trunk = &trunks[ti];
        if trunk.points.len() < 3 {
            continue;
        }

        // Find the layer range where this trunk has overhang points.
        let mut fork_layer = None;
        for (i, overhang) in overhang_points.iter().enumerate().take(n).skip(1) {
            if overhang.is_empty() {
                continue;
            }
            // Check which overhang points are closest to this trunk at layer i.
            let trunk_center = trunk.points[i.min(trunk.points.len() - 1)];
            let nearby: Vec<P2> = overhang
                .iter()
                .copied()
                .filter(|p| p.dist(trunk_center) < settings.merge_distance * 3.0)
                .collect();
            if nearby.len() < 2 {
                continue;
            }
            let diameter = point_cloud_diameter(&nearby);
            if diameter > split_dist {
                fork_layer = Some(i);
                break;
            }
        }

        if let Some(fi) = fork_layer {
            // Fork: create a branch trunk from the fork point upward.
            let fork_idx = fi.min(trunk.points.len() - 1);
            let branch_points: Vec<P2> = trunk.points[fork_idx..].to_vec();
            let branch_radii: Vec<f64> = trunk.radii[fork_idx..]
                .iter()
                .map(|r| r * 0.7) // Branch is thinner
                .collect();

            if !branch_points.is_empty() {
                let branch = TreeTrunk {
                    points: branch_points,
                    radii: branch_radii,
                };
                new_trunks.push(branch);
            }

            // Trim the original trunk at the fork point.
            if let Some(t) = trunks.get_mut(ti) {
                t.points.truncate(fork_idx + 1);
                t.radii.truncate(fork_idx + 1);
            }
        }
    }

    trunks.extend(new_trunks);
}

/// Smooth a trunk path using Catmull-Rom interpolation for organic curvature.
/// Replaces the polyline with a denser set of interpolated points.
fn smooth_trunk_path(trunk: &mut TreeTrunk) {
    if trunk.points.len() < 4 {
        return;
    }

    let points = trunk.points.clone();
    let radii = trunk.radii.clone();
    let segments_per_span = 4; // Number of interpolated points between each pair of control points.
    let alpha = 0.5; // Catmull-Rom tension parameter.

    let mut new_points = Vec::new();
    let mut new_radii = Vec::new();

    let n = points.len();
    for i in 0..n - 1 {
        let p0 = if i > 0 { points[i - 1] } else { points[i] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n {
            points[i + 2]
        } else {
            points[i + 1]
        };

        let _r0 = if i > 0 { radii[i - 1] } else { radii[i] };
        let r1 = radii[i];
        let r2 = radii[i + 1];
        let _r3 = if i + 2 < n {
            radii[i + 2]
        } else {
            radii[i + 1]
        };

        for s in 0..segments_per_span {
            let t = s as f64 / segments_per_span as f64;
            let pt = catmull_rom_point(p0, p1, p2, p3, t, alpha);
            let r = r1 * (1.0 - t) + r2 * t;
            new_points.push(pt);
            new_radii.push(r);
        }
    }

    // Add the last point.
    if let Some(&last) = points.last() {
        new_points.push(last);
        new_radii.push(*radii.last().unwrap_or(&0.0));
    }

    trunk.points = new_points;
    trunk.radii = new_radii;
}

/// Catmull-Rom spline interpolation for a single point.
fn catmull_rom_point(p0: P2, p1: P2, p2: P2, p3: P2, t: f64, alpha: f64) -> P2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let s = alpha;

    let x = s
        * ((-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3
            + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
            + (-p0.x + p2.x) * t
            + 2.0 * p1.x);
    let y = s
        * ((-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3
            + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
            + (-p0.y + p2.y) * t
            + 2.0 * p1.y);

    P2::new(x, y)
}

/// Interpolate the trunk radius at a given layer Z by finding the two
/// surrounding control points.
fn interpolate_trunk_at_z(trunk: &TreeTrunk, z: f64, layers: &[Layer]) -> Option<f64> {
    if trunk.points.is_empty() || trunk.radii.is_empty() {
        return None;
    }
    // Map Z to a fractional index by interpolating against layer Z values.
    let n = layers.len();
    if n == 0 {
        return None;
    }
    let idx = ((z - layers[0].z) / layers.last().map(|l| l.z - layers[0].z).unwrap_or(1.0))
        .clamp(0.0, 1.0);
    let trunk_idx = idx * (trunk.points.len() as f64 - 1.0);
    let lo = (trunk_idx.floor() as usize).min(trunk.radii.len() - 1);
    let hi = (trunk_idx.ceil() as usize).min(trunk.radii.len() - 1);
    let t = trunk_idx - lo as f64;
    Some(trunk.radii[lo] * (1.0 - t) + trunk.radii[hi] * t)
}

/// Generate a closed circular extrusion path centred at `center` with the
/// given `radius` and `segments`.
fn circle_path(center: P2, radius: f64, segments: usize) -> ExtrusionPath {
    let segments = segments.max(3);
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let angle = (i as f64 / segments as f64) * std::f64::consts::TAU;
        pts.push(P2::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    ExtrusionPath::new(pts, true)
}

/// Cluster 2D points by single-linkage clustering with `max_dist` threshold.
fn cluster_points(points: &[P2], max_dist: f64) -> Vec<Vec<P2>> {
    let mut labels: Vec<usize> = (0..points.len()).collect();

    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            if points[i].dist(points[j]) <= max_dist {
                let ri = find(&labels, i);
                let rj = find(&labels, j);
                if ri != rj {
                    labels[rj] = ri;
                }
            }
        }
    }

    let mut groups: Vec<Vec<P2>> = Vec::new();
    let mut map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, &pt) in points.iter().enumerate() {
        let root = find(&labels, i);
        let idx = *map.entry(root).or_insert_with(|| {
            let idx = groups.len();
            groups.push(Vec::new());
            idx
        });
        groups[idx].push(pt);
    }
    groups
}

/// Union-find `find` with path compression.
fn find(labels: &[usize], x: usize) -> usize {
    if labels[x] == x {
        x
    } else {
        find(labels, labels[x])
    }
}

/// Compute the centroid of a set of 2D points.
fn centroid_of(points: &[P2]) -> P2 {
    let n = points.len() as f64;
    if n == 0.0 {
        return P2::new(0.0, 0.0);
    }
    let sx: f64 = points.iter().map(|p| p.x).sum();
    let sy: f64 = points.iter().map(|p| p.y).sum();
    P2::new(sx / n, sy / n)
}

/// Compute the "medoid" of a set of 2D points: the actual sample point
/// closest to the arithmetic centroid.
///
/// An overhang region that wraps all the way around the model (e.g. the rim
/// of a wide mushroom cap overhanging a narrower post) is an annulus, not a
/// convex blob. Its arithmetic centroid falls in the ring's hole — a point
/// that's inside the solid model rather than in the free space a support
/// trunk needs to occupy — so a trunk anchored there gets skipped at every
/// layer as "already inside the part" and no support is ever printed.
/// Snapping to the nearest real sample point guarantees the anchor is always
/// a point that was actually flagged as unsupported.
fn medoid_of(points: &[P2]) -> P2 {
    let c = centroid_of(points);
    points
        .iter()
        .copied()
        .min_by(|a, b| a.dist(c).partial_cmp(&b.dist(c)).unwrap())
        .unwrap_or(c)
}

/// Compute the maximum pairwise distance (diameter) of a point cloud.
fn point_cloud_diameter(points: &[P2]) -> f64 {
    let mut max_d = 0.0;
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            let d = points[i].dist(points[j]);
            if d > max_d {
                max_d = d;
            }
        }
    }
    max_d
}

/// Sample an XY grid inside the bounding box of a layer.
fn sample_grid_for_layer(layer: &Layer, spacing: f64) -> Option<Vec<P2>> {
    let ((minx, miny), (maxx, maxy)) = layer.bbox()?;
    let mut pts = Vec::new();
    let mut x = minx + spacing / 2.0;
    while x <= maxx {
        let mut y = miny + spacing / 2.0;
        while y <= maxy {
            pts.push(P2::new(x, y));
            y += spacing;
        }
        x += spacing;
    }
    Some(pts)
}

fn point_in_any(contours: &[Contour], p: P2) -> bool {
    contours.iter().any(|c| point_in_polygon(c, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn box_solid(cx: f64, cy: f64, z0: f64, z1: f64, half: f64) -> KernSolid {
        let mut s = KernSolid::new();
        let (x0, y0) = (cx - half, cy - half);
        let (x1, y1) = (cx + half, cy + half);
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(x0, y0, z0),
            v(x1, y0, z0),
            v(x1, y1, z0),
            v(x0, y1, z0),
            v(x0, y0, z1),
            v(x1, y0, z1),
            v(x1, y1, z1),
            v(x0, y1, z1),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]);
        f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]);
        f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]);
        f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]);
        f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]);
        f(p[3], p[7], p[4]);
        s
    }

    fn mushroom() -> Vec<Layer> {
        let mut post = box_solid(0.0, 0.0, 0.0, 2.0, 2.0);
        let cap = box_solid(0.0, 0.0, 2.0, 4.0, 5.0);
        let base = post.vertices.len() as u32;
        post.vertices.extend(cap.vertices);
        post.faces.extend(
            cap.faces
                .iter()
                .map(|f| Face::new(f.a + base, f.b + base, f.c + base)),
        );
        crate::layers::slice_solid(&post, 0.0, 4.0, 0.2, 0.2)
    }

    #[test]
    fn tree_supports_generate_paths_for_mushroom() {
        let layers = mushroom();
        let settings = TreeSupportSettings::default();
        let supports = generate_tree_supports(&layers, &settings);
        assert_eq!(supports.len(), layers.len());
        assert!(
            supports.iter().any(|l| !l.paths.is_empty()),
            "expected at least one layer with tree support paths"
        );
    }

    #[test]
    fn medoid_of_ring_avoids_the_hole() {
        // A ring of points around the origin: the arithmetic centroid is
        // (0, 0) — inside the hole, not one of the sampled points — but the
        // medoid must be an actual member of the set.
        let ring = vec![
            P2::new(4.0, 0.0),
            P2::new(-4.0, 0.0),
            P2::new(0.0, 4.0),
            P2::new(0.0, -4.0),
        ];
        let m = medoid_of(&ring);
        assert!(ring
            .iter()
            .any(|p| (p.x - m.x).abs() < 1e-9 && (p.y - m.y).abs() < 1e-9));
    }

    #[test]
    fn no_tree_supports_for_plain_box() {
        let s = box_solid(0.0, 0.0, 0.0, 4.0, 3.0);
        let layers = crate::layers::slice_solid(&s, 0.0, 4.0, 0.2, 0.2);
        let supports = generate_tree_supports(&layers, &TreeSupportSettings::default());
        assert!(
            supports.iter().all(|l| l.paths.is_empty()),
            "expected no tree supports for a simple box"
        );
    }

    #[test]
    fn cluster_points_groups_nearby() {
        let pts = vec![P2::new(0.0, 0.0), P2::new(0.5, 0.0), P2::new(10.0, 10.0)];
        let groups = cluster_points(&pts, 2.0);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn circle_path_produces_closed_loop() {
        let path = circle_path(P2::new(0.0, 0.0), 1.0, 8);
        assert!(path.closed);
        assert_eq!(path.points.len(), 9); // 8 segments + closing point
    }

    #[test]
    fn point_cloud_diameter_basic() {
        let pts = vec![P2::new(0.0, 0.0), P2::new(3.0, 4.0)];
        assert!((point_cloud_diameter(&pts) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn catmull_rom_interpolates_between_p1_and_p2() {
        let p0 = P2::new(0.0, 0.0);
        let p1 = P2::new(1.0, 0.0);
        let p2 = P2::new(2.0, 0.0);
        let p3 = P2::new(3.0, 0.0);
        let mid = catmull_rom_point(p0, p1, p2, p3, 0.5, 0.5);
        assert!((mid.x - 1.5).abs() < 0.01);
        assert!((mid.y).abs() < 0.01);
    }

    #[test]
    fn smooth_trunk_adds_more_points() {
        let mut trunk = TreeTrunk {
            points: vec![
                P2::new(0.0, 0.0),
                P2::new(1.0, 0.5),
                P2::new(2.0, 0.0),
                P2::new(3.0, 0.5),
                P2::new(4.0, 0.0),
            ],
            radii: vec![1.5, 1.2, 1.0, 0.7, 0.4],
        };
        let orig_len = trunk.points.len();
        smooth_trunk_path(&mut trunk);
        assert!(
            trunk.points.len() > orig_len,
            "smoothed path should have more points"
        );
    }

    #[test]
    fn branching_fork_creates_additional_trunk() {
        // Create a wide mushroom with two separated overhang regions
        // to trigger branching.
        let mut post = box_solid(-8.0, 0.0, 0.0, 2.0, 2.0);
        let cap1 = box_solid(-8.0, 0.0, 2.0, 4.0, 5.0);
        let base = post.vertices.len() as u32;
        post.vertices.extend(cap1.vertices);
        post.faces.extend(
            cap1.faces
                .iter()
                .map(|f| Face::new(f.a + base, f.b + base, f.c + base)),
        );
        let base2 = post.vertices.len() as u32;
        let cap2 = box_solid(8.0, 0.0, 2.0, 4.0, 5.0);
        post.vertices.extend(cap2.vertices);
        post.faces.extend(
            cap2.faces
                .iter()
                .map(|f| Face::new(f.a + base2, f.b + base2, f.c + base2)),
        );
        let layers = crate::layers::slice_solid(&post, 0.0, 4.0, 0.2, 0.2);
        let settings = TreeSupportSettings {
            branch_split_distance: 5.0, // Low threshold to force branching
            ..TreeSupportSettings::default()
        };
        let supports = generate_tree_supports(&layers, &settings);
        assert_eq!(supports.len(), layers.len());
        assert!(
            supports.iter().any(|l| !l.paths.is_empty()),
            "expected support paths for wide mushroom"
        );
    }
}
