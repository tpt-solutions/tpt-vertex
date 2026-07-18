//! Scene graph: nodes, transforms, and hierarchy.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use glam::Mat4;
use vertex_kernel::assembly::Assembly;
use vertex_kernel::math::Transform as KernelTransform;

/// A node in the scene graph. Each node carries a local transform and
/// optionally a reference (by index) to a mesh in the scene's mesh list.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub local: Mat4,
    /// Index into the scene's mesh list, if this node draws geometry.
    pub mesh_index: Option<usize>,
    pub material: MaterialId,
    pub children: Vec<Node>,
    /// Stable id used for picking/selection.
    pub id: u64,
}

/// Opaque material identifier.
pub type MaterialId = u32;

impl Node {
    pub fn new(name: impl Into<String>, id: u64) -> Self {
        Node {
            name: name.into(),
            local: Mat4::IDENTITY,
            mesh_index: None,
            material: 0,
            children: Vec::new(),
            id,
        }
    }

    /// Convert a kernel rigid [`KernelTransform`] into a 4x4 column-major
    /// `glam` matrix (matching the kernel's `Transform::to_mat4`).
    pub fn from_kernel_transform(t: KernelTransform) -> Mat4 {
        let r = t.rotation.to_mat3();
        let tr = t.translation;
        Mat4::from_cols_array(&[
            r.cols[0].x as f32, r.cols[0].y as f32, r.cols[0].z as f32, 0.0,
            r.cols[1].x as f32, r.cols[1].y as f32, r.cols[1].z as f32, 0.0,
            r.cols[2].x as f32, r.cols[2].y as f32, r.cols[2].z as f32, 0.0,
            tr.x as f32, tr.y as f32, tr.z as f32, 1.0,
        ])
    }
}

/// A flat, render-ready draw call produced by flattening a scene.
#[derive(Debug, Clone)]
pub struct DrawItem {
    pub node_id: u64,
    pub world: Mat4,
    pub mesh_index: usize,
    pub material: MaterialId,
}

/// A scene: a root node plus a shared mesh list and flattened draw items.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub root: Node,
    pub meshes: Vec<super::mesh::Mesh>,
    pub draw_items: Vec<DrawItem>,
}

impl Default for Node {
    fn default() -> Self {
        Node {
            name: String::new(),
            local: Mat4::IDENTITY,
            mesh_index: None,
            material: 0,
            children: Vec::new(),
            id: 0,
        }
    }
}

impl Scene {
    pub fn new() -> Self {
        Scene::default()
    }

    /// Flatten the graph into draw items (depth-first), resolving world
    /// matrices against `self.meshes`.
    pub fn flatten(&mut self) {
        self.draw_items.clear();
        flatten_node(&self.root, Mat4::IDENTITY, &mut self.draw_items);
    }
}

fn flatten_node(node: &Node, parent_world: Mat4, out: &mut Vec<DrawItem>) {
    let world = parent_world * node.local;
    if let Some(idx) = node.mesh_index {
        out.push(DrawItem {
            node_id: node.id,
            world,
            mesh_index: idx,
            material: node.material,
        });
    }
    for child in &node.children {
        flatten_node(child, world, out);
    }
}

/// Build a scene from a kernel [`Assembly`], collecting all part meshes.
pub fn scene_from_assembly(asm: &Assembly) -> Scene {
    let mut scene = Scene::new();
    for (part_id, part) in asm.parts() {
        let solid = part.solid_in_assembly();
        let mesh = super::mesh::mesh_from_solid(&solid);
        let idx = scene.meshes.len();
        scene.meshes.push(mesh);
        let mut node = Node::new(&part.name, part_id.0);
        node.local = Node::from_kernel_transform(part.transform);
        node.mesh_index = Some(idx);
        node.material = 0;
        scene.root.children.push(node);
    }
    scene.flatten();
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use vertex_kernel::assembly::{Assembly, Part};
    use vertex_kernel::feature_tree::{Feature, FeatureTree};
    use vertex_kernel::geometry::sketch::Sketch;
    use vertex_kernel::math::Vec2;

    #[test]
    fn scene_from_assembly_flattens() {
        let mut tree = FeatureTree::new();
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(1.0, 0.0));
        s.line(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0));
        s.line(Vec2::new(1.0, 1.0), Vec2::ZERO);
        tree.add(Feature::Extrude { sketch: s, height: 1.0 }, None);
        let mut asm = Assembly::new();
        asm.add_part(Part::new("part", tree));
        let scene = scene_from_assembly(&asm);
        assert_eq!(scene.draw_items.len(), 1);
        assert!(!scene.meshes.is_empty());
    }
}
