//! Code-generation binary: emits TypeScript types for the WebSocket protocol.
//!
//! The generated types mirror `src/ws_protocol.rs` exactly.  Run this whenever
//! the protocol changes to keep the frontend in sync:
//!
//! ```bash
//! cargo run --bin gen-ws-schemas > ui/src/generated/ws-protocol.ts
//! ```

fn main() {
    print!("{}", WS_PROTOCOL_TS);
}

/// TypeScript source that mirrors [`slicer_engine::ws_protocol`].
///
/// Field names follow camelCase to match `#[serde(rename_all = "camelCase")]`
/// on the Rust side; variant names follow the `#[serde(tag = "type")]`
/// discriminant values produced by serde's snake_case renaming.
const WS_PROTOCOL_TS: &str = r#"// ============================================================
// GENERATED FILE — do not edit by hand.
// Re-generate with: cargo run --bin gen_ws_schemas > ui/src/generated/ws-protocol.ts
// Source of truth: src/ws_protocol.rs
// ============================================================

/** Slicing parameters sent from the browser with a {@link ClientMessage} `slice` request. */
export interface WsSlicingParams {
  /** Layer height in mm (e.g. 0.2). */
  layerHeight: number;
  /** Print speed in mm/s. */
  printSpeed: number;
  /** Nozzle temperature in °C. */
  nozzleTemp: number;
  /** Heated-bed temperature in °C. */
  bedTemp: number;
  /** G-code dialect — `"marlin"` or `"klipper"`. */
  gcodeFlavor: string;
}

/** Messages sent **from the browser to the server**. */
export type ClientMessage =
  | {
      /** Start a slice job. */
      type: 'slice';
      /** Base64-encoded raw STL file bytes. */
      stlB64: string;
      settings: WsSlicingParams;
    }
  | {
      /** Abort / reset the current state. */
      type: 'reset';
    };

/** Messages sent **from the server to the browser**. */
export type ServerMessage =
  | {
      /** Sent once immediately after the WebSocket handshake. */
      type: 'connected';
      version: string;
    }
  | {
      /** A log line for the status panel. */
      type: 'log';
      /** `"info"` | `"warn"` | `"error"` */
      level: string;
      message: string;
    }
  | {
      /** Incremental slicing progress. */
      type: 'progress';
      currentLayer: number;
      totalLayers: number;
    }
  | {
      /** Slice finished successfully. */
      type: 'sliceComplete';
      /** Full G-code output as a UTF-8 string. */
      gcode: string;
      layerCount: number;
    }
  | {
      /** A fatal error occurred during processing. */
      type: 'error';
      message: string;
    };
"#;
