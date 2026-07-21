//! Printer-profile preset import/export in a Klipper-`printer.cfg`-style INI
//! format (a common config idiom for both Klipper and hand-edited Marlin
//! setups).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This is a basic key/value bridge, not a full Marlin EEPROM (`M92`/`M203`/…)
//! or Klipper config parser: it round-trips the [`PrinterProfile`] fields this
//! crate actually uses, under conventional Klipper-style section/key names, so
//! a preset can be exported for editing, or a real printer's config fed back
//! in (unrecognised sections/keys are ignored, so the whole file can be
//! passed as-is).

use crate::profile::PrinterProfile;

/// Serialize `printer` as a Klipper-style `printer.cfg` fragment.
pub fn export_preset(printer: &PrinterProfile) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n", printer.name));
    out.push_str("[printer]\n");
    out.push_str(&format!("max_x: {:.3}\n", printer.bed_size[0]));
    out.push_str(&format!("max_y: {:.3}\n", printer.bed_size[1]));
    out.push_str(&format!("max_z: {:.3}\n", printer.bed_size[2]));
    out.push_str(&format!("max_velocity: {:.3}\n", printer.travel_speed));
    out.push('\n');
    out.push_str("[extruder]\n");
    out.push_str(&format!("nozzle_diameter: {:.3}\n", printer.nozzle_diameter));
    out.push_str(&format!("filament_diameter: {:.3}\n", printer.filament_diameter));
    out.push_str(&format!("max_extrude_only_velocity: {:.3}\n", printer.print_speed));
    out.push_str(&format!("temperature: {:.1}\n", printer.nozzle_temperature));
    out.push('\n');
    out.push_str("[heater_bed]\n");
    out.push_str(&format!("temperature: {:.1}\n", printer.bed_temperature));
    out.push('\n');
    out.push_str("[firmware_retraction]\n");
    out.push_str(&format!("retract_length: {:.3}\n", printer.retraction_length));
    out.push_str(&format!("retract_speed: {:.3}\n", printer.retraction_speed));
    out.push_str(&format!("lift_z: {:.3}\n", printer.z_hop));
    out
}

/// Parse a Klipper-style config and apply recognised keys onto a copy of
/// `base` (so unrelated `PrinterProfile` fields — name, flow caps, etc. —
/// carry over unchanged).
pub fn import_preset(text: &str, base: &PrinterProfile) -> PrinterProfile {
    let mut p = base.clone();
    let mut section = String::new();

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[') {
            section = name.trim_end_matches(']').trim().to_lowercase();
            continue;
        }
        let Some((key, value)) = split_kv(line) else {
            continue;
        };
        let Ok(v) = value.trim().parse::<f64>() else {
            continue;
        };
        match (section.as_str(), key.trim()) {
            ("printer", "max_x") => p.bed_size[0] = v,
            ("printer", "max_y") => p.bed_size[1] = v,
            ("printer", "max_z") => p.bed_size[2] = v,
            ("printer", "max_velocity") => p.travel_speed = v,
            ("extruder", "nozzle_diameter") => p.nozzle_diameter = v,
            ("extruder", "filament_diameter") => p.filament_diameter = v,
            ("extruder", "max_extrude_only_velocity") => p.print_speed = v,
            ("extruder", "temperature") => p.nozzle_temperature = v,
            ("heater_bed", "temperature") => p.bed_temperature = v,
            ("firmware_retraction", "retract_length") => p.retraction_length = v,
            ("firmware_retraction", "retract_speed") => p.retraction_speed = v,
            ("firmware_retraction", "lift_z") => p.z_hop = v,
            _ => {}
        }
    }
    p
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    if let Some(idx) = line.find(':') {
        Some((&line[..idx], &line[idx + 1..]))
    } else {
        line.find('=').map(|idx| (&line[..idx], &line[idx + 1..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_then_import_round_trips_known_fields() {
        let printer = PrinterProfile {
            bed_size: [300.0, 300.0, 400.0],
            nozzle_diameter: 0.6,
            nozzle_temperature: 235.0,
            bed_temperature: 70.0,
            travel_speed: 200.0,
            print_speed: 80.0,
            filament_diameter: 2.85,
            retraction_length: 4.5,
            retraction_speed: 35.0,
            z_hop: 0.4,
            ..PrinterProfile::default()
        };
        let text = export_preset(&printer);
        let parsed = import_preset(&text, &PrinterProfile::default());

        assert_eq!(parsed.bed_size, printer.bed_size);
        assert_eq!(parsed.nozzle_diameter, printer.nozzle_diameter);
        assert_eq!(parsed.nozzle_temperature, printer.nozzle_temperature);
        assert_eq!(parsed.bed_temperature, printer.bed_temperature);
        assert_eq!(parsed.travel_speed, printer.travel_speed);
        assert_eq!(parsed.print_speed, printer.print_speed);
        assert_eq!(parsed.filament_diameter, printer.filament_diameter);
        assert_eq!(parsed.retraction_length, printer.retraction_length);
        assert_eq!(parsed.retraction_speed, printer.retraction_speed);
        assert_eq!(parsed.z_hop, printer.z_hop);
    }

    #[test]
    fn unknown_keys_and_sections_are_ignored() {
        let text = "[extruder]\nsome_unknown_key: 42\n[totally_unknown]\nfoo: 1\n";
        let parsed = import_preset(text, &PrinterProfile::default());
        assert_eq!(parsed, PrinterProfile::default());
    }
}
