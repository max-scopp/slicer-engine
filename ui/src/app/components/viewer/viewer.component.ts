import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  afterNextRender,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';
import { BufferAttribute, BufferGeometry, Matrix4, Mesh, MeshPhongMaterial } from 'three';
import { ObjectTracker } from '../../services/object-tracker';
import { PrintArea } from '../../services/print-area';
import { SceneEngineService } from '../../services/scene-engine.service';
import { ViewerControl } from '../../services/viewer-control';
import { ChunkedLineGeometry } from './chunked-line-geometry';
import { GcodeSource, loadGcode } from './gcode-loader';
import { ModelSource } from './model-loader';
import { ViewerScene } from './scene';

export type ViewerMode = 'model' | 'gcode';

/**
 * Single-component 3D viewer for both raw meshes and sliced G-code.
 *
 * The viewer is the only entry point for visualization. It owns the
 * Three.js scene, switches between the two render modes without
 * re-initializing WebGL, and drives the streaming G-code pipeline.
 *
 * Usage:
 * ```html
 * <nexus-viewer [model]="stlFileOrUrl" mode="model"></nexus-viewer>
 * <nexus-viewer [gcodeSource]="gcodeUrl" mode="gcode"></nexus-viewer>
 * ```
 */
@Component({
  selector: 'nexus-viewer',
  standalone: true,
  template: `
    <div class="viewer-host" #host></div>
    <div class="viewer-bottom-left">
      @if (fps() > 0) {
        <div class="viewer-fps">{{ fps() }} FPS</div>
      }
      @if (wasmRoundtripMs() !== null) {
        <div class="viewer-fps">
          WASM ↻ {{ wasmRoundtripMs()!.toFixed(1) }} ms
          <span class="viewer-fps-aux"
            >(parse {{ wasmParseMs()!.toFixed(1) }} + render-buf
            {{ wasmRenderBufMs()!.toFixed(1) }} ms)</span
          >
          @if (opStats(); as s) {
            <span class="viewer-fps-aux"
              >· {{ s.label }} {{ s.lastMs.toFixed(2) }} ms (avg {{ s.avgMs.toFixed(2) }} ms /
              {{ s.count }})</span
            >
          }
        </div>
      }
      @if (status() !== 'idle') {
        <div class="viewer-status">{{ statusLabel() }}</div>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        position: relative;
        width: 100%;
        height: 100%;
        background: transparent;
        overflow: hidden;
        user-select: none;
      }
      .viewer-host {
        position: absolute;
        inset: 0;
      }
      .viewer-bottom-left {
        position: absolute;
        bottom: 12px;
        left: 12px;
        display: flex;
        flex-direction: column;
        gap: 4px;
        pointer-events: none;
      }
      .viewer-fps,
      .viewer-status {
        padding: 6px 10px;
        background: rgba(0, 0, 0, 0.55);
        color: #e6e6e6;
        font:
          12px/1.2 ui-monospace,
          monospace;
        border-radius: 4px;
      }
      .viewer-fps-aux {
        color: #9aa4b2;
        margin-left: 6px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ViewerComponent implements OnDestroy {
  readonly mode = input<ViewerMode>('model');
  readonly model = input<ModelSource | null>(null);
  readonly gcodeSource = input<GcodeSource | null>(null);
  readonly showTravel = input(false);

  readonly loadComplete = output<{ mode: ViewerMode; segments: number }>();
  readonly loadError = output<{ mode: ViewerMode; error: unknown }>();

  private readonly hostRef = viewChild.required<ElementRef<HTMLElement>>('host');
  private readonly elementRef = inject(ElementRef);
  private readonly viewerControl = inject(ViewerControl);
  private readonly printArea = inject(PrintArea);
  private readonly objectTracker = inject(ObjectTracker);
  private readonly sceneEngine = inject(SceneEngineService);

  /** Current loading status for the optional overlay. */
  readonly status = signal<'idle' | 'loading' | 'streaming' | 'ready' | 'error'>('idle');
  /** Smoothed frames-per-second reported by the render loop. */
  readonly fps = signal(0);
  /** Smoothed average frame delay in milliseconds. */
  readonly frameDelayMs = signal(0);
  /**
   * End-to-end wall time of the last WASM mesh round-trip, measured from
   * the moment the bytes are handed to `addMesh` to the moment the
   * `RenderBuffer` is fully copied out of WASM memory. `null` until the
   * first model has been loaded through the engine.
   */
  readonly wasmRoundtripMs = signal<number | null>(null);
  /** WASM-side parse time (`addMesh`) of the last model load. */
  readonly wasmParseMs = signal<number | null>(null);
  /** WASM-side render-buffer extraction time (`getRenderBuffer`) of the last load. */
  readonly wasmRenderBufMs = signal<number | null>(null);
  /** Last-op rolling stats from the scene engine, surfaced in the overlay. */
  readonly opStats = computed(() => this.sceneEngine.opStats());
  private readonly progressSegments = signal(0);
  private readonly errorMessage = signal<string>('');

  private scene: ViewerScene | null = null;
  private gcodeGeometry: ChunkedLineGeometry | null = null;
  private currentAbort: AbortController | null = null;
  private loadToken = 0;
  /** SceneObject ids registered for the currently-loaded source. */
  private trackedObjectIds: string[] = [];
  /** Live mapping from WASM scene id to the Three.js mesh that mirrors it. */
  private readonly wasmMeshes = new Map<bigint, Mesh>();
  private readonly tmpMatrix = new Matrix4();
  /**
   * Currently selected WASM object ids (as bigint), kept in sync with the
   * legacy scene's string-id selection set so highlight + drag work.
   */
  private selectedWasmIds: bigint[] = [];
  /**
   * Per-axis displacement (mm) already pushed to the engine for the
   * in-flight drag, indexed by WASM id. Used to convert the cumulative
   * `(dx, dy)` reported by the scene into per-frame deltas suitable for
   * `SceneOp::Translate`.
   */
  private dragApplied = new Map<bigint, { dx: number; dy: number }>();

  constructor() {
    afterNextRender(() => this.initScene());

    // React to input changes — single effect handles mode + source switching.
    effect(() => {
      const mode = this.mode();
      const model = this.model();
      const gcode = this.gcodeSource();
      const showTravel = this.showTravel();

      if (!this.scene) {
        return;
      }
      this.gcodeGeometry?.setTravelVisible(showTravel);
      this.applySource(mode, model, gcode);
    });

    // React to view-preset changes from the toolbar.
    effect(() => {
      const view = this.viewerControl.view();
      this.scene?.setView(view);
    });

    // React to cursor-mode changes from the toolbar.
    effect(() => {
      const mode = this.viewerControl.cursorMode();
      this.scene?.setCursorMode(mode);
    });

    // React to reset requests from the toolbar.
    effect(() => {
      const tick = this.viewerControl.resetTick();
      // Skip the very first emission so we don't redundantly reset on init.
      if (tick === 0) {
        return;
      }
      this.scene?.resetView();
    });

    // React to direction-look requests (e.g. from the viewport-cube gizmo).
    effect(() => {
      const req = this.viewerControl.lookRequest();
      if (!req) {
        return;
      }
      this.scene?.animateToDirection(req.direction, req.up);
    });

    // Mirror the print-area configuration into the scene so the bed grid
    // tracks any settings/UI changes (dimensions or movable-area offset).
    effect(() => {
      const config = this.printArea.config();
      this.scene?.setPrintArea(config);
    });

    // Mirror the application's selection state into the scene so meshes get
    // their highlight as soon as the service signal flips (whether the flip
    // came from a viewer click or from external UI).
    effect(() => {
      const ids = this.printArea.selectedIds();
      this.scene?.setSelectedIds(ids);
    });

    // Mirror tracked-object transforms onto the corresponding mesh nodes.
    // Reading every SceneObject's `transform` signal in this effect makes
    // it depend on each object's position/rotation/scale, so any update
    // (manual API call, drag, future gizmo) re-runs and pushes through.
    //
    // DISABLED during migration to the WASM scene engine. Object transforms
    // now flow through `wasmMeshes` (see effect below); the legacy tracker
    // path is kept dormant for the eventual selection / gizmo work.
    // effect(() => {
    //   const objects = this.objectTracker.objects();
    //   const scene = this.scene;
    //   if (!scene) {
    //     return;
    //   }
    //   for (const obj of objects) {
    //     scene.setObjectTransform(obj.id, obj.transform());
    //   }
    // });

    // Push the WASM scene-engine transform onto each mirrored Three.js mesh
    // every time the snapshot changes. This is the equivalent of the legacy
    // ObjectTracker mirror above — only the source of truth has moved into
    // Rust. Matrices are read column-major (matches glam) and applied with
    // `matrixAutoUpdate = false` so Three.js does not overwrite them.
    effect(() => {
      const objects = this.sceneEngine.objects();
      if (this.wasmMeshes.size === 0) {
        return;
      }
      for (const obj of objects) {
        const mesh = this.wasmMeshes.get(obj.id);
        if (!mesh) {
          continue;
        }
        const m = this.sceneEngine.getMatrix(obj.id);
        this.tmpMatrix.fromArray(m);
        mesh.matrix.copy(this.tmpMatrix);
        mesh.matrixWorldNeedsUpdate = true;
      }
    });
  }

  // ---------------------------------------------------------------------------
  // Selection / drag handlers — XY-only translate via the WASM scene engine.
  //
  // The legacy `ViewerScene` raycast / pointer plumbing already calls these
  // four hooks, identifying objects by their string id (which we set to
  // `String(wasmId)` in `loadModelViaSceneEngine`). All we have to do here
  // is round-trip selection state and dispatch `Translate` ops with the
  // delta-since-last-move on the bed plane (Z fixed at 0).
  // ---------------------------------------------------------------------------

  private handleSelect(stringId: string, additive: boolean): void {
    const id = parseWasmId(stringId);
    if (id === null) {
      return;
    }
    if (additive) {
      if (!this.selectedWasmIds.includes(id)) {
        this.selectedWasmIds = [...this.selectedWasmIds, id];
      }
    } else {
      this.selectedWasmIds = [id];
    }
    this.scene?.setSelectedIds(new Set(this.selectedWasmIds.map(String)));
  }

  private handleClearSelection(): void {
    this.selectedWasmIds = [];
    this.scene?.setSelectedIds(new Set());
  }

  private handleBeginDrag(): boolean {
    if (this.selectedWasmIds.length === 0) {
      return false;
    }
    this.dragApplied.clear();
    for (const id of this.selectedWasmIds) {
      this.dragApplied.set(id, { dx: 0, dy: 0 });
    }
    return true;
  }

  /**
   * `dx`/`dy` are the **cumulative** displacement in mm (bed plane) since
   * drag start. Translate by the per-step delta so the engine state mirrors
   * exactly what the user has dragged.
   */
  private handleDragBy(dx: number, dy: number): void {
    for (const id of this.selectedWasmIds) {
      const applied = this.dragApplied.get(id);
      if (!applied) {
        continue;
      }
      const stepX = dx - applied.dx;
      const stepY = dy - applied.dy;
      if (stepX === 0 && stepY === 0) {
        continue;
      }
      this.sceneEngine.apply({
        op: 'translate',
        args: { id, delta: [stepX, stepY, 0] },
      });
      applied.dx = dx;
      applied.dy = dy;
    }
  }

  private handleEndDrag(): void {
    this.dragApplied.clear();
  }

  ngOnDestroy(): void {
    this.cancelInFlightLoad();
    this.gcodeGeometry?.dispose();
    this.gcodeGeometry = null;
    this.scene?.dispose();
    this.scene = null;
    this.viewerControl.orbitSink = null;
  }

  statusLabel(): string {
    switch (this.status()) {
      case 'loading':
        return 'Loading…';
      case 'streaming':
        return `Streaming… ${this.progressSegments().toLocaleString()} segments`;
      case 'ready':
        return `Ready — ${this.progressSegments().toLocaleString()} segments`;
      case 'error': {
        const detail = this.errorMessage();
        return detail ? `Failed to load — ${detail}` : 'Failed to load';
      }
      default:
        return '';
    }
  }

  private initScene(): void {
    const host = this.hostRef().nativeElement;
    this.scene = new ViewerScene(host, this.printArea.config());
    // Mirror the live camera direction/up into ViewerControl so external
    // overlays (the viewport-cube gizmo) can read it without going through
    // Angular's change-detection.
    const state = this.viewerControl.cameraState;
    this.scene.cameraStateSink = (dir, up, fov) => {
      state.direction.copy(dir);
      state.up.copy(up);
      state.fov = fov;
    };
    this.scene.fpsSink = (fps, delayMs) => {
      this.fps.set(fps);
      this.frameDelayMs.set(delayMs);
    };
    // Allow external gizmos (viewport-cube drag) to orbit the main camera.
    this.viewerControl.orbitSink = (azimuth, polar) => this.scene?.orbitBy(azimuth, polar);
    // Bridge raycast hits / drag gestures from the scene into the WASM
    // scene engine. Selection is stored locally (no PrintArea / tracker yet)
    // and drag deltas are pushed as `Translate` ops constrained to XY.
    this.scene.selectionHandlers = {
      select: (id, additive) => this.handleSelect(id, additive),
      clearSelection: () => this.handleClearSelection(),
      beginDragSelected: () => this.handleBeginDrag(),
      dragSelectedBy: (dx, dy) => this.handleDragBy(dx, dy),
      endDrag: () => this.handleEndDrag(),
      cancelDrag: () => this.handleEndDrag(),
    };
    // Apply the current toolbar selections so the scene starts in sync with
    // whatever view / cursor mode the user already had selected.
    this.scene.setCursorMode(this.viewerControl.cursorMode());
    this.scene.setView(this.viewerControl.view());
    // Seed the bed grid from the current print-area configuration.
    this.scene.setPrintArea(this.printArea.config());
    // Trigger initial source application now that the scene exists.
    this.applySource(this.mode(), this.model(), this.gcodeSource());
  }

  private applySource(
    mode: ViewerMode,
    model: ModelSource | null,
    gcode: GcodeSource | null,
  ): void {
    const scene = this.scene;
    if (!scene) {
      return;
    }
    this.cancelInFlightLoad();
    // Drop any tracked SceneObjects we registered for the previous source
    // so the tracker / selection / drag stores stay a faithful mirror of
    // what is actually on the bed.
    for (const id of this.trackedObjectIds) {
      this.printArea.forgetObject(id);
      this.objectTracker.remove(id);
    }
    this.trackedObjectIds = [];
    // Drop any WASM-scene meshes from the previous source. The scene engine
    // is the source of truth, so we also fire `Remove` ops to free the
    // backing Rust state — otherwise ids would accumulate across reloads.
    for (const id of this.wasmMeshes.keys()) {
      this.scene?.unregisterSelectable(String(id));
      try {
        this.sceneEngine.apply({ op: 'remove', args: { id } });
      } catch {
        // Object may already be gone if the engine reset; safe to ignore.
      }
    }
    this.wasmMeshes.clear();
    this.selectedWasmIds = [];
    this.dragApplied.clear();
    scene.clearContent();
    this.gcodeGeometry?.dispose();
    this.gcodeGeometry = null;
    this.progressSegments.set(0);
    this.errorMessage.set('');

    if (mode === 'model') {
      if (!model) {
        this.status.set('idle');
        return;
      }
      this.startModelLoad(model);
    } else {
      if (!gcode) {
        this.status.set('idle');
        return;
      }
      this.startGcodeLoad(gcode);
    }
  }

  private startModelLoad(source: ModelSource): void {
    const scene = this.scene;
    if (!scene) {
      return;
    }
    const token = ++this.loadToken;
    this.status.set('loading');

    // New WASM-scene-engine path: fetch raw bytes, parse them inside
    // Rust, then build a BufferGeometry from the WASM-emitted render
    // buffer. The scene-engine owns the mesh data and the transform; the
    // Three.js node is a thin display mirror with `matrixAutoUpdate = false`.
    void this.loadModelViaSceneEngine(source, token).catch((error: unknown) => {
      if (token !== this.loadToken) {
        return;
      }
      this.errorMessage.set(messageOf(error));
      this.status.set('error');
      this.loadError.emit({ mode: 'model', error });
    });
  }

  private async loadModelViaSceneEngine(source: ModelSource, token: number): Promise<void> {
    await this.sceneEngine.ready();
    if (token !== this.loadToken || !this.scene) {
      return;
    }
    const { bytes, format, name } = await readModelBytes(source);
    if (token !== this.loadToken || !this.scene) {
      return;
    }
    // Time each phase of the WASM round-trip independently so the overlay
    // can break down where wall time is spent (parse vs. render-buffer
    // copy). `performance.now()` returns a high-resolution monotonic clock.
    const tParseStart = performance.now();
    const id = this.sceneEngine.addMesh(name, format, bytes);
    const tParseEnd = performance.now();
    const buf = this.sceneEngine.getRenderBuffer(id);
    const tRenderBufEnd = performance.now();
    this.wasmParseMs.set(tParseEnd - tParseStart);
    this.wasmRenderBufMs.set(tRenderBufEnd - tParseEnd);
    this.wasmRoundtripMs.set(tRenderBufEnd - tParseStart);
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(buf.positions, 3));
    geometry.setAttribute('normal', new BufferAttribute(buf.normals, 3));
    geometry.setIndex(new BufferAttribute(buf.indices, 1));
    geometry.computeBoundingBox();
    geometry.computeBoundingSphere();
    const material = new MeshPhongMaterial({
      color: 0xa9b4c2,
      flatShading: true,
      shininess: 16,
    });
    const mesh = new Mesh(geometry, material);
    mesh.name = name;
    mesh.matrixAutoUpdate = false;
    // Seed initial matrix so first frame renders correctly even before any
    // op fires the snapshot effect.
    this.tmpMatrix.fromArray(this.sceneEngine.getMatrix(id));
    mesh.matrix.copy(this.tmpMatrix);
    mesh.matrixWorldNeedsUpdate = true;
    this.scene.contentRoot.add(mesh);
    this.wasmMeshes.set(id, mesh);
    // Stamp the same id (stringified) on the legacy scene's selectable
    // registry so the existing raycast / drag pointer plumbing recognises
    // it. The drag handlers translate it back to a bigint.
    this.scene.registerSelectable(String(id), mesh);
    this.scene.fitToContent();
    this.status.set('ready');
    this.loadComplete.emit({ mode: 'model', segments: 0 });
  }

  private startGcodeLoad(source: GcodeSource): void {
    const scene = this.scene;
    if (!scene) {
      return;
    }
    const token = ++this.loadToken;
    this.status.set('loading');

    const geometry = new ChunkedLineGeometry(this.showTravel());
    this.gcodeGeometry = geometry;
    scene.contentRoot.add(geometry.root);

    const controller = new AbortController();
    this.currentAbort = controller;

    loadGcode(source, geometry, {
      signal: controller.signal,
      onFirstGeometry: () => {
        if (token !== this.loadToken || !this.scene) {
          return;
        }
        this.status.set('streaming');
        this.scene.fitToContent();
      },
      onProgress: (total) => {
        if (token !== this.loadToken) {
          return;
        }
        this.progressSegments.set(total);
      },
      onComplete: (total) => {
        if (token !== this.loadToken || !this.scene) {
          return;
        }
        this.progressSegments.set(total);
        this.scene.fitToContent();
        this.status.set('ready');
        this.loadComplete.emit({ mode: 'gcode', segments: total });
      },
    }).catch((error: unknown) => {
      if (token !== this.loadToken) {
        return;
      }
      if (isAbortError(error)) {
        return;
      }
      this.errorMessage.set(messageOf(error));
      this.status.set('error');
      this.loadError.emit({ mode: 'gcode', error });
    });

    // Silence unused lint for elementRef (kept for future use, e.g. resize).
    void this.elementRef;
  }

  private cancelInFlightLoad(): void {
    this.loadToken++;
    if (this.currentAbort) {
      this.currentAbort.abort();
      this.currentAbort = null;
    }
  }
}

function isAbortError(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'name' in error &&
    (error as { name: string }).name === 'AbortError'
  );
}

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  return '';
}

/**
 * Coerce a {@link ModelSource} into the `{ bytes, format, name }` triple
 * expected by `SceneEngineService.addMesh`. The format is detected from the
 * source's filename / URL extension; raw `ArrayBuffer`/`Blob` inputs without
 * a name fall back to STL (the most common payload).
 */
async function readModelBytes(
  source: ModelSource,
): Promise<{ bytes: Uint8Array; format: 'stl' | 'obj' | '3mf'; name: string }> {
  let buffer: ArrayBuffer;
  let name = 'model';
  if (typeof source === 'string') {
    name = source.split(/[\\/]/).pop() ?? 'model';
    const res = await fetch(source);
    buffer = await res.arrayBuffer();
  } else if (source instanceof URL) {
    name = source.pathname.split('/').pop() ?? 'model';
    const res = await fetch(source);
    buffer = await res.arrayBuffer();
  } else if (source instanceof File) {
    name = source.name;
    buffer = await source.arrayBuffer();
  } else if (source instanceof Blob) {
    buffer = await source.arrayBuffer();
  } else {
    buffer = source;
  }
  const ext = name.split('.').pop()?.toLowerCase();
  const format: 'stl' | 'obj' | '3mf' =
    ext === 'obj' || ext === '3mf' || ext === 'stl' ? ext : 'stl';
  return { bytes: new Uint8Array(buffer), format, name };
}

/**
 * Parse the string id stored on the legacy scene's selectable registry back
 * into the WASM `bigint` id used by the scene engine. Returns `null` if the
 * string isn't a valid integer (defensive — should never happen since we
 * stamp the id ourselves at registration time).
 */
function parseWasmId(stringId: string): bigint | null {
  try {
    return BigInt(stringId);
  } catch {
    return null;
  }
}
