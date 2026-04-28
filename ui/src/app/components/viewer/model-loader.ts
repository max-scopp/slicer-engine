import { BufferGeometry, Mesh, MeshStandardMaterial, Object3D } from 'three';
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js';

/** Input accepted by the model loader. */
export type ModelSource = string | URL | File | Blob | ArrayBuffer;

/**
 * Load a model source into a renderable {@link Object3D}.
 *
 * Currently supports STL (binary or ASCII) via Three.js's `STLLoader`.
 * The returned object owns its geometry and material; dispose by traversing.
 */
export async function loadModel(source: ModelSource): Promise<Object3D> {
  const buffer = await readAsArrayBuffer(source);
  const loader = new STLLoader();
  const geometry = loader.parse(buffer);
  return buildMesh(geometry);
}

function buildMesh(geometry: BufferGeometry): Mesh {
  geometry.computeVertexNormals();
  geometry.computeBoundingBox();
  const material = new MeshStandardMaterial({
    color: 0xb0b6bb,
    metalness: 0.05,
    roughness: 0.65,
    flatShading: true,
  });
  const mesh = new Mesh(geometry, material);
  return mesh;
}

async function readAsArrayBuffer(source: ModelSource): Promise<ArrayBuffer> {
  if (source instanceof ArrayBuffer) {
    return source;
  }
  if (source instanceof Blob) {
    return await source.arrayBuffer();
  }
  const url = source instanceof URL ? source.toString() : source;
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to load model: ${response.status} ${response.statusText}`);
  }
  return await response.arrayBuffer();
}
