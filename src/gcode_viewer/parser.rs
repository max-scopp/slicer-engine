use super::types::{InternalLayer, Role};

/// Parse `bytes` as UTF-8 GCode and return one [`InternalLayer`] per detected
/// layer change, plus any segments that appear before the first layer marker
/// in layer 0.
pub(super) fn parse_gcode_bytes(bytes: &[u8]) -> Vec<InternalLayer> {
    let text = String::from_utf8_lossy(bytes);

    let mut layers: Vec<InternalLayer> = Vec::new();
    let mut current = InternalLayer::new(0.0);

    let mut x: f32 = 0.0;
    let mut y: f32 = 0.0;
    let mut z: f32 = 0.0;
    let mut e: f32 = 0.0;
    let mut width: f32 = 0.4;
    let mut height: f32 = 0.2;
    let mut absolute_xyz = true;
    let mut absolute_e = true;
    let mut role = Role::Travel;

    // When true we prefer `;LAYER_CHANGE` comments for layer detection.
    // When false we fall back to Z-change detection (for slicers that don't
    // emit our markers).
    let mut seen_layer_change_comment = false;

    for raw_line in text.lines() {
        let line = match raw_line.find(';') {
            Some(pos) => {
                let comment = raw_line[pos + 1..].trim();
                process_comment(
                    comment,
                    &mut role,
                    &mut layers,
                    &mut current,
                    &mut seen_layer_change_comment,
                    &mut width,
                    &mut height,
                    z,
                );
                raw_line[..pos].trim()
            }
            None => raw_line.trim(),
        };

        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_ascii_whitespace();
        let cmd = match parts.next() {
            Some(c) => c.to_ascii_uppercase(),
            None => continue,
        };

        match cmd.as_str() {
            "G90" => {
                absolute_xyz = true;
                absolute_e = true;
            }
            "G91" => {
                absolute_xyz = false;
                absolute_e = false;
            }
            "M82" => absolute_e = true,
            "M83" => absolute_e = false,
            "G92" => {
                for param in parts {
                    if param.starts_with('E') || param.starts_with('e') {
                        if let Ok(val) = param[1..].parse::<f32>() {
                            e = val;
                        }
                    }
                }
            }
            "G0" | "G1" => {
                let prev_x = x;
                let prev_y = y;
                let prev_z = z;
                let prev_e = e;

                let mut new_x = x;
                let mut new_y = y;
                let mut new_z = z;
                let mut new_e = e;
                let mut has_e = false;

                for param in parts {
                    if param.is_empty() {
                        continue;
                    }
                    let (letter, rest) = param.split_at(1);
                    let Ok(val) = rest.parse::<f32>() else {
                        continue;
                    };
                    match letter.to_ascii_uppercase().as_str() {
                        "X" => new_x = if absolute_xyz { val } else { x + val },
                        "Y" => new_y = if absolute_xyz { val } else { y + val },
                        "Z" => new_z = if absolute_xyz { val } else { z + val },
                        "E" => {
                            has_e = true;
                            new_e = if absolute_e { val } else { e + val };
                        }
                        _ => {}
                    }
                }

                // Z-change layer boundary (fallback when no ;LAYER_CHANGE).
                if !seen_layer_change_comment && (new_z - prev_z).abs() > 1e-6 && new_z > prev_z {
                    let finished = std::mem::replace(&mut current, InternalLayer::new(new_z));
                    layers.push(finished);
                }

                x = new_x;
                y = new_y;
                z = new_z;
                e = new_e;

                let is_extruding = has_e && (new_e - prev_e) > 1e-7;
                let seg_role = if is_extruding { role } else { Role::Travel };

                let moved = (x - prev_x).abs() > 1e-6
                    || (y - prev_y).abs() > 1e-6
                    || (z - prev_z).abs() > 1e-6;
                if moved {
                    current.push_segment(seg_role, prev_x, prev_y, prev_z, x, y, z, width, height);
                }
            }
            _ => {} // G28, G4, M104, M109, T0, etc. — ignore
        }
    }

    layers.push(current);
    layers
}

/// Handle a `;` comment line, mutating parser state as needed.
pub(super) fn process_comment(
    comment: &str,
    role: &mut Role,
    layers: &mut Vec<InternalLayer>,
    current: &mut InternalLayer,
    seen_layer_change_comment: &mut bool,
    width: &mut f32,
    height: &mut f32,
    current_z: f32,
) {
    let trimmed = comment.trim();

    if trimmed.eq_ignore_ascii_case("LAYER_CHANGE")
        || trimmed.eq_ignore_ascii_case("BEFORE_LAYER_CHANGE")
    {
        *seen_layer_change_comment = true;
        if !current.is_empty() {
            let finished = std::mem::replace(current, InternalLayer::new(current_z));
            layers.push(finished);
        }
        // Do not reset current.z here if it is empty, because a preceding ;Z:
        // might have already set the correct future Z height for this empty layer.
        *role = Role::Travel;
    } else if let Some(type_val) = trimmed.strip_prefix("TYPE:") {
        *role = Role::from_type_comment(type_val);
    } else if let Some(z_val) = trimmed.strip_prefix("Z:") {
        if let Ok(z) = z_val.parse::<f32>() {
            if current.is_empty() {
                current.z = z;
            }
        }
    } else if let Some(width_val) = trimmed.strip_prefix("WIDTH:") {
        if let Ok(w) = width_val.parse::<f32>() {
            *width = w;
        }
    } else if let Some(height_val) = trimmed.strip_prefix("HEIGHT:") {
        if let Ok(h) = height_val.parse::<f32>() {
            *height = h;
        }
    } else if let Some(height_val) = trimmed.strip_prefix("LAYER_HEIGHT:") {
        if let Ok(h) = height_val.parse::<f32>() {
            *height = h;
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_GCODE: &str = r#"
; Generated by test
G90
G92 E0
;LAYER_CHANGE
;Z:0.200
G0 Z0.200 F9000
;TYPE:Outer wall
G1 X10 Y10 Z0.2 E1.0 F1800
G1 X20 Y10 Z0.2 E2.0
G1 X20 Y20 Z0.2 E3.0
G1 X10 Y10 Z0.2 E4.0
;TYPE:Infill
G1 X15 Y15 Z0.2 E5.0
;LAYER_CHANGE
;Z:0.400
G0 Z0.400 F9000
;TYPE:Inner wall
G1 X10 Y10 Z0.4 E6.0 F1800
G1 X20 Y10 Z0.4 E7.0
"#;

    #[test]
    fn test_layer_count() {
        let layers = parse_gcode_bytes(SAMPLE_GCODE.as_bytes());
        assert!(
            layers.len() >= 2,
            "expected at least 2 layers, got {}",
            layers.len()
        );
    }

    #[test]
    fn test_outer_wall_segments() {
        let layers = parse_gcode_bytes(SAMPLE_GCODE.as_bytes());
        let has_outer = layers.iter().any(|l| !l.outer_wall.is_empty());
        assert!(has_outer, "expected outer wall segments");
    }

    #[test]
    fn test_infill_segments() {
        let layers = parse_gcode_bytes(SAMPLE_GCODE.as_bytes());
        let has_infill = layers.iter().any(|l| !l.infill.is_empty());
        assert!(has_infill, "expected infill segments");
    }

    #[test]
    fn test_layer_z_values() {
        let layers = parse_gcode_bytes(SAMPLE_GCODE.as_bytes());
        let zs: Vec<f32> = layers.iter().map(|l| l.z).collect();
        assert!(
            zs.iter().any(|&z| (z - 0.2).abs() < 0.01),
            "expected z=0.2 layer, got {:?}",
            zs
        );
    }

    #[test]
    fn test_role_from_type_comment() {
        assert_eq!(Role::from_type_comment("Outer wall"), Role::OuterWall);
        assert_eq!(Role::from_type_comment("OuterWall"), Role::OuterWall);
        assert_eq!(Role::from_type_comment("Inner wall"), Role::InnerWall);
        assert_eq!(Role::from_type_comment("Infill"), Role::Infill);
        assert_eq!(Role::from_type_comment("Sparse infill"), Role::Infill);
        assert_eq!(Role::from_type_comment("Top surface"), Role::TopSurface);
        assert_eq!(
            Role::from_type_comment("Bottom surface"),
            Role::BottomSurface
        );
    }

    #[test]
    fn test_empty_input() {
        let layers = parse_gcode_bytes(b"");
        assert_eq!(
            layers.len(),
            1,
            "empty input should produce exactly 1 (empty) layer"
        );
    }
}
