//! Mesh loading, spatial analysis, and coordinate transformation.
//!
//! # Modules
//! - [`types`]: Core data structures (`Vertex`, `Face`, `AABB`, `Mesh`)
//! - [`io`]: STL file reading (binary and ASCII)
//! - [`analysis`]: Geometry calculations (AABB, volume, surface area)
//! - [`transforms`]: Coordinate transforms (center, drop to floor, translate)

pub mod analysis;
pub mod io;
pub mod transforms;
pub mod types;

pub use types::{Face, Mesh, Vertex, AABB};
