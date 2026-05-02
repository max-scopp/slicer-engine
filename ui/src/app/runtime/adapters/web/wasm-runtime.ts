import { SlicingParams } from '../../../../generated/slicer-engine-ws-client-message-v1';
import { SceneEngine, SceneOp } from '../../../services/scene-engine';
import { RuntimeHistorySession } from '../../domain/history-models';
import { RuntimePreviewSource } from '../../domain/preview-models';
import {
    RuntimeMeshInput,
    RuntimeSceneOp,
    RuntimeSceneSnapshot,
} from '../../domain/scene-commands';
import { RuntimeSliceRequest, RuntimeSliceResult } from '../../domain/slice-commands';
import { RuntimeEventBus } from '../../infrastructure/event-bus';
import { RuntimeCapabilities } from '../../ports/runtime-capabilities';
import { RuntimeError } from '../../ports/runtime-errors';
import { RuntimeEventListener } from '../../ports/runtime-events';
import { RuntimePort, RuntimeSubscription } from '../../ports/runtime-port';

const WEB_CAPABILITIES: RuntimeCapabilities = {
  supportsLocalSlicing: true,
  supportsRemoteJobs: false,
  supportsStreamingProgress: true,
  supportsSceneSnapshotPull: true,
};

export class WasmRuntime implements RuntimePort {
  private readonly bus = new RuntimeEventBus();
  private readonly previewBySlice = new Map<string, string>();
  private initialized = false;

  constructor(private readonly sceneEngine: SceneEngine) {}

  async init(): Promise<void> {
    await this.sceneEngine.ready();
    this.initialized = true;
    this.bus.emit({ type: 'connected', mode: 'web' });
  }

  getCapabilities(): RuntimeCapabilities {
    return WEB_CAPABILITIES;
  }

  async addMesh(input: RuntimeMeshInput): Promise<string> {
    this.requireReady();
    if (!input.bytes) {
      throw new Error(`WASM runtime requires bytes for '${input.fileName}'`);
    }
    const objectId = this.sceneEngine.addMesh(input.fileName, input.format, input.bytes);
    return objectId.toString();
  }

  async applySceneOps(ops: RuntimeSceneOp[]): Promise<void> {
    this.requireReady();
    const mappedOps: SceneOp[] = ops.map((op) => {
      const id = BigInt(op.id);
      switch (op.op) {
        case 'remove':
          return { op: 'remove', args: { id } };
        case 'translate':
          return { op: 'translate', args: { id, delta: op.delta } };
        case 'set_transform':
          return {
            op: 'set_transform',
            args: {
              id,
              translation: op.translation,
              euler_xyz_deg: op.euler_xyz_deg,
              scale: op.scale,
            },
          };
        case 'rotate':
          return { op: 'rotate', args: { id, axis: op.axis, degrees: op.degrees } };
        case 'scale':
          return { op: 'scale', args: { id, factors: op.factors } };
        case 'center_on_bed':
          return { op: 'center_on_bed', args: { id } };
        case 'drop_to_floor':
          return { op: 'drop_to_floor', args: { id } };
        case 'place_face_on_floor':
          return { op: 'place_face_on_floor', args: { id, face_index: op.face_index } };
      }
    });

    this.sceneEngine.applyBatch(mappedOps);
  }

  async getSceneSnapshot(): Promise<RuntimeSceneSnapshot> {
    this.requireReady();
    const snapshot = this.sceneEngine.snapshot();
    return {
      objects: snapshot.objects.map((object) => ({
        id: object.id.toString(),
        name: object.name,
        translation: object.translation,
        euler_xyz_deg: object.euler_xyz_deg,
        scale: object.scale,
        triangle_count: object.triangle_count,
        world_aabb: object.world_aabb,
      })),
    };
  }

  async getHistory(): Promise<RuntimeHistorySession[]> {
    this.requireReady();
    return [];
  }

  async slice(request: RuntimeSliceRequest): Promise<RuntimeSliceResult> {
    this.requireReady();
    this.bus.emit({ type: 'phase-start', sliceId: request.sliceId, phase: 'total' });
    const result = this.sceneEngine.sliceToGcode(request.settings as SlicingParams);
    this.previewBySlice.set(request.sliceId, result.gcode);
    this.bus.emit({ type: 'progress', sliceId: request.sliceId, currentLayer: 1, totalLayers: 1 });
    this.bus.emit({ type: 'phase-end', sliceId: request.sliceId, phase: 'total', elapsedMs: 0 });
    this.bus.emit({
      type: 'slice-complete',
      sliceId: request.sliceId,
      layerCount: result.layer_count,
    });
    return {
      sliceId: request.sliceId,
      layerCount: result.layer_count,
      gcodeText: result.gcode,
    };
  }

  async cancel(_sliceId: string): Promise<void> {
    this.requireReady();
  }

  async getPreviewSource(sliceId: string): Promise<RuntimePreviewSource> {
    this.requireReady();
    const gcode = this.previewBySlice.get(sliceId);
    if (!gcode) {
      return { kind: 'none' };
    }

    return {
      kind: 'gcode-inline',
      gcode,
    };
  }

  onEvent(listener: RuntimeEventListener): RuntimeSubscription {
    return this.bus.subscribe(listener);
  }

  async dispose(): Promise<void> {
    this.initialized = false;
    this.bus.clear();
  }

  private requireReady(): void {
    if (!this.initialized) {
      const error: RuntimeError = {
        code: 'not_ready',
        message: 'Wasm runtime has not been initialized.',
      };
      this.bus.emit({ type: 'error', error });
      throw new Error(error.message);
    }
  }
}
