import { Box3, BufferGeometry, Mesh, MeshPhongMaterial, Object3D, Vector3 } from 'three';
import { ThreeMFLoader } from 'three/examples/jsm/loaders/3MFLoader.js';
import { OBJLoader } from 'three/examples/jsm/loaders/OBJLoader.js';
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js';
import { unzipSync } from 'three/examples/jsm/libs/fflate.module.js';

/** Input accepted by the model loader. */
export type ModelSource = string | URL | File | Blob | ArrayBuffer;

/** Supported model file formats. */
export type ModelFormat = 'stl' | 'obj' | '3mf';

const SUPPORTED_FORMATS: readonly ModelFormat[] = ['stl', 'obj', '3mf'] as const;

/**
 * Conversion factors from a 3MF `<model unit="...">` value to millimeters.
 * Per 3MF Core spec §3.2.1. `ThreeMFLoader` parses the attribute but does
 * not apply any scaling, so we have to honor it ourselves to keep the
 * scene's mm coordinate system consistent.
 */
const MM_PER_3MF_UNIT: Record<string, number> = {
  micron: 0.001,
  millimeter: 1,
  centimeter: 10,
  inch: 25.4,
  foot: 304.8,
  meter: 1000,
};

/**
 * Load a model source into a renderable {@link Object3D}.
 *
 * Supports STL (binary or ASCII), Wavefront OBJ and 3MF. The format is
 * detected from the file name / URL extension, falling back to magic-byte
 * sniffing for raw `ArrayBuffer`/`Blob` inputs. The returned object owns its
 * geometry and material; dispose by traversing.
 */
export async function loadModel(source: ModelSource): Promise<Object3D> {
  const { buffer, hint } = await readSource(source);
  const format = detectFormat(buffer, hint);
  const object = parseByFormat(buffer, format);
  applyDefaultMaterial(object);
  return object;
}

function parseByFormat(buffer: ArrayBuffer, format: ModelFormat): Object3D {
  switch (format) {
    case 'stl': {
      const geometry = new STLLoader().parse(buffer);
      return buildMesh(geometry);
    }
    case 'obj': {
      const text = new TextDecoder().decode(buffer);
      // OBJ does not encode an up-axis. Different DCC tools export with
      // different conventions (Blender/Maya: Y-up; 3D-printing tools often
      // Z-up). Auto-rotating would fix one file and break another, so we
      // import verbatim and let the user re-orient on the bed if needed —
      // matching Cura / PrusaSlicer / Bambu behavior.
      const object = new OBJLoader().parse(text);
      // OBJ also has no unit metadata. Blender's default OBJ exporter
      // writes in meters, so a 100mm part loads as 0.1 — visually
      // microscopic. If the largest dimension is sub-millimeter, assume
      // the file is in meters and scale to mm. (Bambu Studio applies the
      // same heuristic.) The 1mm threshold is intentionally tight: real
      // printable parts are never that small, and rescaling already-mm
      // jewelry-scale geometry would do more harm than good.
      autoScaleSubMillimeterToMm(object);
      return object;
    }
    case '3mf': {
      const object = new ThreeMFLoader().parse(buffer);
      const unit = read3mfUnit(buffer);
      const scale = MM_PER_3MF_UNIT[unit];
      if (scale && scale !== 1) {
        object.scale.setScalar(scale);
        object.updateMatrix();
      }
      return object;
    }
  }
}

function buildMesh(geometry: BufferGeometry): Mesh {
  if (!geometry.getAttribute('normal')) {
    geometry.computeVertexNormals();
  }
  geometry.computeBoundingBox();
  return new Mesh(geometry, defaultMaterial());
}

/**
 * If the loaded object's largest axis-aligned dimension is implausibly small
 * for a printable part, scale it up by 1000× (meters → mm). Used for OBJ
 * files, which carry no unit metadata and are commonly exported in meters
 * by Blender et al.
 *
 * The 10mm threshold is a heuristic: real printable parts on a typical
 * (≥100mm) bed are almost never that small, while Blender's default OBJ
 * export of a 100mm part comes out as raw value 0.1 (and a 1mm part as
 * 0.001). Files exported in centimeters fall in an ambiguous zone and may
 * still appear undersized — there's no way to distinguish unit-less raw
 * values without metadata.
 */
function autoScaleSubMillimeterToMm(root: Object3D): void {
  const box = new Box3().setFromObject(root);
  if (box.isEmpty()) {
    return;
  }
  const size = box.getSize(new Vector3());
  const maxDim = Math.max(size.x, size.y, size.z);
  if (maxDim > 0 && maxDim < 10) {
    root.scale.multiplyScalar(1000);
    root.updateMatrix();
  }
}

/**
 * Walk the loaded scene and ensure every {@link Mesh} has our default
 * material and proper normals. OBJ/3MF loaders return a `Group` whose
 * meshes carry whatever material the file referenced; we override for
 * visual consistency with STL.
 */
function applyDefaultMaterial(root: Object3D): void {
  const material = defaultMaterial();
  root.traverse((node) => {
    if (!(node instanceof Mesh)) {
      return;
    }
    const geometry = node.geometry as BufferGeometry | undefined;
    if (geometry) {
      if (!geometry.getAttribute('normal')) {
        geometry.computeVertexNormals();
      }
      geometry.computeBoundingBox();
    }
    node.material = material;
  });
}

function defaultMaterial(): MeshPhongMaterial {
  // Phong (Blinn-Phong) is meaningfully cheaper than MeshStandardMaterial on
  // tile-based mobile GPUs (iOS / Android), and with `flatShading: true` the
  // visual result is indistinguishable for our use case (no metalness / IBL
  // contribution). Keeping a small specular highlight gives the part some
  // form definition without the full PBR shader cost.
  return new MeshPhongMaterial({
    color: 0xb0b6bb,
    specular: 0x111111,
    shininess: 10,
    flatShading: true,
  });
}

interface SourceData {
  buffer: ArrayBuffer;
  /** Filename or URL used to derive the extension hint, if available. */
  hint?: string;
}

async function readSource(source: ModelSource): Promise<SourceData> {
  if (source instanceof ArrayBuffer) {
    return { buffer: source };
  }
  if (typeof File !== 'undefined' && source instanceof File) {
    return { buffer: await source.arrayBuffer(), hint: source.name };
  }
  if (source instanceof Blob) {
    return { buffer: await source.arrayBuffer() };
  }
  const url = source instanceof URL ? source.toString() : source;
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to load model: ${response.status} ${response.statusText}`);
  }
  return { buffer: await response.arrayBuffer(), hint: url };
}

function detectFormat(buffer: ArrayBuffer, hint?: string): ModelFormat {
  const fromHint = formatFromHint(hint);
  if (fromHint) {
    return fromHint;
  }
  const sniffed = sniffFormat(buffer);
  if (sniffed) {
    return sniffed;
  }
  throw new Error(
    `Unsupported model format. Supported: ${SUPPORTED_FORMATS.join(', ').toUpperCase()}.`,
  );
}

function formatFromHint(hint: string | undefined): ModelFormat | null {
  if (!hint) {
    return null;
  }
  // Strip query / fragment then take the last extension.
  const clean = hint.split(/[?#]/, 1)[0];
  const dot = clean.lastIndexOf('.');
  if (dot < 0) {
    return null;
  }
  const ext = clean.slice(dot + 1).toLowerCase();
  return SUPPORTED_FORMATS.includes(ext as ModelFormat) ? (ext as ModelFormat) : null;
}

function sniffFormat(buffer: ArrayBuffer): ModelFormat | null {
  if (buffer.byteLength < 4) {
    return null;
  }
  const head = new Uint8Array(buffer, 0, Math.min(buffer.byteLength, 256));
  // 3MF is a ZIP archive — magic bytes "PK\x03\x04".
  if (head[0] === 0x50 && head[1] === 0x4b && head[2] === 0x03 && head[3] === 0x04) {
    return '3mf';
  }
  // ASCII STL files start with "solid".
  const text = new TextDecoder('utf-8', { fatal: false }).decode(head);
  const trimmed = text.trimStart().toLowerCase();
  if (trimmed.startsWith('solid')) {
    return 'stl';
  }
  // OBJ files commonly start with comments or vertex/face directives.
  if (/^(#|v\s|vn\s|vt\s|f\s|o\s|g\s|mtllib\s|usemtl\s)/m.test(text)) {
    return 'obj';
  }
  // Fallback: assume binary STL.
  return 'stl';
}

/**
 * Extract the `unit` attribute from the 3MF model XML inside the archive.
 * Defaults to `"millimeter"` if absent or unreadable — matches the 3MF spec
 * default and `ThreeMFLoader`'s own behavior.
 */
function read3mfUnit(buffer: ArrayBuffer): string {
  try {
    const entries = unzipSync(new Uint8Array(buffer), {
      filter: (file) => file.name.endsWith('.model'),
    });
    const modelEntry = Object.values(entries)[0];
    if (!modelEntry) {
      return 'millimeter';
    }
    // Only the opening <model …> tag is needed; decode a small prefix.
    const prefix = new TextDecoder('utf-8', { fatal: false }).decode(
      modelEntry.subarray(0, Math.min(modelEntry.length, 2048)),
    );
    const match = /<model\b[^>]*\bunit\s*=\s*"([^"]+)"/i.exec(prefix);
    return match ? match[1].toLowerCase() : 'millimeter';
  } catch {
    return 'millimeter';
  }
}
