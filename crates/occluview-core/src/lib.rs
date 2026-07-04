//! `occluview-core` — pure logic for OccluView.
//!
//! This crate is intentionally free of I/O, GPU, and platform (Win32) concerns.
//! It contains the domain data model: units, math, mesh representation, the
//! scene graph, and the camera. Both the renderer and the GUI/CLI/shell build on
//! it. See [`AGENTS.md`](../../../AGENTS.md) §0.4 and the
//! [architecture](../../../docs/ARCHITECTURE.md).
//!
//! ## Invariants
//!
//! - **Panic-free.** Every public function returns a `Result` or is total. There
//!   is no `unwrap`/`expect`/`panic!` in this crate (clippy-enforced).
//! - **`Send + Sync`.** All public types are shareable across threads; the
//!   renderer and the file loaders rely on this.
//! - **Millimeter units** internally ([`units::Millimeters`]).
//! - **Right-handed Y-up** coordinate frame ([`frame`]).
//!
//! The crate is organized as follows; each module re-exports its public surface
//! from here so callers can `use occluview_core::Mesh` etc.

#![cfg_attr(not(test), deny(clippy::panic))]
#![forbid(unsafe_code)]

pub mod units;
pub mod frame;
pub mod bbox;
pub mod mesh;
pub mod scene;
pub mod camera;
pub mod error;

pub use bbox::Aabb;
pub use camera::Camera;
pub use error::CoreError;
pub use mesh::{Mesh, MeshBuilder, Vertex};
pub use scene::{Scene, SceneMesh};
pub use units::Millimeters;
