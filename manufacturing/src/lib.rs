//! Manufacturing & interop for TPT Vertex.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides geometry export to manufacturing/file formats (STL binary + ASCII,
//! Wavefront OBJ, glTF 2.0, STEP AP203/214) and STEP import, driven by the
//! kernel's [`tpt_vertex_kernel::geometry::solid::Solid`], plus bill-of-materials
//! (BOM) generation from [`tpt_vertex_kernel::assembly::Assembly`] and 2D drawing
//! (orthographic SVG) generation.

pub mod bom;
pub mod drawing;
pub mod export;
pub mod plugin;
pub mod step;

pub use bom::{BomEntry, BomReport};
pub use export::{export_gltf, export_obj, write_stl_ascii, write_stl_binary, StlError};
pub use plugin::{
    ExporterPlugin, ImporterPlugin, PluginError, PluginInfo, PluginRegistry, ToolPlugin,
};
pub use step::{export_step, import_step};

#[cfg(test)]
mod tests {
    use crate::bom::{bom_from_assembly, bom_simple};
    use crate::drawing::drawing_svg;
    use crate::export::{export_gltf, export_obj, write_stl_ascii, write_stl_binary};

    use std::collections::BTreeMap;

    use tpt_vertex_kernel::assembly::{Assembly, Part};
    use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::geometry::solid::Solid;
    use tpt_vertex_kernel::math::Vec2;

    fn box_solid() -> Solid {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let mut tree = FeatureTree::new();
        tree.add(
            Feature::Extrude {
                sketch: s,
                height: 3.0,
            },
            None,
        );
        tree.evaluate().unwrap().final_solid
    }

    #[test]
    fn stl_binary_round_trips_triangle_count() {
        let solid = box_solid();
        let mut buf: Vec<u8> = Vec::new();
        write_stl_binary(&mut buf, &solid).unwrap();
        let expected = 80 + 4 + solid.triangle_count() * 50;
        assert_eq!(buf.len(), expected);
    }

    #[test]
    fn stl_ascii_contains_facets() {
        let solid = box_solid();
        let mut buf: Vec<u8> = Vec::new();
        write_stl_ascii(&mut buf, &solid).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.starts_with("solid vertex"));
        assert_eq!(
            text.matches("  facet normal").count(),
            solid.triangle_count()
        );
    }

    #[test]
    fn obj_has_one_face_per_triangle() {
        let solid = box_solid();
        let mut buf: Vec<u8> = Vec::new();
        export_obj(&mut buf, &solid).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.matches("\nf ").count(), solid.triangle_count());
        assert_eq!(text.matches("\nv ").count(), solid.vertex_count());
    }

    #[test]
    fn gltf_has_valid_accessor_counts() {
        let solid = box_solid();
        let (json, bin) = export_gltf(&solid).unwrap();
        assert!(json.contains("\"version\": \"2.0\""));
        assert!(json.contains("\"POSITION\": 0"));
        assert!(bin.len() >= solid.vertices.len() * 12 + solid.faces.len() * 12);
    }

    #[test]
    fn bom_reports_volume_and_mass() {
        let mut tree = FeatureTree::new();
        let s = Sketch::new();
        tree.add(
            Feature::Extrude {
                sketch: s,
                height: 1.0,
            },
            None,
        );
        let mut asm = Assembly::new();
        asm.add_part(Part::new("Block", tree));
        let report = bom_simple(&asm);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.total_mass_g, 0.0);

        let mut materials = BTreeMap::new();
        materials.insert(0, "Steel".to_string());
        let report = bom_from_assembly(&asm, &materials);
        assert_eq!(report.entries[0].material, "Steel");
    }

    #[test]
    fn bom_markdown_has_header_and_total() {
        let mut tree = FeatureTree::new();
        let s = Sketch::new();
        tree.add(
            Feature::Extrude {
                sketch: s,
                height: 1.0,
            },
            None,
        );
        let mut asm = Assembly::new();
        asm.add_part(Part::new("A", tree));
        let md = bom_simple(&asm).to_markdown();
        assert!(md.starts_with("| Part"));
        assert!(md.contains("**Total**"));
    }

    #[test]
    fn drawing_svg_has_three_views() {
        let solid = box_solid();
        let svg = drawing_svg(&solid);
        assert!(svg.starts_with("<svg"));
        assert_eq!(
            svg.matches("class=\"edge\"").count(),
            solid.triangle_count() * 3 * 3
        );
    }
}
