//! [`GcodeGenerator`] — per-layer extrusion logic and the `generate_gcode` convenience wrapper.

use std::borrow::Cow;

use crate::core::SliceLayer;
use crate::gcode::dialect::{GcodeDialect, WarnFn};
use crate::gcode::dialects::{KlipperDialect, MarlinDialect};
use crate::gcode::flavor::GcodeFlavor;
use crate::settings::params::{LifecycleMarkerConfig, SlicingParams};

// ── Physical constants ─────────────────────────────────────────────────────────

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

// ── Private helpers ────────────────────────────────────────────────────────────

/// Compute the extrusion length (mm of filament) needed to print a straight
/// line of length `move_len` at the given `layer_height` with the default
/// nozzle and filament diameters.
///
/// Formula: E = line_length × (layer_height × nozzle_diameter) / (π × filament_radius²)
pub(crate) fn extrusion_for_move(move_len: f64, layer_height: f64) -> f64 {
    let filament_radius = FILAMENT_DIAMETER_MM / 2.0;
    let cross_section = layer_height * NOZZLE_DIAMETER_MM;
    let filament_area = std::f64::consts::PI * filament_radius.powi(2);
    move_len * cross_section / filament_area
}

/// Substitute template placeholders in a marker string.
///
/// Replaces `{z}`, `{height}`, `{type}`, and `{width}` with the supplied
/// values.  Placeholders that are not relevant to a particular marker are
/// simply left as-is when the corresponding value is an empty string.
pub(crate) fn render_marker(
    template: &str,
    z: &str,
    height: &str,
    type_name: &str,
    width: &str,
) -> String {
    template
        .replace("{z}", z)
        .replace("{height}", height)
        .replace("{type}", type_name)
        .replace("{width}", width)
}

// ── GcodeGenerator ─────────────────────────────────────────────────────────────

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
/// Custom start / end scripts set via [`GcodeGenerator::with_start_script`] and
/// [`GcodeGenerator::with_end_script`] take precedence over the dialect's
/// built-in defaults.  This supports the priority chain:
/// *CLI argument → global settings → dialect default*.
///
/// Per-flavor lifecycle marker overrides are applied via
/// [`GcodeGenerator::with_marker_config`].  When `marker_config.enabled` is
/// `true` (the default) each layer block is preceded by the full OrcaSlicer /
/// Klipper lifecycle marker block.
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
    /// Per-flavor lifecycle marker configuration.
    marker_config: LifecycleMarkerConfig,
    /// Optional override for the start script (replaces dialect default).
    custom_start_script: Option<Vec<String>>,
    /// Optional override for the end script (replaces dialect default).
    custom_end_script: Option<Vec<String>>,
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
            marker_config: LifecycleMarkerConfig::default(),
            custom_start_script: None,
            custom_end_script: None,
        }
    }

    /// Create a generator with a custom [`GcodeDialect`] implementation.
    ///
    /// Useful for testing or for dialects not covered by [`GcodeFlavor`].
    pub fn with_dialect(dialect: Box<dyn GcodeDialect>) -> Self {
        Self {
            dialect,
            warn_fn: None,
            marker_config: LifecycleMarkerConfig::default(),
            custom_start_script: None,
            custom_end_script: None,
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
    /// Set to `false` to emit a minimal `; layer z=…` comment instead.
    ///
    /// For more fine-grained control use [`GcodeGenerator::with_marker_config`].
    pub fn with_lifecycle_markers(mut self, enabled: bool) -> Self {
        self.marker_config.enabled = enabled;
        self
    }

    /// Apply a full [`LifecycleMarkerConfig`] to this generator.
    ///
    /// This replaces the current marker configuration entirely, allowing callers
    /// to set per-flavor overrides loaded from [`crate::settings::params::GlobalSettings`].
    pub fn with_marker_config(mut self, config: LifecycleMarkerConfig) -> Self {
        self.marker_config = config;
        self
    }

    /// Override the start script with custom G-code lines.
    ///
    /// When set, these lines are emitted instead of the dialect's built-in
    /// [`GcodeDialect::start_script`] output.
    ///
    /// ```rust
    /// use slicer_engine::gcode::{GcodeGenerator, GcodeFlavor};
    /// use slicer_engine::settings::params::SlicingParams;
    ///
    /// let gen = GcodeGenerator::new(GcodeFlavor::Klipper)
    ///     .with_start_script(vec!["START_PRINT BED_TEMP=65 EXTRUDER_TEMP=215".to_string()]);
    /// let gcode = gen.generate(&[], &SlicingParams::default());
    /// assert!(gcode.contains("BED_TEMP=65"));
    /// ```
    pub fn with_start_script(mut self, script: Vec<String>) -> Self {
        self.custom_start_script = Some(script);
        self
    }

    /// Override the end script with custom G-code lines.
    ///
    /// When set, these lines are emitted instead of the dialect's built-in
    /// [`GcodeDialect::end_script`] output.
    ///
    /// ```rust
    /// use slicer_engine::gcode::{GcodeGenerator, GcodeFlavor};
    /// use slicer_engine::settings::params::SlicingParams;
    ///
    /// let gen = GcodeGenerator::new(GcodeFlavor::Klipper)
    ///     .with_end_script(vec!["MY_END_PRINT".to_string()]);
    /// let gcode = gen.generate(&[], &SlicingParams::default());
    /// assert!(gcode.contains("MY_END_PRINT"));
    /// ```
    pub fn with_end_script(mut self, script: Vec<String>) -> Self {
        self.custom_end_script = Some(script);
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

        // ── Metadata header ──────────────────────────────────────────────────
        use std::fmt::Write as _;
        let _ = write!(
            out,
            "; Generated by slicer-engine | flavor: {}\n\
             ; layer_height: {} mm\n\
             ; nozzle_temp: {} °C\n\
             ; bed_temp: {} °C\n\
             ; print_speed: {} mm/s\n\
             ; wall_thickness: {} mm\n\
             ; infill_density: {:.0}%\n\
             ; ---\n",
            self.dialect.flavor_name(),
            params.layer_height,
            params.nozzle_temp,
            params.bed_temp,
            params.print_speed,
            params.wall_thickness,
            params.infill_density * 100.0,
        );

        // ── Start script (custom override or flavor default) ──────────────────
        let start_script: Cow<[String]> = match &self.custom_start_script {
            Some(lines) => Cow::Borrowed(lines),
            None => Cow::Owned(self.dialect.start_script(params)),
        };
        for line in start_script.iter() {
            out.push_str(line);
            out.push('\n');
        }

        // ── Per-layer contours ────────────────────────────────────────────────
        let mut e_total = 0.0_f64;

        for layer in layers {
            let z_str = format!("{:.3}", layer.z);
            let height_str = format!("{:.3}", params.layer_height);

            if self.marker_config.enabled {
                // Lifecycle block: LAYER_CHANGE → BEFORE_LAYER_CHANGE → Z move → AFTER_LAYER_CHANGE
                let layer_change = self
                    .marker_config
                    .layer_change
                    .as_deref()
                    .unwrap_or(";LAYER_CHANGE");
                out.push_str(&render_marker(layer_change, &z_str, &height_str, "", ""));
                out.push('\n');

                let z_marker = self.marker_config.z_marker.as_deref().unwrap_or(";Z:{z}");
                out.push_str(&render_marker(z_marker, &z_str, &height_str, "", ""));
                out.push('\n');

                let height_marker = self
                    .marker_config
                    .height_marker
                    .as_deref()
                    .unwrap_or(";HEIGHT:{height}");
                out.push_str(&render_marker(height_marker, &z_str, &height_str, "", ""));
                out.push('\n');

                let before_lc = self
                    .marker_config
                    .before_layer_change
                    .as_deref()
                    .unwrap_or(";BEFORE_LAYER_CHANGE");
                out.push_str(&render_marker(before_lc, &z_str, &height_str, "", ""));
                out.push('\n');

                // Bare Z-value comment (`;0.200`) matches the OrcaSlicer / PrusaSlicer lifecycle
                // format; it intentionally differs from the `;Z:` label above and is used by
                // post-processing scripts that parse standalone numeric layer markers.
                out.push_str(&format!(";{}\n", z_str));

                // Reset extruder position at layer start
                out.push_str(&format!("{}\n", self.dialect.reset_extruder()));
                e_total = 0.0;

                out.push_str(&format!(
                    "{}\n",
                    self.dialect.move_z(layer.z, TRAVEL_SPEED_MM_MIN)
                ));

                let after_lc = self
                    .marker_config
                    .after_layer_change
                    .as_deref()
                    .unwrap_or(";AFTER_LAYER_CHANGE");
                out.push_str(&render_marker(after_lc, &z_str, &height_str, "", ""));
                out.push('\n');

                // Same bare Z-value convention after the layer change (see note above).
                out.push_str(&format!(";{}\n", z_str));
            } else {
                out.push_str(&format!("; layer z={}\n", z_str));
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
                if self.marker_config.enabled {
                    let role = layer.role_for_path(path_idx);
                    if last_role != Some(role) {
                        let type_name = role.type_name();
                        let width = format!("{:.2}", role.default_width_mm());

                        let type_ann = self
                            .marker_config
                            .type_annotation
                            .as_deref()
                            .unwrap_or(";TYPE:{type}");
                        out.push_str(&render_marker(
                            type_ann,
                            &z_str,
                            &height_str,
                            type_name,
                            &width,
                        ));
                        out.push('\n');

                        let width_ann = self
                            .marker_config
                            .width_annotation
                            .as_deref()
                            .unwrap_or(";WIDTH:{width}mm");
                        out.push_str(&render_marker(
                            width_ann,
                            &z_str,
                            &height_str,
                            type_name,
                            &width,
                        ));
                        out.push('\n');

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

        // ── End script (custom override or flavor default) ────────────────────
        let end_script: Cow<[String]> = match &self.custom_end_script {
            Some(lines) => Cow::Borrowed(lines),
            None => Cow::Owned(self.dialect.end_script()),
        };
        for line in end_script.iter() {
            out.push_str(line);
            out.push('\n');
        }

        out
    }
}

// ── Convenience wrapper ────────────────────────────────────────────────────────

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

// ── Tests ──────────────────────────────────────────────────────────────────────

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
    fn test_generator_klipper_uses_start_print_macro() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Klipper).generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("START_PRINT"),
            "Klipper gcode missing START_PRINT macro: {gcode}"
        );
        assert!(
            gcode.contains("BED_TEMP=60"),
            "Klipper START_PRINT missing BED_TEMP: {gcode}"
        );
        assert!(
            gcode.contains("EXTRUDER_TEMP=210"),
            "Klipper START_PRINT missing EXTRUDER_TEMP: {gcode}"
        );
    }

    #[test]
    fn test_generator_klipper_uses_end_print_macro() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Klipper).generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("END_PRINT"),
            "Klipper gcode missing END_PRINT macro: {gcode}"
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
        assert!(gcode.contains("START_PRINT"));
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

    #[test]
    fn test_lifecycle_markers_custom_layer_change_template() {
        let layer = SliceLayer::new(0.4);
        let config = LifecycleMarkerConfig {
            layer_change: Some(";CUSTOM_LAYER z={z} h={height}".to_string()),
            ..LifecycleMarkerConfig::default()
        };
        let gcode = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_marker_config(config)
            .generate(&[layer], &SlicingParams::default());
        assert!(
            gcode.contains(";CUSTOM_LAYER z=0.400 h=0.200"),
            "custom layer_change template not rendered: {gcode}"
        );
    }

    #[test]
    fn test_lifecycle_markers_custom_type_annotation() {
        use clipper2::Path;
        let mut layer = SliceLayer::new(0.2);
        let sq: Path = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].into();
        layer.paths.push(sq);
        layer.path_roles.push(crate::core::ExtrusionRole::Infill);

        let config = LifecycleMarkerConfig {
            type_annotation: Some(";FEATURE {type}".to_string()),
            ..LifecycleMarkerConfig::default()
        };
        let gcode = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_marker_config(config)
            .generate(&[layer], &SlicingParams::default());
        assert!(
            gcode.contains(";FEATURE Infill"),
            "custom type annotation not rendered: {gcode}"
        );
    }

    #[test]
    fn test_with_marker_config_disabled() {
        let layer = SliceLayer::new(0.2);
        let config = LifecycleMarkerConfig {
            enabled: false,
            ..LifecycleMarkerConfig::default()
        };
        let gcode = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_marker_config(config)
            .generate(&[layer], &SlicingParams::default());
        assert!(!gcode.contains(";LAYER_CHANGE"));
        assert!(gcode.contains("; layer z=0.200"));
    }

    // ── Custom start / end scripts ─────────────────────────────────────────────

    #[test]
    fn test_custom_start_script_overrides_dialect() {
        let gen = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_start_script(vec!["MY_CUSTOM_START".to_string()]);
        let gcode = gen.generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("MY_CUSTOM_START"),
            "custom start script not emitted"
        );
        // G21 (mm mode) is only in the Marlin start script, not the end script —
        // it should be absent when the start script is fully overridden.
        assert!(
            !gcode.contains("G21"),
            "dialect default start should be suppressed by custom script"
        );
    }

    #[test]
    fn test_custom_end_script_overrides_dialect() {
        let gen = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_end_script(vec!["MY_CUSTOM_END".to_string()]);
        let gcode = gen.generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("MY_CUSTOM_END"),
            "custom end script not emitted"
        );
        // Marlin's M84 should NOT be present when custom script overrides it
        assert!(
            !gcode.contains("M84"),
            "dialect default should be suppressed by custom end script"
        );
    }

    #[test]
    fn test_custom_start_script_klipper_override() {
        let gen = GcodeGenerator::new(GcodeFlavor::Klipper)
            .with_start_script(vec!["START_PRINT BED_TEMP=65 EXTRUDER_TEMP=215".to_string()]);
        let gcode = gen.generate(&[], &SlicingParams::default());
        assert!(gcode.contains("BED_TEMP=65"));
        assert!(gcode.contains("EXTRUDER_TEMP=215"));
    }

    #[test]
    fn test_custom_scripts_multiline() {
        let gen = GcodeGenerator::new(GcodeFlavor::Marlin)
            .with_start_script(vec![
                "G28 ; custom home".to_string(),
                "M190 S65 ; bed".to_string(),
            ])
            .with_end_script(vec!["M84 ; motors off".to_string()]);
        let gcode = gen.generate(&[], &SlicingParams::default());
        assert!(gcode.contains("G28 ; custom home"));
        assert!(gcode.contains("M190 S65"));
        assert!(gcode.contains("M84 ; motors off"));
    }

    // ── Metadata header ────────────────────────────────────────────────────────

    #[test]
    fn test_metadata_header_contains_settings() {
        let gcode =
            GcodeGenerator::new(GcodeFlavor::Marlin).generate(&[], &SlicingParams::default());
        assert!(
            gcode.contains("; layer_height: 0.2 mm"),
            "missing layer_height"
        );
        assert!(
            gcode.contains("; nozzle_temp: 210 °C"),
            "missing nozzle_temp"
        );
        assert!(gcode.contains("; bed_temp: 60 °C"), "missing bed_temp");
        assert!(
            gcode.contains("; print_speed: 60 mm/s"),
            "missing print_speed"
        );
        assert!(
            gcode.contains("; wall_thickness: 1.2 mm"),
            "missing wall_thickness"
        );
        assert!(
            gcode.contains("; infill_density: 20%"),
            "missing infill_density"
        );
    }

    // ── resolve_gcode_source ───────────────────────────────────────────────────

    #[test]
    fn test_resolve_gcode_source_inline_string() {
        use crate::gcode::source::resolve_gcode_source;
        let lines = resolve_gcode_source("G28\nM109 S210").unwrap();
        assert_eq!(lines, vec!["G28", "M109 S210"]);
    }

    #[test]
    fn test_resolve_gcode_source_single_line() {
        use crate::gcode::source::resolve_gcode_source;
        let lines = resolve_gcode_source("START_PRINT BED_TEMP=60 EXTRUDER_TEMP=210").unwrap();
        assert_eq!(
            lines,
            vec!["START_PRINT BED_TEMP=60 EXTRUDER_TEMP=210".to_string()]
        );
    }

    #[test]
    fn test_resolve_gcode_source_from_file() {
        use crate::gcode::source::resolve_gcode_source;
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "G28 ; home").unwrap();
        writeln!(tmp, "M109 S210 ; wait").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let lines = resolve_gcode_source(&path).unwrap();
        assert_eq!(lines, vec!["G28 ; home", "M109 S210 ; wait"]);
    }

    #[test]
    fn test_resolve_gcode_source_file_too_large() {
        use crate::gcode::source::resolve_gcode_source;
        use std::io::Write;
        // Create a file that exceeds the 1 MiB limit
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let big_line = "G1 X0 Y0\n".repeat(200_000); // ~1.8 MiB
        tmp.write_all(big_line.as_bytes()).unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let err = resolve_gcode_source(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string().contains("too large"),
            "error should mention file is too large: {err}"
        );
    }

    // ── render_marker ──────────────────────────────────────────────────────────

    #[test]
    fn test_render_marker_substitutes_all_placeholders() {
        let result = render_marker(
            ";z={z} h={height} t={type} w={width}",
            "0.200",
            "0.200",
            "Perimeter",
            "0.40",
        );
        assert_eq!(result, ";z=0.200 h=0.200 t=Perimeter w=0.40");
    }

    #[test]
    fn test_render_marker_no_placeholders() {
        let result = render_marker(";LAYER_CHANGE", "0.200", "0.200", "", "");
        assert_eq!(result, ";LAYER_CHANGE");
    }
}
