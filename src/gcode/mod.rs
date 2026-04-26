//! G-code generation from sliced layers.
//!
//! Converts a `Vec<SliceLayer>` and a `SlicingParams` into a G-code string
//! suitable for FFF (fused-filament fabrication) printers.
//!
//! ## Architecture
//!
//! G-code output is routed through a **dialect abstraction layer** that keeps
//! firmware-specific command differences isolated behind a single trait:
//!
//! ```text
//! GcodeGenerator  ──uses──►  GcodeDialect (trait)
//!                                 ▲           ▲
//!                          MarlinDialect  KlipperDialect
//! ```
//!
//! [`GcodeGenerator`] is the façade: callers create it with a [`GcodeFlavor`]
//! and call [`GcodeGenerator::generate`].  All firmware-specific lines are
//! emitted by the backing [`GcodeDialect`] implementation.
//!
//! ## Supported flavors
//!
//! | Flavor  | Status        | Notes                                        |
//! |---------|---------------|----------------------------------------------|
//! | Marlin  | First-class   | Standard M-command set, wide compatibility   |
//! | Klipper | First-class   | `SET_VELOCITY_LIMIT`, `SET_PRESSURE_ADVANCE` |
//!
//! ## Example
//!
//! ```rust
//! use slicer_engine::gcode::{GcodeGenerator, GcodeFlavor};
//! use slicer_engine::settings::params::SlicingParams;
//!
//! let gen = GcodeGenerator::new(GcodeFlavor::Klipper);
//! let gcode = gen.generate(&[], &SlicingParams::default());
//! assert!(gcode.contains("SET_VELOCITY_LIMIT"));
//! ```

pub mod dialects;

pub use dialects::{KlipperDialect, MarlinDialect};

use crate::core::SliceLayer;
use crate::settings::params::SlicingParams;
use std::str::FromStr;

/// Default filament diameter in mm (standard 1.75 mm PLA/PETG/etc.).
const FILAMENT_DIAMETER_MM: f64 = 1.75;

/// Default nozzle diameter in mm.
const NOZZLE_DIAMETER_MM: f64 = 0.4;

/// Travel (non-print) speed in mm/min.
const TRAVEL_SPEED_MM_MIN: f64 = 9000.0;

/// Z-hop height above the current layer during travel moves (mm).
const Z_HOP_MM: f64 = 0.2;

/// Retraction distance on travel moves (mm).
const RETRACT_MM: f64 = 1.0;

/// Compute the extrusion length (mm of filament) needed to print a straight
/// line of length `move_len` at the given `layer_height` with the default
/// nozzle and filament diameters.
///
/// Formula: E = line_length × (layer_height × nozzle_diameter) / (π × filament_radius²)
fn extrusion_for_move(move_len: f64, layer_height: f64) -> f64 {
    let filament_radius = FILAMENT_DIAMETER_MM / 2.0;
    let cross_section = layer_height * NOZZLE_DIAMETER_MM;
    let filament_area = std::f64::consts::PI * filament_radius.powi(2);
    move_len * cross_section / filament_area
}

// ── Flavor enum ────────────────────────────────────────────────────────────────

/// Supported G-code firmware flavors.
///
/// Each variant selects the concrete [`GcodeDialect`] used by [`GcodeGenerator`].
/// Only **Marlin** and **Klipper** are first-class citizens; additional flavors
/// will be added in future releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcodeFlavor {
    /// Marlin firmware: standard M-command set, widely compatible with consumer FDM printers.
    #[default]
    Marlin,
    /// Klipper firmware: supports `SET_VELOCITY_LIMIT`, `SET_PRESSURE_ADVANCE`, and custom macros.
    Klipper,
}

impl FromStr for GcodeFlavor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "marlin" => Ok(Self::Marlin),
            "klipper" => Ok(Self::Klipper),
            _ => Err(format!(
                "Unknown G-code flavor '{}'. Supported: marlin, klipper",
                s
            )),
        }
    }
}

impl std::fmt::Display for GcodeFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Marlin => write!(f, "marlin"),
            Self::Klipper => write!(f, "klipper"),
        }
    }
}

// ── Dialect trait ──────────────────────────────────────────────────────────────

/// Abstraction over firmware-specific G-code command emission.
///
/// Implement this trait to add support for a new printer firmware flavor.
/// The three **required** methods — [`GcodeDialect::flavor_name`],
/// [`GcodeDialect::start_script`], and [`GcodeDialect::end_script`] — are the
/// minimum needed to produce valid output, because every other method has a
/// default implementation based on standard G-code syntax.
///
/// However, firmware-specific features (e.g. `SET_VELOCITY_LIMIT` for Klipper,
/// or custom fan-speed curves for specialty firmware) should be added either by
/// overriding the relevant default methods or by exposing extra methods directly
/// on the concrete struct.
///
/// Use [`GcodeDialect::unsupported_commands`] to advertise commands that this
/// dialect cannot handle natively; the [`GcodeGenerator`] will emit a warning
/// via its registered warn function before falling back to the standard
/// implementation.
///
/// All dimensional values use millimetres; speeds use **mm/min** (the native
/// unit for G-code `F` parameters).
pub trait GcodeDialect: Send + Sync {
    /// Human-readable name of this dialect (used in header comments).
    fn flavor_name(&self) -> &'static str;

    /// Emit the complete start sequence for the given slicing parameters.
    ///
    /// Typically includes unit mode, positioning mode, temperature targets,
    /// homing, and any firmware-specific preamble.
    fn start_script(&self, params: &SlicingParams) -> Vec<String>;

    /// Emit the complete end sequence.
    ///
    /// Typically includes cooling, final retract, nozzle park, and motor-off.
    fn end_script(&self) -> Vec<String>;

    /// List of command identifiers not natively supported by this dialect.
    ///
    /// When [`GcodeGenerator`] encounters a command in this list it emits a
    /// warning via the registered warn function before falling back to the
    /// default standard G-code implementation.
    ///
    /// Command names should correspond to the method names on this trait
    /// (e.g. `"set_fan_speed"`, `"set_nozzle_temp"`).  Returns an empty slice
    /// by default — i.e. all commands are assumed supported.
    fn unsupported_commands(&self) -> &'static [&'static str] {
        &[]
    }

    /// Format a standalone comment line.
    fn comment(&self, text: &str) -> String {
        format!("; {}", text)
    }

    /// Set extruder (nozzle) temperature.
    ///
    /// When `wait` is `true` the firmware blocks until target is reached
    /// (`M109`); otherwise it sets the target and returns immediately (`M104`).
    fn set_nozzle_temp(&self, temp: f64, wait: bool) -> String {
        if wait {
            format!("M109 S{:.0}", temp)
        } else {
            format!("M104 S{:.0}", temp)
        }
    }

    /// Set heated-bed temperature.
    ///
    /// When `wait` is `true` the firmware blocks until target is reached
    /// (`M190`); otherwise it sets the target and returns immediately (`M140`).
    fn set_bed_temp(&self, temp: f64, wait: bool) -> String {
        if wait {
            format!("M190 S{:.0}", temp)
        } else {
            format!("M140 S{:.0}", temp)
        }
    }

    /// Move to `(x, y)` while extruding filament to absolute E position `e`
    /// at `speed_mm_min` mm/min.
    fn move_extrude(&self, x: f64, y: f64, e: f64, speed_mm_min: f64) -> String {
        format!("G1 X{:.3} Y{:.3} E{:.5} F{:.0}", x, y, e, speed_mm_min)
    }

    /// Move the Z axis to `z` at `speed_mm_min` mm/min (no extrusion).
    fn move_z(&self, z: f64, speed_mm_min: f64) -> String {
        format!("G1 Z{:.3} F{:.0}", z, speed_mm_min)
    }

    /// Travel (non-extrusion) move in XY at `speed_mm_min` mm/min.
    fn travel_xy(&self, x: f64, y: f64, speed_mm_min: f64) -> String {
        format!("G1 X{:.3} Y{:.3} F{:.0}", x, y, speed_mm_min)
    }

    /// Set part-cooling fan speed.
    ///
    /// `speed` is a normalised fraction `0.0` (off) to `1.0` (full).
    /// Emits `M107` when speed rounds to zero, `M106 S<value>` otherwise.
    fn set_fan_speed(&self, speed: f64) -> String {
        let s = (speed.clamp(0.0, 1.0) * 255.0).round() as u8;
        if s == 0 {
            "M107".to_string()
        } else {
            format!("M106 S{}", s)
        }
    }

    /// Home all axes (`G28`).
    fn home_axes(&self) -> String {
        "G28".to_string()
    }

    /// Set the extruder to an absolute `e` position at `speed_mm_min` mm/min.
    ///
    /// Used for both retraction (negative delta) and priming (positive delta).
    fn set_extruder_pos(&self, e: f64, speed_mm_min: f64) -> String {
        format!("G1 E{:.5} F{:.0}", e, speed_mm_min)
    }

    /// Reset the extruder position counter to zero (`G92 E0`).
    fn reset_extruder(&self) -> String {
        "G92 E0".to_string()
    }
}

// ── Generator (façade) ─────────────────────────────────────────────────────────

/// Boxed warning callback type used by [`GcodeGenerator`].
///
/// Receives a human-readable message whenever the active dialect signals that
/// a command is not natively supported.
pub type WarnFn = Box<dyn Fn(&str)>;

/// High-level G-code generator that delegates all firmware-specific command
/// emission to a [`GcodeDialect`] implementation.
///
/// `GcodeGenerator` is the **façade** of the multi-flavor framework: it owns
/// the per-layer extrusion logic while the dialect handles the command syntax.
///
/// An optional **warn function** can be registered via [`GcodeGenerator::with_warn_fn`].
/// It is called when the active dialect advertises unsupported commands (see
/// [`GcodeDialect::unsupported_commands`]), so callers can surface those
/// warnings through the appropriate logging channel.
///
/// # Example
///
/// ```rust
/// use slicer_engine::gcode::{GcodeGenerator, GcodeFlavor};
/// use slicer_engine::settings::params::SlicingParams;
///
/// let gen = GcodeGenerator::new(GcodeFlavor::Marlin);
/// let gcode = gen.generate(&[], &SlicingParams::default());
/// assert!(gcode.contains("G28"));
/// ```
pub struct GcodeGenerator {
    dialect: Box<dyn GcodeDialect>,
    warn_fn: Option<WarnFn>,
    lifecycle_markers: bool,
}

impl GcodeGenerator {
    /// Create a generator for the specified firmware flavor.
    pub fn new(flavor: GcodeFlavor) -> Self {
        let dialect: Box<dyn GcodeDialect> = match flavor {
            GcodeFlavor::Marlin => Box::new(MarlinDialect),
            GcodeFlavor::Klipper => Box::new(KlipperDialect),
        };
        Self {
            dialect,
            warn_fn: None,
            lifecycle_markers: true,
        }
    }

    /// Create a generator with a custom [`GcodeDialect`] implementation.
    ///
    /// Useful for testing or for dialects not covered by [`GcodeFlavor`].
    pub fn with_dialect(dialect: Box<dyn GcodeDialect>) -> Self {
        Self {
            dialect,
            warn_fn: None,
            lifecycle_markers: true,
        }
    }

    /// Register a warn callback invoked when the dialect signals unsupported commands.
    ///
    /// The function receives a human-readable warning message and is responsible
    /// for routing it to the appropriate output channel (e.g. [`crate::cli::emit::Emitter::log_warn`]).
    ///
    /// ```rust
    /// use slicer_engine::gcode::{GcodeGenerator, GcodeFlavor};
    ///
    /// let gen = GcodeGenerator::new(GcodeFlavor::Marlin)
    ///     .with_warn_fn(|msg| eprintln!("[warn] {}", msg));
    /// ```
    pub fn with_warn_fn(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.warn_fn = Some(Box::new(f));
        self
    }

    /// Configure whether layer lifecycle markers are emitted in the output.
    ///
    /// When `true` (the default) each layer block is preceded by:
    /// `;LAYER_CHANGE`, `;Z:`, `;HEIGHT:`, `;BEFORE_LAYER_CHANGE`, `G92 E0`,
    /// `;AFTER_LAYER_CHANGE` and `;TYPE:` / `;WIDTH:` annotations at each
    /// extrusion-role transition.
    ///
    /// Set to `false` to emit a minimal "; layer z=…" comment instead.
    pub fn with_lifecycle_markers(mut self, enabled: bool) -> Self {
        self.lifecycle_markers = enabled;
        self
    }

    /// Return a reference to the active dialect.
    pub fn dialect(&self) -> &dyn GcodeDialect {
        self.dialect.as_ref()
    }

    /// Emit a warning through the registered warn function, if any.
    fn warn(&self, msg: &str) {
        if let Some(f) = &self.warn_fn {
            f(msg);
        }
    }

    /// Generate a complete G-code program from the given layers and parameters.
    ///
    /// The output is a single `String` with lines separated by `'\n'`.
    /// Returns a minimal (start + end only) program when `layers` is empty.
    ///
    /// If any commands are listed in [`GcodeDialect::unsupported_commands`] the
    /// registered warn function is called once per unsupported command before
    /// generation begins.
    pub fn generate(&self, layers: &[SliceLayer], params: &SlicingParams) -> String {
        // Warn about any commands the dialect doesn't natively support
        for cmd in self.dialect.unsupported_commands() {
            self.warn(&format!(
                "Command '{}' is not natively supported by the {} dialect; \
                 falling back to generic G-code",
                cmd,
                self.dialect.flavor_name()
            ));
        }

        let mut out = String::with_capacity(64 * 1024);
        let print_speed_mm_min = params.print_speed * 60.0;

        // ── Generator-level header ───────────────────────────────────────────
        out.push_str(&format!(
            "; Generated by slicer-engine | flavor: {}\n",
            self.dialect.flavor_name()
        ));

        // ── Start script (flavor-specific) ───────────────────────────────────
        for line in self.dialect.start_script(params) {
            out.push_str(&line);
            out.push('\n');
        }

        // ── Per-layer contours ────────────────────────────────────────────────
        let mut e_total = 0.0_f64;

        for layer in layers {
            if self.lifecycle_markers {
                // Lifecycle block: LAYER_CHANGE → BEFORE_LAYER_CHANGE → Z move → AFTER_LAYER_CHANGE
                out.push_str(";LAYER_CHANGE\n");
                out.push_str(&format!(";Z:{:.3}\n", layer.z));
                out.push_str(&format!(";HEIGHT:{:.3}\n", params.layer_height));
                out.push_str(";BEFORE_LAYER_CHANGE\n");
                out.push_str(&format!(";{:.3}\n", layer.z));
                // Reset extruder position at layer start
                out.push_str(&format!("{}\n", self.dialect.reset_extruder()));
                e_total = 0.0;
                out.push_str(&format!(
                    "{}\n",
                    self.dialect.move_z(layer.z, TRAVEL_SPEED_MM_MIN)
                ));
                out.push_str(";AFTER_LAYER_CHANGE\n");
                out.push_str(&format!(";{:.3}\n", layer.z));
            } else {
                out.push_str(&format!("; layer z={:.3}\n", layer.z));
                out.push_str(&format!(
                    "{}\n",
                    self.dialect.move_z(layer.z, TRAVEL_SPEED_MM_MIN)
                ));
            }

            let mut last_role: Option<crate::core::ExtrusionRole> = None;

            for (path_idx, path) in layer.paths.iter().enumerate() {
                let points: Vec<(f64, f64)> = path.iter().map(|p| (p.x(), p.y())).collect();
                if points.len() < 2 {
                    continue;
                }

                // Emit ;TYPE: / ;WIDTH: annotation when the extrusion role changes
                if self.lifecycle_markers {
                    let role = layer.role_for_path(path_idx);
                    if last_role != Some(role) {
                        out.push_str(&format!(";TYPE:{}\n", role.type_name()));
                        out.push_str(&format!(";WIDTH:{:.2}mm\n", role.default_width_mm()));
                        last_role = Some(role);
                    }
                }

                let (start_x, start_y) = points[0];

                // Retract, z-hop, travel, lower, prime
                e_total -= RETRACT_MM;
                out.push_str(&format!(
                    "{} ; retract\n",
                    self.dialect.set_extruder_pos(e_total, 3000.0)
                ));
                out.push_str(&format!(
                    "{} ; z-hop\n",
                    self.dialect.move_z(layer.z + Z_HOP_MM, TRAVEL_SPEED_MM_MIN)
                ));
                out.push_str(&format!(
                    "{} ; travel\n",
                    self.dialect
                        .travel_xy(start_x, start_y, TRAVEL_SPEED_MM_MIN)
                ));
                out.push_str(&format!(
                    "{} ; lower\n",
                    self.dialect.move_z(layer.z, TRAVEL_SPEED_MM_MIN)
                ));
                e_total += RETRACT_MM;
                out.push_str(&format!(
                    "{} ; un-retract\n",
                    self.dialect.set_extruder_pos(e_total, 3000.0)
                ));

                // Print the contour segments
                let mut prev = points[0];
                for &(x, y) in points.iter().skip(1) {
                    let dx = x - prev.0;
                    let dy = y - prev.1;
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 1e-6 {
                        prev = (x, y);
                        continue;
                    }
                    e_total += extrusion_for_move(len, params.layer_height);
                    out.push_str(&format!(
                        "{}\n",
                        self.dialect.move_extrude(x, y, e_total, print_speed_mm_min)
                    ));
                    prev = (x, y);
                }

                // Close the contour
                let dx = start_x - prev.0;
                let dy = start_y - prev.1;
                let len = (dx * dx + dy * dy).sqrt();
                if len >= 1e-6 {
                    e_total += extrusion_for_move(len, params.layer_height);
                    out.push_str(&format!(
                        "{} ; close contour\n",
                        self.dialect
                            .move_extrude(start_x, start_y, e_total, print_speed_mm_min)
                    ));
                }
            }
        }

        // ── End script (flavor-specific) ─────────────────────────────────────
        for line in self.dialect.end_script() {
            out.push_str(&line);
            out.push('\n');
        }

        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Generate a G-code string from a slice result using the default Marlin dialect.
///
/// This is a convenience wrapper around [`GcodeGenerator::new`] with
/// [`GcodeFlavor::Marlin`].  Prefer [`GcodeGenerator`] directly when you need
/// to select a specific firmware flavor.
///
/// # Arguments
/// * `layers` – ordered bottom-to-top slice layers produced by [`crate::core::slice_mesh`]
/// * `params` – slicing parameters (temperatures, speeds, layer height, …)
///
/// # Returns
/// A `String` containing the full G-code program.  Returns a minimal
/// (start + end only) program when `layers` is empty.
///
/// # Example
/// ```
/// use slicer_engine::gcode::generate_gcode;
/// use slicer_engine::settings::params::SlicingParams;
///
/// let gcode = generate_gcode(&[], &SlicingParams::default());
/// assert!(gcode.contains("G28"));
/// assert!(gcode.contains("M104 S0"));
/// ```
pub fn generate_gcode(layers: &[SliceLayer], params: &SlicingParams) -> String {
    GcodeGenerator::new(GcodeFlavor::Marlin).generate(layers, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SliceLayer;

    #[test]
    fn test_generate_gcode_empty_layers_contains_header() {
        let gcode = generate_gcode(&[], &SlicingParams::default());
        assert!(gcode.contains("G28"), "missing G28 home");
        assert!(gcode.contains("G21"), "missing G21 mm mode");
        assert!(gcode.contains("M104 S210"), "missing nozzle temp");
        assert!(gcode.contains("M140 S60"), "missing bed temp");
    }

    #[test]
    fn test_generate_gcode_empty_layers_contains_footer() {
        let gcode = generate_gcode(&[], &SlicingParams::default());
        assert!(gcode.contains("M104 S0"), "missing nozzle off");
        assert!(gcode.contains("M140 S0"), "missing bed off");
        assert!(gcode.contains("M84"), "missing motors off");
    }

    #[test]
    fn test_generate_gcode_layer_z_appears() {
        let layer = SliceLayer::new(1.0);
        let gcode = generate_gcode(&[layer], &SlicingParams::default());
        // With lifecycle markers on by default, expect LAYER_CHANGE block
        assert!(
            gcode.contains(";LAYER_CHANGE"),
            "missing LAYER_CHANGE marker: {gcode}"
        );
        assert!(
            gcode.contains(";Z:1.000"),
            "missing ;Z: annotation: {gcode}"
        );
        assert!(gcode.contains("G1 Z1.000"), "missing Z move");
    }

    #[test]
    fn test_generate_gcode_with_contour() {
        use clipper2::Path;
        let mut layer = SliceLayer::new(0.2);
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(square);

        let gcode = generate_gcode(&[layer], &SlicingParams::default());
        assert!(gcode.contains(" E"), "no extrusion moves in gcode");
        assert!(gcode.contains("X0.000 Y0.000"), "missing start travel");
    }

    #[test]
    fn test_extrusion_for_move_positive() {
        let e = extrusion_for_move(10.0, 0.2);
        assert!(e > 0.0, "extrusion must be positive");
    }

    // ── Flavor enum ────────────────────────────────────────────────────────────

    #[test]
    fn test_gcode_flavor_from_str() {
        assert_eq!(
            "marlin".parse::<GcodeFlavor>().unwrap(),
            GcodeFlavor::Marlin
        );
        assert_eq!(
            "klipper".parse::<GcodeFlavor>().unwrap(),
            GcodeFlavor::Klipper
        );
        assert_eq!(
            "Marlin".parse::<GcodeFlavor>().unwrap(),
            GcodeFlavor::Marlin
        );
        assert_eq!(
            "KLIPPER".parse::<GcodeFlavor>().unwrap(),
            GcodeFlavor::Klipper
        );
    }

    #[test]
    fn test_gcode_flavor_from_str_invalid() {
        let err = "reprap".parse::<GcodeFlavor>().unwrap_err();
        assert!(err.contains("reprap"), "error should mention the bad value");
        assert!(
            err.contains("marlin") && err.contains("klipper"),
            "error should list supported flavors"
        );
    }

    #[test]
    fn test_gcode_flavor_display() {
        assert_eq!(GcodeFlavor::Marlin.to_string(), "marlin");
        assert_eq!(GcodeFlavor::Klipper.to_string(), "klipper");
    }

    #[test]
    fn test_gcode_flavor_default_is_marlin() {
        assert_eq!(GcodeFlavor::default(), GcodeFlavor::Marlin);
    }

    // ── GcodeGenerator ─────────────────────────────────────────────────────────

    #[test]
    fn test_generator_marlin_contains_standard_header() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[], &SlicingParams::default());
        assert!(gcode.contains("G21"), "missing unit mode");
        assert!(gcode.contains("G28"), "missing home");
        assert!(gcode.contains("M104 S210"), "missing nozzle temp");
        assert!(gcode.contains("M140 S60"), "missing bed temp");
    }

    #[test]
    fn test_generator_marlin_contains_standard_footer() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[], &SlicingParams::default());
        assert!(gcode.contains("M104 S0"), "missing nozzle off");
        assert!(gcode.contains("M140 S0"), "missing bed off");
        assert!(gcode.contains("M84"), "missing motors off");
    }

    #[test]
    fn test_generator_klipper_has_velocity_limit() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Klipper).generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("SET_VELOCITY_LIMIT"),
            "Klipper gcode missing SET_VELOCITY_LIMIT: {gcode}"
        );
    }

    #[test]
    fn test_generator_klipper_flavor_name_in_header() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Klipper).generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("Klipper"),
            "header should mention Klipper flavor"
        );
    }

    #[test]
    fn test_generator_marlin_flavor_name_in_header() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("Marlin"),
            "header should mention Marlin flavor"
        );
    }

    #[test]
    fn test_generator_klipper_layer_and_contour() {
        use clipper2::Path;
        let mut layer = SliceLayer::new(0.2);
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(square);

        let gcode =
            GcodeGenerator::new(GcodeFlavor::Klipper).generate(&[layer], &SlicingParams::default());
        // Lifecycle markers are on by default
        assert!(
            gcode.contains(";LAYER_CHANGE"),
            "missing LAYER_CHANGE marker"
        );
        assert!(gcode.contains(";Z:0.200"), "missing ;Z: annotation");
        assert!(gcode.contains(" E"), "no extrusion moves");
        assert!(gcode.contains("X0.000 Y0.000"), "missing start travel");
    }

    // ── KlipperDialect extras ──────────────────────────────────────────────────

    #[test]
    fn test_klipper_dialect_set_pressure_advance() {
        let d = KlipperDialect;
        let cmd = d.set_pressure_advance(0.05);
        assert_eq!(cmd, "SET_PRESSURE_ADVANCE ADVANCE=0.0500");
    }

    #[test]
    fn test_klipper_dialect_set_velocity_limit() {
        let d = KlipperDialect;
        let cmd = d.set_velocity_limit(200.0, 3000.0);
        assert_eq!(cmd, "SET_VELOCITY_LIMIT VELOCITY=200 ACCEL=3000");
    }

    #[test]
    fn test_klipper_dialect_call_macro() {
        let d = KlipperDialect;
        assert_eq!(d.call_macro("print_start"), "PRINT_START");
        assert_eq!(d.call_macro("PRINT_END"), "PRINT_END");
    }

    // ── GcodeDialect default methods ───────────────────────────────────────────

    #[test]
    fn test_dialect_default_comment() {
        let d = MarlinDialect;
        assert_eq!(d.comment("hello"), "; hello");
    }

    #[test]
    fn test_dialect_default_set_nozzle_temp() {
        let d = MarlinDialect;
        assert_eq!(d.set_nozzle_temp(210.0, false), "M104 S210");
        assert_eq!(d.set_nozzle_temp(210.0, true), "M109 S210");
    }

    #[test]
    fn test_dialect_default_set_bed_temp() {
        let d = MarlinDialect;
        assert_eq!(d.set_bed_temp(60.0, false), "M140 S60");
        assert_eq!(d.set_bed_temp(60.0, true), "M190 S60");
    }

    #[test]
    fn test_dialect_default_set_fan_speed() {
        let d = MarlinDialect;
        assert_eq!(d.set_fan_speed(0.0), "M107");
        assert_eq!(d.set_fan_speed(1.0), "M106 S255");
        assert_eq!(d.set_fan_speed(0.5), "M106 S128");
    }

    #[test]
    fn test_dialect_default_home_axes() {
        let d = MarlinDialect;
        assert_eq!(d.home_axes(), "G28");
    }

    #[test]
    fn test_dialect_default_reset_extruder() {
        let d = MarlinDialect;
        assert_eq!(d.reset_extruder(), "G92 E0");
    }

    // ── with_dialect (custom dialect) ─────────────────────────────────────────

    #[test]
    fn test_generator_with_custom_dialect() {
        let gen = GcodeGenerator::with_dialect(Box::new(KlipperDialect));
        let gcode = gen.generate(&[], &SlicingParams::default());
        assert!(gcode.contains("SET_VELOCITY_LIMIT"));
    }

    // ── warn_fn mechanism ──────────────────────────────────────────────────────

    #[test]
    fn test_warn_fn_called_for_unsupported_commands() {
        use std::sync::{Arc, Mutex};

        // A test dialect that advertises one unsupported command
        struct LimitedDialect;
        impl GcodeDialect for LimitedDialect {
            fn flavor_name(&self) -> &'static str {
                "Limited"
            }
            fn start_script(&self, _: &SlicingParams) -> Vec<String> {
                vec![]
            }
            fn end_script(&self) -> Vec<String> {
                vec![]
            }
            fn unsupported_commands(&self) -> &'static [&'static str] {
                &["set_fan_speed"]
            }
        }

        let warnings: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let warnings_clone = Arc::clone(&warnings);
        let gen = GcodeGenerator::with_dialect(Box::new(LimitedDialect))
            .with_warn_fn(move |msg| warnings_clone.lock().unwrap().push(msg.to_string()));

        gen.generate(&[], &SlicingParams::default());

        let warnings = warnings.lock().unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("set_fan_speed"),
            "warning should mention the unsupported command"
        );
        assert!(
            warnings[0].contains("Limited"),
            "warning should mention the dialect name"
        );
    }

    #[test]
    fn test_no_warn_fn_is_silent() {
        // Verify the generator doesn't panic when no warn_fn is set
        // even if the dialect lists unsupported commands
        struct NoFanDialect;
        impl GcodeDialect for NoFanDialect {
            fn flavor_name(&self) -> &'static str {
                "NoFan"
            }
            fn start_script(&self, _: &SlicingParams) -> Vec<String> {
                vec![]
            }
            fn end_script(&self) -> Vec<String> {
                vec![]
            }
            fn unsupported_commands(&self) -> &'static [&'static str] {
                &["set_fan_speed"]
            }
        }

        // Should not panic
        let gen = GcodeGenerator::with_dialect(Box::new(NoFanDialect));
        let gcode = gen.generate(&[], &SlicingParams::default());
        assert!(gcode.contains("; Generated by slicer-engine"));
    }

    // ── Lifecycle markers ──────────────────────────────────────────────────────

    #[test]
    fn test_lifecycle_markers_enabled_by_default() {
        let layer = SliceLayer::new(0.2);
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[layer], &SlicingParams::default());
        assert!(
            gcode.contains(";LAYER_CHANGE"),
            "LAYER_CHANGE must be present"
        );
        assert!(gcode.contains(";Z:0.200"), ";Z: annotation must be present");
        assert!(
            gcode.contains(";HEIGHT:0.200"),
            ";HEIGHT: annotation must be present"
        );
        assert!(
            gcode.contains(";BEFORE_LAYER_CHANGE"),
            ";BEFORE_LAYER_CHANGE must be present"
        );
        assert!(
            gcode.contains(";AFTER_LAYER_CHANGE"),
            ";AFTER_LAYER_CHANGE must be present"
        );
        assert!(gcode.contains("G92 E0"), "extruder reset must be present");
    }

    #[test]
    fn test_lifecycle_markers_disabled_emits_legacy_comment() {
        let layer = SliceLayer::new(0.2);
        let gcode = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_lifecycle_markers(false)
            .generate(&[layer], &SlicingParams::default());
        assert!(
            gcode.contains("; layer z=0.200"),
            "legacy comment must appear when markers disabled"
        );
        assert!(
            !gcode.contains(";LAYER_CHANGE"),
            "LAYER_CHANGE must NOT appear when markers disabled"
        );
    }

    #[test]
    fn test_lifecycle_markers_type_annotation_emitted() {
        use clipper2::Path;
        let mut layer = SliceLayer::new(0.2);
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(square);
        layer.path_roles.push(crate::core::ExtrusionRole::Perimeter);

        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[layer], &SlicingParams::default());
        assert!(
            gcode.contains(";TYPE:Perimeter"),
            ";TYPE: annotation must be present"
        );
        assert!(
            gcode.contains(";WIDTH:0.40mm"),
            ";WIDTH: annotation must be present"
        );
    }

    #[test]
    fn test_lifecycle_markers_type_transition_emitted_once_per_role() {
        use clipper2::Path;
        let mut layer = SliceLayer::new(0.2);
        let sq: Path = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].into();
        // Two Perimeter paths followed by one Infill path
        layer.paths.push(sq.clone());
        layer.paths.push(sq.clone());
        layer.paths.push(sq);
        layer.path_roles.push(crate::core::ExtrusionRole::Perimeter);
        layer.path_roles.push(crate::core::ExtrusionRole::Perimeter);
        layer.path_roles.push(crate::core::ExtrusionRole::Infill);

        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[layer], &SlicingParams::default());

        // Perimeter TYPE should appear exactly once (no duplicate at role boundary)
        let perimeter_count = gcode.matches(";TYPE:Perimeter").count();
        assert_eq!(
            perimeter_count, 1,
            "Perimeter TYPE emitted {} times",
            perimeter_count
        );

        // Infill TYPE should appear exactly once
        let infill_count = gcode.matches(";TYPE:Infill").count();
        assert_eq!(
            infill_count, 1,
            "Infill TYPE emitted {} times",
            infill_count
        );
    }

    #[test]
    fn test_lifecycle_markers_no_type_when_disabled() {
        use clipper2::Path;
        let mut layer = SliceLayer::new(0.2);
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        layer.paths.push(square);

        let gcode = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_lifecycle_markers(false)
            .generate(&[layer], &SlicingParams::default());
        assert!(
            !gcode.contains(";TYPE:"),
            ";TYPE: must NOT appear when markers disabled"
        );
    }
}
