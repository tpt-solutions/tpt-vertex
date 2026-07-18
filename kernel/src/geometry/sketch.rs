//! 2D sketch primitives: points, lines, arcs, circles, and polylines.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::math::Vec2;

/// A unique identifier for a sketch vertex, referenced by constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexId(pub u64);

/// A free 2D point in sketch space. Constraints move these around.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub id: VertexId,
    pub pos: Vec2,
}

impl Point {
    pub fn new(id: VertexId, pos: Vec2) -> Self {
        Point { id, pos }
    }
}

/// A straight line segment between two points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub start: VertexId,
    pub end: VertexId,
}

/// A circular arc defined by two endpoints and a center; `ccw` selects the
/// arc direction from `start` to `end` around `center`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    pub start: VertexId,
    pub end: VertexId,
    pub center: VertexId,
    pub ccw: bool,
}

/// A full circle defined by a center point and a radius point on the rim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub center: VertexId,
    pub radius_point: VertexId,
}

/// A polyline (open or closed) defined by an ordered list of point ids.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Polyline {
    pub points: [VertexId; 2],
    pub closed: bool,
}

/// A single sketch entity. Splines are represented as sampled polylines for
/// v1; an exact NURBS spline is a later refinement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SketchEntity {
    Line(Line),
    Arc(Arc),
    Circle(Circle),
    Polyline(Polyline),
}

impl SketchEntity {
    /// All vertex ids referenced by this entity (useful for topological walks).
    pub fn vertices(&self) -> Vec<VertexId> {
        match self {
            SketchEntity::Line(l) => vec![l.start, l.end],
            SketchEntity::Arc(a) => vec![a.start, a.end, a.center],
            SketchEntity::Circle(c) => vec![c.center, c.radius_point],
            SketchEntity::Polyline(p) => p.points.to_vec(),
        }
    }
}

/// A 2D sketch: a named set of points and entities, the input to extrude/revolve.
#[derive(Debug, Clone, Default)]
pub struct Sketch {
    pub points: Vec<Point>,
    pub entities: Vec<SketchEntity>,
}

impl Sketch {
    pub fn new() -> Self {
        Sketch::default()
    }

    pub fn add_point(&mut self, pos: Vec2) -> VertexId {
        let id = VertexId(self.points.len() as u64);
        self.points.push(Point::new(id, pos));
        id
    }

    pub fn add_entity(&mut self, e: SketchEntity) -> usize {
        self.entities.push(e);
        self.entities.len() - 1
    }

    pub fn line(&mut self, start: Vec2, end: Vec2) -> Line {
        let s = self.add_point(start);
        let e = self.add_point(end);
        let l = Line { start: s, end: e };
        self.add_entity(SketchEntity::Line(l));
        l
    }

    pub fn circle(&mut self, center: Vec2, radius: f64) -> Circle {
        let c = self.add_point(center);
        let r = self.add_point(center + Vec2::new(radius, 0.0));
        let circ = Circle {
            center: c,
            radius_point: r,
        };
        self.add_entity(SketchEntity::Circle(circ));
        circ
    }

    pub fn arc(&mut self, start: Vec2, end: Vec2, center: Vec2, ccw: bool) -> Arc {
        let s = self.add_point(start);
        let e = self.add_point(end);
        let c = self.add_point(center);
        let a = Arc {
            start: s,
            end: e,
            center: c,
            ccw,
        };
        self.add_entity(SketchEntity::Arc(a));
        a
    }

    /// Look up a point by id.
    pub fn point(&self, id: VertexId) -> Option<&Point> {
        self.points.iter().find(|p| p.id == id)
    }

    /// Total arc length of the sketch (sum of entity lengths). Approximate for
    /// arcs (chordless exact arc length).
    pub fn length(&self) -> f64 {
        let mut total = 0.0;
        for e in &self.entities {
            match e {
                SketchEntity::Line(l) => {
                    let a = self.point(l.start).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    let b = self.point(l.end).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    total += a.distance(b);
                }
                SketchEntity::Arc(a) => {
                    let s = self.point(a.start).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    let e = self.point(a.end).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    let c = self.point(a.center).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    let r = s.distance(c);
                    let ang = (e - c).angle() - (s - c).angle();
                    let ang = normalize_angle(ang, a.ccw);
                    total += r * ang.abs();
                }
                SketchEntity::Circle(c) => {
                    let center = self.point(c.center).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    let rp = self
                        .point(c.radius_point)
                        .map(|p| p.pos)
                        .unwrap_or(Vec2::ZERO);
                    total += 2.0 * std::f64::consts::PI * center.distance(rp);
                }
                SketchEntity::Polyline(_) => {}
            }
        }
        total
    }

    /// Bounding box of all points.
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        let mut iter = self.points.iter();
        let first = iter.next()?;
        let (mut min, mut max) = (first.pos, first.pos);
        for p in iter {
            min.x = min.x.min(p.pos.x);
            min.y = min.y.min(p.pos.y);
            max.x = max.x.max(p.pos.x);
            max.y = max.y.max(p.pos.y);
        }
        Some((min, max))
    }
}

/// Normalize a signed angle to a positive sweep in the requested direction.
fn normalize_angle(ang: f64, ccw: bool) -> f64 {
    let mut a = ang;
    while a <= -std::f64::consts::PI {
        a += 2.0 * std::f64::consts::PI;
    }
    while a > std::f64::consts::PI {
        a -= 2.0 * std::f64::consts::PI;
    }
    if ccw {
        if a < 0.0 {
            a + 2.0 * std::f64::consts::PI
        } else {
            a
        }
    } else {
        if a > 0.0 {
            a - 2.0 * std::f64::consts::PI
        } else {
            a
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tolerance::EPSILON;

    #[test]
    fn line_length() {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(3.0, 4.0));
        assert!((s.length() - 5.0).abs() < EPSILON);
    }

    #[test]
    fn circle_length() {
        let mut s = Sketch::new();
        s.circle(Vec2::ZERO, 1.0);
        let expected = 2.0 * std::f64::consts::PI;
        assert!((s.length() - expected).abs() < 1e-9);
    }

    #[test]
    fn arc_length_quarter() {
        let mut s = Sketch::new();
        // 90° arc of radius 1 => length = pi/2
        s.arc(Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::ZERO, true);
        assert!((s.length() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn bounds() {
        let mut s = Sketch::new();
        s.line(Vec2::new(-2.0, -1.0), Vec2::new(3.0, 5.0));
        let (min, max) = s.bounds().unwrap();
        assert_eq!(min, Vec2::new(-2.0, -1.0));
        assert_eq!(max, Vec2::new(3.0, 5.0));
    }
}
