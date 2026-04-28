import {
  AmbientLight,
  Box3,
  BoxGeometry,
  DirectionalLight,
  GridHelper,
  Group,
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
  private grid: GridHelper;
  private rafHandle = 0;
  private disposed = false;
  private currentView: ViewerView = '3D';
  private animation: CameraAnimation | null = null;

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

    this.scene.add(new AmbientLight(0xffffff, 0.55));
    const dir = new DirectionalLight(0xffffff, 0.9);
    dir.position.set(200, 300, 400);
    this.scene.add(dir);

    this.grid = this.createGrid();
    this.scene.add(this.grid);

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

  /** Configure pointer interaction behaviour for the OrbitControls. */
  setCursorMode(mode: ViewerCursorMode): void {
    const c = this.controls;
    c.enableRotate = true;
    c.enablePan = true;
    c.enableZoom = true;
    switch (mode) {
      case 'orbit':
      case 'rotate':
        c.mouseButtons = { LEFT: MOUSE.ROTATE, MIDDLE: MOUSE.DOLLY, RIGHT: MOUSE.PAN };
        break;
      case 'pan':
        c.mouseButtons = { LEFT: MOUSE.PAN, MIDDLE: MOUSE.DOLLY, RIGHT: MOUSE.ROTATE };
        break;
      case 'zoom':
        c.mouseButtons = { LEFT: MOUSE.DOLLY, MIDDLE: MOUSE.DOLLY, RIGHT: MOUSE.PAN };
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
    if (this.animation) {
      this.advanceAnimation();
    } else {
      this.controls.update();
    }
    this.renderer.render(this.scene, this.camera);
  };

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

  /** Build the build-plate grid using the current themed border colour. */
  private createGrid(): GridHelper {
    const color = readBorderColor();
    const grid = new GridHelper(400, 40, color, color);
    grid.rotation.x = Math.PI / 2; // GridHelper is XZ by default; rotate to XY ground plane
    grid.renderOrder = 0;
    return grid;
  }

  /** Re-read `--color-border` and rebuild the grid so it tracks theme changes. */
  private refreshGridColor(): void {
    // GridHelper bakes its two colours into per-vertex colour attributes on
    // its geometry, so a live `material.color.set(...)` has no effect.
    // Cheapest correct fix: swap the helper for a freshly-built one.
    const replacement = this.createGrid();
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    this.scene.add(replacement);
    this.grid = replacement;
  }

  // ---------------------------------------------------------------------------
  // View-preset animation
  // ---------------------------------------------------------------------------

  private contentBoundingSphere(): Sphere | null {
    const box = new Box3().setFromObject(this.contentRoot);
    if (box.isEmpty()) {
      // Fall back to the build-plate grid extent so the camera still has
      // something sensible to frame when no model is loaded.
      box.set(new Vector3(-100, -100, 0), new Vector3(100, 100, 0));
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

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
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
