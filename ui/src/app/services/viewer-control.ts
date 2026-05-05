import { Injectable, signal } from '@angular/core';
import { Vector3 } from 'three';
import type { ViewerMode } from '../components/viewer';

export type ViewerView = 'perspective' | 'ortho';
/**
 * Camera-navigation cursor mode. Controls how left-drag on empty space is
 * interpreted by OrbitControls. Object manipulation is governed separately
 * by {@link ObjectMode}.
 */
/**
 * Object-manipulation mode. Drives the on-canvas gizmo for the current
 * selection. `'none'` is the default — no gizmo is shown, clicks select.
 * `'pullToFloor'` is a transient face-pick mode that auto-exits to
 * `'none'` after a single face has been picked.
 */
export type ObjectMode = 'none' | 'translate' | 'rotate' | 'scale' | 'pullToFloor';

/**
 * Shared state between the 3D-view toolbar and the viewer component.
 *
 * The toolbar lives in the layout shell and the viewer in the routed page,
 * so the two are wired together through this lightweight signal-based store
 * rather than via component I/O.
 */
@Injectable({ providedIn: 'root' })
export class ViewerControl {
  /** Currently selected camera view preset. */
  readonly view = signal<ViewerView>('perspective');

  /** Whether the viewport shows the raw mesh ('model') or sliced G-code ('gcode'). */
  readonly viewMode = signal<ViewerMode>('model');

  /**
   * Currently selected object-manipulation mode. Drives the gizmo shown
   * over the current selection. Independent of camera orbit/pan — the
   * user picks a camera mode and an object mode separately.
   */
  readonly objectMode = signal<ObjectMode>('translate');

  /**
   * Monotonically increasing counter that is bumped every time the user
   * asks the viewer to reset its camera. The viewer reacts to changes of
   * this signal — the value itself is irrelevant.
   */
  readonly resetTick = signal(0);

  /**
   * Live camera orientation, updated by the viewer every frame. Read by the
   * viewport-cube gizmo (which mirrors the main camera in its own scene)
   * without going through Angular's change-detection pipeline.
   */
  readonly cameraState = {
    /** Unit vector from the controls target toward the camera. */
    direction: new Vector3(1, -1, 0.8).normalize(),
    /** Camera up vector. */
    up: new Vector3(0, 0, 1),
    /**
     * Live perspective field-of-view (degrees) of the main camera. The
     * viewport-cube mirrors this so its own projection matches — small FOV
     * (~1°) reads as orthographic, ~45° as perspective.
     */
    fov: 45,
  };

  /**
   * When `true`, every completed object-manipulation gesture automatically
   * drops the affected objects to the floor (applies `DropToFloor`) so
   * objects never float above the bed after being moved or rotated.
   */
  readonly gravityEnabled = signal(false);

  /**
   * Pending request for the viewer to animate to a specific look direction
   * (e.g. when the user clicks a face of the viewport-cube). Cleared after
   * the viewer consumes it; the `tick` field disambiguates repeated requests
   * for the same direction.
   */
  readonly lookRequest = signal<{ direction: Vector3; up: Vector3; tick: number } | null>(null);
  private lookTick = 0;

  /**
   * Direct callback for high-frequency incremental orbit deltas (radians).
   * Set by the viewer; invoked by the viewport-cube gizmo while the user
   * drags it. Bypasses signal/effect overhead.
   */
  orbitSink: ((azimuth: number, polar: number) => void) | null = null;

  /** Request the viewer to fully reset its camera framing. */
  reset(): void {
    this.view.set('perspective');
    this.resetTick.update((v) => v + 1);
  }

  /**
   * Ask the viewer to animate to a specific camera direction (unit vector
   * from the controls target toward the camera) with the given up vector.
   * The current target and distance are preserved.
   */
  lookFrom(direction: Vector3, up: Vector3): void {
    this.lookTick += 1;
    this.lookRequest.set({
      direction: direction.clone().normalize(),
      up: up.clone().normalize(),
      tick: this.lookTick,
    });
  }
}
