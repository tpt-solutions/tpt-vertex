//! GD&T (Geometric Dimensioning and Tolerancing) annotations.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides data structures for ASME Y14.5 / ISO 1101 style GD&T feature
//! control frames applied to model geometry.  These annotations are stored as
//! metadata alongside the feature tree and rendered in 2D drawing views.

/// The primary GD&T geometric characteristic symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometricCharacteristic {
    Straightness,
    Flatness,
    Circularity,
    Cylindricity,
    ProfileOfALine,
    ProfileOfASurface,
    Angularity,
    Perpendicularity,
    Parallelism,
    Position,
    Concentricity,
    Symmetry,
    CircularRunout,
    TotalRunout,
}

impl GeometricCharacteristic {
    pub fn symbol(&self) -> &'static str {
        match self {
            GeometricCharacteristic::Straightness => "-",
            GeometricCharacteristic::Flatness => "\u{25A1}",
            GeometricCharacteristic::Circularity => "\u{25CB}",
            GeometricCharacteristic::Cylindricity => "\u{25D1}",
            GeometricCharacteristic::ProfileOfALine => "\u{23E1}",
            GeometricCharacteristic::ProfileOfASurface => "\u{23E0}",
            GeometricCharacteristic::Angularity => "\u{2220}",
            GeometricCharacteristic::Perpendicularity => "\u{22A5}",
            GeometricCharacteristic::Parallelism => "\u{2225}",
            GeometricCharacteristic::Position => "\u{2316}",
            GeometricCharacteristic::Concentricity => "\u{25CE}",
            GeometricCharacteristic::Symmetry => "\u{233F}",
            GeometricCharacteristic::CircularRunout => "\u{2197}",
            GeometricCharacteristic::TotalRunout => "\u{219D}",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            GeometricCharacteristic::Straightness => "Straightness",
            GeometricCharacteristic::Flatness => "Flatness",
            GeometricCharacteristic::Circularity => "Circularity",
            GeometricCharacteristic::Cylindricity => "Cylindricity",
            GeometricCharacteristic::ProfileOfALine => "Profile of a Line",
            GeometricCharacteristic::ProfileOfASurface => "Profile of a Surface",
            GeometricCharacteristic::Angularity => "Angularity",
            GeometricCharacteristic::Perpendicularity => "Perpendicularity",
            GeometricCharacteristic::Parallelism => "Parallelism",
            GeometricCharacteristic::Position => "Position",
            GeometricCharacteristic::Concentricity => "Concentricity",
            GeometricCharacteristic::Symmetry => "Symmetry",
            GeometricCharacteristic::CircularRunout => "Circular Runout",
            GeometricCharacteristic::TotalRunout => "Total Runout",
        }
    }
}

/// Material condition modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// Maximum material condition.
    MMC,
    /// Least material condition.
    LMC,
    /// Regardless of feature size.
    RFS,
    /// Tangent plane.
    TP,
}

impl Modifier {
    pub fn symbol(&self) -> &'static str {
        match self {
            Modifier::MMC => "(M)",
            Modifier::LMC => "(L)",
            Modifier::RFS => "",
            Modifier::TP => "(T)",
        }
    }
}

/// A datum reference: a named datum (e.g. "A", "B") optionally with a material
/// condition modifier.
#[derive(Debug, Clone, PartialEq)]
pub struct DatumRef {
    pub label: String,
    pub modifier: Option<Modifier>,
}

/// A single feature control frame (FCF): one row in a GD&T callout.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureControlFrame {
    /// The geometric characteristic being controlled.
    pub characteristic: GeometricCharacteristic,
    /// Tolerance value in mm (or degrees for angularity).
    pub tolerance: f64,
    /// Material condition modifier on the tolerance zone.
    pub tol_modifier: Option<Modifier>,
    /// Datum references (primary, secondary, tertiary).
    pub datums: Vec<DatumRef>,
}

impl FeatureControlFrame {
    pub fn new(characteristic: GeometricCharacteristic, tolerance: f64) -> Self {
        FeatureControlFrame {
            characteristic,
            tolerance,
            tol_modifier: None,
            datums: Vec::new(),
        }
    }

    pub fn with_modifier(mut self, m: Modifier) -> Self {
        self.tol_modifier = Some(m);
        self
    }

    pub fn with_datum(mut self, label: impl Into<String>) -> Self {
        self.datums.push(DatumRef {
            label: label.into(),
            modifier: None,
        });
        self
    }

    /// Render the FCF as a human-readable string (single line).
    pub fn to_string_compact(&self) -> String {
        let tol_mod = self.tol_modifier.map(|m| m.symbol()).unwrap_or("");
        let datums: String = self
            .datums
            .iter()
            .map(|d| format!(" {}", d.label))
            .collect();
        format!(
            "[{} | {:.3}{tol} |{datums}]",
            self.characteristic.symbol(),
            self.tolerance,
            tol = tol_mod,
        )
    }
}

/// A GD&T datum target: a specific point, line, or area on a datum feature.
#[derive(Debug, Clone, PartialEq)]
pub struct DatumTarget {
    /// Datum label (e.g. "A").
    pub datum: String,
    /// Target index (A1, A2, etc.).
    pub index: u32,
    /// Target type.
    pub target_type: DatumTargetType,
}

/// Type of datum target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumTargetType {
    Point,
    Line,
    Area,
}

/// A complete GD&T annotation attached to a model: a list of feature control
/// frames and datum definitions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GdtAnnotation {
    /// Feature control frames applied to this annotation.
    pub frames: Vec<FeatureControlFrame>,
    /// Datum definitions (targets referenced by the FCFs).
    pub datums: Vec<DatumTarget>,
}

impl GdtAnnotation {
    pub fn new() -> Self {
        GdtAnnotation::default()
    }

    pub fn add_frame(mut self, fcf: FeatureControlFrame) -> Self {
        self.frames.push(fcf);
        self
    }

    pub fn add_datum(mut self, datum: DatumTarget) -> Self {
        self.datums.push(datum);
        self
    }

    /// Render all FCFs as a multi-line string.
    pub fn to_string_compact(&self) -> String {
        self.frames
            .iter()
            .map(|f| f.to_string_compact())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatness_fcf_renders() {
        let fcf = FeatureControlFrame::new(GeometricCharacteristic::Flatness, 0.05);
        let s = fcf.to_string_compact();
        assert!(s.contains("Flatness") || s.contains("\u{25A1}"));
        assert!(s.contains("0.050"));
    }

    #[test]
    fn position_with_mmc_and_datums() {
        let fcf = FeatureControlFrame::new(GeometricCharacteristic::Position, 0.1)
            .with_modifier(Modifier::MMC)
            .with_datum("A")
            .with_datum("B");
        let s = fcf.to_string_compact();
        assert!(s.contains("A"));
        assert!(s.contains("B"));
        assert!(s.contains("(M)"));
    }

    #[test]
    fn annotation_compact_string() {
        let ann = GdtAnnotation::new()
            .add_frame(FeatureControlFrame::new(
                GeometricCharacteristic::Flatness,
                0.02,
            ))
            .add_frame(
                FeatureControlFrame::new(GeometricCharacteristic::Position, 0.1).with_datum("A"),
            );
        let s = ann.to_string_compact();
        assert!(s.lines().count() == 2);
    }
}
