//! Mesh export to STL (binary + ASCII) and Wavefront OBJ.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;
use std::fmt;
use std::io::{self, Write};

use vertex_kernel::geometry::solid::Solid;

/// Errors surfaced by export routines.
#[derive(Debug)]
pub enum StlError {
    /// Underlying I/O failure while writing the output stream.
    Io(io::Error),
}

impl fmt::Display for StlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StlError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl Error for StlError {}

impl From<io::Error> for StlError {
    fn from(e: io::Error) -> Self {
        StlError::Io(e)
    }
}

/// Write a binary STL (little-endian, 80-byte header, uint32 triangle count).
pub fn write_stl_binary<W: Write>(mut w: W, solid: &Solid) -> Result<(), StlError> {
    let header = [0u8; 80];
    w.write_all(&header)?;
    let tri_count = solid.triangle_count() as u32;
    w.write_all(&tri_count.to_le_bytes())?;

    for f in &solid.faces {
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        let n = face_normal(a, b, c);
        for comp in [n.x, n.y, n.z] {
            w.write_all(&(comp as f32).to_le_bytes())?;
        }
        for p in [a, b, c] {
            for comp in [p.x, p.y, p.z] {
                w.write_all(&(comp as f32).to_le_bytes())?;
            }
        }
        w.write_all(&[0u8; 2])?; // attribute byte count
    }
    Ok(())
}

/// Write an ASCII STL.
pub fn write_stl_ascii<W: Write>(mut w: W, solid: &Solid) -> Result<(), StlError> {
    writeln!(w, "solid vertex")?;
    for f in &solid.faces {
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        let n = face_normal(a, b, c);
        writeln!(w, "  facet normal {} {} {}", n.x, n.y, n.z)?;
        writeln!(w, "    outer loop")?;
        for p in [a, b, c] {
            writeln!(w, "      vertex {} {} {}", p.x, p.y, p.z)?;
        }
        writeln!(w, "    endloop")?;
        writeln!(w, "  endfacet")?;
    }
    writeln!(w, "endsolid vertex")?;
    Ok(())
}

/// Write a Wavefront OBJ (positions + faces; faces are 1-indexed).
pub fn export_obj<W: Write>(mut w: W, solid: &Solid) -> Result<(), StlError> {
    writeln!(w, "# TPT Vertex OBJ export")?;
    for v in &solid.vertices {
        writeln!(w, "v {} {} {}", v.x, v.y, v.z)?;
    }
    for f in &solid.faces {
        // OBJ indices are 1-based.
        writeln!(w, "f {} {} {}", f.a + 1, f.b + 1, f.c + 1)?;
    }
    Ok(())
}

/// Build a minimal glTF 2.0 document (JSON + binary vertex payload) for a solid.
/// Returns `(json, bin)` where `bin` is the padded binary buffer referenced by
/// the JSON. This is sufficient for tooling that consumes standard glTF 2.0.
pub fn export_gltf(solid: &Solid) -> Result<(String, Vec<u8>), StlError> {
    let mut bin: Vec<u8> = Vec::new();
    // POSITION accessor: float32x3 per vertex.
    for v in &solid.vertices {
        bin.extend_from_slice(&(v.x as f32).to_le_bytes());
        bin.extend_from_slice(&(v.y as f32).to_le_bytes());
        bin.extend_from_slice(&(v.z as f32).to_le_bytes());
    }
    let pos_byte_length = (solid.vertices.len() * 12) as u32;
    // INDEX accessor: uint32 per index.
    let index_start = bin.len() as u32;
    for f in &solid.faces {
        bin.extend_from_slice(&f.a.to_le_bytes());
        bin.extend_from_slice(&f.b.to_le_bytes());
        bin.extend_from_slice(&f.c.to_le_bytes());
    }
    let index_byte_length = (solid.faces.len() * 12) as u32;
    // Pad to 4-byte alignment.
    while bin.len() % 4 != 0 {
        bin.push(0);
    }

    let json = format!(
        r#"{{
  "asset": {{ "version": "2.0", "generator": "TPT Vertex" }},
  "scenes": [ {{ "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0 }} ],
  "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }}, "indices": 1, "mode": 4 }} ] }} ],
  "buffers": [ {{ "byteLength": {} }} ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": {}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {}, "byteLength": {}, "target": 34963 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": {}, "type": "VEC3" }},
    {{ "bufferView": 1, "componentType": 5125, "count": {}, "type": "SCALAR" }}
  ]
}}"#,
        bin.len(),
        pos_byte_length,
        index_start,
        index_byte_length,
        solid.vertices.len(),
        solid.faces.len() * 3,
    );
    Ok((json, bin))
}

fn face_normal(a: vertex_kernel::math::Vec3, b: vertex_kernel::math::Vec3, c: vertex_kernel::math::Vec3) -> vertex_kernel::math::Vec3 {
    let ab = b - a;
    let ac = c - a;
    let n = ab.cross(ac);
    let len = n.length();
    if len < 1e-12 {
        vertex_kernel::math::Vec3::ZERO
    } else {
        n * (1.0 / len)
    }
}
