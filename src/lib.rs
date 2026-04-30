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

#[cfg(target_arch = "wasm32")]
pub mod gcode_viewer;

#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod ws_protocol;

#[cfg(not(target_arch = "wasm32"))]
pub mod arachne;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod core;
#[cfg(not(target_arch = "wasm32"))]
pub mod gcode;
#[cfg(not(target_arch = "wasm32"))]
pub mod infill;

#[cfg(not(target_arch = "wasm32"))]
pub mod db;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(not(target_arch = "wasm32"))]
pub use core::*;
