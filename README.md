# Slicer Engine

A high-performance 3D model slicer engine written in Rust, powered by [Clipper2](https://github.com/AngusJohnson/Clipper2) for polygon clipping operations.

## Features

- Cross-platform support (Windows, macOS, WebAssembly)
- Optimized build pipeline with multi-target support
- Leverages Clipper2 for robust geometric operations

## Requirements

- Rust 1.70+ ([Install Rust](https://rustup.rs/))
- For WASM builds: `wasm-pack` ([Install wasm-pack](https://rustwasm.org/wasm-pack/installer/))

## Quick Start

```bash
cd slicer-engine
cargo build --release      # Release build
cargo run --release        # Run the application
cargo test --release       # Run tests
cargo fmt                  # Format code
cargo clippy               # Lint code
```

## Building

### Native Build (Debug)

```bash
cargo build
```

### Native Build (Release)

```bash
cargo build --release
```

### Windows Build

```bash
cargo build --release --target x86_64-pc-windows-msvc
```

### macOS Build

```bash
# Intel Mac
cargo build --release --target x86_64-apple-darwin

# Apple Silicon
cargo build --release --target aarch64-apple-darwin
```

### WebAssembly Build

```bash
wasm-pack build --target web --release
```

### Using Makefile (Linux/macOS)

```bash
make build-release       # Release build
make build-windows       # Windows target
make build-macos         # macOS targets
make build-wasm          # WebAssembly
make test                # Run tests
make lint                # Run clippy
make fmt                 # Format code
```

## Project Structure

```
slicer-engine/
├── src/
│   └── main.rs          # Application entry point
├── Cargo.toml           # Rust package manifest
├── Makefile             # Build targets
└── .github/workflows/   # CI/CD pipelines
```

## Running

```bash
cargo run --release
```

## CLI Commands

The slicer-engine provides a user-friendly command-line interface for slicing 3D models.

### General Help

```bash
# Show available commands and options
cargo run --release -- --help

# Show version
cargo run --release -- --version
```

### Info Command

Display build and library information:

```bash
# Basic info
cargo run --release -- info

# Verbose info with features
cargo run --release -- info --verbose

# JSON format
cargo run --release -- info --output-format json


```

### Slice Command

Slice a 3D model into layers:

```bash
# Basic slice with default layer height (0.2mm)
cargo run --release -- slice --input model.stl

# Slice with custom layer height
cargo run --release -- slice --input model.stl --layer-height 0.1

# Specify output file
cargo run --release -- slice --input model.stl --output result.gcode

# JSON output format
cargo run --release -- slice --input model.stl --output-format json

# Verbose output with debug information
cargo run --release -- slice --input model.stl --verbose

# Show slice command help
cargo run --release -- slice --help
```

## Testing

```bash
cargo test --release
```

## Code Quality

Format code:
```bash
cargo fmt
```

Check for issues with clippy:
```bash
cargo clippy --all-targets --all-features
```

## CI/CD Pipeline

The project includes GitHub Actions workflows that automatically:
- Build for Windows (x86_64)
- Build for macOS (x86_64 and ARM64)
- Build for WebAssembly
- Run tests
- Check code formatting
- Run linter

Workflows are triggered on push to `main` and `develop` branches, and on pull requests.

## Dependencies

- **clipper2**: Polygon clipping library
  - Version: 1.3
  - Used for geometric operations on 2D paths

## License

## License

**LEGAL NOTICE:** This is an interim state. Until an official license is decided and published, all rights are reserved and no use, reproduction, modification, or distribution of this software is permitted without explicit written authorization.

However, this is only a temporary measure while I chart a path forward with the code. The final license will be heavily influenced by community opinions and needs. I welcome your input and feedback on what licensing approach would best serve the community and the project's goals.

TBD

## Contributing

1. Create a feature branch
2. Make changes and test locally
3. Ensure code passes linting: `cargo clippy`
4. Format code: `cargo fmt`
5. Push and create a pull request
