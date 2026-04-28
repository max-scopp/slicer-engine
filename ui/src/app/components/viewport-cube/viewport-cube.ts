import {
    AfterViewInit,
    ChangeDetectionStrategy,
    Component,
    ElementRef,
    OnDestroy,
    inject,
    viewChild,
} from '@angular/core';
import {
    BoxGeometry,
    CanvasTexture,
    LinearFilter,
    Mesh,
    MeshBasicMaterial,
    OrthographicCamera,
    Raycaster,
    Scene,
    Vector2,
    Vector3,
    WebGLRenderer,
} from 'three';
import { ViewerControl } from '../../services/viewer-control';

/**
 * Mapping from a BoxGeometry material index to the look direction (target →
 * camera, in world space) and up vector that the main viewer camera should
 * adopt when the user clicks that face. The Z-up scene convention is:
 *  - +X = RIGHT, -X = LEFT
 *  - +Y = BACK,  -Y = FRONT
 *  - +Z = TOP,   -Z = BOTTOM
 */
interface FaceSpec {
  label: string;
  direction: Vector3;
  up: Vector3;
}

const FACE_SPECS: readonly FaceSpec[] = [
  { label: 'RIGHT', direction: new Vector3(1, 0, 0), up: new Vector3(0, 0, 1) },
  { label: 'LEFT', direction: new Vector3(-1, 0, 0), up: new Vector3(0, 0, 1) },
  { label: 'BACK', direction: new Vector3(0, 1, 0), up: new Vector3(0, 0, 1) },
  { label: 'FRONT', direction: new Vector3(0, -1, 0), up: new Vector3(0, 0, 1) },
  { label: 'TOP', direction: new Vector3(0, 0, 1), up: new Vector3(0, 1, 0) },
  { label: 'BOTTOM', direction: new Vector3(0, 0, -1), up: new Vector3(0, 1, 0) },
];

const CUBE_SIZE = 1;
// Half-extent of the cube's bounding sphere — guarantees the cube fits no
// matter how it is rotated (corner distance from center is sqrt(3)/2 * size).
const CUBE_HALF_EXTENT = (CUBE_SIZE * Math.sqrt(3)) / 2;
// Padding factor around the cube inside the orthographic frustum.
const FRUSTUM_PADDING = 1.1;
// Distance from the camera to the cube. Arbitrary for an orthographic camera
// — only direction matters — but kept large enough to stay well inside the
// near/far range.
const CUBE_DISTANCE = 5;
// Drag-to-orbit sensitivity (radians per pixel).
const DRAG_SENSITIVITY = 0.01;
// Pointer-move distance (in pixels) below which a pointer-up still counts as
// a click rather than the end of a drag.
const CLICK_DRAG_THRESHOLD = 4;

/**
 * Small Fusion-360-style viewport cube. Renders a labelled cube whose
 * orientation mirrors the main viewer camera. Click a face to snap the main
 * camera to look from that direction; click-and-drag the cube to orbit the
 * main camera freely.
 */
@Component({
  selector: 'nexus-viewport-cube',
  standalone: true,
  template: `<canvas #canvas class="cube-canvas"></canvas>`,
  styles: [
    `
      :host {
        display: block;
        width: 96px;
        height: 96px;
        background: transparent;
        pointer-events: auto;
        user-select: none;
        touch-action: none;
      }
      .cube-canvas {
        display: block;
        width: 100%;
        height: 100%;
        background: transparent;
        cursor: grab;
      }
      .cube-canvas.is-dragging {
        cursor: grabbing;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ViewportCube implements AfterViewInit, OnDestroy {
  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>('canvas');
  private readonly viewerControl = inject(ViewerControl);

  private renderer: WebGLRenderer | null = null;
  private scene: Scene | null = null;
  private camera: OrthographicCamera | null = null;
  private cube: Mesh | null = null;
  private raycaster = new Raycaster();
  private rafHandle = 0;
  private hoveredFace = -1;
  private resizeObserver: ResizeObserver | null = null;
  private themeObserver: MutationObserver | null = null;

  private dragging = false;
  private pointerId: number | null = null;
  private dragStart = new Vector2();
  private dragLast = new Vector2();
  private dragMoved = false;

  ngAfterViewInit(): void {
    this.init();
  }

  ngOnDestroy(): void {
    cancelAnimationFrame(this.rafHandle);
    this.resizeObserver?.disconnect();
    this.themeObserver?.disconnect();
    this.cube?.geometry.dispose();
    if (this.cube) {
      const mats = this.cube.material as MeshBasicMaterial[];
      for (const m of mats) {
        m.map?.dispose();
        m.dispose();
      }
    }
    this.renderer?.dispose();
  }

  private init(): void {
    const canvas = this.canvasRef().nativeElement;

    this.renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.setClearColor(0x000000, 0);

    this.scene = new Scene();
    this.scene.background = null;

    // Orthographic camera sized to the cube's bounding sphere so the cube
    // never visually clips against the canvas edges, regardless of rotation.
    const halfExtent = CUBE_HALF_EXTENT * FRUSTUM_PADDING;
    this.camera = new OrthographicCamera(
      -halfExtent,
      halfExtent,
      halfExtent,
      -halfExtent,
      0.1,
      100,
    );
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(0, 0, CUBE_DISTANCE);
    this.camera.lookAt(0, 0, 0);

    const materials = FACE_SPECS.map(
      (face) =>
        new MeshBasicMaterial({
          map: makeFaceTexture(face.label, false, readPalette()),
          transparent: false,
        }),
    );

    const geometry = new BoxGeometry(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE);
    this.cube = new Mesh(geometry, materials);
    this.scene.add(this.cube);

    this.resize();
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(canvas);

    canvas.addEventListener('pointerdown', this.onPointerDown);
    canvas.addEventListener('pointermove', this.onPointerMove);
    canvas.addEventListener('pointerup', this.onPointerUp);
    canvas.addEventListener('pointercancel', this.onPointerUp);
    canvas.addEventListener('pointerleave', this.onPointerLeave);

    // Re-paint face textures whenever the global theme changes so the cube
    // always picks up the current `--color-surface` / `--color-text-primary`
    // / `--color-border` tokens (matching the toolbar buttons).
    this.themeObserver = new MutationObserver(() => this.refreshFaceTextures());
    this.themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'style'],
    });

    this.tick();
  }

  /** Rebuild every face texture, preserving the current hover state. */
  private refreshFaceTextures(): void {
    if (!this.cube) {
      return;
    }
    const materials = this.cube.material as MeshBasicMaterial[];
    const palette = readPalette();
    for (let i = 0; i < materials.length; i++) {
      materials[i].map?.dispose();
      materials[i].map = makeFaceTexture(FACE_SPECS[i].label, i === this.hoveredFace, palette);
      materials[i].needsUpdate = true;
    }
  }

  private resize(): void {
    if (!this.renderer || !this.camera) {
      return;
    }
    const canvas = this.canvasRef().nativeElement;
    const w = Math.max(canvas.clientWidth, 1);
    const h = Math.max(canvas.clientHeight, 1);
    this.renderer.setSize(w, h, false);

    // Keep the cube square inside any non-square canvas by widening the
    // frustum on the longer axis.
    const aspect = w / h;
    const halfExtent = CUBE_HALF_EXTENT * FRUSTUM_PADDING;
    if (aspect >= 1) {
      this.camera.left = -halfExtent * aspect;
      this.camera.right = halfExtent * aspect;
      this.camera.top = halfExtent;
      this.camera.bottom = -halfExtent;
    } else {
      this.camera.left = -halfExtent;
      this.camera.right = halfExtent;
      this.camera.top = halfExtent / aspect;
      this.camera.bottom = -halfExtent / aspect;
    }
    this.camera.updateProjectionMatrix();
  }

  private tick = (): void => {
    this.rafHandle = requestAnimationFrame(this.tick);
    if (!this.renderer || !this.scene || !this.camera || !this.cube) {
      return;
    }
    // Mirror the main viewer's camera orientation: place our fixed-distance
    // camera along the same direction (target → camera) the main camera is
    // viewing from, with a matching up vector.
    const state = this.viewerControl.cameraState;
    this.camera.position.copy(state.direction).multiplyScalar(CUBE_DISTANCE);
    this.camera.up.copy(state.up);
    this.camera.lookAt(0, 0, 0);
    this.renderer.render(this.scene, this.camera);
  };

  private onPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0) {
      return;
    }
    const canvas = this.canvasRef().nativeElement;
    canvas.setPointerCapture(event.pointerId);
    this.pointerId = event.pointerId;
    this.dragging = true;
    this.dragMoved = false;
    this.dragStart.set(event.clientX, event.clientY);
    this.dragLast.copy(this.dragStart);
    canvas.classList.add('is-dragging');
  };

  private onPointerMove = (event: PointerEvent): void => {
    if (this.dragging && event.pointerId === this.pointerId) {
      const dx = event.clientX - this.dragLast.x;
      const dy = event.clientY - this.dragLast.y;
      this.dragLast.set(event.clientX, event.clientY);
      if (
        !this.dragMoved &&
        Math.hypot(event.clientX - this.dragStart.x, event.clientY - this.dragStart.y) >
          CLICK_DRAG_THRESHOLD
      ) {
        this.dragMoved = true;
        // Cancel any face hover styling once a drag begins.
        this.setHover(-1);
      }
      if (this.dragMoved) {
        this.viewerControl.orbitSink?.(dx * DRAG_SENSITIVITY, -dy * DRAG_SENSITIVITY);
      }
      return;
    }
    // Hover highlighting only when not dragging.
    const face = this.pickFace(event);
    if (face !== this.hoveredFace) {
      this.setHover(face);
    }
  };

  private onPointerUp = (event: PointerEvent): void => {
    if (!this.dragging || event.pointerId !== this.pointerId) {
      return;
    }
    const canvas = this.canvasRef().nativeElement;
    if (canvas.hasPointerCapture(event.pointerId)) {
      canvas.releasePointerCapture(event.pointerId);
    }
    canvas.classList.remove('is-dragging');
    const wasDrag = this.dragMoved;
    this.dragging = false;
    this.pointerId = null;
    this.dragMoved = false;

    if (!wasDrag) {
      const face = this.pickFace(event);
      if (face >= 0) {
        const spec = FACE_SPECS[face];
        this.viewerControl.lookFrom(spec.direction, spec.up);
      }
    }
  };

  private onPointerLeave = (): void => {
    if (!this.dragging) {
      this.setHover(-1);
    }
  };

  private setHover(face: number): void {
    if (!this.cube || face === this.hoveredFace) {
      return;
    }
    const materials = this.cube.material as MeshBasicMaterial[];
    const palette = readPalette();
    if (this.hoveredFace >= 0) {
      const prev = materials[this.hoveredFace];
      prev.map?.dispose();
      prev.map = makeFaceTexture(FACE_SPECS[this.hoveredFace].label, false, palette);
      prev.needsUpdate = true;
    }
    if (face >= 0) {
      const next = materials[face];
      next.map?.dispose();
      next.map = makeFaceTexture(FACE_SPECS[face].label, true, palette);
      next.needsUpdate = true;
    }
    this.hoveredFace = face;
  }

  private pickFace(event: PointerEvent): number {
    if (!this.camera || !this.cube) {
      return -1;
    }
    const canvas = this.canvasRef().nativeElement;
    const rect = canvas.getBoundingClientRect();
    const ndc = new Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(ndc, this.camera);
    const hits = this.raycaster.intersectObject(this.cube, false);
    if (hits.length === 0 || hits[0].face == null) {
      return -1;
    }
    return hits[0].face.materialIndex;
  }
}

/**
 * Snapshot of the themed colour tokens used to paint a cube face. Captured
 * once per repaint so all six faces stay visually consistent even mid-theme-
 * transition.
 */
interface CubePalette {
  surface: string;
  surfaceHover: string;
  text: string;
  border: string;
  primary: string;
  primaryLight: string;
  cornerRadius: number;
}

/** Read the current theme tokens from `<html>` computed styles. */
function readPalette(): CubePalette {
  const styles = getComputedStyle(document.documentElement);
  const get = (name: string, fallback: string): string =>
    styles.getPropertyValue(name).trim() || fallback;
  return {
    surface: get('--color-surface', '#ffffff'),
    surfaceHover: get('--color-surface-hover', '#f0f0f0'),
    text: get('--color-text-primary', '#222222'),
    border: get('--color-border', '#cccccc'),
    primary: get('--color-primary', '#5b5bff'),
    primaryLight: get('--color-primary-light', 'rgba(91, 91, 255, 0.12)'),
    cornerRadius: 22,
  };
}

/**
 * Build a CanvasTexture for a single cube face that visually matches a
 * standard themed button: surface fill, rounded inner tile, themed border,
 * primary-tinted hover state, themed label text.
 */
function makeFaceTexture(label: string, hovered: boolean, palette: CubePalette): CanvasTexture {
  const size = 256;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return new CanvasTexture(canvas);
  }

  // Outer fill = surface so seams between faces are invisible against the
  // surrounding UI background.
  ctx.fillStyle = palette.surface;
  ctx.fillRect(0, 0, size, size);

  // Inner rounded tile mimics the button visual: small inset, themed border,
  // primary-tinted background on hover (matching the button :hover token).
  const inset = 14;
  const x = inset;
  const y = inset;
  const w = size - inset * 2;
  const h = size - inset * 2;

  drawRoundedRect(ctx, x, y, w, h, palette.cornerRadius);
  ctx.fillStyle = hovered ? palette.primaryLight : palette.surfaceHover;
  ctx.fill();

  drawRoundedRect(ctx, x + 0.5, y + 0.5, w - 1, h - 1, palette.cornerRadius);
  ctx.lineWidth = 2;
  ctx.strokeStyle = palette.border;
  ctx.stroke();

  ctx.fillStyle = hovered ? palette.primary : palette.text;
  ctx.font = '600 48px "Inter", system-ui, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(label, size / 2, size / 2);

  const tex = new CanvasTexture(canvas);
  tex.minFilter = LinearFilter;
  tex.magFilter = LinearFilter;
  tex.needsUpdate = true;
  return tex;
}

/** Trace a rounded rectangle path on the given 2D context. */
function drawRoundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const radius = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.lineTo(x + w - radius, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + radius);
  ctx.lineTo(x + w, y + h - radius);
  ctx.quadraticCurveTo(x + w, y + h, x + w - radius, y + h);
  ctx.lineTo(x + radius, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - radius);
  ctx.lineTo(x, y + radius);
  ctx.quadraticCurveTo(x, y, x + radius, y);
  ctx.closePath();
}
