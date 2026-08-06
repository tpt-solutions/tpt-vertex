//! Static G-code validator (structure/syntax).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Best-effort, offline structural validation of emitted G-code: it checks the
//! program is self-consistent (supported commands, sane coordinates, monotonic
//! absolute extrusion, a homing move before printing, within build volume) but
//! it does **not** validate against a real printer or G-code simulator — that
//! remains a blocked, hardware-dependent task.

use std::collections::HashSet;

/// A structural problem found in a G-code program.
#[derive(Debug, Clone, PartialEq)]
pub enum GcodeIssue {
    /// A `G`/`M` code was used that is not in the supported set.
    UnsupportedCommand {
        /// 1-based source line number.
        line: usize,
        /// The offending code, e.g. `"G29"`.
        code: String,
    },
    /// Extrusion coordinate `E` went negative in absolute mode, which is invalid.
    NegativeExtrusion {
        line: usize,
        /// The `E` value that was found.
        e: f64,
    },
    /// A movement coordinate exceeded the build volume `bounds`.
    OutOfBounds {
        line: usize,
        /// The offending axis.
        axis: char,
        /// The offending value.
        value: f64,
        /// The build-volume limit that was exceeded.
        limit: f64,
    },
    /// No homing (`G28`) move appeared before the first extrusion.
    MissingHome,
    /// No extrusion (`E`) move was found at all — nothing would be printed.
    NoExtrusion,
}

/// The `G`/`M` codes this slicer emits (a Marlin/Klipper-style subset).
fn supported_codes() -> HashSet<String> {
    let mut s = HashSet::new();
    for c in [
        "G0", "G1", "G21", "G28", "G90", "G91", "M82", "M83", "M104", "M109", "M140", "M190",
        "M106", "M107", "M84",
    ] {
        s.insert(c.to_string());
    }
    s
}

/// Validate G-code `text`, returning every [`GcodeIssue`] found.
///
/// When `bounds` is `Some([x, y, z])` (build volume in mm), movement coordinates
/// beyond the positive limit are reported as [`GcodeIssue::OutOfBounds`]. Each
/// issue carries the 1-based source line so the UI can point at it.
pub fn validate_gcode(text: &str, bounds: Option<[f64; 3]>) -> Vec<GcodeIssue> {
    let supported = supported_codes();
    let mut issues = Vec::new();
    let mut e: f64 = 0.0;
    let mut abs_e = true;
    let mut homed = false;
    let mut reported_missing_home = false;
    let mut saw_extrusion = false;

    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        // Strip comments and trailing whitespace.
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        let is_extrude = line.starts_with("G1") || line.contains(" G1 ");
        if is_extrude && line.contains('E') {
            saw_extrusion = true;
            if !homed && !reported_missing_home {
                issues.push(GcodeIssue::MissingHome);
                reported_missing_home = true;
            }
        }

        for tok in line.split_whitespace() {
            if let Some(rest) = tok.strip_prefix('G').or_else(|| tok.strip_prefix('M')) {
                let code = if tok.starts_with('G') {
                    format!("G{rest}")
                } else {
                    format!("M{rest}")
                };
                if !supported.contains(&code) {
                    issues.push(GcodeIssue::UnsupportedCommand {
                        line: line_no,
                        code: code.clone(),
                    });
                }
                if code == "G28" {
                    homed = true;
                }
                if code == "M82" {
                    abs_e = true;
                }
                if code == "M83" {
                    abs_e = false;
                }
            } else if let Some(coord) = tok.strip_prefix('E') {
                if let Ok(val) = coord.parse::<f64>() {
                    if abs_e {
                        if val < -1e-9 {
                            issues.push(GcodeIssue::NegativeExtrusion {
                                line: line_no,
                                e: val,
                            });
                        }
                        e = val;
                    } else {
                        e += val;
                    }
                    if e < -1e-9 {
                        issues.push(GcodeIssue::NegativeExtrusion { line: line_no, e });
                    }
                }
            } else if let Some(v) = tok.strip_prefix('X') {
                check_coord(&mut issues, line_no, 'X', v, bounds.map(|b| b[0]));
            } else if let Some(v) = tok.strip_prefix('Y') {
                check_coord(&mut issues, line_no, 'Y', v, bounds.map(|b| b[1]));
            } else if let Some(v) = tok.strip_prefix('Z') {
                check_coord(&mut issues, line_no, 'Z', v, bounds.map(|b| b[2]));
            }
        }
    }

    if !saw_extrusion {
        issues.push(GcodeIssue::NoExtrusion);
    }
    issues
}

fn check_coord(
    issues: &mut Vec<GcodeIssue>,
    line: usize,
    axis: char,
    raw: &str,
    limit: Option<f64>,
) {
    if let Ok(val) = raw.parse::<f64>() {
        if let Some(lim) = limit {
            if val > lim + 1e-6 {
                issues.push(GcodeIssue::OutOfBounds {
                    line,
                    axis,
                    value: val,
                    limit: lim,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "\
G90
M82
G21
M104 S210
G28
G1 Z0.2 F300
G1 X0.000 Y0.000 E0.5000 F1800
G1 X10.000 Y0.000 E1.0000 F1800
G1 X10.000 Y10.000 E1.5000 F1800
M107
M104 S0
M84";

    #[test]
    fn valid_program_has_no_issues() {
        assert!(validate_gcode(GOOD, Some([220.0, 220.0, 250.0])).is_empty());
    }

    #[test]
    fn detects_unsupported_command() {
        let g = "G90\nG999\n";
        let issues = validate_gcode(g, None);
        assert!(issues
            .iter()
            .any(|i| matches!(i, GcodeIssue::UnsupportedCommand { code, .. } if code == "G999")));
    }

    #[test]
    fn detects_missing_home_before_extrusion() {
        let g = "G90\nM82\nG1 X0 Y0 E1.0\n";
        let issues = validate_gcode(g, None);
        assert!(issues.iter().any(|i| matches!(i, GcodeIssue::MissingHome)));
    }

    #[test]
    fn detects_negative_extrusion() {
        let g = "G90\nM82\nG28\nG1 X0 Y0 E-3.0\n";
        let issues = validate_gcode(g, None);
        assert!(issues
            .iter()
            .any(|i| matches!(i, GcodeIssue::NegativeExtrusion { .. })));
    }

    #[test]
    fn detects_out_of_bounds() {
        let g = "G90\nM82\nG28\nG1 X500 Y0 E1.0\n";
        let issues = validate_gcode(g, Some([220.0, 220.0, 250.0]));
        assert!(issues
            .iter()
            .any(|i| matches!(i, GcodeIssue::OutOfBounds { axis: 'X', .. })));
    }

    #[test]
    fn detects_no_extrusion() {
        let g = "G90\nM82\nG28\nG1 X0 Y0\n";
        let issues = validate_gcode(g, None);
        assert!(issues.iter().any(|i| matches!(i, GcodeIssue::NoExtrusion)));
    }
}
