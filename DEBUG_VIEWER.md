# Debug Layer Viewer

The debug layer viewer provides a visual interface for inspecting the output of each step in the slicing pipeline. This is invaluable for debugging Boolean operations, checking wall generation, verifying infill patterns, and understanding how top/bottom surfaces are computed.

## Architecture

### Backend Components

**Serialization (`src/core.rs`)**
- `SerializableLayer`: JSON-friendly version of `SliceLayer` that converts Clipper2's Centi coordinates to floating-point millimeters
- `ExtrusionRole`: Now derives `Serialize`/`Deserialize` for role-based color coding in the UI
- `SliceLayer::to_serializable()`: Converts internal representation to debug-friendly format

**API Endpoint (`src/server/handlers.rs`)**
- `GET /api/debug/:uuid`: Returns JSON array of serialized layers for a completed slice job
- Layers are automatically saved to `{work_dir}/{uuid}.layers.json` during slicing

**Pipeline Integration (`src/server/ws_session.rs`)**
- After `process_mesh()` completes, layers are serialized and written to disk
- Non-fatal: if debug file write fails, slicing continues normally

### Frontend Components

**DebugViewerComponent (`ui/src/app/components/debug-viewer/`)**
- Layer slider: scrub through Z-height layers (0 to N)
- Role toggles: show/hide paths by ExtrusionRole
- SVG viewport: renders selected layer with color-coded paths
- Auto-loads debug data when slice completes

**Color Scheme**
| Role          | Color   | Hex       |
|---------------|---------|-----------|
| Perimeter     | Blue    | `#3b82f6` |
| TopSurface    | Red     | `#ef4444` |
| BottomSurface | Green   | `#22c55e` |
| Infill        | Yellow  | `#eab308` |
| Support       | Orange  | `#f97316` |
| Bridge        | Purple  | `#a855f7` |
| Skirt         | Gray    | `#6b7280` |

## Usage

1. **Run the server:**
   ```bash
   cargo run -- serve
   ```

2. **Open the UI** at `http://localhost:4200`

3. **Upload and slice** an STL file

4. **Once slicing completes**, the debug viewer at the bottom of the page automatically loads

5. **Interact with the viewer:**
   - Drag the layer slider to move between layers
   - Toggle role checkboxes to isolate specific path types
   - Zoom/pan by adjusting your browser viewport (SVG auto-scales)

## Implementation Notes

### Coordinate Transformation
Clipper2 uses integer centimeters (`Centi`) internally. The serialization converts to floating-point millimeters for easier debugging:
```rust
[pt.x() as f64 / 100.0, pt.y() as f64 / 100.0]
```

### ViewBox Calculation
The SVG viewBox is computed from the bounding box of all layers plus 10% padding, ensuring the entire model fits in the viewport regardless of size.

### Path Rendering
Each path is rendered as an SVG `<path>` with:
- **Stroke**: role color at 0.2mm width (non-scaling)
- **Fill**: same color at 30% opacity to distinguish overlapping regions
- **Z-order**: paths are drawn in the order they appear in the layer

### Performance
- Layer data is ~5 MB for a 100-layer print with typical wall counts
- All layers are loaded at once (no pagination)
- SVG rendering is hardware-accelerated in modern browsers
- For >1000 layers, consider adding layer range filtering

## Future Enhancements

**Per-step visualization**
Currently shows the final layer output. Could capture intermediate states:
- Raw contours (after `slice_mesh()`)
- After Arachne wall generation
- After top/bottom surface detection
- After infill generation

**3D mode (optional)**
For complex models, a Three.js viewer could show the entire print volume, but adds significant complexity for minimal debugging value (every issue is visible in 2D slices).

**Export to SVG**
Add a button to download the current layer as a standalone SVG file for external analysis or reporting.

## Testing

Run the backend serialization tests:
```bash
cargo test test_serializable_layer
cargo test test_extrusion_role_serialization
```

Build the frontend:
```bash
cd ui && npm run build
```

Manual end-to-end test:
1. Upload `stls/benchy.stl` (or any test model)
2. Slice with default settings
3. Verify debug viewer loads and shows correct layer count
4. Scrub through layers and verify geometry matches expectations
5. Toggle roles and verify filtering works

## Troubleshooting

**"Debug layer data not available"**
- The slice job may have failed before saving layers
- Check server logs for errors during `process_mesh()`
- Verify the work directory has write permissions

**Blank viewport**
- Check browser console for JavaScript errors
- Verify layer data has non-empty `paths` arrays
- Check that `viewBox` calculation didn't produce invalid values

**Colors don't match roles**
- Verify `path_roles` array length matches `paths` array length
- Check that role names in the backend match frontend toggle labels (case-sensitive)
