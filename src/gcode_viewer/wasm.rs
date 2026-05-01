use js_sys::Float32Array;
use wasm_bindgen::prelude::*;

use super::parser::parse_gcode_bytes;
use super::types::InternalLayer;

// ── GcodeLayerBuffer ────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct GcodeLayerBuffer {
    z: f32,
    blocks_roles: Vec<u8>,
    blocks_kinds: Vec<u8>,
    blocks_data: Vec<Float32Array>,
}

#[wasm_bindgen]
impl GcodeLayerBuffer {
    /// Z coordinate of this layer.
    #[wasm_bindgen(getter)]
    pub fn z(&self) -> f32 {
        self.z
    }

    #[wasm_bindgen(js_name = blocksCount)]
    pub fn blocks_count(&self) -> usize {
        self.blocks_roles.len()
    }

    #[wasm_bindgen(js_name = blockRole)]
    pub fn block_role(&self, i: usize) -> u8 {
        self.blocks_roles[i]
    }

    /// Geometric kind of the block at index `i`: `0` = line, `1` = arc.
    /// Line blocks pack 8 floats per segment; arc blocks pack 11.
    #[wasm_bindgen(js_name = blockKind)]
    pub fn block_kind(&self, i: usize) -> u8 {
        self.blocks_kinds[i]
    }

    #[wasm_bindgen(js_name = blockData)]
    pub fn block_data(&self, i: usize) -> Float32Array {
        self.blocks_data[i].clone()
    }
}

fn into_float32_array(data: &[f32]) -> Float32Array {
    Float32Array::from(data)
}

fn layer_to_buffer(layer: &InternalLayer) -> GcodeLayerBuffer {
    let mut roles = Vec::with_capacity(layer.blocks.len());
    let mut kinds = Vec::with_capacity(layer.blocks.len());
    let mut data = Vec::with_capacity(layer.blocks.len());
    for b in &layer.blocks {
        roles.push(b.role.id());
        kinds.push(b.kind.id());
        data.push(into_float32_array(&b.data));
    }
    GcodeLayerBuffer {
        z: layer.z,
        blocks_roles: roles,
        blocks_kinds: kinds,
        blocks_data: data,
    }
}

// ── GcodeHandle ─────────────────────────────────────────────────────────────

/// Owned handle over all parsed layers of a GCode file.
///
/// ```js
/// const handle = GcodeHandle.parse(new Uint8Array(bytes));
/// console.log(handle.layerCount());   // total layers
/// const layer = handle.getLayer(5);   // GcodeLayerBuffer
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
