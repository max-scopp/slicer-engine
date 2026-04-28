import { ChunkedLineGeometry } from './chunked-line-geometry';
import { GcodeStreamParser } from './gcode-stream-parser';

export type GcodeSource = string | URL | ReadableStream<Uint8Array> | Response;

export interface GcodeLoadOptions {
  /** Called periodically with the running segment count for progress UI. */
  readonly onProgress?: (totalSegments: number) => void;
  /** Called once after the first batch of geometry is appended. */
  readonly onFirstGeometry?: () => void;
  /** Called when the stream completes successfully. */
  readonly onComplete?: (totalSegments: number) => void;
  /** AbortSignal to cancel mid-stream (e.g. mode change, navigation). */
  readonly signal?: AbortSignal;
}

/**
 * Stream a G-code source through the incremental parser and into the
 * chunked line geometry. Never buffers the entire file: each network chunk
 * is parsed and its segments handed straight to the renderer.
 */
export async function loadGcode(
  source: GcodeSource,
  geometry: ChunkedLineGeometry,
  options: GcodeLoadOptions = {},
): Promise<void> {
  const stream = await resolveStream(source, options.signal);
  const reader = stream.getReader();
  const parser = new GcodeStreamParser();

  let firstEmitted = false;
  try {
    while (true) {
      if (options.signal?.aborted) {
        throw abortError();
      }
      const { value, done } = await reader.read();
      if (done) {
        const tail = parser.feed('', true);
        if (tail.count > 0) {
          geometry.append(tail.positions, tail.extruding, tail.count);
        }
        options.onComplete?.(parser.totalSegments);
        return;
      }
      const result = parser.feedBytes(value, false);
      if (result.count > 0) {
        geometry.append(result.positions, result.extruding, result.count);
        if (!firstEmitted) {
          firstEmitted = true;
          options.onFirstGeometry?.();
        }
        options.onProgress?.(parser.totalSegments);
      }
    }
  } finally {
    reader.releaseLock();
  }
}

async function resolveStream(
  source: GcodeSource,
  signal?: AbortSignal,
): Promise<ReadableStream<Uint8Array>> {
  if (source instanceof ReadableStream) {
    return source;
  }
  const response = source instanceof Response ? source : await fetch(toUrl(source), { signal });
  if (!response.ok) {
    const detail = await readErrorBody(response);
    const suffix = detail ? ` — ${detail}` : '';
    throw new Error(`${response.status} ${response.statusText}${suffix}`);
  }
  if (!response.body) {
    throw new Error('G-code response has no readable body.');
  }
  return response.body;
}

/**
 * Try to extract a useful human-readable message from an error response body.
 * Handles both plain-text bodies (actix `ErrorNotFound("...")`) and JSON bodies
 * shaped like `{ "error": "..." }`. Returns an empty string on failure so the
 * caller can fall back to the HTTP status line alone.
 */
async function readErrorBody(response: Response): Promise<string> {
  try {
    const text = (await response.text()).trim();
    if (!text) {
      return '';
    }
    if (text.startsWith('{')) {
      try {
        const parsed = JSON.parse(text) as { error?: unknown; message?: unknown };
        const message = parsed.error ?? parsed.message;
        if (typeof message === 'string' && message.length > 0) {
          return message;
        }
      } catch {
        // fall through to raw text
      }
    }
    // Cap very long bodies (HTML error pages, stack traces, …) to keep the
    // overlay readable.
    return text.length > 240 ? `${text.slice(0, 240)}…` : text;
  } catch {
    return '';
  }
}

function toUrl(source: string | URL): string {
  return source instanceof URL ? source.toString() : source;
}

function abortError(): DOMException {
  return new DOMException('G-code load aborted', 'AbortError');
}
