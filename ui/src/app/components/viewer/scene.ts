import {
  AmbientLight,
  Box3,
  BoxGeometry,
  BufferGeometry,
  Color,
  DirectionalLight,
  Float32BufferAttribute,
  Group,
  LineBasicMaterial,
  LineSegments,
  MOUSE,
  Mesh,
  MeshBasicMaterial,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
} from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { PrintAreaConfig } from '../../services/print-area';

export type ViewerView = '3D' | 'Top' | 'Front';
export type ViewerCursorMode = 'orbit' | 'pan' | 'zoom' | 'rotate' | 'pullToSurface';

/** Direction vector (camera → target inverted) of the default 3D framing. */
const DEFAULT_VIEW_DIR = new Vector3(1, -1, 0.8).normalize();
const DEFAULT_FIT_PADDING = 1.4;
const VIEW_TRANSITION_MS = 600;

/** Field of view (deg) used for the perspective "3D" preset. */
const PERSPECTIVE_FOV = 45;
/**
 * Field of view (deg) used to fake an orthographic projection. A very small
 * FOV combined with a proportionally large camera distance approximates a
 * parallel projection while keeping a single PerspectiveCamera — which lets
 * us tween FOV smoothly between ortho and perspective views without ever
 * swapping camera types.
 */
const ORTHO_FOV = 1;

/** Initial camera pose used at startup and as the target of resetView(). */
const INITIAL_CAMERA_POSITION = new Vector3(220, -240, 180);
const INITIAL_CAMERA_TARGET = new Vector3(0, 0, 0);
const INITIAL_CAMERA_UP = new Vector3(0, 0, 1);

/**
 * Fixed near/far plane distances for the camera. Combined with the renderer's
 * `logarithmicDepthBuffer`, this keeps the entire scene visible regardless of
 * how far the user dollies out or which angle they view from.
 */
const CAMERA_NEAR = 0.1;
const CAMERA_FAR = 1_000_000;

/**
 * Adaptive build-plate grid configuration. The grid is bounded to the
 * configured printable area (width × height) and stays at that fixed extent
 * regardless of camera zoom — only the cell spacing snaps to a power of 10
 * in millimetres, picked so one minor cell covers roughly
 * {@link TARGET_MINOR_PIXELS} screen pixels. Major lines are drawn brighter
 * every {@link MAJOR_EVERY} minor cells.
 */
const GRID_MIN_SPACING_MM = 1;
const GRID_MAX_SPACING_MM = 1000;
const MAJOR_EVERY = 10;
const TARGET_MINOR_PIXELS = 14;
const MINOR_OPACITY = 0.25;
const MAJOR_OPACITY = 0.6;
const BED_OUTLINE_OPACITY = 0.9;

const DEFAULT_PRINT_AREA: PrintAreaConfig = {
  printableAreaWidth: 220,
  printableAreaHeight: 220,
  movableAreaX: 0,
  movableAreaY: 0,
};

/**
 * Below this `|cos(angle)|` between the view direction and the grid normal
 * (world +Z), the grid is fully invisible. Above {@link GRID_FADE_FULL}, it
 * is fully opaque. In between, opacity ramps smoothly. This makes the grid
 * disappear when the camera is grazing the build-plate plane (looking
 * “inside” it edge-on) and reappear as the user tilts up or down.
 */
const GRID_FADE_HIDE = 0.05;
const GRID_FADE_FULL = 0.25;

/**
 * Configuration for the Windows-autoscroll-style middle-mouse zoom: while
 * the middle button is held, the camera continuously dollies toward or
 * away from the orbit target with a speed proportional to how far the
 * cursor has been moved (vertically) from the press anchor. Within
 * {@link AUTOSCROLL_DEAD_ZONE_PX} of the anchor the camera does not move,
 * giving users a clean "neutral" position before they commit to a direction.
 */
const AUTOSCROLL_DEAD_ZONE_PX = 6;
/** Linear world-distance multiplier per pixel of cursor offset, per second. */
const AUTOSCROLL_SPEED_PER_PX = 0.012;
/**
 * Superlinear acceleration: the effective rate is multiplied by
 * `(|offsetPx| / AUTOSCROLL_ACCEL_REF_PX) ^ AUTOSCROLL_ACCEL_EXPONENT`, so
 * pushing the cursor further from the anchor accelerates the zoom much more
 * aggressively than a flat linear mapping. Reference distance is 100 px,
 * meaning the speed roughly doubles every additional ~70 px of movement at
 * the default exponent.
 */
const AUTOSCROLL_ACCEL_REF_PX = 100;
const AUTOSCROLL_ACCEL_EXPONENT = 1.6;
/** Hard cap on the per-frame dolly factor so flicks can't blow up. */
const AUTOSCROLL_MAX_FACTOR_PER_FRAME = 4;

/**
 * Owns the Three.js scene, camera, renderer, controls and render loop.
 *
 * Mode-specific content (mesh / gcode lines) is added to {@link contentRoot}.
 * The scene itself is created once and reused across mode switches so that
 * camera state, controls and the WebGL context are not re-initialized.
 */
export class ViewerScene {
  readonly scene = new Scene();
  readonly camera: PerspectiveCamera;
  readonly renderer: WebGLRenderer;
  readonly controls: OrbitControls;
  readonly contentRoot = new Group();

  private readonly host: HTMLElement;
  private readonly resizeObserver: ResizeObserver;
  private readonly themeObserver: MutationObserver;
  private grid: Group;
  private gridMaterials: { material: LineBasicMaterial; baseOpacity: number }[] = [];
  private currentGridSpacingMm = 0;
  private printArea: PrintAreaConfig = { ...DEFAULT_PRINT_AREA };
  private rafHandle = 0;
  private disposed = false;
  private currentView: ViewerView = '3D';
  private animation: CameraAnimation | null = null;
  private autoscroll: AutoscrollState | null = null;
  private lastFrameTime = 0;

  /**
   * Optional sink invoked at the end of every render frame with the live
   * camera direction (target→camera, normalised) and up vector. Used by the
   * viewport-cube gizmo to mirror the main camera's orientation.
   */
  cameraStateSink: ((direction: Vector3, up: Vector3) => void) | null = null;

  constructor(host: HTMLElement) {
    this.host = host;

    // Transparent background so the underlying page (including its themed
    // background colour) shows through.
    this.scene.background = null;
    this.scene.add(this.contentRoot);

    const { clientWidth, clientHeight } = this.sizeOf(host);
    const aspect = clientWidth / clientHeight;
    // Use Z-up so STL/G-code coordinates (printer convention) render with Z
    // as height; XY is the build plate.
    this.camera = new PerspectiveCamera(PERSPECTIVE_FOV, aspect, CAMERA_NEAR, CAMERA_FAR);
    this.camera.up.copy(INITIAL_CAMERA_UP);
    // Diagonal start view: looking at origin from +X, -Y, slightly above the
    // plate, so the user immediately sees all three printer axes.
    this.camera.position.copy(INITIAL_CAMERA_POSITION);
    this.camera.lookAt(INITIAL_CAMERA_TARGET);

    this.renderer = new WebGLRenderer({
      antialias: true,
      alpha: true,
      // Logarithmic depth buffer keeps depth precision usable across the
      // very large near→far range we use (so nothing clips when dollying
      // out or looking up at the build plate from below).
      logarithmicDepthBuffer: true,
      powerPreference: 'high-performance',
    });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.setSize(clientWidth, clientHeight, false);
    host.appendChild(this.renderer.domElement);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    // Punch up the wheel/middle-button dolly speed so a single scroll tick
    // covers noticeably more distance — the default (1.0) feels sluggish
    // given the very large camera range we expose.
    this.controls.zoomSpeed = 2.5;
    // Bound the camera-to-target distance to a sane range. Without an
    // explicit `minDistance`, OrbitControls allows the offset to shrink
    // toward zero asymptotically — which manifests as the autoscroll-zoom
    // visually freezing while still consuming the gesture (each frame the
    // step becomes too small to perceive but never zero). A small positive
    // floor lets the existing clamp in `applyAutoscrollZoom` actually halt
    // motion at the limit. The ceiling matches `CAMERA_FAR` headroom.
    this.controls.minDistance = 1;
    this.controls.maxDistance = 100_000;
    this.installAutoscrollZoom();
    this.scene.add(new AmbientLight(0xffffff, 0.55));
    const dir = new DirectionalLight(0xffffff, 0.9);
    dir.position.set(200, 300, 400);
    this.scene.add(dir);

    this.grid = this.createGrid(10);
    this.scene.add(this.grid);
    this.currentGridSpacingMm = 0; // force first updateAdaptiveGrid to (re)build

    // Thick RGB axes at the build-plate origin. Built from thin BoxGeometry
    // boxes (rather than AxesHelper's GL lines) so they have real volume,
    // can be made visibly thick, and depth-test correctly against the
    // model — letting opaque objects above the bed naturally occlude them.
    this.scene.add(buildAxesGizmo(40, 0.6));

    this.resizeObserver = new ResizeObserver(() => this.handleResize());
    this.resizeObserver.observe(host);

    // Re-read the grid colour from CSS whenever the theme class on <html>
    // changes (AppTheme toggles `html.dark`), so the grid always matches
    // the current `--color-border` token.
    this.themeObserver = new MutationObserver(() => this.refreshGridColor());
    this.themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'style'],
    });

    this.tick();
  }

  /**
   * Update the build-plate dimensions and offset. Rebuilds the grid using
   * the current zoom-driven spacing so callers don't have to think about
   * cache invalidation.
   */
  setPrintArea(config: PrintAreaConfig): void {
    this.printArea = { ...config };
    // Reset the cached spacing so updateAdaptiveGrid is guaranteed to
    // rebuild on the next tick (extent depends on printArea, not just spacing).
    const spacing = this.currentGridSpacingMm > 0 ? this.currentGridSpacingMm : 10;
    this.currentGridSpacingMm = 0;
    const replacement = this.createGrid(spacing);
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    this.scene.add(replacement);
    this.grid = replacement;
  }

  /** Remove every child of {@link contentRoot} and dispose its GPU resources. */
  clearContent(): void {
    for (let i = this.contentRoot.children.length - 1; i >= 0; i--) {
      const child = this.contentRoot.children[i];
      this.contentRoot.remove(child);
      disposeObject(child);
    }
  }

  /** Re-frame the camera so the whole content fits comfortably in view. */
  fitToContent(padding = DEFAULT_FIT_PADDING): void {
    const sphere = this.contentBoundingSphere();
    if (!sphere) {
      return;
    }
    const fovRad = (this.camera.fov * Math.PI) / 180;
    const distance = (sphere.radius * padding) / Math.sin(fovRad / 2);
    this.camera.position.copy(sphere.center).addScaledVector(DEFAULT_VIEW_DIR, distance);
    this.controls.target.copy(sphere.center);
    this.updateNearFar(distance, sphere.radius);
    this.camera.updateProjectionMatrix();
    this.controls.update();
  }

  /**
   * Switch to one of the named view presets with a smooth camera animation.
   * Top / Front are rendered as a near-zero-FOV perspective view at a far
   * distance, which is visually equivalent to an orthographic projection but
   * lets us tween FOV / position continuously between presets.
   */
  setView(view: ViewerView): void {
    if (view === this.currentView && !this.animation) {
      return;
    }
    this.currentView = view;
    this.animateToView(view);
  }

  /** Reset the camera to the default 3D framing using a smooth animation. */
  resetView(): void {
    this.currentView = '3D';
    this.animateToPose({
      position: INITIAL_CAMERA_POSITION.clone(),
      target: INITIAL_CAMERA_TARGET.clone(),
      up: INITIAL_CAMERA_UP.clone(),
      fov: PERSPECTIVE_FOV,
    });
  }

  /**
   * Animate the camera to a specific look direction (unit vector from the
   * controls target toward the camera) while preserving the current target
   * and camera distance. Used by the viewport-cube gizmo.
   */
  animateToDirection(direction: Vector3, up: Vector3): void {
    const target = this.controls.target.clone();
    const distance = Math.max(this.camera.position.distanceTo(target), 1);
    const dir = direction.clone().normalize();
    this.animateToPose({
      position: target.clone().addScaledVector(dir, distance),
      target,
      up: up.clone().normalize(),
      fov: this.camera.fov,
    });
  }

  /**
   * Apply an incremental orbit (in radians) around the controls target,
   *
   * The rotation is screen-relative: `azimuth` rotates around the camera's
   * current up axis and `polar` rotates around the camera's right axis.
   * This is what the user intuitively expects ("drag right ⇒ scene rotates
   * right on screen") regardless of which preset (Top / Front / 3D) the
   * camera was last in. Any in-flight animation is cancelled.
   */
  orbitBy(azimuth: number, polar: number): void {
    this.animation = null;
    this.controls.enabled = true;
    const target = this.controls.target;
    const offset = this.camera.position.clone().sub(target);
    const up = this.camera.up.clone().normalize();

    // Right axis = (target → camera) × up. If they happen to be parallel
    // (degenerate), fall back to world-X so the polar rotation still kicks
    // the camera off the pole.
    let right = new Vector3().crossVectors(offset, up);
    if (right.lengthSq() < 1e-6) {
      right.set(1, 0, 0);
    } else {
      right.normalize();
    }

    if (azimuth !== 0) {
      offset.applyAxisAngle(up, -azimuth);
      right.applyAxisAngle(up, -azimuth).normalize();
    }
    if (polar !== 0) {
      const rotatedOffset = offset.clone().applyAxisAngle(right, -polar);
      const rotatedUp = up.clone().applyAxisAngle(right, -polar);
      offset.copy(rotatedOffset);
      up.copy(rotatedUp);
    }

    this.camera.position.copy(target).add(offset);
    this.camera.up.copy(up).normalize();
    this.camera.lookAt(target);
    this.controls.update();
  }

  /** Configure pointer interaction behaviour for the OrbitControls. */
  setCursorMode(mode: ViewerCursorMode): void {
    const c = this.controls;
    c.enableRotate = true;
    c.enablePan = true;
    c.enableZoom = true;
    // The middle mouse button is reserved for our custom Windows-style
    // autoscroll zoom (see installAutoscrollZoom). Disabling it on
    // OrbitControls prevents drag-to-dolly from fighting the autoscroll.
    const MIDDLE = null as unknown as MOUSE;
    switch (mode) {
      case 'orbit':
      case 'rotate':
        c.mouseButtons = { LEFT: MOUSE.ROTATE, MIDDLE, RIGHT: MOUSE.PAN };
        break;
      case 'pan':
        c.mouseButtons = { LEFT: MOUSE.PAN, MIDDLE, RIGHT: MOUSE.ROTATE };
        break;
      case 'zoom':
        c.mouseButtons = { LEFT: MOUSE.DOLLY, MIDDLE, RIGHT: MOUSE.PAN };
        break;
      case 'pullToSurface':
        // Placeholder: surface-pulling is not implemented yet, so we simply
        // freeze the controls to make the active mode visually distinct.
        c.enableRotate = false;
        c.enablePan = false;
        c.enableZoom = false;
        break;
    }
  }

  dispose(): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    cancelAnimationFrame(this.rafHandle);
    this.resizeObserver.disconnect();
    this.themeObserver.disconnect();
    this.uninstallAutoscrollZoom();
    this.clearContent();
    this.controls.dispose();
    this.renderer.dispose();
    if (this.renderer.domElement.parentElement === this.host) {
      this.host.removeChild(this.renderer.domElement);
    }
  }

  private tick = (): void => {
    if (this.disposed) {
      return;
    }
    this.rafHandle = requestAnimationFrame(this.tick);
    const now = performance.now();
    const dt = this.lastFrameTime === 0 ? 0 : Math.min(0.1, (now - this.lastFrameTime) / 1000);
    this.lastFrameTime = now;
    if (this.animation) {
      this.advanceAnimation();
    } else {
      if (this.autoscroll) {
        this.applyAutoscrollZoom(dt);
      }
      this.controls.update();
    }
    this.updateAdaptiveGrid();
    this.updateGridFade();
    this.renderer.render(this.scene, this.camera);
    this.publishCameraState();
  };

  /** Push the camera's live direction/up to the optional sink. */
  private publishCameraState(): void {
    if (!this.cameraStateSink) {
      return;
    }
    const offset = this.camera.position.clone().sub(this.controls.target);
    if (offset.lengthSq() < 1e-6) {
      offset.copy(DEFAULT_VIEW_DIR);
    }
    this.cameraStateSink(offset.normalize(), this.camera.up.clone().normalize());
  }

  private handleResize(): void {
    const { clientWidth, clientHeight } = this.sizeOf(this.host);
    if (clientWidth === 0 || clientHeight === 0) {
      return;
    }
    this.camera.aspect = clientWidth / clientHeight;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(clientWidth, clientHeight, false);
  }

  private sizeOf(el: HTMLElement): { clientWidth: number; clientHeight: number } {
    return {
      clientWidth: Math.max(el.clientWidth, 1),
      clientHeight: Math.max(el.clientHeight, 1),
    };
  }

  /**
   * Build a two-tier build-plate grid (minor + major + outline) bounded to
   * the current {@link printArea}. Lines lie in the world XY plane and the
   * grid's lower-left corner sits at (movableAreaX, movableAreaY); the
   * gizmo at the world origin is intentionally independent.
   */
  private createGrid(spacingMm: number): Group {
    const color = readBorderColor();
    const group = new Group();
    group.renderOrder = 0;

    const materials: { material: LineBasicMaterial; baseOpacity: number }[] = [];
    const { movableAreaX, movableAreaY, printableAreaWidth, printableAreaHeight } = this.printArea;

    // Build minor + major line sets. Lines are placed at integer multiples
    // of `spacingMm` measured from the bed's lower-left corner, so the
    // pattern stays aligned with the bed regardless of where it sits in
    // machine space (negative or otherwise).
    const { minorPositions, majorPositions } = buildBedGridPositions(
      printableAreaWidth,
      printableAreaHeight,
      spacingMm,
      MAJOR_EVERY,
    );
    const offset = { x: movableAreaX, y: movableAreaY };

    const minor = makeLineSegments(minorPositions, offset, color, MINOR_OPACITY, materials);
    const major = makeLineSegments(majorPositions, offset, color, MAJOR_OPACITY, materials);
    major.renderOrder = 1;

    // Always-visible bed outline so the printable area is unambiguous even
    // when the spacing has snapped to a value that misses one of the edges.
    const outlinePositions = buildBedOutlinePositions(printableAreaWidth, printableAreaHeight);
    const outline = makeLineSegments(
      outlinePositions,
      offset,
      color,
      BED_OUTLINE_OPACITY,
      materials,
    );
    outline.renderOrder = 2;

    group.add(minor);
    group.add(major);
    group.add(outline);
    this.currentGridSpacingMm = spacingMm;
    this.gridMaterials = materials;
    return group;
  }

  /** Re-read `--color-border` and rebuild the grid so it tracks theme changes. */
  private refreshGridColor(): void {
    // GridHelper bakes its two colours into per-vertex colour attributes on
    // its geometry, so a live `material.color.set(...)` has no effect.
    // Cheapest correct fix: swap the helper for a freshly-built one.
    const replacement = this.createGrid(this.currentGridSpacingMm || 10);
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    this.scene.add(replacement);
    this.grid = replacement;
  }

  /**
   * Pick a minor-cell spacing such that one cell projects to roughly
   * {@link TARGET_MINOR_PIXELS} on screen, snap it to the nearest power of
   * 10 in [{@link GRID_MIN_SPACING_MM}, {@link GRID_MAX_SPACING_MM}], and
   * rebuild the grid only when the snapped level actually changes. The
   * grid extent itself stays fixed to the configured print area — only
   * the subdivision granularity reacts to the camera zoom.
   */
  private updateAdaptiveGrid(): void {
    const distance = this.camera.position.distanceTo(this.controls.target);
    if (!Number.isFinite(distance) || distance <= 0) {
      return;
    }
    const viewportHeight = Math.max(this.renderer.domElement.clientHeight, 1);
    const fovRad = (this.camera.fov * Math.PI) / 180;
    // World units per screen pixel at the controls target.
    const worldPerPixel = (2 * Math.tan(fovRad / 2) * distance) / viewportHeight;
    const desiredSpacing = worldPerPixel * TARGET_MINOR_PIXELS;
    const snapped = snapToPowerOfTen(desiredSpacing, GRID_MIN_SPACING_MM, GRID_MAX_SPACING_MM);
    if (snapped === this.currentGridSpacingMm) {
      return;
    }
    const replacement = this.createGrid(snapped);
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    this.scene.add(replacement);
    this.grid = replacement;
  }

  /**
   * Fade the build-plate grid based on the viewing angle. When the camera
   * is grazing the XY plane (i.e. looking edge-on “into” it), the grid
   * collapses visually to a single hard line which is distracting; fading
   * it out below {@link GRID_FADE_HIDE} entirely removes that artefact.
   */
  private updateGridFade(): void {
    if (this.gridMaterials.length === 0) {
      return;
    }
    const viewDir = this.camera.position.clone().sub(this.controls.target);
    const len = viewDir.length();
    if (len < 1e-6) {
      return;
    }
    viewDir.divideScalar(len);
    // Grid lies in the XY plane; its normal is world +Z. |cosθ| is the
    // absolute Z component of the unit view direction.
    const cosAngle = Math.abs(viewDir.z);
    const t = clamp01((cosAngle - GRID_FADE_HIDE) / (GRID_FADE_FULL - GRID_FADE_HIDE));
    // Smoothstep for a softer ramp at both ends of the fade window.
    const fade = t * t * (3 - 2 * t);
    for (const entry of this.gridMaterials) {
      const target = entry.baseOpacity * fade;
      if (entry.material.opacity !== target) {
        entry.material.opacity = target;
        entry.material.visible = target > 0.001;
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Autoscroll-style middle-button zoom
  // ---------------------------------------------------------------------------

  /**
   * Install the Windows-autoscroll-style middle-button zoom on the
   * renderer's canvas. While the middle button is held, vertical cursor
   * offset from the press anchor accelerates a continuous dolly toward
   * (cursor up) or away from (cursor down) the orbit target. Releasing
   * the button \u2014 or any other mouse button being pressed \u2014 ends the gesture.
   */
  private installAutoscrollZoom(): void {
    const el = this.renderer.domElement;
    el.addEventListener('pointerdown', this.onAutoscrollPointerDown);
    el.addEventListener('pointermove', this.onAutoscrollPointerMove);
    el.addEventListener('pointerup', this.onAutoscrollPointerUp);
    el.addEventListener('pointercancel', this.onAutoscrollPointerUp);
    el.addEventListener('contextmenu', this.onAutoscrollContextMenu);
    el.addEventListener('auxclick', this.onAutoscrollAuxClick);
  }

  private uninstallAutoscrollZoom(): void {
    const el = this.renderer.domElement;
    el.removeEventListener('pointerdown', this.onAutoscrollPointerDown);
    el.removeEventListener('pointermove', this.onAutoscrollPointerMove);
    el.removeEventListener('pointerup', this.onAutoscrollPointerUp);
    el.removeEventListener('pointercancel', this.onAutoscrollPointerUp);
    el.removeEventListener('contextmenu', this.onAutoscrollContextMenu);
    el.removeEventListener('auxclick', this.onAutoscrollAuxClick);
  }

  private onAutoscrollPointerDown = (event: PointerEvent): void => {
    if (event.button !== 1) {
      return;
    }
    // Stop OrbitControls (or anything else) from also reacting to this press.
    event.preventDefault();
    event.stopPropagation();
    const el = this.renderer.domElement;
    el.setPointerCapture(event.pointerId);
    el.style.cursor = 'ns-resize';
    this.autoscroll = {
      pointerId: event.pointerId,
      anchorY: event.clientY,
      currentY: event.clientY,
    };
  };

  private onAutoscrollPointerMove = (event: PointerEvent): void => {
    if (!this.autoscroll || event.pointerId !== this.autoscroll.pointerId) {
      return;
    }
    this.autoscroll.currentY = event.clientY;
  };

  private onAutoscrollPointerUp = (event: PointerEvent): void => {
    if (!this.autoscroll || event.pointerId !== this.autoscroll.pointerId) {
      return;
    }
    const el = this.renderer.domElement;
    if (el.hasPointerCapture(event.pointerId)) {
      el.releasePointerCapture(event.pointerId);
    }
    el.style.cursor = '';
    this.autoscroll = null;
  };

  private onAutoscrollContextMenu = (event: Event): void => {
    // Some browsers fire contextmenu on middle-click depending on settings;
    // suppress it while we own the gesture so it doesn't steal focus.
    if (this.autoscroll) {
      event.preventDefault();
    }
  };

  private onAutoscrollAuxClick = (event: MouseEvent): void => {
    if (event.button === 1) {
      // Middle-button auxclick fires on release; we already handle release
      // via pointerup, so just suppress the default browser behaviour
      // (which on many sites would trigger autoscroll mode itself).
      event.preventDefault();
    }
  };

  /**
   * Apply the per-frame dolly while the middle button is held. The cursor's
   * vertical offset from the press anchor maps to an exponential dolly
   * factor so movement feels uniform on a log scale (a sensible match for
   * camera distance, which we already pick on a power-of-10 grid).
   */
  private applyAutoscrollZoom(dt: number): void {
    const state = this.autoscroll;
    if (!state || dt <= 0) {
      return;
    }
    const offsetPx = state.anchorY - state.currentY; // up = positive => zoom in
    const beyondDeadzone =
      Math.sign(offsetPx) * Math.max(0, Math.abs(offsetPx) - AUTOSCROLL_DEAD_ZONE_PX);
    if (beyondDeadzone === 0) {
      return;
    }
    // Exponential dolly: scale = exp(-rate * dt). Negative offset (cursor
    // below anchor) yields scale > 1 (camera retreats); positive offset
    // yields scale < 1 (camera approaches the target). The rate grows
    // super-linearly with cursor distance so the further you push the
    // pointer from the anchor, the faster the camera accelerates.
    const accel = Math.pow(
      Math.abs(beyondDeadzone) / AUTOSCROLL_ACCEL_REF_PX,
      AUTOSCROLL_ACCEL_EXPONENT - 1,
    );
    const rate = beyondDeadzone * AUTOSCROLL_SPEED_PER_PX * accel;
    let scale = Math.exp(-rate * dt);
    // Clamp to sane per-frame bounds so a sudden tab-switch hiccup or huge
    // dt can't teleport the camera through the model.
    scale = Math.min(
      AUTOSCROLL_MAX_FACTOR_PER_FRAME,
      Math.max(1 / AUTOSCROLL_MAX_FACTOR_PER_FRAME, scale),
    );
    const target = this.controls.target;
    const offset = this.camera.position.clone().sub(target);
    offset.multiplyScalar(scale);
    // Respect OrbitControls' configured min/max distance bounds.
    const len = offset.length();
    const minD = (this.controls as unknown as { minDistance?: number }).minDistance ?? 0;
    const maxD = (this.controls as unknown as { maxDistance?: number }).maxDistance ?? Infinity;
    if (len < minD && len > 0) {
      offset.multiplyScalar(minD / len);
    } else if (len > maxD) {
      offset.multiplyScalar(maxD / len);
    }
    this.camera.position.copy(target).add(offset);
  }

  // ---------------------------------------------------------------------------
  // View-preset animation
  // ---------------------------------------------------------------------------

  private contentBoundingSphere(): Sphere | null {
    const box = new Box3().setFromObject(this.contentRoot);
    if (box.isEmpty()) {
      // Fall back to the configured printable area so the camera still has
      // something sensible to frame when no model is loaded.
      const { movableAreaX, movableAreaY, printableAreaWidth, printableAreaHeight } =
        this.printArea;
      box.set(
        new Vector3(movableAreaX, movableAreaY, 0),
        new Vector3(movableAreaX + printableAreaWidth, movableAreaY + printableAreaHeight, 0),
      );
    }
    const sphere = new Sphere();
    box.getBoundingSphere(sphere);
    if (sphere.radius <= 0 || !Number.isFinite(sphere.radius)) {
      return null;
    }
    sphere.radius = Math.max(sphere.radius, 1);
    return sphere;
  }

  /**
   * Compute target framing for the requested view preset — a unit direction
   * from target to camera, the desired FOV, target point and up vector. The
   * camera distance is derived per-frame from FOV and bounding radius so the
   * on-screen framing stays stable while FOV tweens.
   */
  private planView(view: ViewerView): {
    dir: Vector3;
    fov: number;
    target: Vector3;
    up: Vector3;
  } {
    const sphere = this.contentBoundingSphere() ?? new Sphere(new Vector3(), 100);
    switch (view) {
      case 'Top':
        return {
          dir: new Vector3(0, 0, 1),
          fov: ORTHO_FOV,
          target: sphere.center.clone(),
          up: new Vector3(0, 1, 0),
        };
      case 'Front':
        return {
          dir: new Vector3(0, -1, 0),
          fov: ORTHO_FOV,
          target: sphere.center.clone(),
          up: new Vector3(0, 0, 1),
        };
      case '3D':
      default:
        return {
          dir: DEFAULT_VIEW_DIR.clone(),
          fov: PERSPECTIVE_FOV,
          target: sphere.center.clone(),
          up: new Vector3(0, 0, 1),
        };
    }
  }

  private animateToView(view: ViewerView): void {
    const plan = this.planView(view);
    const sphere = this.contentBoundingSphere() ?? new Sphere(new Vector3(), 100);
    // Pick a target distance that frames the bounding sphere with padding
    // at the destination FOV. (For ORTHO_FOV this naturally produces a very
    // large distance — the basis of our "fake ortho" effect.)
    const toFovRad = (plan.fov * Math.PI) / 180;
    const toDistance = (sphere.radius * DEFAULT_FIT_PADDING) / Math.sin(toFovRad / 2);

    this.startAnimation({
      toDir: plan.dir,
      toFov: plan.fov,
      toTarget: plan.target,
      toUp: plan.up,
      toDistance,
    });
  }

  /**
   * Animate to an absolute camera pose (position / target / up / fov). Used
   * by {@link resetView} to restore the exact initial camera state regardless
   * of what content is currently loaded.
   */
  private animateToPose(pose: {
    position: Vector3;
    target: Vector3;
    up: Vector3;
    fov: number;
  }): void {
    const offset = pose.position.clone().sub(pose.target);
    const toDistance = offset.length();
    const toDir = toDistance > 1e-6 ? offset.divideScalar(toDistance) : DEFAULT_VIEW_DIR.clone();

    this.startAnimation({
      toDir,
      toFov: pose.fov,
      toTarget: pose.target,
      toUp: pose.up,
      toDistance,
    });
  }

  private startAnimation(spec: {
    toDir: Vector3;
    toFov: number;
    toTarget: Vector3;
    toUp: Vector3;
    toDistance: number;
  }): void {
    const fromTarget = this.controls.target.clone();
    // Derive the current camera direction from its position relative to the
    // controls target, so the animation always begins from the user's actual
    // current viewpoint (which may have been freely orbited).
    const offset = this.camera.position.clone().sub(fromTarget);
    const fromDistance = offset.length();
    const fromDir =
      fromDistance > 1e-6 ? offset.clone().divideScalar(fromDistance) : DEFAULT_VIEW_DIR.clone();
    const fromUp = this.camera.up.clone().normalize();

    // Disable controls during the transition so user input doesn't fight
    // the tween. They are re-enabled when the animation finishes.
    this.controls.enabled = false;

    this.animation = {
      startTime: performance.now(),
      duration: VIEW_TRANSITION_MS,
      fromDir,
      toDir: spec.toDir.clone().normalize(),
      fromFov: this.camera.fov,
      toFov: spec.toFov,
      fromTarget,
      toTarget: spec.toTarget.clone(),
      fromUp,
      toUp: spec.toUp.clone().normalize(),
      fromDistance,
      toDistance: spec.toDistance,
    };
  }

  private advanceAnimation(): void {
    const anim = this.animation;
    if (!anim) {
      return;
    }
    const now = performance.now();
    const t = Math.min(1, (now - anim.startTime) / anim.duration);
    const eased = easeInOutCubic(t);

    // Interpolated direction (kept unit-length so distance is purely a
    // function of FOV and bounding radius).
    const dir = anim.fromDir.clone().lerp(anim.toDir, eased);
    if (dir.lengthSq() < 1e-6) {
      dir.copy(anim.toDir);
    } else {
      dir.normalize();
    }

    // Interpolated up vector — same lerp-then-normalize trick is fine here
    // because all our up vectors are axis-aligned and never antiparallel.
    const up = anim.fromUp.clone().lerp(anim.toUp, eased);
    if (up.lengthSq() < 1e-6) {
      up.copy(anim.toUp);
    } else {
      up.normalize();
    }

    // Tween FOV and distance independently. Distance is interpolated
    // directly so an absolute pose (e.g. resetView) lands at the exact
    // requested position, while still keeping the on-screen size of the
    // content sensible for view-preset transitions.
    const fov = lerp(anim.fromFov, anim.toFov, eased);
    const distance = lerp(anim.fromDistance, anim.toDistance, eased);

    const target = anim.fromTarget.clone().lerp(anim.toTarget, eased);

    this.camera.up.copy(up);
    this.camera.fov = fov;
    this.camera.position.copy(target).addScaledVector(dir, distance);
    this.controls.target.copy(target);
    this.updateNearFar(distance, Math.max(distance * 0.5, 1));
    this.camera.lookAt(target);
    this.camera.updateProjectionMatrix();

    if (t >= 1) {
      this.controls.enabled = true;
      this.controls.update();
      this.animation = null;
    }
  }

  /**
   * Pin the camera frustum to wide fixed bounds. The renderer is configured
   * with a logarithmic depth buffer so this huge near→far range still has
   * usable depth precision — and nothing is ever clipped, regardless of how
   * far the user dollies out or which angle they look from. The arguments
   * are accepted for API compatibility with the previous implementation.
   */
  private updateNearFar(_distance: number, _radius: number): void {
    if (this.camera.near !== CAMERA_NEAR || this.camera.far !== CAMERA_FAR) {
      this.camera.near = CAMERA_NEAR;
      this.camera.far = CAMERA_FAR;
      this.camera.updateProjectionMatrix();
    }
  }
}

interface CameraAnimation {
  startTime: number;
  duration: number;
  fromDir: Vector3;
  toDir: Vector3;
  fromFov: number;
  toFov: number;
  fromTarget: Vector3;
  toTarget: Vector3;
  fromUp: Vector3;
  toUp: Vector3;
  fromDistance: number;
  toDistance: number;
}

interface AutoscrollState {
  pointerId: number;
  anchorY: number;
  currentY: number;
}

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/**
 * Build minor + major grid line endpoints for a `width × height` bed in the
 * XY plane, with lines spaced `spacingMm` apart aligned to the bed's
 * lower-left corner (which is treated as local (0, 0)). Lines that fall on
 * a multiple of `majorEvery` cells are emitted into the major set instead
 * of the minor set, so the two are mutually exclusive and can be rendered
 * with different opacities without overdraw.
 *
 * Returned arrays are flat XYZ triplets ready for a `BufferGeometry`'s
 * position attribute (every six floats = one line segment).
 */
function buildBedGridPositions(
  width: number,
  height: number,
  spacingMm: number,
  majorEvery: number,
): { minorPositions: Float32Array; majorPositions: Float32Array } {
  const minor: number[] = [];
  const major: number[] = [];
  if (!(width > 0) || !(height > 0) || !(spacingMm > 0)) {
    return { minorPositions: new Float32Array(0), majorPositions: new Float32Array(0) };
  }

  // Vertical lines (constant X, span Y from 0 to height).
  const xCount = Math.floor(width / spacingMm);
  for (let i = 0; i <= xCount; i++) {
    const x = i * spacingMm;
    if (x > width) {
      break;
    }
    const target = i % majorEvery === 0 ? major : minor;
    target.push(x, 0, 0, x, height, 0);
  }

  // Horizontal lines (constant Y, span X from 0 to width).
  const yCount = Math.floor(height / spacingMm);
  for (let j = 0; j <= yCount; j++) {
    const y = j * spacingMm;
    if (y > height) {
      break;
    }
    const target = j % majorEvery === 0 ? major : minor;
    target.push(0, y, 0, width, y, 0);
  }

  return {
    minorPositions: new Float32Array(minor),
    majorPositions: new Float32Array(major),
  };
}

/** Four edges of the bed rectangle as XYZ line-segment endpoints. */
function buildBedOutlinePositions(width: number, height: number): Float32Array {
  if (!(width > 0) || !(height > 0)) {
    return new Float32Array(0);
  }
  return new Float32Array([
    0,
    0,
    0,
    width,
    0,
    0,
    width,
    0,
    0,
    width,
    height,
    0,
    width,
    height,
    0,
    0,
    height,
    0,
    0,
    height,
    0,
    0,
    0,
    0,
  ]);
}

/**
 * Build a `LineSegments` mesh for the given flat XYZ position buffer with a
 * transparent overlay material. The mesh is translated by `offset` so the
 * caller can place the bed at an arbitrary (x, y) in machine coordinates
 * without having to bake the offset into every vertex.
 */
function makeLineSegments(
  positions: Float32Array,
  offset: { x: number; y: number },
  color: number,
  opacity: number,
  sink: { material: LineBasicMaterial; baseOpacity: number }[],
): LineSegments {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new Float32BufferAttribute(positions, 3));
  const material = new LineBasicMaterial({
    color: new Color(color),
    transparent: true,
    opacity,
    depthWrite: false,
  });
  sink.push({ material, baseOpacity: opacity });
  const segments = new LineSegments(geometry, material);
  segments.position.set(offset.x, offset.y, 0);
  return segments;
}

function clamp01(v: number): number {
  if (v < 0) {
    return 0;
  }
  if (v > 1) {
    return 1;
  }
  return v;
}

/**
 * Snap `value` to the nearest power of 10 within `[min, max]`. Used to lock
 * the adaptive grid's minor-cell spacing to clean millimetre/centimetre/...
 * intervals so the user always sees round numbers.
 */
function snapToPowerOfTen(value: number, min: number, max: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return min;
  }
  const exponent = Math.round(Math.log10(value));
  const snapped = Math.pow(10, exponent);
  if (snapped < min) {
    return min;
  }
  if (snapped > max) {
    return max;
  }
  return snapped;
}

function disposeObject(obj: unknown): void {
  const node = obj as {
    traverse?: (cb: (child: unknown) => void) => void;
    geometry?: { dispose?: () => void };
    material?: { dispose?: () => void } | { dispose?: () => void }[];
  };
  if (typeof node.traverse === 'function') {
    node.traverse((child) => {
      const c = child as {
        geometry?: { dispose?: () => void };
        material?: { dispose?: () => void } | { dispose?: () => void }[];
      };
      c.geometry?.dispose?.();
      const mat = c.material;
      if (Array.isArray(mat)) {
        for (const m of mat) {
          m.dispose?.();
        }
      } else {
        mat?.dispose?.();
      }
    });
  }
}

/**
 * Read the current `--color-border` CSS custom property from `<html>` and
 * return it as a numeric Three.js colour. Falls back to a neutral grey when
 * the property isn't present (e.g. in test environments without the global
 * stylesheet loaded).
 */
function readBorderColor(): number {
  const raw = getComputedStyle(document.documentElement).getPropertyValue('--color-border').trim();
  return parseCssColor(raw) ?? 0x444444;
}

function parseCssColor(value: string): number | null {
  if (!value) {
    return null;
  }
  if (value.startsWith('#')) {
    const hex = value.slice(1);
    if (hex.length === 3) {
      const r = hex[0];
      const g = hex[1];
      const b = hex[2];
      return parseInt(`${r}${r}${g}${g}${b}${b}`, 16);
    }
    if (hex.length === 6 || hex.length === 8) {
      return parseInt(hex.slice(0, 6), 16);
    }
    return null;
  }
  // rgb()/rgba() — extract the first three numeric components.
  const match = value.match(/rgba?\(([^)]+)\)/i);
  if (match) {
    const parts = match[1].split(',').map((p) => parseFloat(p.trim()));
    if (parts.length >= 3 && parts.every((p) => !Number.isNaN(p))) {
      const [r, g, b] = parts;
      return ((r & 0xff) << 16) | ((g & 0xff) << 8) | (b & 0xff);
    }
  }
  return null;
}

/**
 * Build an RGB axes gizmo from thin BoxGeometry rods so it has real volume,
 * a controllable thickness, and can depth-test correctly against models.
 *
 * Each rod sits in the positive octant with its near corner at the world
 * origin (x, y, z all start at 0). To eliminate z-fighting where the three
 * rods would otherwise occupy the same `thickness^3` cube at the origin,
 * each rod is shortened by `thickness` and a single neutral-coloured cube
 * fills that shared corner.
 *
 * The grid sits at z = 0; the rods rest on top of it (their bases at z = 0
 * for the X/Y rods, near corner at origin for the Z rod) and use a higher
 * `renderOrder` than the grid so any sub-pixel co-planar overlap is
 * resolved deterministically. Models drawn into {@link ViewerScene.contentRoot}
 * naturally occlude the gizmo via standard depth testing.
 */
function buildAxesGizmo(length: number, thickness: number): Group {
  const group = new Group();
  group.renderOrder = 1; // above grid (renderOrder 0), below default content

  const halfT = thickness / 2;

  // Origin marker: a small neutral cube sitting in the [0, thickness]^3
  // corner so the three rods never overlap each other.
  const originGeometry = new BoxGeometry(thickness, thickness, thickness);
  const originMaterial = new MeshBasicMaterial({ color: 0xdddddd });
  const originMesh = new Mesh(originGeometry, originMaterial);
  originMesh.position.set(halfT, halfT, halfT);
  originMesh.renderOrder = 1;
  group.add(originMesh);

  const axes: Array<{ color: number; axis: 'x' | 'y' | 'z' }> = [
    { color: 0xff3344, axis: 'x' },
    { color: 0x33dd55, axis: 'y' },
    { color: 0x4488ff, axis: 'z' },
  ];

  // Each rod runs from `thickness` to `length` along its own axis (so the
  // origin cube above fills [0, thickness]) and is offset by `halfT` on the
  // two perpendicular axes so its near corner — not its centerline — sits
  // at the origin. This keeps everything in the positive octant.
  const rodLength = length - thickness;
  for (const { color, axis } of axes) {
    const dims: [number, number, number] =
      axis === 'x'
        ? [rodLength, thickness, thickness]
        : axis === 'y'
          ? [thickness, rodLength, thickness]
          : [thickness, thickness, rodLength];
    const geometry = new BoxGeometry(...dims);
    const material = new MeshBasicMaterial({ color });
    const mesh = new Mesh(geometry, material);
    mesh.position.set(halfT, halfT, halfT);
    mesh.position[axis] = thickness + rodLength / 2;
    mesh.renderOrder = 1;
    group.add(mesh);
  }

  return group;
}
