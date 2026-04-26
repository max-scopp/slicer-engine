---
goal: Implement mesh loading, spatial analysis, and settings validation infrastructure
version: 1.0
date_created: 2026-04-26
last_updated: 2026-04-26
owner: Slicer Engine Team
status: 'Planned'
tags: [architecture, feature, mesh, settings, validation]
issues: [VG-1, VG-2]
---

# Introduction

![Status: Planned](https://img.shields.io/badge/status-Planned-blue)

This implementation plan defines the architecture and phased execution for adding mesh loading/spatial analysis (VG-1) and settings validation infrastructure (VG-2) to the slicer-engine. Together, these features form the "eyes" and "contract" of the engine:

- **VG-1 (Mesh Loading & Spatial Analysis):** Read 3D models (STL), calculate geometry (AABB, volume, surface area), and apply coordinate transformations (centering, drop to floor)
- **VG-2 (Settings & Validator):** Define printer profiles and slicing parameters, validate physical constraints, and detect setting overrides

## 1. Requirements & Constraints

### VG-1 Requirements
- **REQ-VG1-001**: Engine must read binary and ASCII STL files
- **REQ-VG1-002**: Engine must calculate Axis-Aligned Bounding Box (AABB) for loaded meshes
- **REQ-VG1-003**: Engine must calculate signed mesh volume and surface area
- **REQ-VG1-004**: Engine must support coordinate transforms: centering and drop-to-floor
- **REQ-VG1-005**: Mesh types must be serializable for API communication

### VG-2 Requirements
- **REQ-VG2-001**: Engine must define PrinterProfile struct with physical constraints
- **REQ-VG2-002**: Engine must define SlicingParams struct with layer height, speeds, temperatures, etc.
- **REQ-VG2-003**: Engine must validate settings against physical limits (e.g., LayerHeight ≤ 0.8 × NozzleDiameter)
- **REQ-VG2-004**: Engine must detect and report setting overrides (Global vs Object level)
- **REQ-VG2-005**: Settings must be serializable to/from JSON (serde)

### Shared Constraints
- **CON-001**: Coordinate system: STL files loaded in native coordinates (no forced transforms on import)
- **CON-002**: All mesh types immutable; transformations return new Mesh instances
- **CON-003**: Settings validation: start with struct definitions + validator trait stubs; real rules deferred to follow-up PR
- **GUD-001**: Follow Rust best practices: use serde for serialization, trait-based validation for extensibility
- **GUD-002**: Module structure: isolate mesh and settings from CLI layer for library reusability (e.g., WASM, embedded use)

## 2. Implementation Steps

### Phase 1: Setup & Dependencies

**Goal**: Add required crates and establish module structure.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-1.1 | Add `stl-io` (STL parsing), `serde` with derive feature to Cargo.toml | None | `cargo check` succeeds, dependencies resolve |
| TASK-1.2 | Create `src/mesh/` module hierarchy | TASK-1.1 | Files created: `mod.rs`, `types.rs`, `io.rs`, `analysis.rs`, `transforms.rs`, all re-exported from `lib.rs` |
| TASK-1.3 | Create `src/settings/` module hierarchy | TASK-1.1 | Files created: `mod.rs`, `profile.rs`, `params.rs`, `validator.rs`, `diff.rs`, all re-exported from `lib.rs` |
| TASK-1.4 | Update `src/lib.rs` to publicly expose mesh and settings modules | TASK-1.2, TASK-1.3 | `pub mod mesh;` and `pub mod settings;` added, doc comments present |

**Phase 1 Completion Criteria**: All modules created and compile without errors. `cargo doc --open` shows module hierarchy visible in documentation.

---

### Phase 2: Mesh Types (VG-1)

**Goal**: Define core data structures for mesh representation.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-2.1 | Define `Vertex` struct in `src/mesh/types.rs` | TASK-1.2 | `Vertex { x: f64, y: f64, z: f64 }` with `distance_to()` method, derives: `Debug, Clone, Copy, PartialEq`, doc comments explain millimeter units |
| TASK-2.2 | Define `Face` (triangle) struct in `src/mesh/types.rs` | TASK-2.1 | `Face { vertices: [Vertex; 3], normal: Option<Vertex> }` with `area()` method (Heron's formula), derives: `Debug, Clone, PartialEq` |
| TASK-2.3 | Define `AABB` struct in `src/mesh/types.rs` | TASK-2.1 | `AABB { min: Vertex, max: Vertex }` with methods: `new_from_vertices()`, `width()`, `height()`, `depth()`, `center()`, `contains_point()`, derives: `Debug, Clone` |
| TASK-2.4 | Define `Mesh` struct in `src/mesh/types.rs` | TASK-2.2, TASK-2.3 | `Mesh { vertices: Vec<Vertex>, faces: Vec<Face>, aabb: Option<AABB> }` with methods: `new()` constructor, `calculate_aabb()` (caches AABB), derives: `Debug, Clone` |

**Phase 2 Completion Criteria**: All types compile. Inline tests verify: Vertex construction, Face area calculation, AABB bounds on sample cube, Mesh creation and AABB cache.

---

### Phase 3: STL Parser (VG-1)

**Goal**: Implement file reading for STL format (binary and ASCII).

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-3.1 | Implement `read_stl()` function in `src/mesh/io.rs` | TASK-2.4, TASK-1.1 | Function signature: `pub fn read_stl(path: &Path) -> Result<Mesh, Box<dyn std::error::Error>>`. Detects binary vs ASCII via file header. Uses stl-io crate. Converts stl-io structs to local Vertex/Face/Mesh types. Returns mesh in native coordinates (no transforms applied). |
| TASK-3.2 | Create test fixtures | TASK-3.1 | Test files: `tests/fixtures/simple-cube.stl` (binary), `tests/fixtures/simple-cube-ascii.stl` (ASCII) — each defines 8 vertices, 12 faces (unit cube) |
| TASK-3.3 | Write unit tests for `read_stl()` | TASK-3.2 | Tests: load binary STL (verify 8 vertices, 12 faces), load ASCII STL (same), handle missing file (error), handle invalid file (error) |

**Phase 3 Completion Criteria**: `cargo test --lib mesh::io` passes. `read_stl()` successfully parses binary and ASCII test files. Error handling for file not found and malformed input.

---

### Phase 4: Spatial Analysis (VG-1)

**Goal**: Implement geometry calculations: AABB, volume, surface area.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-4.1 | Implement `calculate_aabb()` in `src/mesh/analysis.rs` | TASK-2.4 | Function: `pub fn calculate_aabb(mesh: &Mesh) -> AABB`. Iterates vertices, finds min/max for x, y, z. Returns AABB struct with correct bounds. |
| TASK-4.2 | Implement `calculate_volume()` in `src/mesh/analysis.rs` | TASK-2.4 | Function: `pub fn calculate_volume(mesh: &Mesh) -> Result<f64, String>`. Uses signed volume formula (divergence theorem): `volume = abs(sum of (face_normal · face_center)) / 6`. Returns error if mesh appears non-closed. Result in cubic millimeters. |
| TASK-4.3 | Implement `calculate_surface_area()` in `src/mesh/analysis.rs` | TASK-2.4 | Function: `pub fn calculate_surface_area(mesh: &Mesh) -> f64`. Sums Face::area() across all faces. Returns area in square millimeters. |
| TASK-4.4 | Write unit tests for analysis functions | TASK-4.1, TASK-4.2, TASK-4.3 | Tests: 10×10×10 cube AABB = (0,0,0) to (10,10,10), volume ≈ 1000 mm³, surface area ≈ 600 mm². Test edge case: open mesh returns error in volume calculation. |

**Phase 4 Completion Criteria**: `cargo test --lib mesh::analysis` passes. AABB, volume, and surface area calculations accurate to within 1% for test cube.

---

### Phase 5: Coordinate Transforms (VG-1)

**Goal**: Implement spatial transformations: centering, drop to floor, translate.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-5.1 | Implement `center_mesh()` in `src/mesh/transforms.rs` | TASK-2.4, TASK-4.1 | Function: `pub fn center_mesh(mesh: &Mesh) -> Mesh`. Calculates AABB center. Translates all vertices so AABB center is at (0, 0, z_min). Returns new Mesh (original unchanged). |
| TASK-5.2 | Implement `drop_to_floor()` in `src/mesh/transforms.rs` | TASK-2.4, TASK-4.1 | Function: `pub fn drop_to_floor(mesh: &Mesh) -> Mesh`. Calculates AABB. Translates all vertices so AABB.min.z = 0. Returns new Mesh (original unchanged). |
| TASK-5.3 | Implement `translate_mesh()` in `src/mesh/transforms.rs` | TASK-2.4 | Function: `pub fn translate_mesh(mesh: &Mesh, offset: Vertex) -> Mesh`. Adds offset to all vertices. Returns new Mesh. |
| TASK-5.4 | Write unit tests for transforms | TASK-5.1, TASK-5.2, TASK-5.3 | Tests: center_mesh on cube at (10,10,0)-(20,20,10) results in center at (0,0,10). drop_to_floor on cube at (5,5,5)-(15,15,15) results in z_min=0. translate_mesh offsets by (5,5,5) correctly. |

**Phase 5 Completion Criteria**: `cargo test --lib mesh::transforms` passes. Transform calculations verified on test mesh. Original mesh unchanged after each transform.

---

### Phase 6: Printer Profile (VG-2)

**Goal**: Define printer hardware constraints.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-6.1 | Define `PrinterProfile` struct in `src/settings/profile.rs` | TASK-1.3 | Struct with derives: `Debug, Clone, Serialize, Deserialize`. Fields: `name: String`, `nozzle_diameter: f64`, `min_layer_height: f64`, `max_layer_height: f64`, `max_print_speed: f64`, `max_acceleration: f64`. Include doc comments explaining constraints. |
| TASK-6.2 | Implement `Default` for `PrinterProfile` | TASK-6.1 | Default creates standard 0.4mm nozzle profile: nozzle=0.4, min_layer=0.1, max_layer=0.3, max_speed=150, max_accel=1000. |
| TASK-6.3 | Write serialization tests | TASK-6.1, TASK-6.2 | Tests: serialize to JSON and back (round-trip). Verify all fields present. Default profile serializes and deserializes correctly. |

**Phase 6 Completion Criteria**: `cargo test --lib settings::profile` passes. PrinterProfile serializes to JSON with all fields and deserializes correctly.

---

### Phase 7: Slicing Parameters (VG-2)

**Goal**: Define per-print slicing parameters.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-7.1 | Define `SlicingParams` struct in `src/settings/params.rs` | TASK-1.3 | Struct with derives: `Debug, Clone, Serialize, Deserialize`. Fields: `layer_height: f64`, `wall_thickness: f64`, `infill_density: f64`, `print_speed: f64`, `nozzle_temp: f64`, `bed_temp: f64`. Include doc comments. |
| TASK-7.2 | Define `GlobalSettings` struct in `src/settings/params.rs` | TASK-7.1 | Wrapper struct: `GlobalSettings { params: SlicingParams }` with derives: `Debug, Clone, Serialize, Deserialize`. Represents layer 1 (global defaults). |
| TASK-7.3 | Define `ObjectSettings` struct in `src/settings/params.rs` | TASK-7.1 | Struct: `ObjectSettings { object_name: String, overrides: Option<SlicingParams> }` with derives: `Debug, Clone, Serialize, Deserialize`. Represents layer 2 (per-object overrides). |
| TASK-7.4 | Write serialization tests | TASK-7.1, TASK-7.2, TASK-7.3 | Tests: GlobalSettings round-trip, ObjectSettings with/without overrides, full Global+Object structure. Verify JSON has expected fields. |

**Phase 7 Completion Criteria**: `cargo test --lib settings::params` passes. All settings structs serialize/deserialize correctly with round-trip integrity.

---

### Phase 8: Settings Validator (VG-2)

**Goal**: Establish validation infrastructure (trait + stubs for future rule implementations).

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-8.1 | Define `SettingValidator` trait in `src/settings/validator.rs` | TASK-1.3 | Trait method: `fn validate(&self) -> Result<(), Vec<String>>`. Returns Vec of error messages if validation fails, Ok(()) if valid. |
| TASK-8.2 | Define `ValidationRules` struct in `src/settings/validator.rs` | TASK-8.1 | Struct with static helper methods (not yet implemented): `validate_layer_height()`, `validate_positive()`, `validate_range()`. Each returns `Result<(), String>`. Document TODO: rules are stubs returning Ok(()) for now. |
| TASK-8.3 | Implement `SettingValidator` for `SlicingParams` | TASK-8.2, TASK-7.1 | Implement trait: `validate()` calls ValidationRules stub methods. Currently returns Ok(()) (no-op). Future PRs will add real validation logic. |
| TASK-8.4 | Write validator tests | TASK-8.3 | Tests: validate_layer_height stub returns Ok(()), validate_positive stub returns Ok(()), full SlicingParams validation returns Ok(()). |

**Phase 8 Completion Criteria**: `cargo test --lib settings::validator` passes. Validator trait compiles. Trait methods callable on SlicingParams. Stubs return expected Ok(()) values.

---

### Phase 9: Settings Diff Tool (VG-2)

**Goal**: Detect and report setting overrides between Global and Object levels.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-9.1 | Define `SettingsDiff` struct in `src/settings/diff.rs` | TASK-1.3 | Struct: `SettingsDiff { field_name: String, global_value: String, object_value: String, is_override: bool }` with derives: `Debug, Clone, Serialize`. Each field represents one setting difference. |
| TASK-9.2 | Implement `compare_settings()` function in `src/settings/diff.rs` | TASK-9.1, TASK-7.2, TASK-7.3 | Function: `pub fn compare_settings(global: &GlobalSettings, object: &ObjectSettings) -> Vec<SettingsDiff>`. Iterates SlicingParams fields. For each field, creates SettingsDiff with global value, object value (or default if not overridden), and `is_override` flag. Returns full list of all fields. |
| TASK-9.3 | Write diff tests | TASK-9.2 | Tests: compare with no overrides (all is_override=false), with partial overrides (layer_height overridden, others not), verify returned Vec has all fields, verify global and object values match serialized inputs. |

**Phase 9 Completion Criteria**: `cargo test --lib settings::diff` passes. `compare_settings()` correctly identifies overridden and non-overridden fields. Output structure matches schema.

---

### Phase 10: CLI Integration (VG-1 & VG-2)

**Goal**: Expose mesh loading and settings validation through CLI.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-10.1 | Update `src/cli/commands/slice.rs` to load and analyze mesh | TASK-3.1, TASK-4.1, TASK-4.2, TASK-4.3 | In `execute()`: call `mesh::io::read_stl(&self.input)` to load mesh. If verbose, calculate and log AABB, volume, surface area. On error, return `CliError::Io` or `CliError::Failed`. Update help text to describe mesh loading behavior. |
| TASK-10.2 | Add optional mesh transforms to slice command | TASK-5.1, TASK-5.2, TASK-10.1 | Add CLI flags (optional): `--center` (apply center_mesh), `--drop-to-floor` (apply drop_to_floor). Update slice.rs to conditionally apply transforms before passing to slicing logic. |
| TASK-10.3 | Create `src/cli/commands/settings.rs` | TASK-8.1, TASK-9.2 | New command struct with subcommands: `validate` (load GlobalSettings + ObjectSettings JSON, run SettingValidator, output results), `diff` (load settings, run compare_settings, output SettingsDiff as table or JSON). |
| TASK-10.4 | Register settings command in CLI dispatcher | TASK-10.3 | Update `src/cli/mod.rs`: add `Settings` variant to Commands enum. Update `src/cli/commands/mod.rs` to include `pub mod settings;`. Route to settings command handler. |
| TASK-10.5 | Write CLI integration tests | TASK-10.2, TASK-10.4 | Tests: `cargo run -- slice tests/fixtures/simple-cube.stl --verbose` outputs AABB and volume. `cargo run -- settings validate --global global.json --object object.json` outputs validation result. `cargo run -- settings diff --global global.json --object object.json` outputs diff table. |

**Phase 10 Completion Criteria**: `cargo run -- slice --help` shows mesh-related information. Slice command loads STL and logs geometry (if verbose). Settings command exists and routes correctly. All CLI smoke tests pass.

---

### Phase 11: Build & Validation

**Goal**: Ensure code quality, correctness, and release readiness.

| Task ID | Description | Dependencies | Acceptance Criteria |
|---------|-------------|--------------|-------------------|
| TASK-11.1 | Run all tests and verify coverage | TASK-2.1 through TASK-10.5 | `cargo test --release` — all tests pass. `cargo test --lib` — all unit tests pass. No test failures. |
| TASK-11.2 | Run linting and formatting | TASK-11.1 | `cargo fmt` — no changes needed. `cargo clippy -- -D warnings` — zero warnings. Code is idiomatic Rust. |
| TASK-11.3 | Verify documentation | TASK-11.1 | `cargo doc --no-deps --open` — all public items have doc comments. Module hierarchy visible. Examples present in doc comments where helpful. |
| TASK-11.4 | Cross-platform build verification | TASK-11.2 | On Windows: `cargo build --release --target x86_64-pc-windows-msvc` succeeds. (macOS and Linux CI verification deferred to CI pipeline.) |

**Phase 11 Completion Criteria**: All tests pass. Zero clippy warnings. All public items documented. Release build succeeds on target platform(s). Code ready for merge and release.

---

## 3. Module Structure

```
src/
├── mesh/                           # VG-1: Mesh types & operations
│   ├── mod.rs                      # Module root, re-exports all
│   ├── types.rs                    # Vertex, Face, AABB, Mesh structs
│   ├── io.rs                       # read_stl() function
│   ├── analysis.rs                 # calculate_aabb, calculate_volume, calculate_surface_area
│   └── transforms.rs               # center_mesh, drop_to_floor, translate_mesh
│
├── settings/                       # VG-2: Settings & validation
│   ├── mod.rs                      # Module root, re-exports all
│   ├── profile.rs                  # PrinterProfile struct
│   ├── params.rs                   # SlicingParams, GlobalSettings, ObjectSettings
│   ├── validator.rs                # SettingValidator trait + ValidationRules stubs
│   └── diff.rs                     # SettingsDiff struct + compare_settings function
│
├── cli/                            # CLI layer (existing, updated)
│   ├── commands/
│   │   ├── slice.rs                # [UPDATED] Integrate mesh loading
│   │   ├── settings.rs             # [NEW] Settings validation & diff commands
│   │   └── mod.rs                  # [UPDATED] Register settings command
│   └── mod.rs                      # [UPDATED] Add Settings to Commands enum
│
├── core.rs                         # [UNCHANGED] Core slicing types (SliceLayer, etc.)
├── lib.rs                          # [UPDATED] Re-export mesh, settings modules
└── main.rs                         # [UNCHANGED] Entry point
```

---

## 4. Critical Implementation Notes

### Coordinate System
- **Input:** STL files loaded in native coordinates (no transforms applied on import)
- **Transforms:** All are immutable operations returning new Mesh instances
- **Units:** Assumed millimeters throughout (document in type comments)
- **Axis Convention:** Z typically vertical (up) in slicing convention; document assumptions

### Error Handling
- **Mesh I/O:** Use `Box<dyn Error>` for simplicity; upgrade to custom `MeshError` enum if logic becomes complex
- **Settings:** Use `String` error messages for now (consistent with existing patterns); wrap in CliError for CLI exposure
- **CLI:** Map mesh/settings errors to appropriate CliError variants (Io, Invalid, Failed, etc.)

### Extensibility
- **Validator trait:** Allows future implementations for PrinterProfile, GlobalSettings, custom rule engines
- **SettingsDiff:** Serializable to enable tooling (diff viewers, logs, API responses)
- **Mesh module:** Isolated from CLI for library reuse (WASM, embedded)

### Performance Considerations
- **AABB caching:** Mesh::calculate_aabb() caches result in Mesh.aabb field to avoid recalculation
- **Transform overhead:** Each transform creates new Mesh; immutability preferred over performance for safety
- **STL parsing:** stl-io handles large files efficiently; no custom optimization needed initially

---

## 5. Testing Strategy

### Unit Tests (per-phase)
| Phase | Module | Tests |
|-------|--------|-------|
| 2 | mesh::types | Vertex construction & distance, Face area (Heron's), AABB bounds/center, Mesh creation |
| 3 | mesh::io | Load binary STL, load ASCII STL, missing file error, malformed file error |
| 4 | mesh::analysis | AABB calculation on cube, volume on closed/open mesh, surface area on cube |
| 5 | mesh::transforms | center_mesh result, drop_to_floor result, translate_mesh offset, immutability |
| 6 | settings::profile | Default PrinterProfile, serialize/deserialize round-trip, field validation |
| 7 | settings::params | SlicingParams, GlobalSettings, ObjectSettings serialize/deserialize |
| 8 | settings::validator | SettingValidator trait implementation, stub methods return Ok(()) |
| 9 | settings::diff | compare_settings with/without overrides, SettingsDiff structure |

### Integration Tests
1. Load sample STL → extract AABB/volume/surface area → compare to known values
2. Apply transform → verify new AABB reflects offset
3. Serialize settings to JSON → deserialize → compare fields match
4. Compare Global vs Object settings → verify override detection

### CLI Smoke Tests
```bash
# Test mesh loading
cargo run -- slice tests/fixtures/simple-cube.stl --verbose
# Expected: outputs AABB and volume

# Test settings validation
cargo run -- settings validate --global tests/fixtures/global.json --object tests/fixtures/object.json
# Expected: validation output (success or errors)

# Test settings diff
cargo run -- settings diff --global tests/fixtures/global.json --object tests/fixtures/object.json
# Expected: diff table showing overrides
```

### Build & Lint
```bash
cargo check
cargo test --release
cargo fmt && cargo clippy -- -D warnings
cargo doc --no-deps
```

---

## 6. Decisions & Rationale

| Decision | Rationale |
|----------|-----------|
| **stl-io crate** | Pure Rust, supports binary/ASCII STL, minimal dependencies, actively maintained, good documentation |
| **Separate mesh/ and settings/ modules** | Isolates concerns; allows reuse in non-CLI contexts (WASM, library consumers, embedded) |
| **STL native coordinates on load** | Flexibility: transforms on demand, not forced. Reversible. Aligns with API design principle (immutability). |
| **Transforms return new Mesh** | Immutability avoids side effects. Safe. Aligns with Rust idioms and functional programming patterns. |
| **Validator trait (not concrete)** | Extensible: PrinterProfile, SlicingParams, ObjectSettings can all implement trait. Enables rule customization. |
| **Basic validation scope** | Start minimal: struct defs + stubs. Real rules (layer height formula, temp ranges) in next PR. Reduces risk & scope creep. |
| **Diff shows both values** | More actionable than just field names. Enables logging, diff viewers, API responses to show impact of overrides. |
| **serde for serialization** | Industry standard, minimal boilerplate, supports JSON/YAML/TOML via feature flags |

---

## 7. Scope Boundaries

### Included in This Plan
- STL parser (binary + ASCII)
- AABB, volume, surface area calculations
- Mesh transformations (center, drop to floor, translate)
- PrinterProfile and SlicingParams type definitions with serde
- SettingValidator trait structure (stubs, no logic)
- Settings diff tool (comparison, not enforcement)
- CLI commands for mesh loading, settings validate, settings diff
- Test fixtures and comprehensive unit tests
- Documentation via doc comments

### Deliberately Excluded (Future PRs)
- Complex validation rules (layer height ≤ 0.8 × nozzle diameter, temperature ranges, acceleration profiles)
- GCode generation (future feature)
- Multi-format support (OBJ, 3MF, etc.; STL only for now)
- Mesh repair/healing (open edges, self-intersections, non-manifold geometry)
- Advanced transforms (scaling by factor, rotation by angle, mirror)
- Printer profile library/database (users provide JSON, no built-in presets)
- Config file format standardization (JSON acceptable for now; TOML/YAML added if needed)
- Performance profiling & optimization (use default Rust defaults until benchmarks show bottleneck)

---

## 8. Dependency Graph

```
Phase 1 (Setup)
├── TASK-1.1: Add crates to Cargo.toml
├── TASK-1.2: Create mesh/ module structure
├── TASK-1.3: Create settings/ module structure
└── TASK-1.4: Update lib.rs [depends on 1.2, 1.3]

Phase 2-5 (Mesh Implementation, mostly parallel)
├── TASK-2.1-2.4: Define mesh types [depends on 1.2]
├── TASK-3.1-3.3: STL parser [depends on 2.4, 1.1]
├── TASK-4.1-4.4: Analysis [depends on 2.4, parallel with 3]
└── TASK-5.1-5.4: Transforms [depends on 2.4]

Phase 6-9 (Settings, mostly sequential)
├── TASK-6.1-6.3: PrinterProfile [depends on 1.3]
├── TASK-7.1-7.4: SlicingParams [depends on 1.3]
├── TASK-8.1-8.4: Validator [depends on 7.1]
└── TASK-9.1-9.3: Diff Tool [depends on 7.2, 7.3]

Phase 10 (CLI Integration, depends on all above)
├── TASK-10.1: Update slice command [depends on 3.1, 4.1-4.3]
├── TASK-10.2: Add transform flags [depends on 5.1-5.2]
├── TASK-10.3: Create settings command [depends on 8.1, 9.2]
└── TASK-10.4: Register commands [depends on 10.3]

Phase 11 (Validation & Release)
├── TASK-11.1: Test coverage
├── TASK-11.2: Linting & formatting
├── TASK-11.3: Documentation
└── TASK-11.4: Cross-platform build
```

**Critical Path:**
TASK-1.1 → TASK-1.2, TASK-1.3 → TASK-2.1-2.4, TASK-7.1 → TASK-4.1-4.3, TASK-8.1-8.4, TASK-9.1-9.3 → TASK-10.1, 10.3 → TASK-10.4 → TASK-11.1-11.4

---

## 9. Success Criteria

### Phase Completion
1. **Phase 1:** Module structure created, `cargo check` succeeds
2. **Phase 2:** Mesh types defined, inline tests pass
3. **Phase 3:** STL parser reads binary/ASCII files, test fixtures load correctly
4. **Phase 4:** Geometry calculations accurate (±1% on test cube)
5. **Phase 5:** Transforms produce correct coordinate changes, original mesh unchanged
6. **Phase 6:** PrinterProfile serializes/deserializes, Default works
7. **Phase 7:** Settings structs round-trip through JSON
8. **Phase 8:** SettingValidator trait compiles, stubs callable
9. **Phase 9:** compare_settings() identifies overrides correctly
10. **Phase 10:** CLI loads mesh, settings command exists and routes, smoke tests pass
11. **Phase 11:** All tests pass, zero clippy warnings, release build succeeds

### Overall Success
- ✅ VG-1 complete: `cargo run -- slice <STL> --verbose` loads mesh, reports AABB/volume/surface area
- ✅ VG-2 complete: `cargo run -- settings validate` and `cargo run -- settings diff` work correctly
- ✅ Code quality: `cargo test --release` all pass, `cargo clippy -- -D warnings` zero warnings
- ✅ Documentation: All public items have doc comments, module hierarchy clear
- ✅ Ready for merge: Code review, no blockers, CI passing

---

## 10. Further Considerations

1. **Error Handling Approach**
   - *Current:* Use `Box<dyn Error>` for mesh I/O, `String` for validation errors
   - *Future:* If error types become complex, define custom `MeshError` and `ValidationError` enums with variants for specific failure modes

2. **Coordinate System Documentation**
   - *Action:* Add to mesh type doc comments: "Coordinates assumed in millimeters. Z-axis typically vertical (up). Handedness: right-handed coordinate system."
   - *Where:* `Mesh` struct docs, `read_stl()` function docs, README with coordinate system diagram

3. **Printer Profile Library**
   - *Current:* User provides JSON file for PrinterProfile
   - *Future:* Built-in preset profiles (Prusa i3 MK3S+, Ultimaker S5, etc.) via CLI subcommand or config directory

4. **Settings Validation Phasing**
   - *Phase 2 (this PR):* Validator trait + stubs
   - *Phase 3 (follow-up PR):* Implement ValidationRules methods (layer height ≤ 0.8 × nozzle diameter, positive values, range checks)
   - *Phase 4 (later):* Dependency checks (e.g., bed temp ≤ max bed temp for printer), cross-field validation

5. **Performance & Memory**
   - *Mesh loading:* For very large STL files (>100MB), consider streaming parser instead of loading all vertices into Vec. Profile if needed.
   - *AABB caching:* Current design caches AABB in Mesh struct. Re-calculate if vertices modified (currently immutable, so no issue).

6. **API Stability**
   - *mesh::* module: Part of public API; maintain semver compatibility in future releases
   - *settings::* module: Part of public API; be mindful when adding new settings fields (use Option or default values)

---

## 11. Related Documentation

- [AGENTS.md](AGENTS.md) — Project-wide guidance
- [architecture-cli-layer-1.md](architecture-cli-layer-1.md) — CLI layer foundation (Phase 1-4)
- [README.md](README.md) — User guide and feature overview (update with mesh loading, settings)
- [SETUP_COMPLETE.md](SETUP_COMPLETE.md) — Initial setup record

---

**Last Updated:** 2026-04-26  
**Status:** Ready for Implementation  
**Assigned to:** Slicer Engine Team  
**Est. Duration:** 15–20 hours (distributed across phases 1–11)

