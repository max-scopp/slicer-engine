/**
 * Incremental G-code parser.
 *
 * Designed to be fed arbitrary text chunks (as produced by streaming a network
 * response) and to emit toolpath segments on the fly without ever buffering
 * the entire file. Each call to {@link feed} returns the segments produced
 * since the previous call, so the renderer can append geometry progressively.
 *
 * This is intentionally a thin TypeScript implementation so the surrounding
 * pipeline (streaming → parser → typed buffers → Three.js) can be wired up
 * end-to-end. It is structured to be swapped for a Rust-compiled WASM parser
 * later: the input is bytes/text chunks and the output is plain typed-array
 * batches, which is exactly the contract WASM exports would expose.
 */

/** A single emitted toolpath segment. Coordinates are in millimetres. */
export interface GcodeSegment {
  readonly x0: number;
  readonly y0: number;
  readonly z0: number;
  readonly x1: number;
  readonly y1: number;
  readonly z1: number;
  /** Whether this segment extrudes material (true) or is a travel move (false). */
  readonly extruding: boolean;
}

/** Result of feeding one chunk into the parser. */
export interface GcodeFeedResult {
  /** Segment count produced from this chunk. */
  readonly count: number;
  /** Flat positions array: [x0,y0,z0, x1,y1,z1, ...] of length count*6. */
  readonly positions: Float32Array;
  /** Per-segment flag (1 byte): 1 = extruding, 0 = travel. Length = count. */
  readonly extruding: Uint8Array;
}

const EMPTY: GcodeFeedResult = Object.freeze({
  count: 0,
  positions: new Float32Array(0),
  extruding: new Uint8Array(0),
});

export class GcodeStreamParser {
  private readonly decoder = new TextDecoder('utf-8');
  private leftover = '';

  private absolutePositioning = true;
  private absoluteExtrusion = true;

  private x = 0;
  private y = 0;
  private z = 0;
  private e = 0;
  private hasPosition = false;

  /** Total segments emitted across all feeds. */
  totalSegments = 0;

  /** Feed a chunk of bytes. Pass `flush=true` for the final chunk. */
  feedBytes(chunk: Uint8Array, flush = false): GcodeFeedResult {
    const text = this.decoder.decode(chunk, { stream: !flush });
    return this.feed(text, flush);
  }

  /** Feed a chunk of text. Pass `flush=true` to process any final partial line. */
  feed(text: string, flush = false): GcodeFeedResult {
    if (text.length === 0 && !flush) {
      return EMPTY;
    }

    const combined = this.leftover + text;
    let lineStart = 0;
    let cursor = 0;

    // Estimate a reasonable initial capacity from chunk size.
    const estimatedLines = Math.max(16, Math.ceil(combined.length / 32));
    let positions = new Float32Array(estimatedLines * 6);
    let extruding = new Uint8Array(estimatedLines);
    let count = 0;

    const ensureCapacity = (needed: number): void => {
      if (needed <= extruding.length) {
        return;
      }
      let newCap = extruding.length || 16;
      while (newCap < needed) {
        newCap *= 2;
      }
      const np = new Float32Array(newCap * 6);
      np.set(positions);
      positions = np;
      const ne = new Uint8Array(newCap);
      ne.set(extruding);
      extruding = ne;
    };

    while (cursor < combined.length) {
      const ch = combined.charCodeAt(cursor);
      if (ch === 10 /* \n */ || ch === 13 /* \r */) {
        if (cursor > lineStart) {
          const seg = this.processLine(combined, lineStart, cursor);
          if (seg !== null) {
            ensureCapacity(count + 1);
            const o = count * 6;
            positions[o] = seg.x0;
            positions[o + 1] = seg.y0;
            positions[o + 2] = seg.z0;
            positions[o + 3] = seg.x1;
            positions[o + 4] = seg.y1;
            positions[o + 5] = seg.z1;
            extruding[count] = seg.extruding ? 1 : 0;
            count++;
          }
        }
        lineStart = cursor + 1;
      }
      cursor++;
    }

    if (flush && lineStart < combined.length) {
      const seg = this.processLine(combined, lineStart, combined.length);
      if (seg !== null) {
        ensureCapacity(count + 1);
        const o = count * 6;
        positions[o] = seg.x0;
        positions[o + 1] = seg.y0;
        positions[o + 2] = seg.z0;
        positions[o + 3] = seg.x1;
        positions[o + 4] = seg.y1;
        positions[o + 5] = seg.z1;
        extruding[count] = seg.extruding ? 1 : 0;
        count++;
      }
      this.leftover = '';
    } else {
      this.leftover = combined.slice(lineStart);
    }

    if (count === 0) {
      return EMPTY;
    }

    this.totalSegments += count;
    return {
      count,
      positions: positions.subarray(0, count * 6),
      extruding: extruding.subarray(0, count),
    };
  }

  /** Drop any retained state. */
  reset(): void {
    this.leftover = '';
    this.absolutePositioning = true;
    this.absoluteExtrusion = true;
    this.x = 0;
    this.y = 0;
    this.z = 0;
    this.e = 0;
    this.hasPosition = false;
    this.totalSegments = 0;
  }

  private processLine(src: string, start: number, end: number): GcodeSegment | null {
    // Strip inline comment.
    let lineEnd = end;
    for (let i = start; i < end; i++) {
      if (src.charCodeAt(i) === 59 /* ; */) {
        lineEnd = i;
        break;
      }
    }
    if (lineEnd <= start) {
      return null;
    }

    // First word — command code.
    let i = start;
    while (i < lineEnd && src.charCodeAt(i) <= 32) {
      i++;
    }
    if (i >= lineEnd) {
      return null;
    }
    const cmdLetter = src.charCodeAt(i);
    if (cmdLetter !== 71 /* G */ && cmdLetter !== 77 /* M */) {
      return null;
    }
    const cmdNumStart = i + 1;
    let j = cmdNumStart;
    while (j < lineEnd && isWordChar(src.charCodeAt(j))) {
      j++;
    }
    const cmdNum = parseInt(src.slice(cmdNumStart, j), 10);
    if (Number.isNaN(cmdNum)) {
      return null;
    }

    // Mode commands.
    if (cmdLetter === 71 /* G */) {
      if (cmdNum === 90) {
        this.absolutePositioning = true;
        return null;
      }
      if (cmdNum === 91) {
        this.absolutePositioning = false;
        return null;
      }
      if (cmdNum === 92) {
        this.applyG92(src, j, lineEnd);
        return null;
      }
      if (cmdNum !== 0 && cmdNum !== 1) {
        return null;
      }
    } else {
      // M-codes we care about.
      if (cmdNum === 82) {
        this.absoluteExtrusion = true;
      } else if (cmdNum === 83) {
        this.absoluteExtrusion = false;
      }
      return null;
    }

    // G0/G1 — parse parameters.
    let nx = this.x;
    let ny = this.y;
    let nz = this.z;
    let ne = this.e;
    let sawXY = false;
    let sawE = false;

    let k = j;
    while (k < lineEnd) {
      const c = src.charCodeAt(k);
      if (c <= 32) {
        k++;
        continue;
      }
      const valStart = k + 1;
      let valEnd = valStart;
      while (valEnd < lineEnd && isNumChar(src.charCodeAt(valEnd))) {
        valEnd++;
      }
      if (valEnd === valStart) {
        k = valEnd;
        continue;
      }
      const value = parseFloat(src.slice(valStart, valEnd));
      if (!Number.isNaN(value)) {
        switch (c) {
          case 88: // X
            nx = this.absolutePositioning ? value : this.x + value;
            sawXY = true;
            break;
          case 89: // Y
            ny = this.absolutePositioning ? value : this.y + value;
            sawXY = true;
            break;
          case 90: // Z
            nz = this.absolutePositioning ? value : this.z + value;
            break;
          case 69: // E
            ne = this.absoluteExtrusion ? value : this.e + value;
            sawE = true;
            break;
          default:
            break;
        }
      }
      k = valEnd;
    }

    const prevX = this.x;
    const prevY = this.y;
    const prevZ = this.z;
    const prevE = this.e;
    const hadPosition = this.hasPosition;

    this.x = nx;
    this.y = ny;
    this.z = nz;
    this.e = ne;
    this.hasPosition = true;

    if (!hadPosition) {
      return null;
    }
    if (prevX === nx && prevY === ny && prevZ === nz) {
      return null;
    }

    const extruding = sawE && ne > prevE && (sawXY || prevZ !== nz);

    return {
      x0: prevX,
      y0: prevY,
      z0: prevZ,
      x1: nx,
      y1: ny,
      z1: nz,
      extruding,
    };
  }

  private applyG92(src: string, from: number, to: number): void {
    let k = from;
    while (k < to) {
      const c = src.charCodeAt(k);
      if (c <= 32) {
        k++;
        continue;
      }
      const valStart = k + 1;
      let valEnd = valStart;
      while (valEnd < to && isNumChar(src.charCodeAt(valEnd))) {
        valEnd++;
      }
      const value = parseFloat(src.slice(valStart, valEnd));
      if (!Number.isNaN(value)) {
        switch (c) {
          case 88:
            this.x = value;
            break;
          case 89:
            this.y = value;
            break;
          case 90:
            this.z = value;
            break;
          case 69:
            this.e = value;
            break;
          default:
            break;
        }
      }
      k = valEnd === valStart ? valEnd + 1 : valEnd;
    }
  }
}

function isWordChar(code: number): boolean {
  // digits, '.', '-', '+'
  return (code >= 48 && code <= 57) || code === 46 || code === 45 || code === 43;
}

function isNumChar(code: number): boolean {
  return (
    (code >= 48 && code <= 57) ||
    code === 46 ||
    code === 45 ||
    code === 43 ||
    code === 101 ||
    code === 69 // e/E for scientific notation
  );
}
