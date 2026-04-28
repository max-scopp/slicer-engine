import { BufferAttribute, BufferGeometry, LineBasicMaterial, LineSegments, Object3D } from 'three';

const TRAVEL_COLOR = 0x3a4a55;
const PRINT_COLOR = 0x4ec9b0;

/**
 * Chunked line-segment renderer for G-code toolpaths.
 *
 * Holds two {@link LineSegments} objects (one for printing moves, one for
 * travel moves) backed by growing {@link BufferGeometry} position attributes.
 * New segments produced by the streaming parser are appended in place — the
 * underlying typed array is only reallocated when capacity is exhausted, and
 * existing draw calls are never rebuilt.
 *
 * This is the renderer-side half of the streaming pipeline:
 *
 *     stream → WASM/TS parser → typed buffers → ChunkedLineGeometry
 */
export class ChunkedLineGeometry {
  readonly root = new Object3D();

  private readonly print: GrowingLineSegments;
  private readonly travel: GrowingLineSegments;

  constructor(showTravel = false) {
    this.print = new GrowingLineSegments(PRINT_COLOR, 1);
    this.travel = new GrowingLineSegments(TRAVEL_COLOR, 0.5);
    this.travel.mesh.visible = showTravel;
    this.root.add(this.print.mesh);
    this.root.add(this.travel.mesh);
  }

  /**
   * Append a batch of segments emitted by the parser.
   * `positions` is a flat [x0,y0,z0,x1,y1,z1,...] array of length count*6.
   * `extrudingFlags[i]` indicates whether segment i extrudes material.
   */
  append(positions: Float32Array, extrudingFlags: Uint8Array, count: number): void {
    if (count === 0) {
      return;
    }
    // Partition into print vs travel by walking the flag array.
    // For each contiguous run of equal flags, copy the positions in one go.
    let runStart = 0;
    while (runStart < count) {
      const flag = extrudingFlags[runStart];
      let runEnd = runStart + 1;
      while (runEnd < count && extrudingFlags[runEnd] === flag) {
        runEnd++;
      }
      const slice = positions.subarray(runStart * 6, runEnd * 6);
      if (flag === 1) {
        this.print.appendPositions(slice);
      } else {
        this.travel.appendPositions(slice);
      }
      runStart = runEnd;
    }
  }

  setTravelVisible(visible: boolean): void {
    this.travel.mesh.visible = visible;
  }

  dispose(): void {
    this.print.dispose();
    this.travel.dispose();
  }
}

class GrowingLineSegments {
  readonly mesh: LineSegments;
  private geometry: BufferGeometry;
  private readonly material: LineBasicMaterial;
  private positions: Float32Array;
  private vertexCount = 0; // total vertices written (always even)

  constructor(color: number, opacity: number) {
    this.material = new LineBasicMaterial({
      color,
      transparent: opacity < 1,
      opacity,
    });
    this.geometry = new BufferGeometry();
    this.positions = new Float32Array(INITIAL_CAPACITY * 3);
    const attr = new BufferAttribute(this.positions, 3);
    attr.setUsage(35048 /* THREE.DynamicDrawUsage */);
    this.geometry.setAttribute('position', attr);
    this.geometry.setDrawRange(0, 0);
    this.mesh = new LineSegments(this.geometry, this.material);
    this.mesh.frustumCulled = false; // bounds change every frame during streaming
  }

  appendPositions(slice: Float32Array): void {
    const newVertices = slice.length / 3;
    if (newVertices === 0) {
      return;
    }
    const required = (this.vertexCount + newVertices) * 3;
    if (required > this.positions.length) {
      this.grow(required);
    }
    this.positions.set(slice, this.vertexCount * 3);
    const attr = this.geometry.getAttribute('position') as BufferAttribute;
    attr.addUpdateRange(this.vertexCount * 3, slice.length);
    attr.needsUpdate = true;
    this.vertexCount += newVertices;
    this.geometry.setDrawRange(0, this.vertexCount);
  }

  dispose(): void {
    this.geometry.dispose();
    this.material.dispose();
  }

  private grow(minCapacityFloats: number): void {
    let newLen = this.positions.length || 3;
    while (newLen < minCapacityFloats) {
      newLen *= 2;
    }
    const np = new Float32Array(newLen);
    np.set(this.positions.subarray(0, this.vertexCount * 3));
    this.positions = np;

    // Replace attribute (typed array changed) and restore draw range.
    const attr = new BufferAttribute(this.positions, 3);
    attr.setUsage(35048);
    this.geometry.setAttribute('position', attr);
    this.geometry.setDrawRange(0, this.vertexCount);
    attr.needsUpdate = true;
  }
}

const INITIAL_CAPACITY = 4096; // segments (each segment = 2 vertices)
