//! WebAssembly bindings for GCode visualization.
//!
//! Parses a `.gcode` file (as raw bytes) entirely in Rust and returns
//! per-layer geometry buffers that the Angular UI hands directly to Three.js
//! `LineSegments`. No GCode parsing takes place in JavaScript.
//!
//! ## Data flow
//! ```text
//! bytes (Uint8Array)
//!   → GcodeHandle::parse()
//!       → Vec<InternalLayer>          (parser.rs)
//!           → GcodeHandle::get_layer(i)
//!               → GcodeLayerBuffer    (wasm.rs)
//!                   → Three.js LineSegments
//! ```
//!
//! Each `Float32Array` holds flat line-segment pairs:
//! `[x0, y0, z0,  x1, y1, z1,  …]`  (6 floats per segment).

mod parser;
mod types;
mod wasm;

pub use wasm::{GcodeHandle, GcodeLayerBuffer};
