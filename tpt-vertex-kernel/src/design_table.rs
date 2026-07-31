//! Design tables / configurations: same model, multiple parameter sets.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`DesignTable`] is a named collection of [`Configuration`]s.  Each
//! configuration maps string parameter names to concrete values.  Activating a
//! configuration applies its parameter overrides to a [`FeatureTree`], allowing
//! a single parametric model to drive many physical variants (size, material,
//! etc.) without duplicating the feature graph.

use crate::feature_tree::{EvalError, Feature, FeatureId, FeatureTree};
use std::collections::HashMap;

/// A single parameter value inside a configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
}

impl ParamValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParamValue::Float(v) => Some(*v),
            ParamValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ParamValue::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// One named configuration: a set of parameter overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// Human-friendly name (e.g. "Large", "Small", "Metric").
    pub name: String,
    /// Parameter name → value overrides.
    pub params: HashMap<String, ParamValue>,
}

impl Configuration {
    pub fn new(name: impl Into<String>) -> Self {
        Configuration {
            name: name.into(),
            params: HashMap::new(),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: ParamValue) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    pub fn with_float(self, key: impl Into<String>, value: f64) -> Self {
        self.with_param(key, ParamValue::Float(value))
    }
}

/// A design table holding multiple named configurations for one model.
#[derive(Debug, Clone, Default)]
pub struct DesignTable {
    configurations: Vec<Configuration>,
    /// Index of the currently active configuration, if any.
    active: Option<usize>,
}

impl DesignTable {
    pub fn new() -> Self {
        DesignTable::default()
    }

    /// Add a configuration and return its index.
    pub fn add(&mut self, config: Configuration) -> usize {
        let idx = self.configurations.len();
        self.configurations.push(config);
        idx
    }

    /// Remove a configuration by index.  If the active config was removed the
    /// active selection is cleared.
    pub fn remove(&mut self, idx: usize) -> Option<Configuration> {
        if idx < self.configurations.len() {
            let removed = self.configurations.remove(idx);
            if self.active == Some(idx) {
                self.active = None;
            } else if let Some(a) = self.active {
                if a > idx {
                    self.active = Some(a - 1);
                }
            }
            Some(removed)
        } else {
            None
        }
    }

    pub fn get(&self, idx: usize) -> Option<&Configuration> {
        self.configurations.get(idx)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Configuration> {
        self.configurations.get_mut(idx)
    }

    pub fn configurations(&self) -> &[Configuration] {
        &self.configurations
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active(&self) -> Option<&Configuration> {
        self.active.and_then(|i| self.configurations.get(i))
    }

    /// Set the active configuration by index.
    pub fn set_active(&mut self, idx: usize) -> bool {
        if idx < self.configurations.len() {
            self.active = Some(idx);
            true
        } else {
            false
        }
    }

    /// Clear the active configuration (revert to defaults).
    pub fn clear_active(&mut self) {
        self.active = None;
    }

    /// Apply the active configuration's overrides to a feature tree.  Each
    /// parameter name in the configuration is matched against feature nodes
    /// by a naming convention: `"feature_id.param"` where `feature_id` is the
    /// decimal representation of a [`FeatureId`] and `param` is the parameter
    /// name (e.g. `"0.height"` for the height of feature 0).
    ///
    /// Returns the number of parameters applied, or an error if the tree
    /// cannot be modified.
    pub fn apply_active(&self, tree: &mut FeatureTree) -> Result<usize, EvalError> {
        let config = match self.active() {
            Some(c) => c,
            None => return Ok(0),
        };
        let mut applied = 0;
        for (key, value) in &config.params {
            if let Some((id, param)) = parse_param_key(key) {
                if let Some(feature) = tree.get(id) {
                    let mut updated = feature.clone();
                    apply_param(&mut updated, param, value);
                    tree.update(id, updated);
                    applied += 1;
                }
            }
        }
        Ok(applied)
    }

    /// Extract the current parameter values from a feature tree for the given
    /// feature ids, using the same `"feature_id.param"` naming convention.
    pub fn extract_params(
        tree: &FeatureTree,
        feature_ids: &[FeatureId],
    ) -> HashMap<String, ParamValue> {
        let mut params = HashMap::new();
        for &fid in feature_ids {
            if let Some(feature) = tree.get(fid) {
                extract_feature_params(fid, feature, &mut params);
            }
        }
        params
    }

    /// Export the design table to CSV format.
    ///
    /// The CSV has:
    /// - First column: `Configuration`
    /// - Remaining columns: one per parameter key (sorted alphabetically)
    /// - Each row: a configuration's name followed by its parameter values
    pub fn to_csv(&self) -> String {
        // Collect all parameter keys across all configurations.
        let mut all_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for config in &self.configurations {
            for key in config.params.keys() {
                all_keys.insert(key.clone());
            }
        }
        let keys: Vec<String> = all_keys.into_iter().collect();

        let mut csv = String::from("Configuration");
        for key in &keys {
            csv.push(',');
            csv.push_str(key);
        }
        csv.push('\n');

        for config in &self.configurations {
            csv.push_str(&config.name);
            for key in &keys {
                csv.push(',');
                if let Some(val) = config.params.get(key) {
                    match val {
                        ParamValue::Float(f) => csv.push_str(&format!("{f}")),
                        ParamValue::Int(i) => csv.push_str(&format!("{i}")),
                        ParamValue::Bool(b) => csv.push_str(if *b { "true" } else { "false" }),
                        ParamValue::Text(s) => {
                            // Escape commas and quotes in text values.
                            if s.contains(',') || s.contains('"') {
                                csv.push('"');
                                csv.push_str(&s.replace('"', "\"\""));
                                csv.push('"');
                            } else {
                                csv.push_str(s);
                            }
                        }
                    }
                }
            }
            csv.push('\n');
        }

        csv
    }

    /// Import a design table from CSV format.
    ///
    /// Expects the same format as [`to_csv`]: first column is the configuration
    /// name, remaining columns are parameter keys.
    pub fn from_csv(csv: &str) -> Result<Self, String> {
        let mut lines = csv.lines();
        let header = lines.next().ok_or("empty CSV")?;
        let keys: Vec<String> = header
            .split(',')
            .skip(1) // Skip "Configuration" column
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if keys.is_empty() {
            return Err("no parameter columns found in CSV header".to_string());
        }

        let mut table = DesignTable::new();

        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let values: Vec<&str> = parse_csv_line(line);
            if values.is_empty() {
                continue;
            }
            let name = values[0].trim().to_string();
            let mut config = Configuration::new(name);

            for (i, key) in keys.iter().enumerate() {
                if i + 1 < values.len() {
                    let raw = values[i + 1].trim();
                    if raw.is_empty() {
                        continue;
                    }
                    // Try to parse as f64 first, then as bool, then as text.
                    if let Ok(f) = raw.parse::<f64>() {
                        config.params.insert(key.clone(), ParamValue::Float(f));
                    } else if raw.eq_ignore_ascii_case("true") {
                        config.params.insert(key.clone(), ParamValue::Bool(true));
                    } else if raw.eq_ignore_ascii_case("false") {
                        config.params.insert(key.clone(), ParamValue::Bool(false));
                    } else {
                        config
                            .params
                            .insert(key.clone(), ParamValue::Text(raw.to_string()));
                    }
                }
            }

            table.add(config);
        }

        Ok(table)
    }
}

/// Parse a CSV line respecting quoted fields.
fn parse_csv_line(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;

    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                let field = &line[start..i];
                fields.push(strip_quotes(field));
                start = i + 1;
            }
            _ => {}
        }
    }
    fields.push(strip_quotes(&line[start..]));
    fields
}

/// Strip leading/trailing double-quotes from a CSV field value.
fn strip_quotes(field: &str) -> &str {
    if field.starts_with('"') && field.ends_with('"') && field.len() >= 2 {
        &field[1..field.len() - 1]
    } else {
        field
    }
}

/// Parse a `"feature_id.param"` key into the feature id and param name.
fn parse_param_key(key: &str) -> Option<(FeatureId, &str)> {
    let (id_str, param) = key.split_once('.')?;
    let id: u64 = id_str.parse().ok()?;
    Some((FeatureId(id), param))
}

/// Apply a parameter value to a feature (mutating in place).
fn apply_param(feature: &mut Feature, param: &str, value: &ParamValue) {
    match feature {
        Feature::Extrude { height, .. } if param == "height" => {
            if let Some(v) = value.as_f64() {
                *height = v;
            }
        }
        Feature::Revolve { angle, .. } if param == "angle" => {
            if let Some(v) = value.as_f64() {
                *angle = v;
            }
        }
        Feature::Fillet { radius, .. } if param == "radius" => {
            if let Some(v) = value.as_f64() {
                *radius = v;
            }
        }
        Feature::Chamfer { distance, .. } if param == "distance" => {
            if let Some(v) = value.as_f64() {
                *distance = v;
            }
        }
        Feature::Loft { height, .. } if param == "height" => {
            if let Some(v) = value.as_f64() {
                *height = v;
            }
        }
        Feature::Transform { translation, .. } if param == "tx" => {
            if let Some(v) = value.as_f64() {
                translation.x = v;
            }
        }
        Feature::Transform { translation, .. } if param == "ty" => {
            if let Some(v) = value.as_f64() {
                translation.y = v;
            }
        }
        Feature::Transform { translation, .. } if param == "tz" => {
            if let Some(v) = value.as_f64() {
                translation.z = v;
            }
        }
        Feature::Transform { rotation, .. } if param == "rx" => {
            if let Some(v) = value.as_f64() {
                rotation.x = v;
            }
        }
        Feature::Transform { rotation, .. } if param == "ry" => {
            if let Some(v) = value.as_f64() {
                rotation.y = v;
            }
        }
        Feature::Transform { rotation, .. } if param == "rz" => {
            if let Some(v) = value.as_f64() {
                rotation.z = v;
            }
        }
        _ => {}
    }
}

/// Extract feature parameters into the param map using the `"id.param"` convention.
fn extract_feature_params(
    fid: FeatureId,
    feature: &Feature,
    out: &mut HashMap<String, ParamValue>,
) {
    let id = fid.0;
    match feature {
        Feature::Extrude { height, .. } => {
            out.insert(format!("{id}.height"), ParamValue::Float(*height));
        }
        Feature::Revolve {
            angle, segments, ..
        } => {
            out.insert(format!("{id}.angle"), ParamValue::Float(*angle));
            out.insert(format!("{id}.segments"), ParamValue::Int(*segments as i64));
        }
        Feature::Fillet { radius, .. } => {
            out.insert(format!("{id}.radius"), ParamValue::Float(*radius));
        }
        Feature::Chamfer { distance, .. } => {
            out.insert(format!("{id}.distance"), ParamValue::Float(*distance));
        }
        Feature::Loft { height, .. } => {
            out.insert(format!("{id}.height"), ParamValue::Float(*height));
        }
        Feature::Transform {
            translation,
            rotation,
            ..
        } => {
            out.insert(format!("{id}.tx"), ParamValue::Float(translation.x));
            out.insert(format!("{id}.ty"), ParamValue::Float(translation.y));
            out.insert(format!("{id}.tz"), ParamValue::Float(translation.z));
            out.insert(format!("{id}.rx"), ParamValue::Float(rotation.x));
            out.insert(format!("{id}.ry"), ParamValue::Float(rotation.y));
            out.insert(format!("{id}.rz"), ParamValue::Float(rotation.z));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::sketch::Sketch;
    use crate::math::Vec2;

    fn box_sketch() -> Sketch {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        s
    }

    #[test]
    fn add_and_activate_configuration() {
        let mut table = DesignTable::new();
        let idx = table.add(Configuration::new("Small").with_float("0.height", 1.0));
        table.add(Configuration::new("Large").with_float("0.height", 5.0));
        assert_eq!(idx, 0);
        assert!(table.active().is_none());

        table.set_active(0);
        assert_eq!(table.active().unwrap().name, "Small");
    }

    #[test]
    fn apply_configuration_updates_tree() {
        let mut tree = FeatureTree::new();
        let id = tree.add(
            Feature::Extrude {
                sketch: box_sketch(),
                height: 2.0,
            },
            None,
        );
        let v1 = tree.evaluate().unwrap().final_solid.volume().abs();

        let mut table = DesignTable::new();
        table.add(Configuration::new("Tall").with_float(format!("{}.height", id.0), 10.0));
        table.set_active(0);
        let applied = table.apply_active(&mut tree).unwrap();
        assert_eq!(applied, 1);

        let v2 = tree.evaluate().unwrap().final_solid.volume().abs();
        // Original: triangle area 2 × height 2 = 4, new: area 2 × height 10 = 20
        assert!((v1 - 4.0).abs() < 1e-6);
        assert!((v2 - 20.0).abs() < 1e-6);
    }

    #[test]
    fn extract_params_round_trips() {
        let mut tree = FeatureTree::new();
        let id = tree.add(
            Feature::Extrude {
                sketch: box_sketch(),
                height: 3.0,
            },
            None,
        );
        let params = DesignTable::extract_params(&tree, &[id]);
        assert_eq!(
            params.get(&format!("{}.height", id.0)),
            Some(&ParamValue::Float(3.0))
        );
    }

    #[test]
    fn remove_active_clears_selection() {
        let mut table = DesignTable::new();
        table.add(Configuration::new("A"));
        table.set_active(0);
        assert!(table.active().is_some());
        table.remove(0);
        assert!(table.active().is_none());
    }

    #[test]
    fn csv_round_trip() {
        let mut table = DesignTable::new();
        table.add(
            Configuration::new("Small")
                .with_float("0.height", 1.0)
                .with_float("0.radius", 0.5),
        );
        table.add(
            Configuration::new("Large")
                .with_float("0.height", 5.0)
                .with_float("0.radius", 2.0),
        );

        let csv = table.to_csv();
        assert!(csv.contains("Configuration,0.height,0.radius"));
        assert!(csv.contains("Small,1,0.5"));
        assert!(csv.contains("Large,5,2"));

        let table2 = DesignTable::from_csv(&csv).unwrap();
        assert_eq!(table2.configurations().len(), 2);
        assert_eq!(table2.configurations()[0].name, "Small");
        assert_eq!(
            table2.configurations()[0].params.get("0.height"),
            Some(&ParamValue::Float(1.0))
        );
        assert_eq!(
            table2.configurations()[1].params.get("0.radius"),
            Some(&ParamValue::Float(2.0))
        );
    }

    #[test]
    fn csv_handles_text_values_with_commas() {
        let mut table = DesignTable::new();
        table.add(
            Configuration::new("Test")
                .with_param("name", ParamValue::Text("hello, world".to_string())),
        );
        let csv = table.to_csv();
        let table2 = DesignTable::from_csv(&csv).unwrap();
        assert_eq!(
            table2.configurations()[0].params.get("name"),
            Some(&ParamValue::Text("hello, world".to_string()))
        );
    }
}
