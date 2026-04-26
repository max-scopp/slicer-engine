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

pub mod cli;
pub mod core;
pub mod gcode;
pub mod mesh;
pub mod settings;
pub mod ws_protocol;

#[cfg(not(target_arch = "wasm32"))]
pub mod db;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

pub use core::*;
