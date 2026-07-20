//! Simulation for TPT Vertex: static FEA and assembly motion.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides linear-elastic static-stress analysis (small deformation, isotropic
//! material) over a tetrahedralized volume mesh derived from a kernel `Solid`,
//! plus forward-kinematics motion playback over kernel `Assembly` mates.
//!
//! This crate is currently a scaffold: the module layout and public API are in
//! place, but the FEA and motion pipelines themselves are not yet implemented
//! (see the project's `todo.md`, Phase 11).
