import { RuntimeMeshInput, RuntimeSceneSnapshot } from './scene-commands';

export interface RuntimeSliceRequest {
  sliceId: string;
  request_uuid?: string;
  model?: RuntimeMeshInput;
  scene?: RuntimeSceneSnapshot;
  settings: Record<string, unknown>;
}

export interface RuntimeSliceResult {
  sliceId: string;
  layerCount: number;
  gcodeText?: string;
  downloadUrl?: string;
}
