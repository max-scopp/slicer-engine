import { Box3, type Group, type PerspectiveCamera, Sphere, Vector3 } from 'three';
import type { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { PrintAreaConfig } from '../../../services/print-area';
import type { ViewerView } from './types';

const DEFAULT_VIEW_DIR = new Vector3(1, -1, 0.8).normalize();
const DEFAULT_FIT_PADDING = 1.4;
const VIEW_TRANSITION_MS = 600;
const PERSPECTIVE_FOV = 45;
const ORTHO_FOV = 1;
const INITIAL_CAMERA_OFFSET = new Vector3(220, -240, 180);
export const INITIAL_CAMERA_UP = new Vector3(0, 0, 1);
export const INITIAL_PERSPECTIVE_FOV = PERSPECTIVE_FOV;
const CAMERA_NEAR = 0.1;
const CAMERA_FAR = 1_000_000;

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

/**
 * Handles camera positioning, view-preset animations, fit-to-content,
 * and near/far plane management for the viewer scene.
 */
export class SceneCamera {
  private currentView: ViewerView = '3D';
  private animation: CameraAnimation | null = null;
  private printArea: PrintAreaConfig;

  constructor(
    private readonly camera: PerspectiveCamera,
    private readonly controls: OrbitControls,
    private readonly contentRoot: Group,
    initialPrintArea: PrintAreaConfig,
  ) {
    this.printArea = { ...initialPrintArea };
  }

  /**
   * Compute the default camera pose relative to the given print area.
   * Called from ViewerScene constructor before SceneCamera exists.
   */
  static computeInitialPose(config: PrintAreaConfig): { position: Vector3; target: Vector3 } {
    const { movableAreaX, movableAreaY, printableAreaWidth, printableAreaHeight } = config;
    const target = new Vector3(
      movableAreaX + printableAreaWidth / 2,
      movableAreaY + printableAreaHeight / 2,
      0,
    );
    return { position: target.clone().add(INITIAL_CAMERA_OFFSET), target };
  }

  setPrintArea(config: PrintAreaConfig): void {
    this.printArea = { ...config };
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

  setView(view: ViewerView): void {
    if (view === this.currentView && !this.animation) {
      return;
    }
    this.currentView = view;
    this.animateToView(view);
  }

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

  orbitBy(azimuth: number, polar: number): void {
    this.animation = null;
    this.controls.enabled = true;
    const target = this.controls.target;
    const offset = this.camera.position.clone().sub(target);
    const up = this.camera.up.clone().normalize();
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

  /** Advance an in-flight camera animation one frame. Returns true while animating. */
  advance(): boolean {
    if (!this.animation) {
      return false;
    }
    this.advanceAnimation();
    return this.animation !== null;
  }

  isAnimating(): boolean {
    return this.animation !== null;
  }

  updateNearFar(distance?: number, radius?: number): void {
    const dist =
      distance !== undefined && Number.isFinite(distance) && distance > 0
        ? distance
        : Math.max(this.camera.position.distanceTo(this.controls.target), 1);
    const { printableAreaWidth, printableAreaHeight } = this.printArea;
    const bedRadius = Math.max(printableAreaWidth, printableAreaHeight, 200);
    const sceneRadius = Math.max(radius ?? 0, bedRadius);
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
    near = quantise(near, 0.005);
    far = quantise(far, 0.005);
    if (this.camera.near !== near || this.camera.far !== far) {
      this.camera.near = near;
      this.camera.far = far;
      this.camera.updateProjectionMatrix();
    }
  }

  private initialPoseForBed(): { position: Vector3; target: Vector3 } {
    return SceneCamera.computeInitialPose(this.printArea);
  }

  private contentBoundingSphere(): Sphere | null {
    const box = new Box3().setFromObject(this.contentRoot);
    if (box.isEmpty()) {
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
    const offset = this.camera.position.clone().sub(fromTarget);
    const fromDistance = offset.length();
    const fromDir =
      fromDistance > 1e-6 ? offset.clone().divideScalar(fromDistance) : DEFAULT_VIEW_DIR.clone();
    const fromUp = this.camera.up.clone().normalize();
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
    const dir = anim.fromDir.clone().lerp(anim.toDir, eased);
    if (dir.lengthSq() < 1e-6) {
      dir.copy(anim.toDir);
    } else {
      dir.normalize();
    }
    const up = anim.fromUp.clone().lerp(anim.toUp, eased);
    if (up.lengthSq() < 1e-6) {
      up.copy(anim.toUp);
    } else {
      up.normalize();
    }
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
}

function quantise(value: number, step: number): number {
  if (value === 0) {
    return 0;
  }
  const scale = Math.abs(value) * step;
  return Math.round(value / scale) * scale;
}

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}
