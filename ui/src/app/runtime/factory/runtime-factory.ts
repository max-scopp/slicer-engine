import { SceneEngine } from '../../services/scene-engine';
import { SlicerConnection } from '../../services/slicer-connection';
import { SlicerFile } from '../../services/slicer-file';
import { CloudRuntime } from '../adapters/cloud/cloud-runtime';
import { TauriRuntime } from '../adapters/native/tauri-runtime';
import { WasmRuntime } from '../adapters/web/wasm-runtime';
import { RuntimeMode } from '../domain/runtime-mode';
import { RuntimePort } from '../ports/runtime-port';

export interface RuntimeFactoryInput {
  mode: RuntimeMode;
  apiUrl: string;
  wsUrl: string;
  sceneEngine: SceneEngine;
  slicerConnection: SlicerConnection;
  slicerFile: SlicerFile;
}

export function createRuntime(input: RuntimeFactoryInput): RuntimePort {
  switch (input.mode) {
    case 'native':
      return new TauriRuntime(input.sceneEngine);
    case 'cloud':
      return new CloudRuntime(
        input.apiUrl,
        input.slicerConnection,
        input.slicerFile,
        input.sceneEngine,
      );
    case 'web':
      return new WasmRuntime(input.sceneEngine);
    default:
      throw new Error('Unsupported runtime mode.');
  }
}
