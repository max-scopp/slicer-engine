use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Options controlling the auto-orient algorithm.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AutoOrientOptions {
    /// When `true`, additionally sample a Fibonacci-sphere grid (~128
    /// candidates) in addition to the flat-face candidates.  Recommended for
    /// organic shapes with no large flat faces, e.g. figurines.  When
    /// `false` (default), only unique flat-face-normal directions are tested —
    /// fast and correct for box-like objects.
    pub allow_rotations: bool,

    /// After finding the best face-down orientation, additionally rotate the
    /// object around Z by this many degrees.  Set to `45.0` for CoreXY
    /// printers to align the seam line with the stepper axes.  `0.0` =
    /// disabled (default).
    pub preferred_z_rotation_deg: f64,

    /// Faces whose outward normal points more than this many degrees below
    /// horizontal are counted as overhanging (and penalised).  Should match
    /// the printer's support angle threshold.  **Default: 45°.**
    pub overhang_threshold_deg: f64,
}

impl Default for AutoOrientOptions {
    fn default() -> Self {
        Self {
            allow_rotations: false,
            preferred_z_rotation_deg: 0.0,
            overhang_threshold_deg: 45.0,
        }
    }
}
