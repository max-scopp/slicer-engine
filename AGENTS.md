# Slicer Engine - AI Agent Guidance

A high-performance 3D model slicer engine written in Rust, powered by [Clipper2](https://github.com/AngusJohnson/Clipper2) for polygon clipping operations.

## Quick Commands

```bash
# Build and run
cargo build --release
cargo run --release

# Test and lint
cargo test --release
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings

# Cross-platform builds
cargo build --release --target x86_64-pc-windows-msvc   # Windows
cargo build --release --target x86_64-apple-darwin      # macOS Intel
cargo build --release --target aarch64-apple-darwin     # macOS ARM
wasm-pack build --target web --release                   # WebAssembly
```

Or use **Makefile targets** (Linux/macOS):
```bash
make build-release build-windows build-macos build-wasm test lint fmt
```

## Architecture & Design

### Core Components

| Component | Location | Purpose |
|-----------|----------|---------|
| **SliceLayer** | [src/core.rs](src/core.rs) | Data structure representing a single layer (Z-coordinate + paths) |
| **Clipper2 Integration** | [src/core.rs](src/core.rs) | Geometric polygon clipping operations |
| **Library Interface** | [src/lib.rs](src/lib.rs) | Public API exposing core functionality |
| **CLI Layer** | [src/cli/](src/cli/) | User-friendly command-line interface bridging library API to commands |
| **Build Configuration** | [build.rs](build.rs) | Platform detection and environment setup |

### Module Organization

```
src/
├── cli/                    # CLI layer (user-friendly commands)
│   ├── mod.rs             # CLI entry point, command dispatcher
│   ├── commands/          # Command implementations
│   │   ├── slice.rs       # Slice operation command
│   │   └── info.rs        # Information command
│   ├── io/                # File I/O layer
│   │   ├── validation.rs  # Path/file validation
│   │   ├── reader.rs      # File reader implementations
│   │   └── writer.rs      # File writer implementations
│   ├── output.rs          # Output formatting (JSON, GCode, CSV)
│   ├── error.rs           # CLI error types
│   └── adapters.rs        # Library API adapters
├── core.rs                # Core data structures & operations
├── lib.rs                 # Public library root
└── main.rs                # Application entry point (uses CLI)
```

- **lib.rs**: Public library root - re-exports core module and CLI
- **core.rs**: Core data structures (SliceLayer) and Clipper2 operations
- **cli/**: CLI layer providing user-friendly commands and file I/O
- **main.rs**: Application entry point that delegates to CLI layer

### Cross-Platform Strategy

The build script ([build.rs](build.rs)) detects target platform and sets environment variables:
- **Windows**: x86_64-pc-windows-msvc
- **macOS**: x86_64-apple-darwin (Intel), aarch64-apple-darwin (Silicon)
- **WebAssembly**: wasm32-unknown-unknown with wasm-pack

## Development Conventions

### CLI Layer Architecture

The CLI layer uses the **adapter pattern** to bridge the library API to user-friendly commands:

- **Separation of Concerns**: CLI commands in `src/cli/` don't modify core library code
- **Error Handling**: Custom `CliError` type provides user-friendly error messages
- **Output Formatting**: Pluggable formatters support JSON, human-readable, and CSV outputs
- **File I/O**: Dedicated `io/` submodule handles all file operations with validation
- **Backward Compatibility**: Library API remains unchanged; CLI is purely additive

Example CLI Usage:
```bash
# Slice a model with 0.2mm layer height
slicer-engine slice --input model.stl --layer-height 0.2 --output result.gcode

# Display build information
slicer-engine info --verbose

# Get help on any command
slicer-engine slice --help
```

### Code Style
- Follow [Rust Edition 2021 conventions](https://doc.rust-lang.org/edition-guide/rust-2021/index.html)
- Use `cargo fmt` for formatting (enforced by CI)
- Run `cargo clippy -- -D warnings` before committing
- Write inline tests with `#[cfg(test)]` in the same module

### Performance Priorities
- **Release builds prioritized**: LTO enabled, opt-level 3, codegen-units 1
- Minimize allocations in hot paths (especially in slicing operations)
- Profile with `cargo flamegraph` if performance regressions suspected
- Consider compile-time vs runtime tradeoffs for polygon operations

### Documentation
- Use doc comments (`///`) for public types and functions
- Include usage examples in doc comments for core APIs
- Update [README.md](README.md) for user-facing changes

### Testing
- Write tests inline with `#[cfg(test)]` modules
- Use `cargo test --release` to verify release build compatibility
- Test all three platforms: native, WASM, and cross-compilation

## Project Dependencies

| Crate | Version | Usage |
|-------|---------|-------|
| **clipper2** | 0.5 | Polygon clipping, geometric operations |

*Note: Keep clipper2 dependency current for bug fixes and performance improvements.*

## Common Tasks

### Adding a CLI Command

1. Create command module in `src/cli/commands/your_command.rs`
2. Define command struct with `#[derive(Parser)]` from clap
3. Implement command logic using library API adapters
4. Register in `src/cli/commands/mod.rs` and main dispatcher
5. Add error handling with `CliError` conversions
6. Test with `cargo run -- your-command --help`

See [architecture-cli-layer-1.md](plan/architecture-cli-layer-1.md) for detailed implementation phases.

### Adding a New Data Structure
1. Create in appropriate module (usually core.rs)
2. Implement `Debug` and `Clone` traits for inspection and flexibility
3. Add inline tests within `#[cfg(test)]` block
4. Document with `///` doc comments including examples
5. Re-export from lib.rs if part of public API

### Implementing Geometric Operations
1. Leverage Clipper2 API for polygon clipping (avoid reimplementing)
2. Define clear input/output types using SliceLayer or similar structures
3. Write tests covering edge cases (empty paths, degenerate polygons, etc.)
4. Profile performance on large datasets (>10k paths)
5. Document assumptions about coordinate precision

### Cross-Platform Testing
1. Use conditional compilation (`#[cfg(...)]`) for platform-specific code
2. Test locally: `cargo test --release`
3. Test WASM builds with `wasm-pack test --headless --firefox`
4. Verify CI passes all platform targets before merging

## CI/CD Pipeline

GitHub Actions ([.github/workflows/build.yml](.github/workflows/build.yml)) automatically:
- Runs on push and pull requests
- Builds all three platform targets
- Runs linting (clippy) and formatting checks (fmt)
- Executes test suite

**Do not bypass CI checks.** All builds must pass before merge.

## Known Constraints & Pitfalls

- **Clipper2 Coordinate System**: Uses integer-based `Centi` (centimeter precision). Be aware when converting from floating-point models.
- **CLI Framework**: Uses clap v4 for argument parsing. Keep derive macros in sync with command requirements.
- **WASM Memory**: Be mindful of WebAssembly memory limits when processing large 3D models.
- **File I/O in WASM**: CLI file operations require JavaScript bindings; not all features available in WASM target.
- **LTO Compilation**: Release builds are slower due to LTO. Use debug builds during iterative development.
- **Cross-compilation**: Requires appropriate target toolchains installed. CI verifies these work.

## Related Documentation

- [README.md](README.md) - User guide and feature overview
- [SETUP_COMPLETE.md](SETUP_COMPLETE.md) - Initial setup record
- [architecture-cli-layer-1.md](plan/architecture-cli-layer-1.md) - CLI layer implementation plan
- [Clipper2 Documentation](https://github.com/AngusJohnson/Clipper2) - Polygon clipping reference

---

**Last Updated**: 2026-04-26  
**Maintainer Guidance**: Keep this file in sync with project structure changes, new conventions, or significant architectural decisions.
