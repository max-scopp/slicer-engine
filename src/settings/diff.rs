//! Settings diff tool: detect and report overrides between Global and Object settings.

use serde::Serialize;

use crate::settings::params::{GlobalSettings, ObjectSettings, SlicingParams};

/// Represents the difference for a single setting field between the global
/// defaults and an object-level override.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsDiff {
    /// The name of the setting field (e.g. `"layer_height"`).
    pub field_name: String,
    /// The global value formatted as a string.
    pub global_value: String,
    /// The object value formatted as a string (or the global value if not overridden).
    pub object_value: String,
    /// `true` if the object value differs from the global value.
    pub is_override: bool,
}

/// Compare global and object settings and return a diff for every field.
///
/// If `object.overrides` is `None` the object inherits all global values;
/// every field will be listed with `is_override = false`.
pub fn compare_settings(global: &GlobalSettings, object: &ObjectSettings) -> Vec<SettingsDiff> {
    let g = &global.params;
    let o: &SlicingParams = object.overrides.as_ref().unwrap_or(g);

    macro_rules! diff {
        ($field:ident) => {
            SettingsDiff {
                field_name: stringify!($field).to_string(),
                global_value: g.$field.to_string(),
                object_value: o.$field.to_string(),
                is_override: (g.$field - o.$field).abs() > 1e-9,
            }
        };
    }

    vec![
        diff!(layer_height),
        diff!(wall_thickness),
        diff!(infill_density),
        diff!(print_speed),
        diff!(nozzle_temp),
        diff!(bed_temp),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::params::{GlobalSettings, ObjectSettings, SlicingParams};

    #[test]
    fn test_no_overrides_all_false() {
        let global = GlobalSettings::default();
        let object = ObjectSettings {
            object_name: "obj".to_string(),
            overrides: None,
        };
        let diff = compare_settings(&global, &object);
        assert_eq!(diff.len(), 6, "Should have 6 fields");
        for d in &diff {
            assert!(
                !d.is_override,
                "Field '{}' should not be overridden",
                d.field_name
            );
            assert_eq!(d.global_value, d.object_value);
        }
    }

    #[test]
    fn test_partial_override_detected() {
        let global = GlobalSettings::default();
        let object = ObjectSettings {
            object_name: "obj".to_string(),
            overrides: Some(SlicingParams {
                layer_height: 0.1, // overridden
                ..SlicingParams::default()
            }),
        };
        let diff = compare_settings(&global, &object);

        let lh = diff
            .iter()
            .find(|d| d.field_name == "layer_height")
            .unwrap();
        assert!(lh.is_override, "layer_height should be flagged as override");
        assert_eq!(lh.global_value, "0.2");
        assert_eq!(lh.object_value, "0.1");

        // All other fields should not be overridden
        for d in diff.iter().filter(|d| d.field_name != "layer_height") {
            assert!(
                !d.is_override,
                "Field '{}' should not be overridden",
                d.field_name
            );
        }
    }

    #[test]
    fn test_diff_contains_all_fields() {
        let global = GlobalSettings::default();
        let object = ObjectSettings {
            object_name: "obj".to_string(),
            overrides: None,
        };
        let diff = compare_settings(&global, &object);
        let field_names: Vec<&str> = diff.iter().map(|d| d.field_name.as_str()).collect();
        assert!(field_names.contains(&"layer_height"));
        assert!(field_names.contains(&"wall_thickness"));
        assert!(field_names.contains(&"infill_density"));
        assert!(field_names.contains(&"print_speed"));
        assert!(field_names.contains(&"nozzle_temp"));
        assert!(field_names.contains(&"bed_temp"));
    }
}
