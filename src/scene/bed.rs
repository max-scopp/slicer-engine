//! Print bed configuration for the scene.

#[cfg(not(target_arch = "wasm32"))]
use crate::config::types::MachineConfig;
use serde::{Deserialize, Serialize};

/// Print bed dimensions and origin offset.
///
/// All units are millimeters. The bed lies in the XY plane with its origin
/// (printer 0,0) at `(origin_offset_x, origin_offset_y)` in scene coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BedConfig {
    /// Width along the X axis (mm).
    pub width: f64,
    /// Depth along the Y axis (mm).
    pub depth: f64,
    /// Maximum print height along the Z axis (mm).
    pub height: f64,
    /// X offset of the printer origin from the scene origin (mm).
    pub origin_offset_x: f64,
    /// Y offset of the printer origin from the scene origin (mm).
    pub origin_offset_y: f64,
}

impl Default for BedConfig {
    fn default() -> Self {
        Self {
            width: 220.0,
            depth: 220.0,
            height: 250.0,
            origin_offset_x: 0.0,
            origin_offset_y: 0.0,
        }
    }
}

impl BedConfig {
    /// Geometric center of the bed in scene coordinates.
    pub fn center_xy(&self) -> (f64, f64) {
        (
            self.origin_offset_x + self.width / 2.0,
            self.origin_offset_y + self.depth / 2.0,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<&MachineConfig> for BedConfig {
    fn from(m: &MachineConfig) -> Self {
        Self {
            width: m.build_volume_x,
            depth: m.build_volume_y,
            height: m.build_volume_z,
            origin_offset_x: 0.0,
            origin_offset_y: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn from_machine_config_copies_dimensions() {
        let mc = MachineConfig {
            build_volume_x: 256.0,
            build_volume_y: 256.0,
            build_volume_z: 256.0,
            ..MachineConfig::default()
        };
        let bed: BedConfig = (&mc).into();
        assert_eq!(bed.width, 256.0);
        assert_eq!(bed.depth, 256.0);
        assert_eq!(bed.height, 256.0);
    }

    #[test]
    fn center_xy_accounts_for_offset() {
        let bed = BedConfig {
            width: 200.0,
            depth: 100.0,
            height: 250.0,
            origin_offset_x: 10.0,
            origin_offset_y: 20.0,
        };
        let (cx, cy) = bed.center_xy();
        assert!((cx - 110.0).abs() < 1e-9);
        assert!((cy - 70.0).abs() < 1e-9);
    }
}
