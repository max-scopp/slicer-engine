import { Signal, computed, signal } from '@angular/core';

import { EulerRotation, IDENTITY_TRANSFORM, SceneObjectInit, Transform, Vec3 } from './types';

/**
 * Live, reactive representation of a single object placed in the 3D scene.
 *
 * Each instance owns its own signals for translation, rotation, and scale,
 * so consumers (the Three.js mesh mirror in the viewer, transform gizmos, a
 * placement-list UI, etc.) can subscribe to exactly the slice they care
 * about without re-rendering on unrelated changes.
 *
 * `SceneObject`s are created and owned by `ObjectTrackerService`; that
 * service is the only thing that should `new` them so the tracker can keep
 * its `objects` signal authoritative.
 */
export class SceneObject {
  readonly id: string;
  /** Optional human-readable label; mutated freely by the owner. */
  name: string | undefined;

  private readonly _position = signal<Vec3>({ ...IDENTITY_TRANSFORM.position });
  private readonly _rotation = signal<EulerRotation>({ ...IDENTITY_TRANSFORM.rotation });
  private readonly _scale = signal<Vec3>({ ...IDENTITY_TRANSFORM.scale });

  /** Live position in machine coordinates (mm). */
  readonly position = this._position.asReadonly();
  /** Live Euler rotation (radians, XYZ order). */
  readonly rotation = this._rotation.asReadonly();
  /** Live scale factors per axis. */
  readonly scale = this._scale.asReadonly();

  /**
   * Convenience aggregate: a single read-only handle that tracks any
   * change to the position / rotation / scale signals. Useful for the
   * viewer's mesh-mirror effect, which wants one dependency per object
   * regardless of which channel updated.
   */
  readonly transform: Signal<Transform> = computed(() => ({
    position: this._position(),
    rotation: this._rotation(),
    scale: this._scale(),
  }));

  constructor(id: string, init: SceneObjectInit = {}) {
    this.id = id;
    this.name = init.name;
    if (init.position) {
      this._position.set({ ...IDENTITY_TRANSFORM.position, ...init.position });
    }
    if (init.rotation) {
      this._rotation.set({ ...IDENTITY_TRANSFORM.rotation, ...init.rotation });
    }
    if (init.scale) {
      this._scale.set({ ...IDENTITY_TRANSFORM.scale, ...init.scale });
    }
  }

  // ---------------------------------------------------------------------------
  // Position
  // ---------------------------------------------------------------------------

  /** Set the absolute position (any omitted axis keeps its current value). */
  setPosition(x: number, y: number, z?: number): void {
    const current = this._position();
    if (!Number.isFinite(x) || !Number.isFinite(y) || (z !== undefined && !Number.isFinite(z))) {
      return;
    }
    const nz = z ?? current.z;
    if (current.x === x && current.y === y && current.z === nz) {
      return;
    }
    this._position.set({ x, y, z: nz });
  }

  /** Translate by a delta on each axis (Z optional). */
  translate(dx: number, dy: number, dz = 0): void {
    const current = this._position();
    this.setPosition(current.x + dx, current.y + dy, current.z + dz);
  }

  // ---------------------------------------------------------------------------
  // Rotation
  // ---------------------------------------------------------------------------

  /** Set the absolute Euler rotation (radians, XYZ order). */
  setRotation(x: number, y: number, z: number): void {
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) {
      return;
    }
    const current = this._rotation();
    if (current.x === x && current.y === y && current.z === z) {
      return;
    }
    this._rotation.set({ x, y, z });
  }

  /** Rotate by a delta on each Euler axis (radians). */
  rotateBy(dx: number, dy: number, dz: number): void {
    const current = this._rotation();
    this.setRotation(current.x + dx, current.y + dy, current.z + dz);
  }

  // ---------------------------------------------------------------------------
  // Scale
  // ---------------------------------------------------------------------------

  /** Set the absolute scale per axis. */
  setScale(x: number, y: number, z: number): void {
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) {
      return;
    }
    const current = this._scale();
    if (current.x === x && current.y === y && current.z === z) {
      return;
    }
    this._scale.set({ x, y, z });
  }

  /** Apply a uniform scale factor on every axis. */
  setUniformScale(s: number): void {
    this.setScale(s, s, s);
  }

  // ---------------------------------------------------------------------------
  // Whole-transform helpers
  // ---------------------------------------------------------------------------

  /** Snapshot the current transform as a plain value (no live binding). */
  snapshot(): Transform {
    return {
      position: { ...this._position() },
      rotation: { ...this._rotation() },
      scale: { ...this._scale() },
    };
  }

  /** Restore a previously taken snapshot or any partial transform. */
  applyTransform(t: Partial<Transform>): void {
    if (t.position) {
      this.setPosition(
        t.position.x ?? this._position().x,
        t.position.y ?? this._position().y,
        t.position.z ?? this._position().z,
      );
    }
    if (t.rotation) {
      this.setRotation(
        t.rotation.x ?? this._rotation().x,
        t.rotation.y ?? this._rotation().y,
        t.rotation.z ?? this._rotation().z,
      );
    }
    if (t.scale) {
      this.setScale(
        t.scale.x ?? this._scale().x,
        t.scale.y ?? this._scale().y,
        t.scale.z ?? this._scale().z,
      );
    }
  }

  /** Reset to the identity transform. */
  resetTransform(): void {
    this._position.set({ ...IDENTITY_TRANSFORM.position });
    this._rotation.set({ ...IDENTITY_TRANSFORM.rotation });
    this._scale.set({ ...IDENTITY_TRANSFORM.scale });
  }
}
