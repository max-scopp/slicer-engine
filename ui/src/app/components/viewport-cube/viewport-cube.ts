import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  inject,
  viewChild,
} from '@angular/core';
import {
  BoxGeometry,
  CanvasTexture,
  ConeGeometry,
  CylinderGeometry,
  Group,
  LinearFilter,
  Mesh,
  MeshBasicMaterial,
  PerspectiveCamera,
  Raycaster,
  Scene,
  Sprite,
  SpriteMaterial,
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
  /**
   * Rotation (radians) applied to the face label canvas so the text reads
   * upright when the camera is snapped to face that side with the
   * corresponding `up` vector. Required because BoxGeometry's per-face UV
   * tangent frames don't align with the world Z-up convention used by the
   * scene, so unrotated text would appear sideways or upside-down on the
   * ±X / +Y / -Z faces.
   */
  textRotation: number;
}

const FACE_SPECS: readonly FaceSpec[] = [
  {
    label: 'RIGHT',
    direction: new Vector3(1, 0, 0),
    up: new Vector3(0, 0, 1),
    textRotation: -Math.PI / 2,
  },
  {
    label: 'LEFT',
    direction: new Vector3(-1, 0, 0),
    up: new Vector3(0, 0, 1),
    textRotation: Math.PI / 2,
  },
  {
    label: 'BACK',
    direction: new Vector3(0, 1, 0),
    up: new Vector3(0, 0, 1),
    textRotation: Math.PI,
  },
  {
    label: 'FRONT',
    direction: new Vector3(0, -1, 0),
    up: new Vector3(0, 0, 1),
    textRotation: 0,
  },
  {
    label: 'TOP',
    direction: new Vector3(0, 0, 1),
    up: new Vector3(0, 1, 0),
    textRotation: 0,
  },
  {
    label: 'BOTTOM',
    direction: new Vector3(0, 0, -1),
    up: new Vector3(0, 1, 0),
    textRotation: Math.PI,
  },
];

const CUBE_SIZE = 1;
// Half-extent of the cube's bounding sphere — guarantees the cube fits no
// matter how it is rotated (corner distance from center is sqrt(3)/2 * size).
const CUBE_HALF_EXTENT = (CUBE_SIZE * Math.sqrt(3)) / 2;
// Padding factor around the cube inside the orthographic frustum. Wider than
// strictly needed for the cube alone so the dimensional-guide axes (which
// run along the three cube edges meeting at the -X/-Y/-Z corner) and their
// X/Y/Z end labels stay fully on-screen at every camera orientation.
const FRUSTUM_PADDING = 1.55;

// RGB axes gizmo — colour convention matches the main scene's
// `buildAxesGizmo` (X = red, Y = green, Z = blue) so the orientation cube
// reads identically to the build-plate gizmo in the main viewer.
const AXIS_COLOR_X = 0xff3344;
const AXIS_COLOR_Y = 0x33dd55;
const AXIS_COLOR_Z = 0x4488ff;
// The gizmo lives at the cube's -X/-Y/-Z corner (visually "bottom-left-back")
// and its three coloured shafts run **along** the three cube edges that meet
// at that corner. The shafts therefore double as dimensional guides: each
// edge is annotated with the world-axis (X/Y/Z) it represents, with an arrow
// head + label at the +end of every edge so the user can read the build-volume
// orientation at a glance. Each shaft is offset slightly outboard of its edge
// (perpendicular to its own axis) so it sits just outside the cube faces and
// doesn't z-fight with the textured face beneath it.
const AXIS_EDGE_OFFSET = 0.04;
const AXIS_LENGTH = CUBE_SIZE;
const AXIS_SHAFT_RADIUS = 0.018;
const AXIS_HEAD_LENGTH = 0.14;
const AXIS_HEAD_RADIUS = 0.05;
// Small perpendicular tick mark at the origin end of each axis — mimics the
// end caps on architectural dimension lines, making it obvious that the
// coloured shaft represents the *length* of the cube edge along that axis.
const AXIS_TICK_LENGTH = 0.09;
const AXIS_TICK_RADIUS = 0.01;
// Distance from the tip of the arrow head to the centre of the X/Y/Z label
// sprite, expressed in world units of the cube scene.
const AXIS_LABEL_OFFSET = 0.12;
const AXIS_LABEL_SIZE = 0.26;

// Opacity of the cube's face tiles. Low enough that the RGB gizmo running
// along the back edges shows clearly through the front faces, high enough
// that the face labels (FRONT / BACK / TOP / etc.) and themed border still
// read as a solid clickable button.
const CUBE_FACE_OPACITY = 0.85;
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
export class ViewportCube {
  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>('canvas');
  private readonly viewerControl = inject(ViewerControl);
  private readonly destroyRef = inject(DestroyRef);

  private renderer: WebGLRenderer | null = null;
  private scene: Scene | null = null;
  private camera: PerspectiveCamera | null = null;
  private cube: Mesh | null = null;
  private axesGizmo: Group | null = null;
  private raycaster = new Raycaster();
  private rafHandle = 0;
  private hoveredFace = -1;
  private resizeObserver: ResizeObserver | null = null;
  private themeObserver: MutationObserver | null = null;

  /**
   * Dirty flag for on-demand rendering. The cube is a static helper — most
   * frames it does not need to redraw at all. We render only when the
   * mirrored camera state changes, the hover state changes, the canvas
   * resizes, or the theme repaints the textures. This keeps an idle iPad
   * from burning a second WebGL pipeline at 60 fps for nothing.
   */
  private needsRender = true;
  private readonly lastRenderedDirection = new Vector3(NaN, NaN, NaN);
  private readonly lastRenderedUp = new Vector3(NaN, NaN, NaN);
  private lastRenderedFov = NaN;

  private dragging = false;
  private pointerId: number | null = null;
  private dragStart = new Vector2();
  private dragLast = new Vector2();
  private dragMoved = false;

  constructor() {
    afterNextRender(() => this.init());

    this.destroyRef.onDestroy(() => {
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
      if (this.axesGizmo) {
        disposeAxesGizmo(this.axesGizmo);
      }
      this.renderer?.dispose();
    });
  }

  private init(): void {
    const canvas = this.canvasRef().nativeElement;

    this.renderer = new WebGLRenderer({
      canvas,
      alpha: true,
      // At Retina DPR (≥2) MSAA cost is non-trivial on iOS for a UI gizmo
      // this small — the labels are antialiased through the canvas-2D path
      // already.
      antialias: window.devicePixelRatio < 2,
      powerPreference: 'high-performance',
    });
    // Cap DPR for the same reason as the main viewer: iPads / phones at
    // DPR 3 quadruple the fragment cost for no perceptible benefit on a
    // 96×96 CSS-pixel widget.
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setClearColor(0x000000, 0);

    this.scene = new Scene();
    this.scene.background = null;

    // Perspective camera that mirrors the main scene's FOV every frame so
    // the cube reads as orthographic when the main view is ortho (FOV ≈ 1°)
    // and as perspective when the main view is perspective (FOV ≈ 45°).
    // Initial parameters are placeholders — `resize()` and `tick()` set the
    // real aspect / FOV / distance.
    this.camera = new PerspectiveCamera(45, 1, 0.01, 100);
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(0, 0, CUBE_DISTANCE);
    this.camera.lookAt(0, 0, 0);

    const materials = FACE_SPECS.map(
      (face) =>
        new MeshBasicMaterial({
          map: makeFaceTexture(face.label, false, readPalette(), face.textRotation),
          // Semi-transparent so the RGB axis gizmo running along the cube's
          // back edges remains visible through the front faces \u2014 a tinted
          // glass effect that keeps the orientation guides readable from
          // every angle without forcing the gizmo above the cube in z-order.
          transparent: true,
          opacity: CUBE_FACE_OPACITY,
          depthWrite: false,
        }),
    );

    const geometry = new BoxGeometry(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE);
    this.cube = new Mesh(geometry, materials);
    this.scene.add(this.cube);

    this.axesGizmo = buildAxesGizmo();
    this.scene.add(this.axesGizmo);

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
      materials[i].map = makeFaceTexture(
        FACE_SPECS[i].label,
        i === this.hoveredFace,
        palette,
        FACE_SPECS[i].textRotation,
      );
      materials[i].needsUpdate = true;
    }
    this.needsRender = true;
  }

  private resize(): void {
    if (!this.renderer || !this.camera) {
      return;
    }
    const canvas = this.canvasRef().nativeElement;
    const w = Math.max(canvas.clientWidth, 1);
    const h = Math.max(canvas.clientHeight, 1);
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.needsRender = true;
  }

  private tick = (): void => {
    this.rafHandle = requestAnimationFrame(this.tick);
    if (!this.renderer || !this.scene || !this.camera || !this.cube) {
      return;
    }
    // Mirror the main viewer's camera orientation: place our fixed-distance
    // camera along the same direction (target → camera) the main camera is
    // viewing from, with a matching up vector. Detect changes vs. the last
    // rendered state so we can skip the (otherwise per-frame) render call
    // when the user is doing nothing — a major battery / thermal win on
    // iPads where this would otherwise run a second WebGL pipeline at the
    // display refresh rate.
    const state = this.viewerControl.cameraState;
    if (
      !this.lastRenderedDirection.equals(state.direction) ||
      !this.lastRenderedUp.equals(state.up) ||
      this.lastRenderedFov !== state.fov
    ) {
      this.lastRenderedDirection.copy(state.direction);
      this.lastRenderedUp.copy(state.up);
      this.lastRenderedFov = state.fov;
      this.needsRender = true;
    }
    if (!this.needsRender) {
      return;
    }
    // Mirror the main camera's FOV. Distance from the cube is then derived
    // from FOV so the cube + axes gizmo always inscribe the same fraction
    // of the viewport regardless of projection — narrow FOV = far camera
    // (visually orthographic), wide FOV = closer camera (visually perspective).
    const fovRad = (state.fov * Math.PI) / 180;
    const fitRadius = CUBE_HALF_EXTENT * FRUSTUM_PADDING;
    // Account for non-square aspects: the limiting half-fov is on the
    // smaller axis (vertical FOV by default; horizontal scales by aspect).
    const aspect = this.camera.aspect;
    const vHalfFov = fovRad / 2;
    const hHalfFov = Math.atan(Math.tan(vHalfFov) * aspect);
    const limitHalfFov = Math.min(vHalfFov, hHalfFov);
    const distance = fitRadius / Math.tan(limitHalfFov);
    this.camera.fov = state.fov;
    this.camera.near = Math.max(distance - fitRadius * 2, 0.01);
    this.camera.far = distance + fitRadius * 2;
    this.camera.updateProjectionMatrix();
    this.camera.position.copy(state.direction).multiplyScalar(distance);
    this.camera.up.copy(state.up);
    this.camera.lookAt(0, 0, 0);
    this.renderer.render(this.scene, this.camera);
    this.needsRender = false;
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
        // dx > 0 = drag right → positive azimuth → camera orbits CCW from above → RIGHT face shown.
        // dy < 0 = drag up  → −dy > 0 → positive polar → newPhi = phi − polar decreases → camera rises toward TOP.
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
      prev.map = makeFaceTexture(
        FACE_SPECS[this.hoveredFace].label,
        false,
        palette,
        FACE_SPECS[this.hoveredFace].textRotation,
      );
      prev.needsUpdate = true;
    }
    if (face >= 0) {
      const next = materials[face];
      next.map?.dispose();
      next.map = makeFaceTexture(
        FACE_SPECS[face].label,
        true,
        palette,
        FACE_SPECS[face].textRotation,
      );
      next.needsUpdate = true;
    }
    this.hoveredFace = face;
    this.needsRender = true;
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
    // Outer fill matches the viewer background so face seams are invisible.
    surface: get('--color-bg-primary', '#f4f5f8'),
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
function makeFaceTexture(
  label: string,
  hovered: boolean,
  palette: CubePalette,
  textRotation: number,
): CanvasTexture {
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
  // Monospace so every face label has identical letter geometry, which keeps
  // the cube reading like a uniform button grid even when the labels rotate
  // per-face. Bumped a notch above the previous Inter size for legibility at
  // the small canvas-texture footprint.
  ctx.font = '700 64px "JetBrains Mono", "Fira Code", "SF Mono", Consolas, ui-monospace, monospace';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  // Rotate around the canvas centre so the label reads upright when this
  // face is viewed head-on with the scene's Z-up convention. BoxGeometry's
  // per-face UV tangent frames don't all align with world Z-up, so without
  // this correction ±X / +Y / -Z labels would appear sideways or upside-down.
  if (textRotation !== 0) {
    ctx.save();
    ctx.translate(size / 2, size / 2);
    ctx.rotate(textRotation);
    ctx.fillText(label, 0, 0);
    ctx.restore();
  } else {
    ctx.fillText(label, size / 2, size / 2);
  }

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

/**
 * Build the small RGB axes gizmo that sits at the cube's -X/-Y/-Z corner
 * ("bottom-left-back"). The three coloured shafts run **along** the three
 * cube edges meeting at that corner, doubling as dimensional guides for the
 * build-volume orientation: red = X, green = Y, blue = Z, each ending in a
 * matching arrow head + billboarded sprite label so the user can always
 * read which colour maps to which axis regardless of camera angle.
 *
 * The gizmo lives in the same scene as the cube — depth testing against
 * the cube's opaque faces naturally hides the parts that should be behind
 * the cube when viewed from the opposite octant.
 */
function buildAxesGizmo(): Group {
  const group = new Group();
  const half = CUBE_SIZE / 2;
  const eps = AXIS_EDGE_OFFSET;

  // Each axis runs along the cube edge starting at -half on its own axis,
  // offset by `eps` outboard on the two perpendicular axes so the shaft sits
  // just outside the cube faces (no z-fighting with the textured face).
  const xOrigin = new Vector3(-half, -half - eps, -half - eps);
  const yOrigin = new Vector3(-half - eps, -half, -half - eps);
  const zOrigin = new Vector3(-half - eps, -half - eps, -half);

  group.add(buildAxisArrow('X', new Vector3(1, 0, 0), AXIS_COLOR_X, xOrigin));
  group.add(buildAxisArrow('Y', new Vector3(0, 1, 0), AXIS_COLOR_Y, yOrigin));
  group.add(buildAxisArrow('Z', new Vector3(0, 0, 1), AXIS_COLOR_Z, zOrigin));

  return group;
}

/**
 * Build one axis arrow (start tick + shaft + head + label sprite) pointing
 * along `direction` from `origin`. `direction` must be a unit cardinal vector.
 * The shaft length spans the full cube edge so the arrow visually annotates
 * the edge as a dimension line.
 */
function buildAxisArrow(label: string, direction: Vector3, color: number, origin: Vector3): Group {
  const arrow = new Group();
  const shaftLength = AXIS_LENGTH - AXIS_HEAD_LENGTH;

  // Standard depth-tested opaque material — the gizmo lives "inside" the
  // cube space and we want the cube's semi-transparent faces to visibly
  // overlay it from the front while the axes stay readable through the
  // tinted glass effect. Opaque draws before transparent in three.js, so
  // ordering is automatic.
  const material = new MeshBasicMaterial({ color });

  // Perpendicular tick at the origin end — architectural dimension-line cap
  // marking the start of the measured span. Built along +X (perpendicular to
  // the shaft's local +Y) so it sits flat against the cube corner.
  const tickGeometry = new CylinderGeometry(
    AXIS_TICK_RADIUS,
    AXIS_TICK_RADIUS,
    AXIS_TICK_LENGTH,
    8,
  );
  const tick = new Mesh(tickGeometry, material);
  tick.rotation.z = Math.PI / 2;
  arrow.add(tick);

  // Shaft — CylinderGeometry's default axis is +Y, so build along +Y and
  // rotate the whole arrow into place via setFromUnitVectors below.
  const shaftGeometry = new CylinderGeometry(AXIS_SHAFT_RADIUS, AXIS_SHAFT_RADIUS, shaftLength, 16);
  const shaft = new Mesh(shaftGeometry, material);
  shaft.position.y = shaftLength / 2;
  arrow.add(shaft);

  // Arrow head sits on top of the shaft.
  const headGeometry = new ConeGeometry(AXIS_HEAD_RADIUS, AXIS_HEAD_LENGTH, 16);
  const head = new Mesh(headGeometry, material);
  head.position.y = shaftLength + AXIS_HEAD_LENGTH / 2;
  arrow.add(head);

  // Billboarded label sprite — always faces the camera, sits just past the
  // arrow tip in the arrow's local +Y direction (rotated into world space
  // alongside the rest of the arrow). Standard depth testing means the
  // semi-transparent cube tints the labels on the far side, matching the
  // shafts.
  const sprite = new Sprite(
    new SpriteMaterial({
      map: makeAxisLabelTexture(label, color),
      transparent: true,
    }),
  );
  sprite.position.y = AXIS_LENGTH + AXIS_LABEL_OFFSET;
  sprite.scale.set(AXIS_LABEL_SIZE, AXIS_LABEL_SIZE, 1);
  arrow.add(sprite);

  // Orient the +Y-aligned arrow so its tip points along `direction`.
  arrow.position.copy(origin);
  arrow.quaternion.setFromUnitVectors(new Vector3(0, 1, 0), direction);

  return arrow;
}

/**
 * Build a CanvasTexture for an axis label sprite. The label is drawn in the
 * matching axis colour on a transparent background.
 */
function makeAxisLabelTexture(label: string, color: number): CanvasTexture {
  const size = 128;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return new CanvasTexture(canvas);
  }

  const hex = `#${color.toString(16).padStart(6, '0')}`;
  ctx.font = '700 96px "Inter", system-ui, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  // Subtle dark halo for legibility against light cube faces.
  ctx.lineWidth = 8;
  ctx.strokeStyle = 'rgba(0, 0, 0, 0.55)';
  ctx.strokeText(label, size / 2, size / 2);
  ctx.fillStyle = hex;
  ctx.fillText(label, size / 2, size / 2);

  const tex = new CanvasTexture(canvas);
  tex.minFilter = LinearFilter;
  tex.magFilter = LinearFilter;
  tex.needsUpdate = true;
  return tex;
}

/** Recursively dispose every Mesh / Sprite resource owned by the gizmo. */
function disposeAxesGizmo(root: Group): void {
  root.traverse((obj) => {
    if (obj instanceof Mesh) {
      obj.geometry.dispose();
      const mat = obj.material as MeshBasicMaterial | MeshBasicMaterial[];
      if (Array.isArray(mat)) {
        for (const m of mat) m.dispose();
      } else {
        mat.dispose();
      }
    } else if (obj instanceof Sprite) {
      const mat = obj.material;
      mat.map?.dispose();
      mat.dispose();
    }
  });
}
