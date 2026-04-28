import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  afterNextRender,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';
import { PrintAreaService } from '../../services/print-area';
import { ViewerControl } from '../../services/viewer-control';
import { ChunkedLineGeometry } from './chunked-line-geometry';
import { GcodeSource, loadGcode } from './gcode-loader';
import { ModelSource, loadModel } from './model-loader';
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
    @if (status() !== 'idle') {
      <div class="viewer-status">{{ statusLabel() }}</div>
    }
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
      }
      .viewer-host {
        position: absolute;
        inset: 0;
      }
      .viewer-status {
        position: absolute;
        bottom: 12px;
        left: 12px;
        padding: 6px 10px;
        background: rgba(0, 0, 0, 0.55);
        color: #e6e6e6;
        font:
          12px/1.2 ui-monospace,
          monospace;
        border-radius: 4px;
        pointer-events: none;
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
  private readonly printArea = inject(PrintAreaService);

  /** Current loading status for the optional overlay. */
  readonly status = signal<'idle' | 'loading' | 'streaming' | 'ready' | 'error'>('idle');
  private readonly progressSegments = signal(0);
  private readonly errorMessage = signal<string>('');

  private scene: ViewerScene | null = null;
  private gcodeGeometry: ChunkedLineGeometry | null = null;
  private currentAbort: AbortController | null = null;
  private loadToken = 0;

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
    this.scene = new ViewerScene(host);
    // Mirror the live camera direction/up into ViewerControl so external
    // overlays (the viewport-cube gizmo) can read it without going through
    // Angular's change-detection.
    const state = this.viewerControl.cameraState;
    this.scene.cameraStateSink = (dir, up) => {
      state.direction.copy(dir);
      state.up.copy(up);
    };
    // Allow external gizmos (viewport-cube drag) to orbit the main camera.
    this.viewerControl.orbitSink = (azimuth, polar) => this.scene?.orbitBy(azimuth, polar);
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

    loadModel(source)
      .then((object) => {
        if (token !== this.loadToken || !this.scene) {
          return;
        }
        this.scene.contentRoot.add(object);
        this.scene.fitToContent();
        this.status.set('ready');
        this.loadComplete.emit({ mode: 'model', segments: 0 });
      })
      .catch((error: unknown) => {
        if (token !== this.loadToken) {
          return;
        }
        this.errorMessage.set(messageOf(error));
        this.status.set('error');
        this.loadError.emit({ mode: 'model', error });
      });
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
