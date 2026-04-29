---
goal: 'Feature: Derive Schema Descriptions from Rust Code to Angular UI'
version: 1.0
date_created: 2026-04-28
owner: max-scopp
status: 'Planned'
tags: [feature, documentation, schema, UI, schemars]
github_issue: 45
---

# Introduction

![Status: Planned](https://img.shields.io/badge/status-Planned-blue)

This plan implements **Issue #45**: Convert documentation and inline code comments to `schemars` description fields so that descriptions are centralized in Rust source code and automatically propagated to the Angular UI via generated JSON schemas.

**Goal**: Create a single source of truth for parameter documentation by deriving schema descriptions directly from Rust code, eliminating duplication and ensuring UI help text is always synchronized with the actual implementation.

## 1. Requirements & Constraints

### Functional Requirements
- **REQ-001**: All public fields in `SlicingParams` must have `schemars(description = "...")` attributes
- **REQ-002**: All enum variants affecting settings must have descriptions
- **REQ-003**: CLI command structures (`SliceCommand`, `InfoCommand`, etc.) must have descriptions
- **REQ-004**: JSON schema generation must include descriptions in `description` fields
- **REQ-005**: Angular UI must display descriptions from schema for user guidance
- **REQ-006**: All settings parameters must have descriptions derived from Rust source

### Non-Functional Requirements
- **NFR-001**: Descriptions must be clear, concise (1-2 sentences), and user-friendly
- **NFR-002**: Descriptions must include units and value ranges where applicable
- **NFR-003**: No breaking changes to the library API or CLI interface
- **NFR-004**: Generated schemas must remain backward compatible with existing consumers

### Constraints
- **CON-001**: schemars crate (v1.2.1) already in Cargo.toml; no new dependencies needed
- **CON-002**: Existing schema generation command (`gen_schemas`) must continue to work
- **CON-003**: All generated schemas must be committed to `ui/schemas/` directory
- **CON-004**: Documentation must not duplicate between Rust comments and schemars attributes
- **CON-005**: WASM builds must compile successfully after changes

### Guidelines
- **GUD-001**: Use structured Rust doc comments (`///`) for internal documentation; use `schemars(description)` for user-facing schema descriptions
- **GUD-002**: Keep descriptions to 1-2 sentences; use examples sparingly
- **GUD-003**: Include units for dimensional values (mm, °C, mm/s, mm/min, %)
- **GUD-004**: For fractional values, specify range and meaning (e.g., "0.0–1.0")

## 2. Implementation Steps

### Phase 1: Add Descriptions to SlicingParams (src/settings/params.rs)

**GOAL-P1**: Annotate all `SlicingParams` struct fields with `schemars(description = "...")` attributes.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P1-001** | `src/settings/params.rs` | Add `schemars` attribute to `layer_height` field | `#[schemars(description = "Layer height in mm (e.g., 0.2). Smaller values produce finer detail but increase print time.")]` |
| **TASK-P1-002** | `src/settings/params.rs` | Add `schemars` attribute to `wall_count` field | `#[schemars(description = "Maximum number of perimeter (wall) beads per layer. Arachne places up to this many concentric wall paths around each shell polygon.")]` |
| **TASK-P1-003** | `src/settings/params.rs` | Add `schemars` attribute to `wall_line_width_min` field | `#[schemars(description = "Minimum allowed bead width as a fraction of nozzle diameter (0.5–1.0). Beads narrower than this fraction are skipped.")]` |
| **TASK-P1-004** | `src/settings/params.rs` | Add `schemars` attribute to `wall_line_width_max` field | `#[schemars(description = "Maximum allowed bead width as a fraction of nozzle diameter (1.0–2.0). Variable-width beads are capped to avoid excessive over-extrusion.")]` |
| **TASK-P1-005** | `src/settings/params.rs` | Add `schemars` attribute to `wall_transition_threshold` field | `#[schemars(description = "Minimum wall thickness (as fraction of nozzle diameter) before bead count transitions. Avoids adding thin beads when space is narrow.")]` |
| **TASK-P1-006** | `src/settings/params.rs` | Add `schemars` attribute to `wall_transition_length` field | `#[schemars(description = "Length (mm) over which a bead-count transition is smoothed. Larger values produce gradual ramps; smaller values produce abrupt transitions.")]` |
| **TASK-P1-007** | `src/settings/params.rs` | Add `schemars` attribute to `wall_distribution_count` field | `#[schemars(description = "Number of inner walls that absorb width variation. Innermost beads widen proportionally to fill narrow gaps.")]` |
| **TASK-P1-008** | `src/settings/params.rs` | Add `schemars` attribute to `infill_density` field | `#[schemars(description = "Infill density as a fraction: 0.0 = hollow, 1.0 = solid. Typical range 0.1–0.3 for faster prints with good strength.")]` |
| **TASK-P1-009** | `src/settings/params.rs` | Add `schemars` attribute to `infill_pattern` field | `#[schemars(description = "Infill pattern type: 'rectilinear' (grid lines), 'grid', 'honeycomb', or 'gyroid' (smooth wavy). Defaults to 'rectilinear'.")]` |
| **TASK-P1-010** | `src/settings/params.rs` | Add `schemars` attribute to `infill_base_angle` field | `#[schemars(description = "Base angle in degrees for sparse infill lines (default 45°). Alternating layers rotate +90° on top of this base angle.")]` |
| **TASK-P1-011** | `src/settings/params.rs` | Add `schemars` attribute to `print_speed` field | `#[schemars(description = "Print speed in mm/s. Typical range 40–100 mm/s; slower speeds improve quality, faster speeds reduce print time.")]` |
| **TASK-P1-012** | `src/settings/params.rs` | Add `schemars` attribute to `nozzle_temp` field | `#[schemars(description = "Nozzle temperature in °C. Material-dependent: PLA 200–210°C, PETG 230–250°C, ABS 240–260°C.")]` |
| **TASK-P1-013** | `src/settings/params.rs` | Add `schemars` attribute to `bed_temp` field | `#[schemars(description = "Heated bed temperature in °C. PLA 60–80°C, PETG 80–100°C, ABS 100–120°C. Use 0 for unheated bed.")]` |
| **TASK-P1-014** | `src/settings/params.rs` | Add `schemars` attribute to `top_layers` field | `#[schemars(description = "Number of solid top layers (horizontal surfaces facing up). Typical: 4–6 layers for 0.2mm layer height.")]` |
| **TASK-P1-015** | `src/settings/params.rs` | Add `schemars` attribute to `bottom_layers` field | `#[schemars(description = "Number of solid bottom layers (horizontal surfaces facing down). Typical: 3–4 layers for bed adhesion.")]` |
| **TASK-P1-016** | `src/settings/params.rs` | Add `schemars` attribute to `surface_infill_angle` field | `#[schemars(description = "Angle in degrees for top/bottom surface infill lines (e.g., 45° for diagonal). Affects surface finish appearance.")]` |
| **TASK-P1-017** | `src/settings/params.rs` | Add `schemars` attribute to `filament_diameter_mm` field | `#[schemars(description = "Filament diameter in mm. Standard sizes: 1.75 mm (most common) or 2.85 mm. Used to calculate extrusion volume.")]` |
| **TASK-P1-018** | `src/settings/params.rs` | Add `schemars` attribute to `nozzle_diameter_mm` field | `#[schemars(description = "Nozzle diameter in mm. Standard: 0.4 mm. Affects minimum feature size and line width calculations.")]` |
| **TASK-P1-019** | `src/settings/params.rs` | Add `schemars` attribute to `travel_speed_mm_min` field | `#[schemars(description = "Non-print (travel) speed in mm/min (e.g., 9000 = 150 mm/s). Fast travel reduces print time but may affect print quality.")]` |
| **TASK-P1-020** | `src/settings/params.rs` | Add `schemars` attribute to `z_hop_mm` field | `#[schemars(description = "Z-hop height in mm applied during travel moves. Lifts nozzle to avoid stringing; typical: 0.2–0.5 mm. Set to 0 to disable.")]` |
| **TASK-P1-021** | `src/settings/params.rs` | Add `schemars` attribute to `retract_mm` field | `#[schemars(description = "Retraction distance in mm on travel moves. Pulls filament back to prevent stringing; typical: 3–5 mm.")]` |
| **TASK-P1-022** | `src/settings/params.rs` | Add `schemars` attribute to `only_one_wall_top` field | `#[schemars(description = "Use only outer wall on the last layer of top surfaces. Creates cleaner top finish with less pillowing and visible infill patterns.")]` |
| **TASK-P1-023** | `src/settings/params.rs` | Add `schemars` attribute to remaining fields | Repeat pattern for all other fields in `SlicingParams` (e.g., `only_one_wall_bottom`, `infill_before_walls`, etc.) |
| **TASK-P1-024** | `src/settings/params.rs` | Remove duplicate documentation | Delete doc comments (`///`) that duplicate schemars descriptions; keep high-level internal docs |

**Dependencies**: None (schemars already in Cargo.toml)  
**Completion Criteria**:
- [ ] All `SlicingParams` fields have `#[schemars(description = "...")]` attributes
- [ ] Each description is 1–2 sentences with units and typical ranges
- [ ] No duplicate documentation (remove redundant `///` comments)
- [ ] Code compiles without warnings

---

### Phase 2: Add Descriptions to CLI Command Structures (src/cli/commands/)

**GOAL-P2**: Annotate all CLI command structures and arguments with descriptions.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P2-001** | `src/cli/commands/slice.rs` | Add `schemars` to `SliceCommand` struct | Ensure all fields have descriptions |
| **TASK-P2-002** | `src/cli/commands/gen_schemas.rs` | Add `schemars` to `GenSchemasCommand` struct | Include descriptions for `output_dir`, `schema`, `pretty` |
| **TASK-P2-003** | `src/cli/commands/*.rs` (all command files) | Ensure clap `#[arg]` attributes have help text | Verify `help = "..."` is present on all arguments |
| **TASK-P2-004** | `src/cli/schemas.rs` | Add `schemars(description)` to all schema types | `ResultSchema`, `ErrorSchema`, `LogSchema`, `InfoResultSchema`, `SliceResultSchema`, `ValidateResultSchema`, `SettingsDiffSchema`, `DiffResultSchema`, `ShowResultSchema`, `GetResultSchema`, `SetResultSchema` |

**Dependencies**: None (uses existing schemars)  
**Completion Criteria**:
- [ ] All CLI command structs have field descriptions
- [ ] All schema structs in `src/cli/schemas.rs` have descriptions
- [ ] Code compiles without warnings
- [ ] Clap help text matches descriptions

---

### Phase 3: Verify Schema Generation Includes Descriptions

**GOAL-P3**: Ensure the schema generation pipeline properly extracts and includes descriptions from schemars attributes.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P3-001** | `src/cli/commands/gen_schemas.rs` | Run gen_schemas command locally | Execute: `cargo run -- gen-schemas --output-dir ./schemas --pretty true` |
| **TASK-P3-002** | `./schemas/slicer-engine-*.json` | Inspect generated JSON schemas | Verify `description` field is present in each field schema: `"properties": { "layer_height": { "description": "...", "type": "number" }, ... }` |
| **TASK-P3-003** | `src/cli/mod.rs` (tests or integration) | Write test verifying descriptions | Create test that loads schemas and asserts `description` field is non-empty for critical parameters |
| **TASK-P3-004** | Cargo.toml (if needed) | Ensure schemars features are enabled | Verify schemars is listed without feature restrictions (or with `derive` if available) |

**Dependencies**: Phase 1, Phase 2 completion  
**Completion Criteria**:
- [ ] `cargo run -- gen-schemas` executes successfully
- [ ] Generated JSON schemas contain `"description"` fields
- [ ] Sample schema file validates against JSON Schema spec
- [ ] Test passes: schemas have non-empty descriptions

---

### Phase 4: Update Angular UI to Display Descriptions

**GOAL-P4**: Integrate schema descriptions into the Angular UI for user-facing contextual help.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P4-001** | `ui/src/` (TypeScript schema model) | Load schema JSON files | Ensure Angular service/component loads schemas from `ui/schemas/slicer-engine-*.json` |
| **TASK-P4-002** | `ui/src/` (Settings component) | Extract descriptions from schema | Modify settings form component to read `schema.properties[fieldName].description` |
| **TASK-P4-003** | `ui/src/` (Settings component template) | Render descriptions in UI | Add tooltip, help icon, or collapsible description panel next to each setting field |
| **TASK-P4-004** | `ui/src/` (Settings component) | Display units and ranges from descriptions | Example: display "Layer height in mm" near input field |
| **TASK-P4-005** | `ui/src/` (Tests or e2e) | Test description rendering | Verify descriptions appear in Settings page for at least 3 key parameters |

**Dependencies**: Phase 3 completion (schemas must contain descriptions)  
**Completion Criteria**:
- [ ] Angular app loads schema JSON files without errors
- [ ] Settings component displays description text for at least one parameter
- [ ] Descriptions are styled appropriately (tooltip or help section)
- [ ] No breaking changes to existing UI

---

### Phase 5: Regenerate and Commit JSON Schemas

**GOAL-P5**: Generate final schemas with descriptions and commit to repository.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P5-001** | Terminal | Run schema generation | Execute: `cargo run -- gen-schemas --output-dir ./ui/schemas --pretty true` |
| **TASK-P5-002** | `ui/schemas/` | Verify generated schema files | Check that all `.json` files in `ui/schemas/` contain descriptions |
| **TASK-P5-003** | Git | Add schemas to staging | `git add ui/schemas/*.json` |
| **TASK-P5-004** | Git | Commit changes | `git commit -m "feat(schema): derive descriptions from Rust code via schemars (closes #45)"` |
| **TASK-P5-005** | `CHANGELOG.md` (if applicable) | Document schema change | Add entry: "Schema descriptions now auto-derived from Rust documentation" |

**Dependencies**: Phase 1–4 completion  
**Completion Criteria**:
- [ ] All schema files regenerated with descriptions
- [ ] Schema files committed to `ui/schemas/`
- [ ] Commit message references issue #45
- [ ] No uncommitted changes remain

---

### Phase 6: Testing & Validation

**GOAL-P6**: Comprehensive testing to ensure descriptions are correctly propagated and displayed.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P6-001** | `src/cli/` (tests) | Unit test: schemars descriptions present | Write test that asserts `schemars::schema_for!(SlicingParams).properties["layer_height"].description.is_some()` |
| **TASK-P6-002** | `src/cli/` (tests) | Unit test: enum descriptions | Test that enum variants have descriptions (if applicable) |
| **TASK-P6-003** | `src/` (integration tests) | Integration test: gen_schemas command | Verify `cargo run -- gen-schemas` produces valid JSON with descriptions |
| **TASK-P6-004** | `ui/` (Angular tests) | Component test: description rendering | Mock schema service and verify description text appears in DOM |
| **TASK-P6-005** | Manual validation | Smoke test Settings page | Manually verify descriptions display correctly for 5+ parameters |
| **TASK-P6-006** | `Cargo.toml` | Run full test suite | Execute `cargo test` and verify all tests pass |
| **TASK-P6-007** | Terminal | Build WASM target | Execute `wasm-pack build --target web` and verify no errors |

**Dependencies**: Phase 1–5 completion  
**Completion Criteria**:
- [ ] All unit tests pass
- [ ] Integration tests pass
- [ ] Angular component tests pass
- [ ] Manual smoke test successful
- [ ] `cargo test --release` passes (including WASM compilation check)
- [ ] No regressions in existing functionality

---

### Phase 7: Documentation & Review

**GOAL-P7**: Update project documentation and prepare for code review.

| Task | File | Action | Specific Details |
|------|------|--------|------------------|
| **TASK-P7-001** | `CONTRIBUTING.md` | Document schemars convention | Add section: "When adding new settings parameters, always include `#[schemars(description = \"...\")]` attributes." |
| **TASK-P7-002** | `src/settings/README.md` (if exists) | Update settings documentation | Explain that descriptions are auto-derived from Rust source |
| **TASK-P7-003** | `ui/README.md` | Document schema-driven help text | Explain how Angular loads descriptions from schema files |
| **TASK-P7-004** | Pull Request | Create PR with all changes | Include link to issue #45, summary of changes, and testing performed |
| **TASK-P7-005** | Pull Request | Address review comments | Respond to and fix any reviewer feedback |

**Dependencies**: Phase 1–6 completion  
**Completion Criteria**:
- [ ] Documentation updated
- [ ] PR created and linked to issue #45
- [ ] Code review completed successfully
- [ ] All requested changes implemented

---

## 3. Implementation Order & Dependencies

```
Phase 1 (SlicingParams annotations)
    ↓
Phase 2 (CLI command annotations)
    ↓
Phase 3 (Verify schema generation)
    ↓
Phase 4 (Angular UI integration)
    ↓
Phase 5 (Regenerate & commit schemas)
    ↓
Phase 6 (Testing & validation)
    ↓
Phase 7 (Documentation & review)
```

**Critical Path**: Phases 1 → 3 → 4 → 6 (minimum)  
**Recommended Approach**: Execute sequentially to allow testing at each phase before proceeding.

---

## 4. Files Affected

### Core Changes
- `src/settings/params.rs` — Add schemars descriptions to SlicingParams
- `src/cli/schemas.rs` — Add descriptions to all schema types
- `src/cli/commands/*.rs` — Add descriptions to command structures

### Generated/Committed
- `ui/schemas/*.json` — Regenerated with descriptions

### Angular Frontend
- `ui/src/` (components, services) — Display descriptions in UI

### Documentation
- `CONTRIBUTING.md` — Document schemars convention
- `README.md` (optional) — Mention schema-driven help

---

## 5. Rollback Strategy

If issues arise:

1. **Revert Rust changes**: `git revert <commit-hash>` for Phase 1–2 code
2. **Restore old schemas**: `git checkout HEAD~1 ui/schemas/`
3. **Rebuild UI with old schemas**: Angular will fall back to displaying field names only
4. **Impact**: Minimal; UI gracefully degrades if schema descriptions are missing

---

## 6. Success Criteria

✅ **Phase 1–2**: All Rust code compiles without warnings; schemars annotations present  
✅ **Phase 3**: Generated JSON schemas contain non-empty `description` fields  
✅ **Phase 4**: Angular UI renders descriptions next to settings parameters  
✅ **Phase 5**: Schemas committed; no uncommitted changes  
✅ **Phase 6**: All tests pass; WASM builds successfully  
✅ **Phase 7**: Documentation updated; PR merged

---

## 7. Estimated Effort

| Phase | Effort | Time |
|-------|--------|------|
| Phase 1 (SlicingParams) | 25–30 attributes × 2 min each | ~60 min |
| Phase 2 (CLI commands) | 10–15 structs × 5 min each | ~75 min |
| Phase 3 (Schema verification) | Code inspection, 1 test | ~30 min |
| Phase 4 (Angular integration) | Component update, template modification | ~120 min |
| Phase 5 (Regenerate & commit) | Schema generation, git commit | ~15 min |
| Phase 6 (Testing) | Unit, integration, manual tests | ~90 min |
| Phase 7 (Documentation & review) | Update docs, PR, review cycle | ~60 min |
| **Total** | | **~450 min (~7.5 hours)** |

---

## 8. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|-----------|
| Duplicate docs (Rust + schemars) | High | Low | Establish convention: remove `///` if description exists in schemars |
| Schema generation fails | Low | High | Test on all platforms (native, WASM); run `cargo test --release` |
| Angular UI breaks on missing description | Low | Medium | Graceful fallback: UI displays field name if description absent |
| Backward compatibility break | Very Low | High | Ensure schema structure unchanged; only add/populate descriptions |
| Large PR difficult to review | Medium | Medium | Break into smaller PRs per phase if needed |

---

## 9. Next Steps

1. **Start with Phase 1**: Begin adding `schemars(description)` attributes to `SlicingParams` fields
2. **Run gen_schemas**: Verify descriptions appear in generated JSON
3. **Test locally**: Manually inspect generated schema files before committing
4. **Create PR**: Link to issue #45 with summary of all changes
5. **Iterate**: Address review feedback and complete remaining phases

---

**Author**: Implementation Plan (Issue #45)  
**Last Updated**: 2026-04-28  
**Status**: Ready for implementation
