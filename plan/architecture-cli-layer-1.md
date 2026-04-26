---
goal: Add a CLI layer that bridges the library API to a user-friendly command-line interface
version: 1.0
date_created: 2026-04-26
last_updated: 2026-04-26
owner: Slicer Engine Team
status: 'Planned'
tags: [architecture, feature, cli]
---

# Introduction

![Status: Planned](https://img.shields.io/badge/status-Planned-blue)

This implementation plan defines the architecture and phased execution for adding a dedicated CLI layer to the slicer-engine. The CLI layer will provide a user-friendly command-line interface that abstracts the library API, enabling users to interact with the slicing engine without writing Rust code. The CLI will support file I/O operations, batch processing, and customizable output formats.

## 1. Requirements & Constraints

- **REQ-001**: CLI must expose core slicing operations through intuitive commands
- **REQ-002**: CLI must support file input/output (STL, OBJ, GCode formats planned)
- **REQ-003**: CLI must provide human-readable error messages and help documentation
- **REQ-004**: CLI must be cross-platform (Windows, macOS, Linux, WebAssembly)
- **REQ-005**: CLI must maintain separation from library API
- **REQ-006**: CLI commands must be discoverable via `--help` and `--version` flags
- **SEC-001**: CLI must validate and sanitize all file paths
- **CON-001**: CLI framework must not increase release binary size significantly (target: <10MB)
- **CON-002**: CLI should reuse existing library structures without modification
- **GUD-001**: Follow Rust CLI best practices (clap v4 for argument parsing)
- **GUD-002**: Provide both terse (for scripting) and verbose (for humans) output modes
- **PAT-001**: CLI module should use the adapter pattern to bridge library to user commands

## 2. Implementation Steps

### Phase 1: CLI Infrastructure Setup

- **GOAL-001**: Establish CLI module structure and dependencies

#### Tasks

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-1.1 | Add `clap` v4 and `anyhow` to Cargo.toml dependencies | None | Dependencies resolve without conflicts |
| TASK-1.2 | Create `src/cli/mod.rs` module | TASK-1.1 | Module compiles with no warnings |
| TASK-1.3 | Create `src/cli/commands/mod.rs` submodule | TASK-1.2 | Submodule structure established |
| TASK-1.4 | Create `src/cli/error.rs` for error handling | TASK-1.2 | Custom error types defined with Display impl |
| TASK-1.5 | Create `src/cli/output.rs` for formatting | TASK-1.2 | Output formatter trait defined |
| TASK-1.6 | Modify `src/lib.rs` to expose CLI module | TASK-1.2 | CLI module publicly accessible |

**Phase 1 Completion Criteria**: All CLI infrastructure files created and project compiles without errors or warnings.

---

### Phase 2: Core Command Structure

- **GOAL-002**: Implement base command infrastructure using clap

#### Tasks

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-2.1 | Create `CliArgs` struct with clap derive macros in `src/cli/mod.rs` | TASK-1.3, TASK-1.1 | Struct defines version, help, and subcommand routing |
| TASK-2.2 | Implement `slice` command in `src/cli/commands/slice.rs` | TASK-1.3 | Command accepts input file path and layer height parameters |
| TASK-2.3 | Implement `info` command in `src/cli/commands/info.rs` | TASK-1.3 | Command displays library and build information |
| TASK-2.4 | Create command dispatcher in `src/cli/mod.rs` | TASK-2.1, TASK-2.2, TASK-2.3 | Routes commands to appropriate handlers |
| TASK-2.5 | Update `src/main.rs` to use CLI layer | TASK-2.4 | Main function delegates to CLI dispatcher |

**Phase 2 Completion Criteria**: `--help` displays all commands, `--version` shows package version, basic command routing works.

---

### Phase 3: Output and Error Handling

- **GOAL-003**: Implement robust error handling and output formatting

#### Tasks

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-3.1 | Implement `OutputFormatter` trait in `src/cli/output.rs` | TASK-1.5 | Trait supports JSON, human-readable, and CSV output |
| TASK-3.2 | Create `CliError` enum in `src/cli/error.rs` with error categories | TASK-1.4 | Covers: IO, parsing, validation, slicing errors |
| TASK-3.3 | Implement Display and From conversions for CliError | TASK-3.2 | Converts std::io::Error, clipper2 errors automatically |
| TASK-3.4 | Add `--output-format` flag to CLI | TASK-3.1 | Commands accept output format parameter |
| TASK-3.5 | Add `--verbose` flag for debug output | TASK-3.1 | Verbose mode prints diagnostic information |

**Phase 3 Completion Criteria**: Errors produce user-friendly messages, multiple output formats supported, no panics on invalid input.

---

### Phase 4: File I/O Layer

- **GOAL-004**: Implement cross-platform file handling with validation

#### Tasks

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-4.1 | Create `src/cli/io/mod.rs` module | TASK-1.3 | Module structure created |
| TASK-4.2 | Implement path validation in `src/cli/io/validation.rs` | TASK-4.1 | Validates existence, permissions, extension |
| TASK-4.3 | Create file reader interface in `src/cli/io/reader.rs` | TASK-4.1 | Trait-based design allows format extensions |
| TASK-4.4 | Implement STL file reader (stub) | TASK-4.3 | Returns mock data for testing |
| TASK-4.5 | Create file writer interface in `src/cli/io/writer.rs` | TASK-4.1 | Supports GCode, JSON output formats |

**Phase 4 Completion Criteria**: Can read/write files with proper error handling and path validation, no security vulnerabilities.

---

### Phase 5: Integration with Core API

- **GOAL-005**: Connect CLI commands to library functions

#### Tasks

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-5.1 | Extend `SliceLayer` API if needed in `src/core.rs` | None | Core API remains unchanged or minimally modified |
| TASK-5.2 | Implement slice operation wrapper in `src/cli/adapters.rs` | TASK-5.1, TASK-2.2 | Wraps library calls with CLI-appropriate error handling |
| TASK-5.3 | Integrate `ClipperAdapter` to bridge Clipper2 to CLI | TASK-5.2 | Can invoke Clipper2 operations from CLI context |
| TASK-5.4 | Add batch processing support | TASK-5.2 | Process multiple files with configuration |

**Phase 5 Completion Criteria**: CLI slice command executes using library API, produces valid output layers.

---

### Phase 6: Documentation and Testing

- **GOAL-006**: Document CLI architecture and ensure quality

#### Tasks

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-6.1 | Update `AGENTS.md` with CLI layer architecture | All phases | Architecture diagram includes CLI layer |
| TASK-6.2 | Add CLI command examples to README.md | TASK-5.4 | Examples cover common usage patterns |
| TASK-6.3 | Write integration tests for CLI commands | TASK-5.4 | Tests cover happy path and error cases |
| TASK-6.4 | Run linter and formatter | All phases | `cargo fmt && cargo clippy -- -D warnings` passes |
| TASK-6.5 | Cross-platform build verification | All phases | Builds successfully on Windows, macOS, WASM targets |

**Phase 6 Completion Criteria**: Documentation complete, tests pass, CI builds succeed on all platforms.

---

## 3. Module Architecture

```
src/
├── cli/                        # New CLI module
│   ├── mod.rs                 # CLI entry point, CliArgs, dispatcher
│   ├── commands/
│   │   ├── mod.rs             # Command module exports
│   │   ├── slice.rs           # Slice operation command
│   │   ├── info.rs            # Information command
│   │   └── validate.rs        # Validation command (future)
│   ├── io/
│   │   ├── mod.rs
│   │   ├── validation.rs      # Path and file validation
│   │   ├── reader.rs          # File reader trait & implementations
│   │   └── writer.rs          # File writer trait & implementations
│   ├── output.rs              # OutputFormatter trait, implementations
│   ├── error.rs               # CliError enum, error conversions
│   └── adapters.rs            # Library API adapters
├── core.rs                     # Existing core API (unchanged)
├── lib.rs                      # Updated to expose cli module
└── main.rs                     # Updated to delegate to CLI
```

## 4. Dependency Changes

### New Dependencies to Add

```toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
anyhow = "1.0"
serde_json = { version = "1.0", optional = true }
csv = { version = "1.3", optional = true }
```

### Rationale

- **clap**: Industry-standard CLI argument parser with derive macros for ergonomic command definition
- **anyhow**: Flexible error handling with context chains
- **serde_json**: Optional JSON output formatting
- **csv**: Optional CSV output for layer data

## 5. API Contract

### CLI Command Interface

```rust
// Example: slicer-engine slice --input model.stl --layer-height 0.2 --output output.gcode
CliArgs {
    command: Commands::Slice(SliceCommand {
        input: PathBuf,           // Path to 3D model file
        layer_height: f64,        // Vertical spacing between layers (mm)
        output: Option<PathBuf>,  // Output file path (default: auto-generated)
        output_format: OutputFormat,  // JSON, GCode, CSV
        verbose: bool,
    })
}

// Example: slicer-engine info
// Displays version, build target, dependencies
```

### File Format Support

| Format | Phase | Status |
|--------|-------|--------|
| STL (ASCII/Binary) | 4 | Planned |
| OBJ | Future | Backlog |
| GCode | 4 | Planned |
| JSON | 3 | Planned |
| CSV | 3 | Planned |

## 6. Cross-Platform Considerations

- **Path handling**: Use `std::path::PathBuf` for platform-agnostic paths
- **WASM target**: CLI layer will be available but file I/O handled via JavaScript bindings
- **Windows UAC**: No special handling needed; standard file permissions apply
- **Case sensitivity**: Document platform-specific filename behavior

## 7. Backward Compatibility

- Library API (`SliceLayer`, `core::*`) remains unchanged
- Current library users unaffected
- New `cli` module is additive only
- Main.rs refactored but binary behavior consistent

## 8. Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Command discovery | 100% | `--help` lists all commands |
| Error clarity | <50 chars | All error messages fit terminal width |
| Release binary size | <10MB | `cargo build --release` measurement |
| Cross-platform tests | Pass | CI success on 3+ platforms |
| Documentation | Complete | README examples work as-written |
| User feedback time | <2s | CLI response time on typical model |

## 9. Risk Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Binary bloat from clap | Medium | High | Use feature flags; profile with `cargo bloat` |
| Breaking library API | Low | High | Maintain separation; use adapters |
| File security issues | Medium | High | Validate paths; use canonical paths |
| WASM compatibility | High | Medium | Conditional compilation; test WASM build |
| Windows path issues | Medium | Low | Use `PathBuf` abstractions; test on Windows |

---

## Implementation Execution Order

Execute phases sequentially: **Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6**

Each phase must be completed and verified before proceeding to the next phase.
