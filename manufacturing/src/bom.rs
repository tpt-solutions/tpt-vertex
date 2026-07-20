//! Bill-of-materials generation from [`tpt_vertex_kernel::assembly::Assembly`].
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use tpt_vertex_kernel::assembly::Assembly;

/// A single BOM line item.
#[derive(Debug, Clone, PartialEq)]
pub struct BomEntry {
    pub part_id: u64,
    pub name: String,
    /// Material label; defaults to "Unspecified" when unknown.
    pub material: String,
    /// Solid volume in mm³ (kernel units are treated as millimetres).
    pub volume_mm3: f64,
    /// Estimated mass in grams (volume × density).
    pub mass_g: f64,
}

/// A complete bill of materials report.
#[derive(Debug, Clone, Default)]
pub struct BomReport {
    pub entries: Vec<BomEntry>,
    pub total_mass_g: f64,
}

impl BomReport {
    /// Summarize the report to a Markdown table.
    pub fn to_markdown(&self) -> String {
        let mut s = String::from("| Part | Material | Volume (mm³) | Mass (g) |\n");
        s.push_str("|------|----------|-------------|----------|\n");
        for e in &self.entries {
            s.push_str(&format!(
                "| {} | {} | {:.2} | {:.2} |\n",
                e.name, e.material, e.volume_mm3, e.mass_g
            ));
        }
        s.push_str(&format!(
            "| **Total** | | | **{:.2}** |\n",
            self.total_mass_g
        ));
        s
    }
}

/// Look up a material density (g/mm³) from the kernel's shared material table
/// (`tpt_vertex_kernel::material::Material::table`), defaulting to a generic
/// engineering plastic. The kernel table is now the single source of truth for
/// densities (folded in from the former local density table).
pub fn density_of(material: &str) -> f64 {
    tpt_vertex_kernel::material::Material::from_name(material).density
}

/// Generate a BOM from an assembly. `materials` maps part id -> material name;
/// missing entries fall back to "Unspecified" (generic plastic density).
pub fn bom_from_assembly(asm: &Assembly, materials: &BTreeMap<u64, String>) -> BomReport {
    let mut report = BomReport::default();
    for (part_id, part) in asm.parts() {
        let solid = part.solid_in_assembly();
        let volume = solid.volume().abs();
        let material = materials
            .get(&part_id.0)
            .cloned()
            .unwrap_or_else(|| "Unspecified".to_string());
        let mass = volume * density_of(&material);
        report.entries.push(BomEntry {
            part_id: part_id.0,
            name: part.name.clone(),
            material,
            volume_mm3: volume,
            mass_g: mass,
        });
        report.total_mass_g += mass;
    }
    report
}

/// Convenience: BOM with all parts defaulting to unspecified material.
pub fn bom_simple(asm: &Assembly) -> BomReport {
    bom_from_assembly(asm, &BTreeMap::new())
}
