use js_sys::Float32Array;
use wasm_bindgen::prelude::*;

use super::parser::parse_gcode_bytes;
use super::types::InternalLayer;

// ── GcodeLayerBuffer ────────────────────────────────────────────────────────

/// Per-layer geometry buffers, one `Float32Array` per extrusion role.
///
/// Each array contains flat line-segment pairs:
/// `[x0, y0, z0,  x1, y1, z1,  …]`  (6 floats per segment).
#[wasm_bindgen]
pub struct GcodeLayerBuffer {
    z: f32,
    outer_wall: Float32Array,
    inner_wall: Float32Array,
    infill: Float32Array,
    top_surface: Float32Array,
    bottom_surface: Float32Array,
    travel: Float32Array,
    other: Float32Array,
}

#[wasm_bindgen]
impl GcodeLayerBuffer {
    /// Z coordinate of this layer.
    #[wasm_bindgen(getter)]
    pub fn z(&self) -> f32 {
        self.z
    }

    /// Outer wall / perimeter move segments.
    #[wasm_bindgen(getter)]
    pub fn outer_wall(&self) -> Float32Array {
        self.outer_wall.clone()
    }

    /// Inner wall / inner perimeter move segments.
    #[wasm_bindgen(getter)]
    pub fn inner_wall(&self) -> Float32Array {
        self.inner_wall.clone()
    }

    /// Sparse infill move segments.
    #[wasm_bindgen(getter)]
    pub fn infill(&self) -> Float32Array {
        self.infill.clone()
    }

    /// Top surface solid infill move segments.
    #[wasm_bindgen(getter)]
    pub fn top_surface(&self) -> Float32Array {
        self.top_surface.clone()
    }

    /// Bottom surface solid infill move segments.
    #[wasm_bindgen(getter)]
    pub fn bottom_surface(&self) -> Float32Array {
        self.bottom_surface.clone()
    }

    /// Travel (non-extruding) move segments.
    #[wasm_bindgen(getter)]
    pub fn travel(&self) -> Float32Array {
        self.travel.clone()
    }

    /// Segments not matching any recognised role.
    #[wasm_bindgen(getter)]
    pub fn other(&self) -> Float32Array {
        self.other.clone()
    }
}

fn into_float32_array(data: &[f32]) -> Float32Array {
    Float32Array::from(data)
}

fn layer_to_buffer(layer: &InternalLayer) -> GcodeLayerBuffer {
    GcodeLayerBuffer {
        z: layer.z,
        outer_wall: into_float32_array(&layer.outer_wall),
        inner_wall: into_float32_array(&layer.inner_wall),
        infill: into_float32_array(&layer.infill),
        top_surface: into_float32_array(&layer.top_surface),
        bottom_surface: into_float32_array(&layer.bottom_surface),
        travel: into_float32_array(&layer.travel),
        other: into_float32_array(&layer.other),
    }
}

// ── GcodeHandle ─────────────────────────────────────────────────────────────

/// Owned handle over all parsed layers of a GCode file.
///
/// ```js
/// const handle = GcodeHandle.parse(new Uint8Array(bytes));
/// console.log(handle.layerCount());   // total layers
/// const layer = handle.getLayer(5);   // GcodeLayerBuffer
/// const buf   = layer.outer_wall;     // Float32Array [x0,y0,z0, x1,y1,z1, …]
/// ```
#[wasm_bindgen]
pub struct GcodeHandle {
    layers: Vec<InternalLayer>,
}

#[wasm_bindgen]
impl GcodeHandle {
    /// Parse a complete GCode file from raw bytes.
    ///
    /// Accepts both UTF-8 and ASCII. Invalid byte sequences are replaced with
    /// the Unicode replacement character (`U+FFFD`).
    #[wasm_bindgen]
    pub fn parse(bytes: &[u8]) -> GcodeHandle {
        console_error_panic_hook::set_once();
        GcodeHandle {
            layers: parse_gcode_bytes(bytes),
        }
    }

    /// Total number of layers detected in the file.
    #[wasm_bindgen(js_name = layerCount)]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Z coordinate for the layer at `index`. Returns `0.0` for out-of-bounds.
    #[wasm_bindgen(js_name = layerZ)]
    pub fn layer_z(&self, index: usize) -> f32 {
        self.layers.get(index).map(|l| l.z).unwrap_or(0.0)
    }

    /// Geometry buffers for the layer at `index`.
    ///
    /// Returns a `JsValue` error if `index >= layer_count()`.
    #[wasm_bindgen(js_name = getLayer)]
    pub fn get_layer(&self, index: usize) -> Result<GcodeLayerBuffer, JsValue> {
        self.layers.get(index).map(layer_to_buffer).ok_or_else(|| {
            JsValue::from_str(&format!(
                "layer index {index} out of range (layer_count = {})",
                self.layers.len()
            ))
        })
    }
}
