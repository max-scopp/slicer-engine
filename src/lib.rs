//! # Slicer Engine
//!
//! A high-performance 3D model slicer engine written in Rust.
//! Powered by Clipper2 for robust polygon clipping operations.
//!
//! ## Features
//! - Cross-platform support (Windows, macOS, WebAssembly)
//! - Optimized for multi-threaded environments
//! - Type-safe geometric operations
//! - Mesh loading and spatial analysis (STL binary/ASCII)
//! - Printer profile and slicing parameter validation
//! - User-friendly CLI layer for command-line usage

pub mod logging;
pub mod mesh;
pub mod scene;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-slicer"))]
pub mod settings;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-slicer"))]
pub mod arachne;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-slicer"))]
pub mod core;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-slicer"))]
pub mod gcode;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-slicer"))]
pub mod infill;

#[cfg(target_arch = "wasm32")]
pub mod gcode_viewer;

#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod ws_protocol;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

#[cfg(not(target_arch = "wasm32"))]
pub mod db;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(any(not(target_arch = "wasm32"), feature = "web-slicer"))]
pub use core::*;
