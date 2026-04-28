import {
  AmbientLight,
  Box3,
  BoxGeometry,
  DirectionalLight,
  GridHelper,
  Group,
  Mesh,
  MeshBasicMaterial,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
} from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';

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

  constructor(host: HTMLElement) {
    this.host = host;

    // Transparent background so the underlying page (including its themed
    // background colour) shows through.
    this.scene.background = null;
    this.scene.add(this.contentRoot);

    const { clientWidth, clientHeight } = this.sizeOf(host);
    // Use Z-up so STL/G-code coordinates (printer convention) render with Z
    // as height; XY is the build plate.
    this.camera = new PerspectiveCamera(45, clientWidth / clientHeight, 0.1, 5000);
    this.camera.up.set(0, 0, 1);
    // Diagonal start view: looking at origin from +X, -Y, slightly above the
    // plate, so the user immediately sees all three printer axes.
    this.camera.position.set(220, -240, 180);
    this.camera.lookAt(0, 0, 0);

    this.renderer = new WebGLRenderer({
      antialias: true,
      alpha: true,
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
  fitToContent(padding = 1.4): void {
    const box = new Box3().setFromObject(this.contentRoot);
    if (box.isEmpty()) {
      return;
    }
    const sphere = new Sphere();
    box.getBoundingSphere(sphere);
    const radius = Math.max(sphere.radius, 1);
    const fov = (this.camera.fov * Math.PI) / 180;
    const distance = (radius * padding) / Math.sin(fov / 2);
    // Match the initial diagonal Z-up view: front-right, slightly above.
    const dir = new Vector3(1, -1, 0.8).normalize();
    this.camera.position.copy(sphere.center).addScaledVector(dir, distance);
    this.camera.near = Math.max(distance / 1000, 0.1);
    this.camera.far = distance * 10;
    this.camera.updateProjectionMatrix();
    this.controls.target.copy(sphere.center);
    this.controls.update();
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
    this.controls.update();
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
