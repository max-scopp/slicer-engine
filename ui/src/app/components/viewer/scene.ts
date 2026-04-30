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
  Material,
  Mesh,
  MeshBasicMaterial,
  MOUSE,
  Object3D,
  PerspectiveCamera,
  Plane,
  Quaternion,
  Raycaster,
  Scene,
  Sphere,
  Spherical,
  TOUCH,
  Vector2,
  Vector3,
  WebGLRenderer,
} from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { PrintAreaConfig } from '../../services/print-area';
import type { ObjectMode } from '../../services/viewer-control';
import { computeSelectionCentroid, GizmoDelta, GizmoManager, raycastFace } from './gizmo';

/**
 * Callbacks invoked by {@link ViewerScene} when the user interacts with a
 * registered selectable object. The viewer wires these to the application's
 * selection store (see `PrintArea`); the scene itself knows nothing
 * about the store, only the contract.
 *
 * Object manipulation (translate / rotate / scale / pull-to-floor) is
 * routed through {@link SceneGizmoHandlers} instead — the selection
 * handlers only deal with selection itself.
 */
export interface SceneSelectionHandlers {
  /** A bare click on a selectable object — `additive` for ctrl/⌘/shift. */
  select(id: string, additive: boolean): void;
  /** Click landed on empty space (deselect). */
  clearSelection(): void;
}

/**
 * Callbacks invoked by {@link ViewerScene} during gizmo-driven object
 * manipulation. Each callback receives the **list of currently-selected**
 * object ids — the host is responsible for dispatching one WASM op per id.
 */
export interface SceneGizmoHandlers {
  /** Fired on each frame's incremental delta during a drag. */
  delta(ids: readonly string[], delta: GizmoDelta): void;
  /** Fired when the gesture finishes (pointer-up). Flush history here. */
  end(): void;
  /** Fired when a face has been picked in `pullToFloor` mode. */
  facePicked(objectId: string, faceIndex: number): void;
}

export type ViewerView = '3D' | 'Top' | 'Front';
export type ViewerCursorMode = 'orbit' | 'pan' | 'zoom';

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

/**
 * Initial camera pose, expressed relative to the centre of the printable
 * area. The camera sits at `bedCenter + INITIAL_CAMERA_OFFSET` and looks at
 * `bedCenter`, so the user always starts framed on the bed regardless of
 * where the bed is placed in machine space.
 */
const INITIAL_CAMERA_OFFSET = new Vector3(220, -240, 180);
const INITIAL_CAMERA_UP = new Vector3(0, 0, 1);

/**
 * Hard floor / ceiling on the dynamic near/far range. The renderer no
 * longer uses `logarithmicDepthBuffer` (it is prohibitively expensive on
 * iOS / Apple GPUs because it forces `gl_FragDepth` writes and disables
 * early-Z), so we instead recompute near/far every frame from the current
 * camera→target distance and a conservative scene radius. These constants
 * just clamp the result so a degenerate frame can't produce a zero / NaN
 * frustum.
 */
const CAMERA_NEAR = 0.1;
const CAMERA_FAR = 1_000_000;

/**
 * Maximum device-pixel-ratio actually pushed to the renderer. iPads and
 * recent iPhones report DPR 2–3, which combined with antialiasing can
 * quadruple or more the fragment workload for no perceptible quality gain.
 * Cap at 2× — the visible difference vs. native DPR on a Retina display is
 * negligible, but the perf / thermal headroom freed up on mobile is large.
 */
const MAX_PIXEL_RATIO = 2;

/**
 * `true` when the platform's effective DPR is high enough that hardware
 * MSAA gives essentially no perceptible benefit. On those devices we ask
 * for `antialias: false` so the GPU isn't paying for a multisample
 * resolve every frame — this is one of the largest single wins for iOS
 * Safari / iPadOS.
 */
function shouldDisableAntialias(): boolean {
  return typeof window !== 'undefined' && window.devicePixelRatio >= 2;
}

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
/** Duration (ms) of the cross-fade between old and new grid scales. */
const GRID_SCALE_TRANSITION_MS = 350;

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
 * Pixel distance the cursor must travel between pointerdown and pointermove
 * before a press on a selectable object is interpreted as a drag rather than
 * a click. Below this threshold the gesture is treated as a selection click
 * on pointerup.
 */
const SELECTION_DRAG_THRESHOLD_PX = 4;

/** Emissive tint applied to selected meshes (warm orange). */
const SELECTION_EMISSIVE = new Color(0xff8a3d);
const SELECTION_EMISSIVE_INTENSITY = 0.55;

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
 * Sentinel value used to disable a slot in {@link OrbitControls.touches}.
 * The TOUCH enum exposes ROTATE / PAN / DOLLY_PAN / DOLLY_ROTATE; assigning
 * any other numeric value causes OrbitControls' state machine to fall
 * through to the no-op `default` branch, leaving the gesture untouched
 * for our custom handler to claim. We need a numeric (not `null`) so the
 * `touches` object's typing stays valid.
 */
const TOUCH_DISABLED = -1 as unknown as TOUCH;

/**
 * Below this pinch-distance change (in CSS pixels) the dolly is suppressed,
 * to keep small finger jitter from accumulating into noticeable zoom drift
 * during a primarily-rotate-or-pan two-finger gesture.
 */
const TWO_FINGER_DOLLY_DEAD_ZONE_PX = 1.5;
/**
 * Below this twist (in radians) per move event, the roll is suppressed.
 * Typical untrained two-finger pan/zoom gestures wobble by ~0.5° between
 * frames; ignoring those keeps the horizon level unless the user actively
 * twists.
 */
const TWO_FINGER_ROLL_DEAD_ZONE_RAD = 0.01;

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
  private gridTransition: GridTransition | null = null;
  private printArea: PrintAreaConfig = { ...DEFAULT_PRINT_AREA };
  private rafHandle = 0;
  private disposed = false;
  private currentView: ViewerView = '3D';
  private animation: CameraAnimation | null = null;
  private autoscroll: AutoscrollState | null = null;
  private lastFrameTime = 0;

  /**
   * Custom orbit/pan inertia. OrbitControls' built-in `enableDamping` is
   * unsuitable here: with `dampingFactor < 1` the camera lags behind the
   * pointer during drag (perceived as "weight"), and with `dampingFactor = 1`
   * no exit velocity remains on release. Instead we keep damping disabled,
   * sample the orbit angles and pan target each `change` event during an
   * active gesture, derive smoothed angular / linear velocities, and on the
   * controls' `end` event we let those velocities coast in the render loop
   * with exponential decay until they drop below a small threshold.
   */
  private orbitInteracting = false;
  private orbitLastSampleTime = 0;
  private orbitLastAzimuth = 0;
  private orbitLastPolar = 0;
  private orbitLastTarget = new Vector3();
  /** Smoothed angular velocity (rad/s) around the controls' azimuth axis. */
  private orbitVelAzimuth = 0;
  /** Smoothed angular velocity (rad/s) around the controls' polar axis. */
  private orbitVelPolar = 0;
  /** Smoothed pan velocity (world units / s) of the orbit target. */
  private orbitVelTarget = new Vector3();

  /**
   * Optional sink invoked at the end of every render frame with the live
   * camera direction (target→camera, normalised) and up vector. Used by the
   * viewport-cube gizmo to mirror the main camera's orientation.
   */
  cameraStateSink: ((direction: Vector3, up: Vector3, fov: number) => void) | null = null;

  /**
   * Optional sink invoked periodically with the smoothed frames-per-second
   * and average frame delay (ms) of the render loop. Called approximately once per second.
   */
  fpsSink: ((fps: number, delayMs: number) => void) | null = null;

  /** Exponentially-smoothed FPS estimate. */
  private smoothedFps = 0;
  /** Exponentially-smoothed frame delay in milliseconds. */
  private smoothedDelayMs = 0;
  /** Timestamp of the last time {@link fpsSink} was called. */
  private lastFpsPublishTime = 0;

  /**
   * Hook for selection / drag interactions. The viewer assigns this once it
   * has wired up the print-area service. While `null`, raycast hits on
   * selectable objects are ignored and OrbitControls receives every gesture.
   */
  selectionHandlers: SceneSelectionHandlers | null = null;

  /**
   * Hook for gizmo-driven object manipulation (translate / rotate / scale)
   * and the `pullToFloor` face-pick workflow. Set by the viewer.
   */
  gizmoHandlers: SceneGizmoHandlers | null = null;

  // --- Selection / drag state --------------------------------------------
  private currentCursorMode: ViewerCursorMode = 'orbit';
  /** Object-manipulation mode driving the gizmo and pull-to-floor pick. */
  private currentObjectMode: ObjectMode = 'none';
  /** On-canvas transform gizmo manager (created in the constructor). */
  private readonly gizmo: GizmoManager;
  private readonly selectables = new Map<string, Object3D>();
  private currentSelectedIds: ReadonlySet<string> = new Set();
  /** Per-mesh original emissive snapshots so highlight is reversible. */
  private readonly originalEmissive = new Map<Mesh, { color: Color; intensity: number }[]>();
  private readonly raycaster = new Raycaster();
  private readonly ndcScratch = new Vector2();
  /** Overlay triangles drawn on the coplanar face group the user hovers in pull-to-floor mode. */
  private readonly faceHighlight: Mesh = (() => {
    const geo = new BufferGeometry();
    // Start with capacity for 1 triangle; resized as needed.
    geo.setAttribute('position', new Float32BufferAttribute(new Float32Array(9), 3));
    const mat = new MeshBasicMaterial({
      color: 0x2ecc71,
      transparent: true,
      opacity: 0.55,
      depthTest: false,
      depthWrite: false,
      side: 2, // THREE.DoubleSide — pull-to-floor cares about either winding
    });
    const m = new Mesh(geo, mat);
    m.renderOrder = 998;
    m.visible = false;
    m.matrixAutoUpdate = false;
    return m;
  })();
  private pressState: SelectionPressState | null = null;
  /**
   * Tracks a left-button press that landed on empty space (no selectable
   * raycast hit). If the gesture stays under {@link SELECTION_DRAG_THRESHOLD_PX}
   * by pointerup, it is treated as a deselect-click; otherwise it was the
   * start of a camera orbit and is ignored. We deliberately do not
   * preventDefault on these events so OrbitControls keeps handling the orbit.
   */
  private emptyPressState: { pointerId: number; downX: number; downY: number } | null = null;

  constructor(host: HTMLElement, initialPrintArea?: PrintAreaConfig) {
    this.host = host;
    if (initialPrintArea) {
      this.printArea = { ...initialPrintArea };
    }

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
    // Diagonal start view: looking at the centre of the bed from a fixed
    // diagonal offset, so the user immediately sees the printable area
    // (not the machine origin / gizmo).
    const initialPose = this.initialPoseForBed();
    this.camera.position.copy(initialPose.position);
    this.camera.lookAt(initialPose.target);

    this.renderer = new WebGLRenderer({
      // Skip MSAA on Retina-class displays — at DPR ≥ 2 the resolve cost
      // outweighs the visual difference, especially on iOS where the
      // tile-based GPU is fragment-bound.
      antialias: !shouldDisableAntialias(),
      alpha: true,
      // `logarithmicDepthBuffer` is intentionally NOT enabled. It writes
      // `gl_FragDepth` from every fragment shader, which on iOS / Apple
      // GPUs disables early-Z and roughly halves fill-rate. We instead
      // tighten near/far around the current camera distance every frame —
      // see {@link updateNearFar}.
      powerPreference: 'high-performance',
    });
    // Cap pixel ratio to keep iPads / high-DPR phones from rendering at
    // 4×–9× pixel area for a marginal quality gain.
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, MAX_PIXEL_RATIO));
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.setSize(clientWidth, clientHeight);
    // Suppress the browser's default touch gestures (page scroll, pinch-zoom,
    // double-tap zoom) on the canvas itself. Without this, OrbitControls'
    // pinch-zoom and one-finger rotate fight the browser's own gesture
    // recognisers — gestures register intermittently, scroll the page, or
    // trigger the OS pull-to-refresh, all of which feel "off" on touch.
    this.renderer.domElement.style.touchAction = 'none';
    host.appendChild(this.renderer.domElement);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    // Built-in damping is disabled — see the field comment on
    // `orbitInteracting` for the rationale. We implement inertia ourselves
    // by sampling the controls' state during interaction and coasting it on
    // release.
    this.controls.enableDamping = false;
    // Defaults preserve the original desktop feel:
    //   - zoomToCursor = false   (wheel/dolly along the camera→target axis)
    //   - screenSpacePanning = true (right-drag pan in screen space)
    // For touch, both `zoomToCursor` and a recomputed pan basis are
    // toggled on per-gesture in {@link installTouchOrbitTuning} so that
    // pinch-zoom converges on the finger and two-finger pan stays
    // proportional to screen pixels — without disturbing mouse behaviour.
    this.controls.zoomToCursor = false;
    this.controls.screenSpacePanning = true;
    this.installOrbitInertia();
    this.installTouchOrbitTuning();
    this.installCustomTwoFingerControls();
    // Anchor the orbit target at the bed centre so dragging rotates around
    // the printable area instead of the machine origin / gizmo.
    this.controls.target.copy(initialPose.target);
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
    this.installSelectionHandlers();
    this.gizmo = new GizmoManager(this.scene, this.camera, this.renderer);
    this.gizmo.onDragStart = () => {
      // Park OrbitControls so the gizmo gets exclusive pointer ownership.
      this.controls.enabled = false;
    };
    this.gizmo.onDelta = (delta) => {
      const ids = Array.from(this.currentSelectedIds);
      this.gizmoHandlers?.delta(ids, delta);
    };
    this.gizmo.onDragEnd = () => {
      this.controls.enabled = true;
      this.gizmoHandlers?.end();
    };
    this.scene.add(this.faceHighlight);
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
    // Drop any in-flight selection/drag bookkeeping first — the meshes it
    // pointed at are about to be disposed.
    this.cancelActiveDrag();
    this.clearSelectables();
    for (let i = this.contentRoot.children.length - 1; i >= 0; i--) {
      const child = this.contentRoot.children[i];
      this.contentRoot.remove(child);
      disposeObject(child);
    }
  }

  // ---------------------------------------------------------------------------
  // Selection / drag — public API consumed by the viewer component
  // ---------------------------------------------------------------------------

  /**
   * Mark `object` as selectable under the given id. The id is also stamped
   * onto `object.userData.selectableId` so deeply-nested raycast hits can be
   * resolved back to the registered root via parent walking.
   */
  registerSelectable(id: string, object: Object3D): void {
    object.userData['selectableId'] = id;
    this.selectables.set(id, object);
  }

  /** Stop tracking the given id and (if applicable) drop its highlight. */
  unregisterSelectable(id: string): void {
    const obj = this.selectables.get(id);
    if (!obj) {
      return;
    }
    if (this.currentSelectedIds.has(id)) {
      this.applyHighlight(obj, false);
    }
    delete obj.userData['selectableId'];
    this.selectables.delete(id);
  }

  /** Drop every registered selectable (called from {@link clearContent}). */
  clearSelectables(): void {
    for (const obj of this.selectables.values()) {
      this.applyHighlight(obj, false);
      delete obj.userData['selectableId'];
    }
    this.selectables.clear();
    this.currentSelectedIds = new Set();
    this.originalEmissive.clear();
  }

  /** Mirror the application's selection state into the scene. */
  setSelectedIds(ids: ReadonlySet<string>): void {
    // Diff old vs new so we only touch materials whose state actually flipped.
    for (const id of this.currentSelectedIds) {
      if (ids.has(id)) {
        continue;
      }
      const obj = this.selectables.get(id);
      if (obj) {
        this.applyHighlight(obj, false);
      }
    }
    for (const id of ids) {
      if (this.currentSelectedIds.has(id)) {
        continue;
      }
      const obj = this.selectables.get(id);
      if (obj) {
        this.applyHighlight(obj, true);
      }
    }
    this.currentSelectedIds = ids;
    // Refresh the gizmo position whenever the selection changes \u2014 any
    // currently-active gizmo (translate/rotate/scale) follows the new
    // centroid, and a now-empty selection hides the gizmo entirely.
    this.gizmo.setCentroid(this.computeSelectionCentroid());
  }

  /**
   * World-space AABB centroid of the currently-selected objects, or `null`
   * if nothing is selected. Drives gizmo placement.
   */
  private computeSelectionCentroid(): Vector3 | null {
    if (this.currentSelectedIds.size === 0) {
      return null;
    }
    const objects: Object3D[] = [];
    for (const id of this.currentSelectedIds) {
      const obj = this.selectables.get(id);
      if (obj) {
        objects.push(obj);
      }
    }
    return computeSelectionCentroid(objects);
  }

  /**
   * Mirror the registered object's transform onto the underlying Three.js
   * node. Position is in machine-space mm; rotation is XYZ-Euler radians;
   * scale is a per-axis multiplier. Each channel is compared before the
   * write so identical values do not retrigger Three.js's matrix-dirty
   * flag.
   */
  setObjectTransform(
    id: string,
    transform: {
      position: { x: number; y: number; z: number };
      rotation: { x: number; y: number; z: number };
      scale: { x: number; y: number; z: number };
    },
  ): void {
    const obj = this.selectables.get(id);
    if (!obj) {
      return;
    }
    const { position, rotation, scale } = transform;
    if (
      obj.position.x !== position.x ||
      obj.position.y !== position.y ||
      obj.position.z !== position.z
    ) {
      obj.position.set(position.x, position.y, position.z);
    }
    if (
      obj.rotation.x !== rotation.x ||
      obj.rotation.y !== rotation.y ||
      obj.rotation.z !== rotation.z
    ) {
      obj.rotation.set(rotation.x, rotation.y, rotation.z);
    }
    if (obj.scale.x !== scale.x || obj.scale.y !== scale.y || obj.scale.z !== scale.z) {
      obj.scale.set(scale.x, scale.y, scale.z);
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
    const pose = this.initialPoseForBed();
    this.animateToPose({
      position: pose.position,
      target: pose.target,
      up: INITIAL_CAMERA_UP.clone(),
      fov: PERSPECTIVE_FOV,
    });
  }

  /**
   * Compute the default camera pose as a function of the current print area:
   * target = bed centre at z = 0, position = target + {@link INITIAL_CAMERA_OFFSET}.
   * Centralised so the constructor and {@link resetView} stay in lock-step.
   */
  private initialPoseForBed(): { position: Vector3; target: Vector3 } {
    const { movableAreaX, movableAreaY, printableAreaWidth, printableAreaHeight } = this.printArea;
    const target = new Vector3(
      movableAreaX + printableAreaWidth / 2,
      movableAreaY + printableAreaHeight / 2,
      0,
    );
    const position = target.clone().add(INITIAL_CAMERA_OFFSET);
    return { position, target };
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
    if (this.currentCursorMode !== mode) {
      // A mode change mid-gesture is rare but possible (e.g. a hotkey while
      // dragging). Cancel any in-flight drag so we don't leave OrbitControls
      // disabled forever.
      this.cancelActiveDrag();
    }
    this.currentCursorMode = mode;
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
        c.mouseButtons = { LEFT: MOUSE.ROTATE, MIDDLE, RIGHT: MOUSE.PAN };
        // Two-finger gestures (pinch-zoom + pan + twist-to-roll) are handled
        // entirely by {@link installCustomTwoFingerControls}; disable the
        // built-in TWO action so OrbitControls doesn't fight us.
        c.touches = { ONE: TOUCH.ROTATE, TWO: TOUCH_DISABLED };
        break;
      case 'pan':
        c.mouseButtons = { LEFT: MOUSE.PAN, MIDDLE, RIGHT: MOUSE.ROTATE };
        c.touches = { ONE: TOUCH.PAN, TWO: TOUCH_DISABLED };
        break;
      case 'zoom':
        c.mouseButtons = { LEFT: MOUSE.DOLLY, MIDDLE, RIGHT: MOUSE.PAN };
        c.touches = { ONE: TOUCH.DOLLY_PAN, TWO: TOUCH_DISABLED };
        break;
    }
  }

  /**
   * Switch the on-canvas object-manipulation gizmo. `'none'` and
   * `'pullToFloor'` hide the handles \u2014 `'pullToFloor'` then waits for
   * the next pointer-down anywhere on the model and routes it to the
   * face-pick handler instead.
   */
  setObjectMode(mode: ObjectMode): void {
    this.currentObjectMode = mode;
    this.gizmo.setMode(mode, this.computeSelectionCentroid());
    if (mode !== 'pullToFloor') {
      this.hideFaceHighlight();
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
    this.uninstallSelectionHandlers();
    this.gizmo.dispose();
    if (this.highlightRafHandle !== 0) {
      cancelAnimationFrame(this.highlightRafHandle);
      this.highlightRafHandle = 0;
    }
    this.faceHighlight.geometry.dispose();
    (this.faceHighlight.material as Material).dispose();
    this.clearContent();
    this.controls.dispose();
    this.renderer.dispose();
    if (this.renderer.domElement.parentElement === this.host) {
      this.host.removeChild(this.renderer.domElement);
    }
  }

  // ---------------------------------------------------------------------------
  // Selection / drag — pointer plumbing
  // ---------------------------------------------------------------------------

  /**
   * Install capture-phase pointer listeners so we get a chance to claim the
   * gesture before OrbitControls (which is registered with default bubbling)
   * starts a camera rotation. We only consume the event when the pointerdown
   * actually hits a registered selectable in a mode where left-click would
   * otherwise rotate (orbit / rotate); empty-space clicks fall through and
   * OrbitControls behaves as before.
   */
  private installSelectionHandlers(): void {
    const el = this.renderer.domElement;
    el.addEventListener('pointerdown', this.onSelectionPointerDown, { capture: true });
    el.addEventListener('pointermove', this.onSelectionPointerMove, { capture: true });
    el.addEventListener('pointerup', this.onSelectionPointerUp, { capture: true });
    el.addEventListener('pointercancel', this.onSelectionPointerCancel, { capture: true });
  }

  private uninstallSelectionHandlers(): void {
    const el = this.renderer.domElement;
    el.removeEventListener('pointerdown', this.onSelectionPointerDown, { capture: true });
    el.removeEventListener('pointermove', this.onSelectionPointerMove, { capture: true });
    el.removeEventListener('pointerup', this.onSelectionPointerUp, { capture: true });
    el.removeEventListener('pointercancel', this.onSelectionPointerCancel, { capture: true });
  }

  private onSelectionPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0 || !this.selectionHandlers) {
      return;
    }
    // The transform gizmo handles its own pointer events on the renderer's
    // domElement (it was constructed with the same element). When the
    // cursor is over a handle, TC will claim the gesture; we must not
    // start a selection raycast on the same frame.
    if (this.gizmo.isHovering() || this.gizmo.isDragging()) {
      return;
    }
    // Pull-to-floor mode: any click on any face dispatches an
    // `align_face_to_floor` op via the gizmo handlers, regardless of
    // current selection. Camera orbit is suppressed for this gesture.
    if (this.currentObjectMode === 'pullToFloor') {
      const hit = this.pickFace(event);
      if (hit) {
        event.preventDefault();
        event.stopPropagation();
        this.gizmoHandlers?.facePicked(hit.objectId, hit.faceIndex);
      }
      return;
    }
    // Selection requires the camera mode to be `'orbit'` \u2014 in pan/zoom
    // we don't want left-click to also select, since the user explicitly
    // chose a navigation mode.
    if (this.currentCursorMode !== 'orbit') {
      return;
    }
    if (this.selectables.size === 0) {
      return;
    }
    const hitId = this.raycastSelectable(event);
    if (hitId === null) {
      // Don't steal empty-bed clicks \u2014 let OrbitControls orbit the camera.
      // But remember the press so a clean (non-drag) release can deselect.
      if (this.currentSelectedIds.size > 0) {
        this.emptyPressState = {
          pointerId: event.pointerId,
          downX: event.clientX,
          downY: event.clientY,
        };
      }
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    this.pressState = {
      pointerId: event.pointerId,
      downX: event.clientX,
      downY: event.clientY,
      hitId,
      additive: event.ctrlKey || event.metaKey || event.shiftKey,
    };
  };

  private onSelectionPointerMove = (event: PointerEvent): void => {
    if (this.currentObjectMode === 'pullToFloor') {
      // Coalesce rapid pointer-moves to one raycast per frame. Without
      // this, devtools-amplified raycast cost can stall the UI thread.
      this.pendingHighlightEvent = event;
      if (this.highlightRafHandle === 0) {
        this.highlightRafHandle = requestAnimationFrame(this.flushFaceHighlight);
      }
    }
    const eps = this.emptyPressState;
    if (eps && event.pointerId === eps.pointerId) {
      const dxPx = event.clientX - eps.downX;
      const dyPx = event.clientY - eps.downY;
      if (Math.hypot(dxPx, dyPx) >= SELECTION_DRAG_THRESHOLD_PX) {
        // Promoted to an orbit drag \u2014 abandon the deselect intent.
        this.emptyPressState = null;
      }
    }
    const ps = this.pressState;
    if (!ps || event.pointerId !== ps.pointerId || !this.selectionHandlers) {
      return;
    }
    // The selectable was clicked but the user is now dragging. Object
    // movement always goes through the gizmo \u2014 a bare drag-on-mesh just
    // promotes to a camera orbit so the model can be inspected without
    // a separate mode switch.
    const dxPx = event.clientX - ps.downX;
    const dyPx = event.clientY - ps.downY;
    if (Math.hypot(dxPx, dyPx) >= SELECTION_DRAG_THRESHOLD_PX) {
      this.pressState = null;
    }
  };

  private onSelectionPointerUp = (event: PointerEvent): void => {
    const eps = this.emptyPressState;
    if (eps && event.pointerId === eps.pointerId) {
      this.emptyPressState = null;
      const dxPx = event.clientX - eps.downX;
      const dyPx = event.clientY - eps.downY;
      if (Math.hypot(dxPx, dyPx) < SELECTION_DRAG_THRESHOLD_PX) {
        this.selectionHandlers?.clearSelection();
      }
    }
    const ps = this.pressState;
    if (!ps || event.pointerId !== ps.pointerId) {
      return;
    }
    this.pressState = null;
    // Pure click on a selectable \u2014 apply selection now. (Drags were
    // already cleared in `onSelectionPointerMove`, leaving OrbitControls
    // free to handle the camera orbit.)
    this.selectionHandlers?.select(ps.hitId, ps.additive);
    event.preventDefault();
    event.stopPropagation();
  };

  private onSelectionPointerCancel = (event: PointerEvent): void => {
    this.hideFaceHighlight();
    if (this.emptyPressState && event.pointerId === this.emptyPressState.pointerId) {
      this.emptyPressState = null;
    }
    const ps = this.pressState;
    if (!ps || event.pointerId !== ps.pointerId) {
      return;
    }
    this.cancelActiveDrag();
  };

  /**
   * Raycast every selectable for `event` and return the front-most face
   * (object id + triangle index). Returns `null` if no selectable was hit.
   */
  private pickFace(event: PointerEvent): { objectId: string; faceIndex: number } | null {
    const ndc = this.toNdc(event, this.ndcScratch);
    const targets = Array.from(this.selectables.values());
    if (targets.length === 0) {
      return null;
    }
    return raycastFace(this.raycaster, this.camera, ndc, targets);
  }

  /**
   * Update the green pull-to-floor face overlay to track the coplanar group
   * currently under the pointer. On hit, all triangles sharing the same
   * coplanar group id as the hit face are collected and rendered as a single
   * multi-triangle overlay — so a flat bottom face lights up as a whole
   * region rather than one triangle at a time.
   *
   * Falls back to single-triangle highlight when no `faceGroups` map is
   * present (e.g. the mesh was loaded before this feature was deployed).
   */
  private updateFaceHighlight(event: PointerEvent): void {
    const ndc = this.toNdc(event, this.ndcScratch);
    const targets = Array.from(this.selectables.values());
    if (targets.length === 0) {
      this.hideFaceHighlight();
      return;
    }
    this.raycaster.setFromCamera(ndc, this.camera);
    const hits = this.raycaster.intersectObjects(targets, true);

    for (const hit of hits) {
      const mesh = hit.object;
      const face = hit.face;
      if (!face || !(mesh instanceof Mesh) || !mesh.geometry) {
        continue;
      }
      const posAttr = mesh.geometry.getAttribute('position');
      if (!posAttr) {
        continue;
      }

      // --- Determine which face indices belong to the hovered group --------
      const faceGroups: Uint32Array | undefined = mesh.userData['faceGroups'];
      // hit.faceIndex is the THREE.js triangle index into the index buffer.
      const hitFaceIdx = hit.faceIndex ?? 0;
      const targetGroup =
        faceGroups && faceGroups.length > hitFaceIdx ? faceGroups[hitFaceIdx] : -1;

      // Fast path: same mesh + same group as last frame → nothing to rebuild.
      const cache = this.faceHighlightCache;
      if (
        cache !== null &&
        cache.meshUuid === mesh.uuid &&
        cache.groupId === targetGroup &&
        this.faceHighlight.visible
      ) {
        return;
      }

      let faceIndices: number[];
      if (faceGroups && targetGroup >= 0) {
        faceIndices = [];
        for (let i = 0; i < faceGroups.length; i++) {
          if (faceGroups[i] === targetGroup) {
            faceIndices.push(i);
          }
        }
      } else {
        // Fallback: just highlight the single hit triangle.
        faceIndices = [hitFaceIdx];
      }

      // --- Build world-space position buffer for the highlight mesh --------
      // Each face contributes 3 vertices × 3 floats.
      const triCount = faceIndices.length;
      const posArr = new Float32Array(triCount * 9);

      // Compute a shared lift direction from the hit face normal.
      const va0 = this.faceTriScratchA.fromBufferAttribute(posAttr, face.a);
      const vb0 = this.faceTriScratchB.fromBufferAttribute(posAttr, face.b);
      const vc0 = this.faceTriScratchC.fromBufferAttribute(posAttr, face.c);
      mesh.localToWorld(va0);
      mesh.localToWorld(vb0);
      mesh.localToWorld(vc0);
      const nx = (vb0.y - va0.y) * (vc0.z - va0.z) - (vb0.z - va0.z) * (vc0.y - va0.y);
      const ny = (vb0.z - va0.z) * (vc0.x - va0.x) - (vb0.x - va0.x) * (vc0.z - va0.z);
      const nz = (vb0.x - va0.x) * (vc0.y - va0.y) - (vb0.y - va0.y) * (vc0.x - va0.x);
      const nlen = Math.hypot(nx, ny, nz) || 1;
      const lift = 0.02;
      const lx = (nx / nlen) * lift;
      const ly = (ny / nlen) * lift;
      const lz = (nz / nlen) * lift;

      // Get the index buffer to resolve face → vertex indices.
      const indexAttr = mesh.geometry.getIndex();

      for (let t = 0; t < triCount; t++) {
        const fi = faceIndices[t];
        let ia: number, ib: number, ic: number;
        if (indexAttr) {
          ia = indexAttr.getX(fi * 3);
          ib = indexAttr.getX(fi * 3 + 1);
          ic = indexAttr.getX(fi * 3 + 2);
        } else {
          ia = fi * 3;
          ib = fi * 3 + 1;
          ic = fi * 3 + 2;
        }
        const va = this.faceTriScratchA.fromBufferAttribute(posAttr, ia);
        const vb = this.faceTriScratchB.fromBufferAttribute(posAttr, ib);
        const vc = this.faceTriScratchC.fromBufferAttribute(posAttr, ic);
        mesh.localToWorld(va);
        mesh.localToWorld(vb);
        mesh.localToWorld(vc);
        const base = t * 9;
        posArr[base] = va.x + lx;
        posArr[base + 1] = va.y + ly;
        posArr[base + 2] = va.z + lz;
        posArr[base + 3] = vb.x + lx;
        posArr[base + 4] = vb.y + ly;
        posArr[base + 5] = vb.z + lz;
        posArr[base + 6] = vc.x + lx;
        posArr[base + 7] = vc.y + ly;
        posArr[base + 8] = vc.z + lz;
      }

      // Replace (or reuse) the position attribute with the new data. We
      // reuse the existing Float32BufferAttribute storage when the size
      // matches to avoid GC churn on rapid hovers.
      const existing = this.faceHighlight.geometry.getAttribute('position');
      if (
        existing instanceof Float32BufferAttribute &&
        (existing.array as Float32Array).length === posArr.length
      ) {
        (existing.array as Float32Array).set(posArr);
        existing.needsUpdate = true;
      } else {
        this.faceHighlight.geometry.setAttribute('position', new Float32BufferAttribute(posArr, 3));
      }
      this.faceHighlight.geometry.deleteAttribute('index');
      this.faceHighlight.geometry.computeBoundingSphere();
      this.faceHighlight.visible = true;
      this.faceHighlightCache = { meshUuid: mesh.uuid, groupId: targetGroup };
      return;
    }
    this.hideFaceHighlight();
  }

  private hideFaceHighlight(): void {
    this.faceHighlight.visible = false;
    this.faceHighlightCache = null;
    if (this.highlightRafHandle !== 0) {
      cancelAnimationFrame(this.highlightRafHandle);
      this.highlightRafHandle = 0;
    }
    this.pendingHighlightEvent = null;
  }

  /** Scratch vectors so {@link updateFaceHighlight} doesn't allocate per frame. */
  private readonly faceTriScratchA = new Vector3();
  private readonly faceTriScratchB = new Vector3();
  private readonly faceTriScratchC = new Vector3();
  /**
   * Cache of the last hovered (mesh, group). While the pointer stays on the
   * same coplanar group we can skip the full raycast → geometry rebuild,
   * which is a meaningful savings when devtools are open and source maps /
   * CDP overhead amplify per-frame work.
   */
  private faceHighlightCache: {
    meshUuid: string;
    groupId: number;
  } | null = null;
  /** Pointer event waiting to drive the next highlight update (rAF coalesced). */
  private pendingHighlightEvent: PointerEvent | null = null;
  private highlightRafHandle = 0;
  private flushFaceHighlight = (): void => {
    this.highlightRafHandle = 0;
    const ev = this.pendingHighlightEvent;
    this.pendingHighlightEvent = null;
    if (ev !== null && this.currentObjectMode === 'pullToFloor') {
      this.updateFaceHighlight(ev);
    }
  };

  /** Convert pointer to NDC and raycast against every registered selectable. */
  private raycastSelectable(event: PointerEvent): string | null {
    const ndc = this.toNdc(event, this.ndcScratch);
    this.raycaster.setFromCamera(ndc, this.camera);
    const targets = Array.from(this.selectables.values());
    if (targets.length === 0) {
      return null;
    }
    const hits = this.raycaster.intersectObjects(targets, true);
    if (hits.length === 0) {
      return null;
    }
    return this.findSelectableId(hits[0].object);
  }

  /** Walk parents of a hit object until we find one carrying a selectable id. */
  private findSelectableId(obj: Object3D | null): string | null {
    let cur: Object3D | null = obj;
    while (cur) {
      const id = cur.userData?.['selectableId'];
      if (typeof id === 'string') {
        return id;
      }
      cur = cur.parent;
    }
    return null;
  }

  /** Pointer client coords → normalised device coords for the renderer canvas. */
  private toNdc(event: PointerEvent, out: Vector2): Vector2 {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / Math.max(rect.width, 1)) * 2 - 1;
    const y = -(((event.clientY - rect.top) / Math.max(rect.height, 1)) * 2 - 1);
    return out.set(x, y);
  }

  /**
   * Cancel any selection press currently in flight without committing
   * positions. Called when the underlying selectable goes away
   * (clearContent), the cursor mode changes, or the OS cancels the pointer
   * (touch interrupt, etc.). Object-manipulation drags are owned by the
   * gizmo and clean themselves up on `mouseUp`.
   */
  private cancelActiveDrag(): void {
    this.pressState = null;
    this.emptyPressState = null;
  }

  /**
   * Toggle the emissive highlight on every Mesh descendant of `root`. When
   * `on` is true the original emissive colour + intensity is snapshotted
   * so the inverse call restores the material exactly. Materials without an
   * `emissive` channel (e.g. line / basic materials) are silently skipped.
   */
  private applyHighlight(root: Object3D, on: boolean): void {
    root.traverse((node) => {
      if (!(node instanceof Mesh)) {
        return;
      }
      const materials = Array.isArray(node.material) ? node.material : [node.material];
      if (on) {
        const snapshot: { color: Color; intensity: number }[] = [];
        for (const mat of materials) {
          const m = mat as Material & { emissive?: Color; emissiveIntensity?: number };
          if (!m.emissive) {
            snapshot.push({ color: new Color(0, 0, 0), intensity: 0 });
            continue;
          }
          snapshot.push({
            color: m.emissive.clone(),
            intensity: m.emissiveIntensity ?? 1,
          });
          m.emissive.copy(SELECTION_EMISSIVE);
          if ('emissiveIntensity' in m) {
            m.emissiveIntensity = SELECTION_EMISSIVE_INTENSITY;
          }
        }
        this.originalEmissive.set(node, snapshot);
      } else {
        const snapshot = this.originalEmissive.get(node);
        if (!snapshot) {
          return;
        }
        for (let i = 0; i < materials.length; i++) {
          const m = materials[i] as Material & {
            emissive?: Color;
            emissiveIntensity?: number;
          };
          const orig = snapshot[i];
          if (!m.emissive || !orig) {
            continue;
          }
          m.emissive.copy(orig.color);
          if ('emissiveIntensity' in m) {
            m.emissiveIntensity = orig.intensity;
          }
        }
        this.originalEmissive.delete(node);
      }
    });
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
      this.applyOrbitInertia(dt);
    }
    this.updateAdaptiveGrid();
    this.updateGridFade();
    // Recompute near/far against the live camera-target distance so depth
    // precision stays tight across the full zoom range without needing the
    // (mobile-hostile) logarithmicDepthBuffer extension.
    this.updateNearFar();
    // Keep the gizmo glued to the selection centroid (which can drift each
    // frame if WASM transforms have been updated since the last selection
    // change) and resize it to a roughly fixed screen size.
    if (!this.gizmo.isDragging()) {
      this.gizmo.setCentroid(this.computeSelectionCentroid());
    }
    this.gizmo.update();
    this.renderer.render(this.scene, this.camera);
    this.publishCameraState();
    this.publishFps(now, dt);
  };

  /** Update the smoothed FPS / delay estimates and push to {@link fpsSink} ~once/s. */
  private publishFps(now: number, dt: number): void {
    if (!this.fpsSink) {
      return;
    }
    if (dt > 0) {
      const instantFps = 1 / dt;
      const instantDelayMs = dt * 1000;
      // Exponential moving average — α=0.1 smooths over ~10 frames.
      this.smoothedFps =
        this.smoothedFps === 0 ? instantFps : 0.9 * this.smoothedFps + 0.1 * instantFps;
      this.smoothedDelayMs =
        this.smoothedDelayMs === 0
          ? instantDelayMs
          : 0.9 * this.smoothedDelayMs + 0.1 * instantDelayMs;
    }
    if (now - this.lastFpsPublishTime >= 500) {
      this.lastFpsPublishTime = now;
      this.fpsSink(Math.round(this.smoothedFps), Math.round(this.smoothedDelayMs * 10) / 10);
    }
  }

  /** Push the camera's live direction/up to the optional sink. */
  private publishCameraState(): void {
    if (!this.cameraStateSink) {
      return;
    }
    const offset = this.camera.position.clone().sub(this.controls.target);
    if (offset.lengthSq() < 1e-6) {
      offset.copy(DEFAULT_VIEW_DIR);
    }
    this.cameraStateSink(offset.normalize(), this.camera.up.clone().normalize(), this.camera.fov);
  }

  private handleResize(): void {
    const { clientWidth, clientHeight } = this.sizeOf(this.host);
    if (clientWidth === 0 || clientHeight === 0) {
      return;
    }
    this.camera.aspect = clientWidth / clientHeight;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(clientWidth, clientHeight);
    // Re-render immediately so the canvas never shows a blank frame between
    // the buffer clear (caused by setSize) and the next RAF tick.
    this.renderer.render(this.scene, this.camera);
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

    // Clean up any previous in-flight transition immediately.
    if (this.gridTransition) {
      this.scene.remove(this.gridTransition.outgoing);
      disposeObject(this.gridTransition.outgoing);
      this.gridTransition = null;
    }

    // Keep the current grid in the scene as the outgoing (fading-out) layer.
    const outgoing = this.grid;
    const outgoingMaterials = this.gridMaterials;

    // createGrid updates this.gridMaterials and this.currentGridSpacingMm.
    const incoming = this.createGrid(snapped);
    this.scene.add(incoming);
    this.grid = incoming;

    this.gridTransition = { outgoing, outgoingMaterials, startTime: performance.now() };
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

    // Cross-fade between the outgoing (old) grid and the incoming (new) grid.
    let crossFadeIn = 1.0;
    if (this.gridTransition) {
      const elapsed = performance.now() - this.gridTransition.startTime;
      const progress = Math.min(elapsed / GRID_SCALE_TRANSITION_MS, 1.0);
      const smooth = progress * progress * (3 - 2 * progress);
      crossFadeIn = smooth;
      const crossFadeOut = 1.0 - smooth;

      for (const entry of this.gridTransition.outgoingMaterials) {
        const opacity = entry.baseOpacity * fade * crossFadeOut;
        entry.material.opacity = opacity;
        entry.material.visible = opacity > 0.001;
      }

      if (progress >= 1.0) {
        this.scene.remove(this.gridTransition.outgoing);
        disposeObject(this.gridTransition.outgoing);
        this.gridTransition = null;
      }
    }

    for (const entry of this.gridMaterials) {
      const opacity = entry.baseOpacity * fade * crossFadeIn;
      if (entry.material.opacity !== opacity) {
        entry.material.opacity = opacity;
        entry.material.visible = opacity > 0.001;
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Orbit / pan inertia (custom — see field comment on `orbitInteracting`)
  // ---------------------------------------------------------------------------

  /**
   * Toggle `zoomToCursor` on while a touch gesture is active and back off
   * on release. Touch users want pinch-zoom to converge on the finger
   * point; mouse users prefer the default dolly-toward-target behaviour
   * because it matches our autoscroll zoom and keeps wheel zoom stable
   * regardless of cursor position.
   */
  private installTouchOrbitTuning(): void {
    const el = this.renderer.domElement;
    const activeTouches = new Set<number>();
    const onDown = (event: PointerEvent): void => {
      if (event.pointerType !== 'touch') {
        return;
      }
      activeTouches.add(event.pointerId);
      this.controls.zoomToCursor = true;
    };
    const onEnd = (event: PointerEvent): void => {
      if (event.pointerType !== 'touch') {
        return;
      }
      activeTouches.delete(event.pointerId);
      if (activeTouches.size === 0) {
        this.controls.zoomToCursor = false;
      }
    };
    el.addEventListener('pointerdown', onDown);
    el.addEventListener('pointerup', onEnd);
    el.addEventListener('pointercancel', onEnd);
  }

  /**
   * Install a capture-phase touch handler that takes over the canvas
   * whenever two (or more) touches are active. While engaged it processes
   * the gesture as a combined pinch (dolly), centroid translation (pan)
   * and twist (roll around the camera's view axis), then releases control
   * back to OrbitControls when the user drops to one finger or lifts off.
   *
   * Why this is custom instead of using {@link TOUCH.DOLLY_PAN} or
   * {@link TOUCH.DOLLY_ROTATE}: OrbitControls only exposes one TWO-finger
   * action, so the user has to pick between two-finger pan and two-finger
   * rotate. A modern 3D viewer (Onshape, Sketchfab, Shapr3D) lets the
   * user do all three simultaneously, and "twist" naturally maps to roll
   * — which OrbitControls does not support at all (its `_quat` is fixed
   * to the camera's original up vector). We side-step both limitations
   * by manipulating `camera.position`, `controls.target` and `camera.up`
   * directly; OrbitControls' next `update()` call simply re-derives the
   * orientation via `lookAt(target)` using the rolled-up vector, so the
   * roll persists.
   */
  private installCustomTwoFingerControls(): void {
    const el = this.renderer.domElement;
    const touches = new Map<number, { x: number; y: number }>();
    const state = {
      active: false,
      lastDist: 0,
      lastAngle: 0,
      lastCx: 0,
      lastCy: 0,
      savedControlsEnabled: true,
      // Set while we are dispatching synthetic pointercancel events to
      // OrbitControls. Our own capture-phase `onUp` must ignore those or
      // it would drop the touches it just engaged on.
      suppressOwnCancel: false,
    };

    const recomputeAnchors = (): void => {
      const pts = [...touches.values()];
      if (pts.length < 2) return;
      const a = pts[0];
      const b = pts[1];
      state.lastDist = Math.hypot(b.x - a.x, b.y - a.y);
      state.lastAngle = Math.atan2(b.y - a.y, b.x - a.x);
      state.lastCx = (a.x + b.x) / 2;
      state.lastCy = (a.y + b.y) / 2;
    };

    const beginTwoFinger = (): void => {
      state.active = true;
      state.savedControlsEnabled = this.controls.enabled;
      this.controls.enabled = false;
      // Cancel any orbit inertia and any in-flight selection drag started
      // by the first finger so the second finger doesn't fight a stale
      // single-finger gesture.
      this.orbitVelAzimuth = 0;
      this.orbitVelPolar = 0;
      this.orbitVelTarget.set(0, 0, 0);
      this.cancelActiveDrag();
      // OrbitControls already started tracking the FIRST finger as a
      // single-touch ROTATE before we engaged. If we just toggle
      // `controls.enabled = false`, its internal pointer set still holds
      // that pointer — and the moment we re-enable on lift-down to one
      // finger, it resumes rotating from a stale anchor (causing a wild
      // spin / jump). Fire a synthetic `pointercancel` for every active
      // touch so OrbitControls' `_onPointerCancel` clears its state.
      // Re-disabling immediately afterwards keeps it from acting on the
      // cancel itself. We toggle around the dispatch because OC's cancel
      // handler bails early when `enabled === false`.
      this.controls.enabled = true;
      state.suppressOwnCancel = true;
      for (const id of touches.keys()) {
        try {
          el.dispatchEvent(
            new PointerEvent('pointercancel', { pointerId: id, pointerType: 'touch' }),
          );
        } catch {
          // Older browsers may reject the constructor; harmless to skip.
        }
      }
      state.suppressOwnCancel = false;
      this.controls.enabled = false;
      recomputeAnchors();
    };

    const endTwoFinger = (): void => {
      if (!state.active) return;
      state.active = false;
      this.controls.enabled = state.savedControlsEnabled;
    };

    const onDown = (event: PointerEvent): void => {
      if (event.pointerType !== 'touch') return;
      touches.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (touches.size === 2 && !state.active) {
        beginTwoFinger();
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
      } else if (state.active) {
        // Third+ finger: keep tracking but re-anchor so the centroid
        // doesn't jump.
        recomputeAnchors();
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
      }
    };

    const onMove = (event: PointerEvent): void => {
      if (event.pointerType !== 'touch') return;
      const t = touches.get(event.pointerId);
      if (!t) return;
      t.x = event.clientX;
      t.y = event.clientY;
      // While engaged, swallow ALL touch moves — including the lone
      // remaining finger after a partial lift — so OrbitControls cannot
      // pick up the residual pointer and start an unwanted rotate.
      if (state.active) {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
      }
      if (!state.active || touches.size < 2) return;

      const pts = [...touches.values()];
      const a = pts[0];
      const b = pts[1];
      const dist = Math.hypot(b.x - a.x, b.y - a.y);
      const angle = Math.atan2(b.y - a.y, b.x - a.x);
      const cx = (a.x + b.x) / 2;
      const cy = (a.y + b.y) / 2;

      // --- Pinch → dolly --------------------------------------------------
      if (state.lastDist > 0 && Math.abs(dist - state.lastDist) > TWO_FINGER_DOLLY_DEAD_ZONE_PX) {
        const factor = state.lastDist / Math.max(dist, 1e-3);
        this.applyTouchDolly(factor, cx, cy);
      }

      // --- Twist → roll ---------------------------------------------------
      let dAngle = angle - state.lastAngle;
      if (dAngle > Math.PI) dAngle -= 2 * Math.PI;
      else if (dAngle < -Math.PI) dAngle += 2 * Math.PI;
      if (Math.abs(dAngle) > TWO_FINGER_ROLL_DEAD_ZONE_RAD) {
        // Screen Y grows downward but world rotation conventions are
        // right-handed; negate so a clockwise twist of the fingers (as the
        // user sees them) rolls the scene clockwise too.
        this.applyTouchRoll(-dAngle);
      }

      // --- Centroid → pan -------------------------------------------------
      const dx = cx - state.lastCx;
      const dy = cy - state.lastCy;
      if (dx !== 0 || dy !== 0) {
        this.applyTouchPan(dx, dy);
      }

      state.lastDist = dist;
      state.lastAngle = angle;
      state.lastCx = cx;
      state.lastCy = cy;
    };

    const onUp = (event: PointerEvent): void => {
      if (event.pointerType !== 'touch') return;
      // Ignore the synthetic pointercancel events we dispatch at engage
      // time — those are aimed at OrbitControls, not at our own state.
      if (state.suppressOwnCancel) return;
      if (!touches.delete(event.pointerId)) return;
      if (!state.active) return;
      // Defer ending the gesture (and re-enabling OrbitControls) until
      // every finger has lifted. If we re-enabled while the user still
      // had one finger down, OrbitControls would immediately resume a
      // single-touch ROTATE from the lone remaining pointer — which the
      // user perceives as the camera "spinning off" the moment they let
      // go of one finger. While we wait, also swallow the up event so OC
      // doesn't see it as the end of a gesture it never started.
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      if (touches.size === 0) {
        endTwoFinger();
      } else {
        // 3+→2 or 2→1: re-anchor so the remaining touches don't jump.
        recomputeAnchors();
      }
    };

    el.addEventListener('pointerdown', onDown, { capture: true });
    el.addEventListener('pointermove', onMove, { capture: true });
    el.addEventListener('pointerup', onUp, { capture: true });
    el.addEventListener('pointercancel', onUp, { capture: true });
  }

  /**
   * Dolly toward / away from the world point under the pinch centroid.
   * `factor < 1` zooms in (fingers spreading apart), `factor > 1` zooms
   * out. Implements the standard "zoom-to-cursor" math:
   *   C' = T + (C - T) * f                    (basic dolly toward target)
   *   S  = (W - T) * (1 - f)                  (shift to anchor W on screen)
   *   C'' = C' + S,   T'' = T + S
   * where W is the intersection of the pinch-centroid ray with the plane
   * through T perpendicular to the view direction. With this, the world
   * point under the centroid stays under the centroid as the user pinches.
   */
  private applyTouchDolly(factor: number, cx: number, cy: number): void {
    const camera = this.camera;
    const target = this.controls.target;
    // Multiple touch transforms can chain within a single pointermove
    // event — refresh world matrices so the raycaster sees the live pose.
    camera.updateMatrixWorld(true);
    const offset = camera.position.clone().sub(target);
    const oldDist = offset.length();
    if (oldDist < 1e-6) return;
    let newDist = oldDist * factor;
    newDist = Math.max(this.controls.minDistance, Math.min(this.controls.maxDistance, newDist));
    const f = newDist / oldDist;
    if (Math.abs(f - 1) < 1e-6) return;

    // Anchor W: ray through pinch centroid intersected with the plane
    // through `target` whose normal points back toward the camera.
    const viewNormal = offset.clone().normalize();
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndcX = ((cx - rect.left) / Math.max(rect.width, 1)) * 2 - 1;
    const ndcY = -(((cy - rect.top) / Math.max(rect.height, 1)) * 2 - 1);
    this.raycaster.setFromCamera(this.ndcScratch.set(ndcX, ndcY), camera);
    const plane = new Plane().setFromNormalAndCoplanarPoint(viewNormal, target);
    const W = new Vector3();
    const hit = this.raycaster.ray.intersectPlane(plane, W);

    // Apply dolly. Order matters: shift after the dolly, both about T.
    camera.position.copy(target).add(offset.multiplyScalar(f));

    if (hit) {
      const shift = W.sub(target).multiplyScalar(1 - f);
      camera.position.add(shift);
      target.add(shift);
    }
  }

  /**
   * Translate both camera and target by a screen-space pixel delta of the
   * pinch centroid, projected to world units at the current target depth.
   * Mirrors OrbitControls' screen-space pan but driven by our own delta
   * stream so it composes cleanly with the simultaneous dolly + roll.
   */
  private applyTouchPan(dxPx: number, dyPx: number): void {
    const camera = this.camera;
    const target = this.controls.target;
    // The right/up basis is read from `camera.matrix`; keep it in sync
    // with any dolly/roll applied earlier in the same move event.
    camera.updateMatrix();
    const distance = camera.position.distanceTo(target);
    const fovRad = (camera.fov * Math.PI) / 180;
    const viewportHeight = Math.max(this.renderer.domElement.clientHeight, 1);
    const worldPerPixel = (2 * Math.tan(fovRad / 2) * distance) / viewportHeight;

    // Right and up basis vectors in world space, derived from the live
    // camera matrix so they include any roll we have already applied.
    const right = new Vector3().setFromMatrixColumn(camera.matrix, 0);
    const up = new Vector3().setFromMatrixColumn(camera.matrix, 1);
    const pan = new Vector3();
    pan.addScaledVector(right, -dxPx * worldPerPixel);
    pan.addScaledVector(up, dyPx * worldPerPixel);
    camera.position.add(pan);
    target.add(pan);
  }

  /**
   * Roll the camera by `angle` radians around its forward (view) axis.
   * Implemented by rotating `camera.up`; OrbitControls' `update()` calls
   * `lookAt(target)` every frame using the live up vector, so the roll
   * persists across subsequent orbit updates without any further hooks.
   */
  private applyTouchRoll(angle: number): void {
    const camera = this.camera;
    const forward = this.controls.target.clone().sub(camera.position).normalize();
    if (forward.lengthSq() < 1e-12) return;
    const q = new Quaternion().setFromAxisAngle(forward, angle);
    camera.up.applyQuaternion(q).normalize();
    camera.lookAt(this.controls.target);
    // `lookAt` updates the quaternion but not the matrix; refresh so the
    // following pan in the same move event reads the new basis vectors.
    camera.updateMatrix();
  }

  /**
   * Wire up the controls' `start` / `change` / `end` events to track angular
   * velocity (azimuth, polar) and target-pan velocity during interaction.
   * The accumulated velocities are consumed on each render frame by
   * {@link applyOrbitInertia} once the gesture ends.
   */
  private installOrbitInertia(): void {
    this.controls.addEventListener('start', () => {
      this.orbitInteracting = true;
      this.orbitLastSampleTime = performance.now();
      this.orbitLastAzimuth = this.controls.getAzimuthalAngle();
      this.orbitLastPolar = this.controls.getPolarAngle();
      this.orbitLastTarget.copy(this.controls.target);
      // Reset any in-progress coast — the new gesture takes over.
      this.orbitVelAzimuth = 0;
      this.orbitVelPolar = 0;
      this.orbitVelTarget.set(0, 0, 0);
    });
    this.controls.addEventListener('change', () => {
      if (!this.orbitInteracting) {
        return;
      }
      const now = performance.now();
      const dt = (now - this.orbitLastSampleTime) / 1000;
      this.orbitLastSampleTime = now;
      if (dt <= 0 || dt > 0.1) {
        // First sample of a gesture or a stalled frame — capture state but
        // don't derive a velocity from a bogus dt.
        this.orbitLastAzimuth = this.controls.getAzimuthalAngle();
        this.orbitLastPolar = this.controls.getPolarAngle();
        this.orbitLastTarget.copy(this.controls.target);
        return;
      }
      const azNow = this.controls.getAzimuthalAngle();
      const polNow = this.controls.getPolarAngle();
      // Wrap azimuth delta into (-π, π] so a discontinuity at ±π doesn't
      // produce a huge spurious velocity spike.
      let dAz = azNow - this.orbitLastAzimuth;
      if (dAz > Math.PI) dAz -= 2 * Math.PI;
      else if (dAz < -Math.PI) dAz += 2 * Math.PI;
      const dPol = polNow - this.orbitLastPolar;
      const dTarget = this.controls.target.clone().sub(this.orbitLastTarget);
      // Exponential moving average so a single noisy frame can't dominate
      // the released velocity.
      const smoothing = 0.5;
      this.orbitVelAzimuth = lerp(this.orbitVelAzimuth, dAz / dt, smoothing);
      this.orbitVelPolar = lerp(this.orbitVelPolar, dPol / dt, smoothing);
      this.orbitVelTarget.lerp(dTarget.divideScalar(dt), smoothing);
      this.orbitLastAzimuth = azNow;
      this.orbitLastPolar = polNow;
      this.orbitLastTarget.copy(this.controls.target);
    });
    this.controls.addEventListener('end', () => {
      this.orbitInteracting = false;
      // If the final frame of the gesture was "still" (mouse paused before
      // release), zero out the velocity so the camera stops dead instead of
      // coasting from a stale earlier sample.
      const sinceLastSample = (performance.now() - this.orbitLastSampleTime) / 1000;
      if (sinceLastSample > 0.08) {
        this.orbitVelAzimuth = 0;
        this.orbitVelPolar = 0;
        this.orbitVelTarget.set(0, 0, 0);
        return;
      }
      // Scale released velocity down so the coast is a subtle hint of
      // follow-through, not a noticeable drift.
      const releaseScale = 0.35;
      this.orbitVelAzimuth *= releaseScale;
      this.orbitVelPolar *= releaseScale;
      this.orbitVelTarget.multiplyScalar(releaseScale);
    });
  }

  /**
   * Coast the camera by the velocities sampled during the last gesture,
   * decaying exponentially. A no-op while the user is actively interacting
   * (during which OrbitControls drives the camera 1:1 with the pointer) or
   * when all velocities have decayed below the visible-motion threshold.
   */
  private applyOrbitInertia(dt: number): void {
    if (this.orbitInteracting || dt <= 0) {
      return;
    }
    const azSpeed = Math.abs(this.orbitVelAzimuth);
    const polSpeed = Math.abs(this.orbitVelPolar);
    const panSpeed = this.orbitVelTarget.length();
    // Visible-motion threshold: ~0.05°/s of orbit, ~0.01 mm/s of pan.
    if (azSpeed < 1e-3 && polSpeed < 1e-3 && panSpeed < 1e-2) {
      this.orbitVelAzimuth = 0;
      this.orbitVelPolar = 0;
      this.orbitVelTarget.set(0, 0, 0);
      return;
    }
    // Apply orbit rotation around the controls' target using the same
    // azimuth/polar convention OrbitControls uses internally (Y-up frame
    // rotated to match `camera.up`).
    if (azSpeed > 0 || polSpeed > 0) {
      const offset = this.camera.position.clone().sub(this.controls.target);
      const yUp = new Vector3(0, 1, 0);
      const q = new Quaternion().setFromUnitVectors(this.camera.up, yUp);
      const qInv = q.clone().invert();
      offset.applyQuaternion(q);
      const sph = new Spherical().setFromVector3(offset);
      sph.theta += this.orbitVelAzimuth * dt;
      sph.phi += this.orbitVelPolar * dt;
      const eps = 1e-3;
      sph.phi = Math.max(eps, Math.min(Math.PI - eps, sph.phi));
      offset.setFromSpherical(sph).applyQuaternion(qInv);
      this.camera.position.copy(this.controls.target).add(offset);
    }
    // Apply pan velocity on the target (and consequently the camera, since
    // the offset is preserved by adding the same delta to both).
    if (panSpeed > 0) {
      const dT = this.orbitVelTarget.clone().multiplyScalar(dt);
      this.controls.target.add(dT);
      this.camera.position.add(dT);
    }
    this.camera.lookAt(this.controls.target);
    // Exponential decay — short half-life keeps the coast subtle: just a
    // hint of follow-through after the pointer is released, settling within
    // a couple hundred milliseconds rather than visibly drifting.
    const halfLifeSeconds = 0.05;
    const decay = Math.pow(0.5, dt / halfLifeSeconds);
    this.orbitVelAzimuth *= decay;
    this.orbitVelPolar *= decay;
    this.orbitVelTarget.multiplyScalar(decay);
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
   * Tighten the camera's near / far planes around the current camera→target
   * distance plus a generous scene-radius margin. Called every frame from
   * {@link tick} (and on demand from camera animations) so depth precision
   * stays usable across the full zoom range without needing the very
   * expensive `logarithmicDepthBuffer` extension. Values are quantised so
   * we don't trigger a `updateProjectionMatrix` on every micro-change.
   */
  private updateNearFar(distance?: number, radius?: number): void {
    const dist =
      distance !== undefined && Number.isFinite(distance) && distance > 0
        ? distance
        : Math.max(this.camera.position.distanceTo(this.controls.target), 1);
    // Use the larger of the requested radius and a print-area-derived
    // baseline so an empty scene still gets a sensible far plane.
    const { printableAreaWidth, printableAreaHeight } = this.printArea;
    const bedRadius = Math.max(printableAreaWidth, printableAreaHeight, 200);
    const sceneRadius = Math.max(radius ?? 0, bedRadius);

    // Symmetric padding around the target distance: near pulls back to half
    // a scene-radius behind the target, far extends four scene-radii ahead.
    // The 0.5 / 4 asymmetry biases precision toward the foreground (where
    // the model sits) rather than the deep background.
    let near = (dist - sceneRadius) * 0.5;
    let far = (dist + sceneRadius) * 4;
    if (!Number.isFinite(near) || near < CAMERA_NEAR) {
      near = CAMERA_NEAR;
    }
    if (!Number.isFinite(far) || far > CAMERA_FAR) {
      far = CAMERA_FAR;
    }
    if (far <= near + 1) {
      far = near + 1;
    }
    // Quantise to ~0.5% so tiny jitter doesn't repeatedly dirty the
    // projection matrix.
    near = quantise(near, 0.005);
    far = quantise(far, 0.005);
    if (this.camera.near !== near || this.camera.far !== far) {
      this.camera.near = near;
      this.camera.far = far;
      this.camera.updateProjectionMatrix();
    }
  }
}

/** Round `value` to the nearest multiple of `value * step`, preserving sign. */
function quantise(value: number, step: number): number {
  if (value === 0) {
    return 0;
  }
  const scale = Math.abs(value) * step;
  return Math.round(value / scale) * scale;
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

interface SelectionPressState {
  pointerId: number;
  /** Pointer client X at pointerdown (used for the drag-threshold check). */
  downX: number;
  downY: number;
  /** Selectable id under the cursor at pointerdown. */
  hitId: string;
  /** Whether ctrl/⌘/shift was held at pointerdown (additive selection). */
  additive: boolean;
}

interface GridTransition {
  outgoing: Group;
  outgoingMaterials: { material: LineBasicMaterial; baseOpacity: number }[];
  startTime: number;
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
