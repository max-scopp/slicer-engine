export type RuntimeObjectId = string;

export type RuntimeSceneOp =
  | { op: 'remove'; id: RuntimeObjectId }
  | { op: 'translate'; id: RuntimeObjectId; delta: [number, number, number] }
  | {
      op: 'set_transform';
      id: RuntimeObjectId;
      translation: [number, number, number];
      euler_xyz_deg: [number, number, number];
      scale: [number, number, number];
    }
  | { op: 'rotate'; id: RuntimeObjectId; axis: [number, number, number]; degrees: number }
  | { op: 'scale'; id: RuntimeObjectId; factors: [number, number, number] }
  | { op: 'center_on_bed'; id: RuntimeObjectId }
  | { op: 'drop_to_floor'; id: RuntimeObjectId }
  | { op: 'place_face_on_floor'; id: RuntimeObjectId; face_index: number };

export interface RuntimeMeshInput {
  fileName: string;
  format: 'stl' | 'obj' | '3mf';
  /** Raw mesh bytes. Required for WASM/cloud runtimes. For the native Tauri
   *  runtime this is `undefined` when a file was selected via the OS dialog —
   *  the runtime reads from `filePath` directly instead. */
  bytes?: Uint8Array;
  /** Absolute filesystem path when the file was selected via native dialog.
   *  When set, the Tauri runtime passes only this path to Rust (no bytes over IPC). */
  filePath?: string;
}

export interface RuntimeSceneObject {
  id: RuntimeObjectId;
  name: string;
  translation: [number, number, number];
  euler_xyz_deg: [number, number, number];
  scale: [number, number, number];
  triangle_count: number;
  world_aabb: [[number, number, number], [number, number, number]];
}

export interface RuntimeSceneSnapshot {
  objects: RuntimeSceneObject[];
}
