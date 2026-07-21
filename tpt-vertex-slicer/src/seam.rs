//! Seam placement: choose where each perimeter loop starts/ends, instead of
//! always using the arbitrary first point contour-stitching happened to
//! produce.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::layers::{Contour, P2};

/// Strategy for choosing a perimeter loop's start (and, since it's closed,
/// end) point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeamMode {
    /// Start the loop at the point nearest a fixed XY location (e.g. a back
    /// corner of the bed), so seams tend to line up on one side of the print.
    NearestTo(P2),
    /// Start the loop at its sharpest interior vertex (the largest direction
    /// change between consecutive edges), where a seam blemish is least
    /// visible on typical printed geometry.
    SharpestCorner,
}

/// Rotate `contour`'s point order so the seam start matches `mode`. Winding
/// and geometry are unchanged — only which point is listed first (and thus
/// where the extruder starts/stops the loop).
pub fn place_seam(contour: &Contour, mode: SeamMode) -> Contour {
    let n = contour.points.len();
    if n < 3 {
        return contour.clone();
    }

    let start = match mode {
        SeamMode::NearestTo(target) => {
            let mut best = 0;
            let mut best_d = f64::INFINITY;
            for (i, &p) in contour.points.iter().enumerate() {
                let d = p.dist(target);
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            best
        }
        SeamMode::SharpestCorner => {
            let mut best = 0;
            let mut best_turn = -1.0;
            for i in 0..n {
                let prev = contour.points[(i + n - 1) % n];
                let cur = contour.points[i];
                let next = contour.points[(i + 1) % n];
                let v1 = (cur.x - prev.x, cur.y - prev.y);
                let v2 = (next.x - cur.x, next.y - cur.y);
                let l1 = (v1.0 * v1.0 + v1.1 * v1.1).sqrt();
                let l2 = (v2.0 * v2.0 + v2.1 * v2.1).sqrt();
                if l1 < 1e-9 || l2 < 1e-9 {
                    continue;
                }
                let cos = ((v1.0 * v2.0 + v1.1 * v2.1) / (l1 * l2)).clamp(-1.0, 1.0);
                let turn = cos.acos(); // 0 = straight through, larger = sharper turn
                if turn > best_turn {
                    best_turn = turn;
                    best = i;
                }
            }
            best
        }
    };

    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        pts.push(contour.points[(start + i) % n]);
    }
    Contour { points: pts }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_to_picks_the_closest_point() {
        let c = Contour {
            points: vec![
                P2::new(0.0, 0.0),
                P2::new(10.0, 0.0),
                P2::new(10.0, 10.0),
                P2::new(0.0, 10.0),
            ],
        };
        let seamed = place_seam(&c, SeamMode::NearestTo(P2::new(9.0, 9.0)));
        assert_eq!(seamed.points[0], P2::new(10.0, 10.0));
    }

    #[test]
    fn sharpest_corner_picks_the_spike() {
        // A square with a tall, thin needle-like spike replacing part of the
        // top edge: its apex turns almost 180°, far sharper than the
        // square's own 90° corners.
        let c = Contour {
            points: vec![
                P2::new(0.0, 0.0),
                P2::new(10.0, 0.0),
                P2::new(10.0, 10.0),
                P2::new(5.0, 100.0), // needle-tip spike
                P2::new(0.0, 10.0),
            ],
        };
        let seamed = place_seam(&c, SeamMode::SharpestCorner);
        assert_eq!(seamed.points[0], P2::new(5.0, 100.0));
    }

    #[test]
    fn short_contour_is_unchanged() {
        let c = Contour {
            points: vec![P2::new(0.0, 0.0), P2::new(1.0, 0.0)],
        };
        let seamed = place_seam(&c, SeamMode::SharpestCorner);
        assert_eq!(seamed, c);
    }
}
