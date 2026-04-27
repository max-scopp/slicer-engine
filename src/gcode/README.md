# `gcode` — G-code generation

Converts `Vec<SliceLayer>` → a firmware-ready G-code `String`.

---

## Module layout

```
gcode/
├── mod.rs          re-exports; module-level docs
├── flavor.rs       GcodeFlavor enum (Marlin | Klipper)
├── dialect.rs      GcodeDialect trait + WarnFn
├── generator.rs    GcodeGenerator façade + generate_gcode()
├── source.rs       resolve_gcode_source() file/string resolver
└── dialects/
    ├── mod.rs      re-exports
    ├── marlin.rs   MarlinDialect  (M104/M109/M140/M190 etc.)
    └── klipper.rs  KlipperDialect (START_PRINT / END_PRINT macros)
```

---

## Call flow

```mermaid
flowchart TD
    caller["Caller\n(CLI / WebSocket)"]
    gen["GcodeGenerator::new(flavor)\n.with_*(…)\n.generate(layers, params)"]
    header["① Write metadata header"]
    start["② Emit start script\n(custom override or dialect default)"]
    layers["③ For each SliceLayer"]
    markers["lifecycle markers block\nLAYER_CHANGE · Z · HEIGHT\nBEFORE · reset E · Z move · AFTER"]
    paths["For each path in layer"]
    retract["retract → z-hop → travel → lower → un-retract"]
    extrude["extrude segments\n(compute E per move)"]
    end["④ Emit end script"]
    out["G-code String"]

    caller --> gen --> header --> start --> layers
    layers --> markers --> paths --> retract --> extrude
    extrude --> paths
    paths --> layers
    layers --> end --> out
```

---

## Dialect abstraction

```mermaid
classDiagram
    class GcodeDialect {
        <<trait>>
        +flavor_name() &str
        +start_script(params) Vec~String~
        +end_script() Vec~String~
        +unsupported_commands() &[&str]
        +move_extrude(x,y,e,f) String
        +move_z(z,f) String
        +travel_xy(x,y,f) String
        +set_fan_speed(speed) String
        ...default impls for all moves...
    }
    class MarlinDialect {
        G21 G90 M82 M104 M140 G28 M109 M190
    }
    class KlipperDialect {
        START_PRINT / END_PRINT macros
        +set_velocity_limit(v,a)
        +set_pressure_advance(pa)
        +call_macro(name)
    }
    GcodeDialect <|-- MarlinDialect
    GcodeDialect <|-- KlipperDialect

    class GcodeGenerator {
        -dialect Box~dyn GcodeDialect~
        -marker_config LifecycleMarkerConfig
        -custom_start_script Option~Vec~
        -custom_end_script Option~Vec~
        +new(flavor) Self
        +with_dialect(d) Self
        +with_warn_fn(f) Self
        +with_lifecycle_markers(bool) Self
        +with_marker_config(cfg) Self
        +with_start_script(lines) Self
        +with_end_script(lines) Self
        +generate(layers, params) String
    }
    GcodeGenerator --> GcodeDialect
```

---

## Extrusion math

For each XY segment of length *L* the required filament advance is:

```
E = L × (layer_height × nozzle_ø) / (π × (filament_ø/2)²)
```

This is the **volumetric flow balance**: the rectangular cross-section of the
deposited bead `(layer_height × nozzle_ø)` must equal the volume of filament
pushed through `(π r² × E)`.

Defaults: filament ø = 1.75 mm, nozzle ø = 0.40 mm.

---

## Travel sequence per path

Every path (closed contour or infill line) is surrounded by a
**retract / z-hop / travel / lower / un-retract** guard to prevent stringing:

```mermaid
sequenceDiagram
    participant P as Previous path end
    participant H as Hotend
    participant N as Next path start

    P->>H: G1 E-1.0 (retract 1 mm)
    H->>H: G1 Z+0.2 (z-hop)
    H->>N: G1 X… Y… F9000 (travel)
    N->>H: G1 Z (lower back)
    H->>H: G1 E+1.0 (un-retract)
    Note over H,N: then extrude contour at print speed
```

---

## Lifecycle markers

When `LifecycleMarkerConfig::enabled` is `true` (default), each layer emits a
structured block compatible with OrcaSlicer / PrusaSlicer post-processors:

```
;LAYER_CHANGE
;Z:{z}
;HEIGHT:{height}
;BEFORE_LAYER_CHANGE
;{z}            ← bare numeric marker for post-processing scripts
G92 E0          ← extruder reset (E tracking restarts each layer)
G1 Z{z} F9000
;AFTER_LAYER_CHANGE
;{z}

;TYPE:{role}    ← emitted once per extrusion-role transition
;WIDTH:{w}mm
```

All marker strings are **templates**: `{z}`, `{height}`, `{type}`, `{width}`
are substituted at render time via `render_marker()`.  Per-flavor overrides
are stored in `GlobalSettings::lifecycle_markers` (keyed by flavor name).

---

## Script priority chain

```
CLI --start-gcode argument
        ↓  (overrides)
GlobalSettings.start_print_gcode
        ↓  (overrides)
GcodeDialect::start_script()  ← firmware default
```

`resolve_gcode_source(input)` auto-detects whether `input` is a file path or
an inline G-code string (1 MiB file size limit enforced).
