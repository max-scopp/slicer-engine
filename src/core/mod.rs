//! Core slicing operations and data structures

mod infill;
mod pipeline;
mod slicer;
mod surfaces;
mod types;
mod walls;

pub use infill::add_infill_to_layers;
pub use pipeline::process_mesh;
pub use slicer::slice_mesh;
pub use surfaces::{generate_top_bottom_surfaces, generate_top_bottom_surfaces_with_interior};
pub use types::{ExtrusionRole, SliceLayer};

#[cfg(test)]
mod tests {
    use super::surfaces::{add_solid_infill_for_region, generate_rectilinear_infill};
    use super::*;
    use crate::mesh::types::{Face, Mesh, Vertex};
    use clipper2::{Path, Paths};

    /// Build a simple 10×10×10 mm axis-aligned box mesh (12 triangles).
    fn make_cube_mesh() -> Mesh {
        let v = [
            Vertex::new(0.0, 0.0, 0.0),    // 0
            Vertex::new(10.0, 0.0, 0.0),   // 1
            Vertex::new(10.0, 10.0, 0.0),  // 2
            Vertex::new(0.0, 10.0, 0.0),   // 3
            Vertex::new(0.0, 0.0, 10.0),   // 4
            Vertex::new(10.0, 0.0, 10.0),  // 5
            Vertex::new(10.0, 10.0, 10.0), // 6
            Vertex::new(0.0, 10.0, 10.0),  // 7
        ];

        let face_indices: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];

        let faces = face_indices
            .iter()
            .map(|idx| Face::new([v[idx[0]], v[idx[1]], v[idx[2]]]))
            .collect();

        Mesh {
            vertices: v.to_vec(),
            faces,
            aabb: None,
        }
    }

    #[test]
    fn test_slice_layer_creation() {
        let layer = SliceLayer::new(1.0);
        assert_eq!(layer.z, 1.0);
        assert!(layer.paths.is_empty());
        assert!(layer.path_roles.is_empty());
    }

    /// Regression test: with `only_one_wall_top = true`, the topmost layer
    /// of each top-surface run must be reduced to a single (outer) wall, and
    /// its TopSurface must extend out to the outer-wall edge — mirroring the
    /// behaviour of `only_one_wall_first_layer` on layer 0.
    ///
    /// The previous role-based detection in `apply_single_wall_restrictions`
    /// was a no-op (TopSurface roles are assigned later, after this runs),
    /// so the topmost layer kept all walls and the top surface was confined
    /// to a tiny disk inside the innermost wall — leaving a visible
    /// inter-wall gap that users perceived as the "between walls" bug
    /// persisting on top surfaces.
    #[test]
    fn test_only_one_wall_top_reduces_topmost_layer() {
        use crate::logging::NullLogger;
        let mesh = make_cube_mesh();
        let params = crate::settings::params::SlicingParams {
            layer_height: 2.0,
            top_layers: 2,
            bottom_layers: 2,
            surface_infill_angle: 0.0,
            only_one_wall_first_layer: true,
            only_one_wall_top: true,
            wall_count: 3,
            nozzle_diameter_mm: 0.4,
            infill_overlap_percent: 0.25,
            ..crate::settings::params::SlicingParams::default()
        };

        let layers = process_mesh(&mesh, &params, &NullLogger);
        assert!(!layers.is_empty(), "expected sliced layers");

        let last = layers.len() - 1;
        let n_outer_top = layers[last]
            .path_roles
            .iter()
            .filter(|r| **r == ExtrusionRole::OuterWall)
            .count();
        let n_inner_top = layers[last]
            .path_roles
            .iter()
            .filter(|r| **r == ExtrusionRole::InnerWall)
            .count();
        assert!(n_outer_top >= 1, "topmost layer must keep its outer wall");
        assert_eq!(
            n_inner_top, 0,
            "only_one_wall_top should strip all InnerWall paths from the topmost \
             layer of a top-surface run, but {n_inner_top} remain"
        );

        // The layer below the topmost is also part of the top-surface run
        // (top_layers = 2) but is NOT the last layer of the run — it must
        // keep its inner walls.
        let n_inner_below_top = layers[last - 1]
            .path_roles
            .iter()
            .filter(|r| **r == ExtrusionRole::InnerWall)
            .count();
        assert!(
            n_inner_below_top > 0,
            "only_one_wall_top must NOT strip inner walls from layers in the \
             middle of a top-surface run (only the very topmost)"
        );

        // With only the outer wall remaining on the topmost layer, the
        // TopSurface should now extend to the outer-wall edge (within the
        // configured overlap), exactly mirroring the BottomSurface AABB on
        // layer 0 where only_one_wall_first_layer has the same effect.
        let top_pts: Vec<(f64, f64)> = layers[last]
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layers[last].role_for_path(*i) == ExtrusionRole::TopSurface)
            .flat_map(|(_, p)| p.iter().map(|pt| (pt.x(), pt.y())).collect::<Vec<_>>())
            .collect();
        assert!(
            !top_pts.is_empty(),
            "topmost layer should have TopSurface paths"
        );
        let xmax = top_pts
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let xmin = top_pts
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::INFINITY, f64::min);
        // Cube spans [0, 10]. With 1 wall (centerline ~0.2mm in) plus 25%
        // overlap, the surface should reach within ~0.5mm of each edge.
        // The buggy 3-wall behaviour confined it to ~[1.1, 8.9] (≥1.1mm from
        // each edge), so this threshold reliably separates fixed vs broken.
        assert!(
            xmax >= 9.5 && xmin <= 0.5,
            "top surface should extend close to the outer wall edge \
             (got xmin={xmin:.2}, xmax={xmax:.2}); the buggy multi-wall behaviour \
             would confine it to ~[1.1, 8.9]"
        );
    }

    #[test]
    fn test_slice_layer_role_for_path_default() {
        let layer = SliceLayer::new(1.0);
        // No roles set → should fall back to OuterWall
        assert_eq!(layer.role_for_path(0), ExtrusionRole::OuterWall);
        assert_eq!(layer.role_for_path(99), ExtrusionRole::OuterWall);
    }

    #[test]
    fn test_slice_layer_role_for_path_explicit() {
        let mut layer = SliceLayer::new(1.0);
        layer.path_roles.push(ExtrusionRole::Skirt);
        layer.path_roles.push(ExtrusionRole::Infill);
        assert_eq!(layer.role_for_path(0), ExtrusionRole::Skirt);
        assert_eq!(layer.role_for_path(1), ExtrusionRole::Infill);
        // Out of bounds → OuterWall default
        assert_eq!(layer.role_for_path(2), ExtrusionRole::OuterWall);
    }

    #[test]
    fn test_extrusion_role_type_names() {
        assert_eq!(ExtrusionRole::OuterWall.type_name(), "Outer wall");
        assert_eq!(ExtrusionRole::InnerWall.type_name(), "Inner wall");
        assert_eq!(ExtrusionRole::Infill.type_name(), "Sparse infill");
        assert_eq!(ExtrusionRole::Bridge.type_name(), "Bridge");
        assert_eq!(ExtrusionRole::TopSurface.type_name(), "Top surface");
        assert_eq!(ExtrusionRole::BottomSurface.type_name(), "Bottom surface");
        assert_eq!(ExtrusionRole::Support.type_name(), "Support material");
        assert_eq!(ExtrusionRole::Skirt.type_name(), "Skirt");
    }

    #[test]
    fn test_extrusion_role_widths_positive() {
        for role in [
            ExtrusionRole::OuterWall,
            ExtrusionRole::InnerWall,
            ExtrusionRole::Infill,
            ExtrusionRole::Bridge,
            ExtrusionRole::TopSurface,
            ExtrusionRole::Support,
            ExtrusionRole::Skirt,
        ] {
            assert!(
                role.default_width_mm() > 0.0,
                "{:?} width must be positive",
                role
            );
        }
    }

    #[test]
    fn test_slice_mesh_path_roles_match_paths() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 2.0);
        for layer in &layers {
            assert_eq!(
                layer.paths.len(),
                layer.path_roles.len(),
                "path_roles length must match paths length at z={}",
                layer.z
            );
            for role in &layer.path_roles {
                assert_eq!(
                    *role,
                    ExtrusionRole::OuterWall,
                    "slice_mesh assigns OuterWall"
                );
            }
        }
    }

    #[test]
    fn test_slice_mesh_empty_mesh() {
        let mesh = Mesh::new();
        let layers = slice_mesh(&mesh, 0.2);
        assert!(layers.is_empty());
    }

    #[test]
    fn test_slice_mesh_zero_layer_height() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 0.0);
        assert!(layers.is_empty());
    }

    #[test]
    fn test_slice_mesh_negative_layer_height() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, -0.2);
        assert!(layers.is_empty());
    }

    #[test]
    fn test_slice_mesh_cube_layer_count() {
        let mesh = make_cube_mesh();
        // 10 mm cube sliced at 2 mm → 5 layers at z=1,3,5,7,9
        let layers = slice_mesh(&mesh, 2.0);
        assert_eq!(layers.len(), 5, "Expected 5 layers, got {}", layers.len());
    }

    #[test]
    fn test_slice_mesh_cube_z_values() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 2.0);
        let zs: Vec<f64> = layers.iter().map(|l| l.z).collect();
        // First layer at z_min + layer_height/2 = 0 + 1 = 1.0
        assert!((zs[0] - 1.0).abs() < 1e-10, "First layer z={}", zs[0]);
        assert!((zs[1] - 3.0).abs() < 1e-10, "Second layer z={}", zs[1]);
    }

    #[test]
    fn test_slice_mesh_cube_has_contours() {
        let mesh = make_cube_mesh();
        let layers = slice_mesh(&mesh, 2.0);
        // Every layer through the cube should have at least one contour
        for layer in &layers {
            assert!(
                !layer.paths.is_empty(),
                "Layer at z={} has no contours",
                layer.z
            );
        }
    }

    #[test]
    fn test_add_infill_to_layers_basic() {
        use crate::infill::InfillPattern;

        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);

        // Before infill: only wall paths
        for layer in &layers {
            for role in &layer.path_roles {
                assert!(
                    *role == ExtrusionRole::OuterWall || *role == ExtrusionRole::InnerWall,
                    "Expected wall role, got {:?}",
                    role
                );
            }
        }

        // Add infill
        add_infill_to_layers(&mut layers, 0.2, InfillPattern::Rectilinear, 45.0, 0.4, None);

        // After infill: should have both wall and infill paths
        for layer in &layers {
            let has_walls = layer
                .path_roles
                .iter()
                .any(|r| *r == ExtrusionRole::OuterWall || *r == ExtrusionRole::InnerWall);
            let has_infill = layer.path_roles.contains(&ExtrusionRole::Infill);
            assert!(has_walls, "Layer at z={} missing walls", layer.z);
            assert!(has_infill, "Layer at z={} missing infill", layer.z);
        }
    }

    #[test]
    fn test_add_infill_to_layers_zero_density() {
        use crate::infill::InfillPattern;

        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let initial_path_count: usize = layers.iter().map(|l| l.paths.len()).sum();

        // Add zero-density infill (should do nothing)
        add_infill_to_layers(&mut layers, 0.0, InfillPattern::Rectilinear, 45.0, 0.4, None);

        let final_path_count: usize = layers.iter().map(|l| l.paths.len()).sum();
        assert_eq!(
            initial_path_count, final_path_count,
            "Zero density should not add paths"
        );
    }

    #[test]
    fn test_add_infill_to_layers_grid_pattern() {
        use crate::infill::InfillPattern;

        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);

        // Add grid infill
        add_infill_to_layers(&mut layers, 0.3, InfillPattern::Grid, 45.0, 0.4, None);

        // Grid pattern should produce more infill paths than rectilinear
        for layer in &layers {
            let infill_count = layer
                .path_roles
                .iter()
                .filter(|r| **r == ExtrusionRole::Infill)
                .count();
            assert!(
                infill_count > 0,
                "Layer at z={} has no infill paths",
                layer.z
            );
        }
    }

    #[test]
    fn test_infill_not_placed_on_fully_solid_surface_layers() {
        use crate::infill::InfillPattern;

        // A 2-layer cube: with 2 top + 2 bottom layers, every layer is a
        // surface layer — sparse infill should not be added on top of solid surfaces.
        let mesh = make_cube_mesh();
        // 5 layers at 2mm.  Use top=2/bottom=2 so the first two and last two
        // layers are fully solid surfaces; the middle layer is interior.
        let mut layers = slice_mesh(&mesh, 2.0);
        generate_top_bottom_surfaces(&mut layers, 2, 2, 2.0, 45.0);

        // Confirm solid_regions are populated for the top/bottom layers.
        let n = layers.len();
        assert!(
            !layers[0].solid_regions.is_empty(),
            "Layer 0 should have solid_regions"
        );
        assert!(
            !layers[n - 1].solid_regions.is_empty(),
            "Last layer should have solid_regions"
        );

        // Count surface-only infill paths before adding sparse infill.
        let surface_counts: Vec<usize> = layers
            .iter()
            .map(|l| {
                l.path_roles
                    .iter()
                    .filter(|r| {
                        **r == ExtrusionRole::TopSurface || **r == ExtrusionRole::BottomSurface
                    })
                    .count()
            })
            .collect();

        // Now add sparse infill.
        add_infill_to_layers(&mut layers, 0.3, InfillPattern::Rectilinear, 45.0, 0.4, None);

        // For a layer that is entirely solid (solid_regions == perimeter area),
        // no new Infill paths should have been added.
        // Layers 0 and n-1 are entirely solid surfaces on a simple cube.
        for i in [0, n - 1] {
            let infill_added = layers[i]
                .path_roles
                .iter()
                .filter(|r| **r == ExtrusionRole::Infill)
                .count();
            assert_eq!(
                infill_added, 0,
                "Layer {} (fully solid surface) should not have sparse infill (got {})",
                i, infill_added
            );
            // Surface paths must remain unchanged.
            let surface_now = layers[i]
                .path_roles
                .iter()
                .filter(|r| {
                    **r == ExtrusionRole::TopSurface || **r == ExtrusionRole::BottomSurface
                })
                .count();
            assert_eq!(
                surface_now, surface_counts[i],
                "Surface path count on layer {} must not change",
                i
            );
        }
    }

    #[test]
    fn test_solid_regions_populated_by_surface_generation() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);

        // Initially no solid regions.
        for layer in &layers {
            assert!(layer.solid_regions.is_empty());
        }

        generate_top_bottom_surfaces(&mut layers, 3, 3, 2.0, 45.0);

        // After surface generation the topmost and bottommost layers must have
        // non-empty solid_regions.
        let n = layers.len();
        assert!(
            !layers[0].solid_regions.is_empty(),
            "Bottom layer should have solid_regions after surface generation"
        );
        assert!(
            !layers[n - 1].solid_regions.is_empty(),
            "Top layer should have solid_regions after surface generation"
        );
    }

    #[test]
    fn test_generate_top_bottom_surfaces_empty_layers() {
        let mut layers: Vec<SliceLayer> = vec![];
        generate_top_bottom_surfaces(&mut layers, 3, 3, 0.2, 45.0);
        // Should handle empty input gracefully
        assert!(layers.is_empty());
    }

    #[test]
    fn test_generate_top_bottom_surfaces_zero_count() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let original_count = layers.len();

        generate_top_bottom_surfaces(&mut layers, 0, 0, 2.0, 45.0);

        // Layers should remain unchanged when both counts are 0
        assert_eq!(layers.len(), original_count);
    }

    #[test]
    fn test_generate_top_bottom_surfaces_adds_infill() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let original_paths_first = layers[0].paths.len();

        // Generate bottom surfaces for first 2 layers, top for last 2
        generate_top_bottom_surfaces(&mut layers, 2, 2, 2.0, 45.0);

        // First layer should have more paths (original perimeters + infill)
        assert!(
            layers[0].paths.len() > original_paths_first,
            "Expected infill to be added to bottom layer"
        );
    }

    #[test]
    fn test_generate_top_bottom_surfaces_roles_assigned() {
        let mesh = make_cube_mesh();
        let mut layers = slice_mesh(&mesh, 2.0);
        let total = layers.len();

        generate_top_bottom_surfaces(&mut layers, 2, 2, 2.0, 45.0);

        // Check that bottom layers have BottomSurface role
        for (i, layer) in layers.iter().take(2).enumerate() {
            let has_bottom_role = layer.path_roles.contains(&ExtrusionRole::BottomSurface);
            assert!(
                has_bottom_role,
                "Layer {} should have BottomSurface role",
                i
            );
        }

        // Check that top layers have TopSurface role
        for (i, layer) in layers.iter().enumerate().skip(total - 2).take(2) {
            let has_top_role = layer.path_roles.contains(&ExtrusionRole::TopSurface);
            assert!(has_top_role, "Layer {} should have TopSurface role", i);
        }
    }

    #[test]
    fn test_generate_top_bottom_surfaces_mid_model_detection() {
        // Build a stacked two-cube mesh: a 10×10×4 base with a 6×6×4 column on top.
        // When sliced at layer_height=2 we get:
        //   layer 0  z=1 – base (10×10)
        //   layer 1  z=3 – base (10×10)
        //   layer 2  z=5 – column (6×6)
        //   layer 3  z=7 – column (6×6)
        //
        // With top_layers=1, bottom_layers=1 (intersection-based algorithm):
        //
        //   TopSurface on layer 1 (z=3):
        //     covered = intersect(10×10, layer_above=6×6) = 6×6
        //     top_region = diff(10×10, 6×6) = annular region → non-empty
        //     → layer 1 must have TopSurface infill
        //
        //   No TopSurface on layer 2 (z=5):
        //     covered = intersect(6×6, layer_above=6×6) = 6×6
        //     top_region = diff(6×6, 6×6) = empty
        //     → layer 2 is fully covered by layer 3 and must NOT have TopSurface infill
        //
        //   BottomSurface on layer 0 (z=1):
        //     i < j → covered = empty → bottom_region = perimeters[0] (first layer)
        //
        //   No BottomSurface on layer 2 (z=5):
        //     covered = intersect(6×6, layer_below=10×10) = 6×6 (column inside base)
        //     bottom_region = diff(6×6, 6×6) = empty
        //     → the column is fully supported; it must NOT get spurious BottomSurface infill

        let v: Vec<Vertex> = vec![
            // Base cube 10×10×4 (z 0..4)
            Vertex::new(0.0, 0.0, 0.0),
            Vertex::new(10.0, 0.0, 0.0),
            Vertex::new(10.0, 10.0, 0.0),
            Vertex::new(0.0, 10.0, 0.0),
            Vertex::new(0.0, 0.0, 4.0),
            Vertex::new(10.0, 0.0, 4.0),
            Vertex::new(10.0, 10.0, 4.0),
            Vertex::new(0.0, 10.0, 4.0),
            // Upper column 6×6×4 (z 4..8), centred at (2,2)..(8,8)
            Vertex::new(2.0, 2.0, 4.0),
            Vertex::new(8.0, 2.0, 4.0),
            Vertex::new(8.0, 8.0, 4.0),
            Vertex::new(2.0, 8.0, 4.0),
            Vertex::new(2.0, 2.0, 8.0),
            Vertex::new(8.0, 2.0, 8.0),
            Vertex::new(8.0, 8.0, 8.0),
            Vertex::new(2.0, 8.0, 8.0),
        ];
        let face_indices: &[[usize; 3]] = &[
            // Base cube faces
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
            // Column cube faces
            [8, 10, 9],
            [8, 11, 10],
            [12, 13, 14],
            [12, 14, 15],
            [8, 9, 13],
            [8, 13, 12],
            [10, 11, 15],
            [10, 15, 14],
            [8, 12, 15],
            [8, 15, 11],
            [9, 10, 14],
            [9, 14, 13],
        ];

        let faces = face_indices
            .iter()
            .map(|idx| Face::new([v[idx[0]], v[idx[1]], v[idx[2]]]))
            .collect();

        let mesh = Mesh {
            vertices: v,
            faces,
            aabb: None,
        };

        let mut layers = slice_mesh(&mesh, 2.0);
        assert_eq!(layers.len(), 4, "Expected 4 layers for the step mesh");

        generate_top_bottom_surfaces(&mut layers, 1, 1, 2.0, 45.0);

        // Layer 0 (z=1) is the absolute bottom → BottomSurface
        assert!(
            layers[0].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 0 (z=1) should be a bottom surface (first layer)"
        );

        // Layer 1 (z=3) is below the column; the annular 10×10 minus 6×6 region
        // is exposed above → TopSurface infill must be added.
        assert!(
            layers[1].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 1 (z=3) should detect the step-down as a top surface"
        );

        // Layer 2 (z=5) is fully covered by layer 3 above → no TopSurface here.
        assert!(
            !layers[2].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 2 (z=5) is fully covered above and must NOT have TopSurface infill"
        );

        // Layer 2 (z=5) is fully supported by layer 1 below → no BottomSurface.
        assert!(
            !layers[2].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 2 (z=5) is fully supported and must NOT have spurious BottomSurface infill"
        );

        // Layer 3 (z=7) is the absolute top → TopSurface
        assert!(
            layers[3].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 3 (z=7) should be a top surface (last layer)"
        );
    }

    #[test]
    fn test_infill_clipped_to_contour() {
        // Verify that infill lines are clipped to the contour and don't extend
        // beyond the bounding box of the given paths.
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        let mut paths = Paths::new(vec![]);
        paths.push(square);

        let infill = generate_rectilinear_infill(&paths, 1.0, 0.0);

        assert!(!infill.is_empty(), "Expected infill lines to be generated");

        // All clipped endpoints should lie within the contour bounding box
        // (with a small epsilon for floating-point rounding by Clipper2).
        let eps = 0.01;
        for path in infill.iter() {
            for pt in path.iter() {
                let x = pt.x();
                let y = pt.y();
                assert!(
                    x >= -eps && x <= 10.0 + eps,
                    "Infill x={x} is outside contour bounds [0, 10]"
                );
                assert!(
                    y >= -eps && y <= 10.0 + eps,
                    "Infill y={y} is outside contour bounds [0, 10]"
                );
            }
        }
    }

    #[test]
    fn test_add_solid_infill_for_region_empty_region() {
        let mut layer = SliceLayer::new(1.0);
        let empty: Paths = Paths::new(vec![]);
        add_solid_infill_for_region(&mut layer, &empty, ExtrusionRole::TopSurface, 0.2, 45.0);
        // Should handle empty region gracefully – no paths added
        assert!(layer.paths.is_empty());
    }

    #[test]
    fn test_extrusion_role_bottom_surface() {
        assert_eq!(ExtrusionRole::BottomSurface.type_name(), "Bottom surface");
        assert!(ExtrusionRole::BottomSurface.default_width_mm() > 0.0);
    }

    #[test]
    fn test_process_mesh() {
        use crate::logging::NullLogger;
        let mesh = make_cube_mesh();
        let params = crate::settings::params::SlicingParams {
            layer_height: 2.0,
            top_layers: 2,
            bottom_layers: 2,
            surface_infill_angle: 45.0,
            // Use old defaults for this test to verify basic functionality
            only_one_wall_first_layer: false,
            only_one_wall_top: false,
            ..crate::settings::params::SlicingParams::default()
        };

        let layers = process_mesh(&mesh, &params, &NullLogger);

        // Should have layers
        assert!(!layers.is_empty());

        // First layer should have BottomSurface paths
        assert!(layers[0].path_roles.contains(&ExtrusionRole::BottomSurface));

        // Last layer should have TopSurface paths
        let last_idx = layers.len() - 1;
        assert!(layers[last_idx]
            .path_roles
            .contains(&ExtrusionRole::TopSurface));
    }

    /// Regression test for the "surfaces between walls" bug.
    ///
    /// Slices a 10×10×10 mm cube with the default 3 walls and verifies that
    /// every BottomSurface coordinate on layer 0 is well inside the innermost
    /// wall, i.e. no surface line is drawn in the band between concentric
    /// walls.  The previous EvenOdd-based interior calculation produced
    /// surfaces in that band on multi-wall layers; this test guards against
    /// that regression.
    #[test]
    fn test_smart_surface_skipping_no_between_walls_artifacts() {
        use crate::logging::NullLogger;
        let mesh = make_cube_mesh();
        let params = crate::settings::params::SlicingParams {
            layer_height: 2.0,
            top_layers: 2,
            bottom_layers: 2,
            surface_infill_angle: 0.0,
            // Disable single-wall restrictions so all layers carry the full
            // multi-wall stack – this is the configuration that triggered the
            // original bug.
            only_one_wall_first_layer: false,
            only_one_wall_top: false,
            // Explicit defaults to make the geometry expectations precise.
            wall_count: 3,
            nozzle_diameter_mm: 0.4,
            infill_overlap_percent: 0.25,
            ..crate::settings::params::SlicingParams::default()
        };

        let layers = process_mesh(&mesh, &params, &NullLogger);
        assert!(!layers.is_empty(), "expected sliced layers");

        // Cube is at [0, 10]² in XY.  With 3 × 0.4 mm walls the innermost
        // wall centerline sits ~1.0 mm from each edge, so its inner bound is
        // ~1.2 mm.  The 25 % overlap (= 0.1 mm) lets surfaces extend back
        // out to ~1.1 mm.  Any surface point closer than 0.5 mm to an edge
        // would lie in the inter-wall band and is the bug we are guarding.
        const SAFE_MARGIN_MM: f64 = 0.5;

        let mut total_surface_points = 0;
        for layer in &layers {
            for (i, path) in layer.paths.iter().enumerate() {
                let role = layer.role_for_path(i);
                if role != ExtrusionRole::BottomSurface && role != ExtrusionRole::TopSurface {
                    continue;
                }
                for pt in path.iter() {
                    total_surface_points += 1;
                    let (x, y) = (pt.x(), pt.y());
                    assert!(
                        (SAFE_MARGIN_MM..=10.0 - SAFE_MARGIN_MM).contains(&x)
                            && (SAFE_MARGIN_MM..=10.0 - SAFE_MARGIN_MM).contains(&y),
                        "surface point ({x}, {y}) lies in the inter-wall band on \
                         layer z={} (role={:?}) – smart surface skipping regressed",
                        layer.z,
                        role,
                    );
                }
            }
        }

        assert!(
            total_surface_points > 0,
            "expected some surface paths; smart-skip should not skip cube top/bottom"
        );
    }

    #[test]
    fn test_generate_rectilinear_infill_basic() {
        // Create a simple square path
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        let mut paths = Paths::new(vec![]);
        paths.push(square);

        let infill = generate_rectilinear_infill(&paths, 1.0, 45.0);

        // Should generate some infill lines
        assert!(!infill.is_empty(), "Expected infill lines to be generated");
    }

    #[test]
    fn test_generate_rectilinear_infill_empty_contours() {
        let paths = Paths::new(vec![]);
        let infill = generate_rectilinear_infill(&paths, 1.0, 45.0);
        assert!(infill.is_empty());
    }

    #[test]
    fn test_generate_rectilinear_infill_zero_spacing() {
        let square: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();
        let mut paths = Paths::new(vec![]);
        paths.push(square);

        let infill = generate_rectilinear_infill(&paths, 0.0, 45.0);
        assert!(infill.is_empty());
    }

    /// Regression test: the surface detection algorithm must use progressive
    /// intersection, not a single comparison against the N-th neighbour.
    ///
    /// The "hourglass" scenario has a wide layer, a narrow intermediate layer,
    /// and then wide layers again.  The old `difference(layer[i], layer[i+N])`
    /// approach compared a wide layer against a later wide layer and returned
    /// empty → no top surface, silently missing the narrow gap in between.
    /// With the intersection-based approach the coverage is narrowed by the
    /// intermediate narrow layer, so the annular region is correctly flagged.
    #[test]
    fn test_surface_detection_non_monotonic_shape() {
        // Layers (manual construction, not mesh-derived):
        //   layer 0: 10×10 wide
        //   layer 1: 10×10 wide
        //   layer 2:  4×4  narrow  ← the "waist"
        //   layer 3: 10×10 wide
        //   layer 4: 10×10 wide
        //   layer 5: 10×10 wide
        //
        // With top_layers=3:
        //
        //   layer 2 (narrow, 4×4): covered by layers 3,4,5 (all 10×10 ⊇ 4×4)
        //     → NOT a top surface ✓
        //
        //   layer 0 (wide, 10×10):
        //     NEW: j=1 → intersect(10×10, 10×10) = 10×10
        //          j=2 → intersect(10×10, 4×4)   = 4×4   ← narrows
        //          j=3 → intersect(4×4,  10×10)  = 4×4
        //          top_region = diff(10×10, 4×4) = annular  ← TOP SURFACE ✓
        //     OLD: diff(layer[0], layer[3]) = diff(10×10, 10×10) = empty  ✗
        let make_rect_layer = |z: f64, w: f64, h: f64| -> SliceLayer {
            let mut layer = SliceLayer::new(z);
            let path: Path = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].into();
            layer.paths.push(path);
            layer.path_roles.push(ExtrusionRole::OuterWall);
            layer
        };

        let mut layers = vec![
            make_rect_layer(1.0, 10.0, 10.0), // 0 – wide
            make_rect_layer(2.0, 10.0, 10.0), // 1 – wide
            make_rect_layer(3.0, 4.0, 4.0),   // 2 – narrow
            make_rect_layer(4.0, 10.0, 10.0), // 3 – wide
            make_rect_layer(5.0, 10.0, 10.0), // 4 – wide
            make_rect_layer(6.0, 10.0, 10.0), // 5 – wide
        ];

        generate_top_bottom_surfaces(&mut layers, 3, 0, 1.0, 45.0);

        // Layer 2 (narrow): fully covered by the three wide layers above it.
        assert!(
            !layers[2].path_roles.contains(&ExtrusionRole::TopSurface),
            "Narrow layer 2 is fully covered above and must NOT have TopSurface infill"
        );

        // Layer 0: the 10×10 annular area is NOT covered at layer 2 (only 4×4)
        // → must be flagged as a top surface even though layer[0+3]=layer3 is
        //   also 10×10 (the gap at layer 2 is in between).
        assert!(
            layers[0].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 0 should have TopSurface infill: the annular region is exposed at layer 2"
        );

        // Layers 3, 4, 5 are the top-3 wide layers → must all be top surfaces.
        for idx in [3, 4, 5] {
            assert!(
                layers[idx].path_roles.contains(&ExtrusionRole::TopSurface),
                "Layer {idx} is within top_layers=3 of the model top and must have TopSurface"
            );
        }
    }

    /// Test that top and bottom surfaces don't overlap on the same layer.
    /// This was a bug where regions could be marked as both top AND bottom,
    /// causing incorrect G-code output.
    #[test]
    fn test_no_overlapping_top_bottom_surfaces() {
        // Create a simple layer stack where the first layer could potentially
        // be marked as both top and bottom if the algorithm is broken.
        let make_rect_layer = |z: f64, w: f64, h: f64| -> SliceLayer {
            let mut layer = SliceLayer::new(z);
            let path: Path = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].into();
            layer.paths.push(path);
            layer.path_roles.push(ExtrusionRole::OuterWall);
            layer
        };

        let mut layers = vec![
            make_rect_layer(1.0, 10.0, 10.0), // 0 – base layer
            make_rect_layer(2.0, 10.0, 10.0), // 1
            make_rect_layer(3.0, 10.0, 10.0), // 2
            make_rect_layer(4.0, 10.0, 10.0), // 3 – top layer
        ];

        // Generate with both top_layers and bottom_layers enabled
        generate_top_bottom_surfaces(&mut layers, 2, 2, 1.0, 45.0);

        // Check each layer to ensure no path is in BOTH top and bottom regions
        for (layer_idx, layer) in layers.iter().enumerate() {
            let has_top = layer.path_roles.contains(&ExtrusionRole::TopSurface);
            let has_bottom = layer.path_roles.contains(&ExtrusionRole::BottomSurface);

            // Count the actual number of each type
            let top_count = layer
                .path_roles
                .iter()
                .filter(|&&r| r == ExtrusionRole::TopSurface)
                .count();
            let bottom_count = layer
                .path_roles
                .iter()
                .filter(|&&r| r == ExtrusionRole::BottomSurface)
                .count();

            if has_top && has_bottom {
                panic!(
                    "Layer {} has BOTH top ({}) and bottom ({}) surface paths - they should not overlap!",
                    layer_idx, top_count, bottom_count
                );
            }
        }

        // Layer 0 should be bottom only (first two layers)
        assert!(
            layers[0].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 0 should have bottom surface"
        );
        assert!(
            !layers[0].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 0 should NOT have top surface"
        );

        // Layer 3 (top) should be top only
        assert!(
            layers[3].path_roles.contains(&ExtrusionRole::TopSurface),
            "Layer 3 should have top surface"
        );
        assert!(
            !layers[3].path_roles.contains(&ExtrusionRole::BottomSurface),
            "Layer 3 should NOT have bottom surface"
        );
    }

    /// Test that surface generation correctly handles holes (inner contours).
    /// When a layer has a hole, the surface infill should not fill the hole.
    ///
    /// Critically, this test uses the **same winding order** for both outer
    /// and hole contours — the exact case that `FillRule::NonZero` gets wrong
    /// (it treats the hole as doubly-wound solid material).  `EvenOdd` handles
    /// it correctly regardless of winding direction.
    #[test]
    fn test_surface_generation_with_holes() {
        use clipper2::Path;

        // Create a layer with an outer square and an inner square (hole).
        // Both contours use the same winding order (right → up → left → down),
        // which is the problematic case for NonZero but handled correctly by EvenOdd.
        let mut layer = SliceLayer::new(1.0);

        // Outer square 10x10 (same winding as inner = the hard case)
        let outer: Path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into();

        // Inner square 4x4 (same winding as outer — NonZero would treat this as solid)
        let hole: Path = vec![(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)].into();

        layer.paths.push(outer);
        layer.path_roles.push(ExtrusionRole::OuterWall);
        layer.paths.push(hole);
        layer.path_roles.push(ExtrusionRole::OuterWall);

        // Create a simple 1-layer setup
        let mut layers = vec![layer];

        // Generate bottom surfaces (first layer, no layers below)
        generate_top_bottom_surfaces(&mut layers, 0, 1, 1.0, 45.0);

        // Count the surface infill paths
        let surface_path_count = layers[0]
            .path_roles
            .iter()
            .filter(|&&r| r == ExtrusionRole::BottomSurface)
            .count();

        // There should be surface paths
        assert!(
            surface_path_count > 0,
            "Should have generated bottom surface infill"
        );

        // Collect all bottom surface path segments
        let surface_paths: Vec<&Path> = layers[0]
            .paths
            .iter()
            .enumerate()
            .filter(|(i, _)| layers[0].role_for_path(*i) == ExtrusionRole::BottomSurface)
            .map(|(_, p)| p)
            .collect();

        println!("Generated {} bottom surface paths", surface_paths.len());

        // Check if any surface path segments pass through the hole region
        // The hole is at (3,3) to (7,7).
        // With EvenOdd fill rule, holes are correctly excluded regardless of
        // the winding order of the contours, so no infill should be inside.
        let mut paths_in_hole = 0;
        for path in &surface_paths {
            for pt in path.iter() {
                let x = pt.x();
                let y = pt.y();
                // Check if point is inside the hole region (with small margin)
                if x > 3.5 && x < 6.5 && y > 3.5 && y < 6.5 {
                    paths_in_hole += 1;
                    break; // Count each path only once
                }
            }
        }

        assert_eq!(
            paths_in_hole, 0,
            "Surface infill must not penetrate the hole region (found {} paths inside hole). \
             EvenOdd fill rule should handle this regardless of contour winding order.",
            paths_in_hole
        );
    }
}
