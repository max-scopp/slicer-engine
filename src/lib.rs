//! # Slicer Engine
//!
//! A high-performance 3D model slicer engine written in Rust.
//! Powered by Clipper2 for robust polygon clipping operations.
//!
//! ## Features
//! - Cross-platform support (Windows, macOS, WebAssembly)
//! - Optimized for multi-threaded environments
//! - Type-safe geometric operations
//! - User-friendly CLI layer for command-line usage

pub mod core;
pub mod cli;

pub use core::*;
