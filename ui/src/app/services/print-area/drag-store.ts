import { computed, signal } from '@angular/core';

import { SceneObject } from '../object-tracker';
import { DragAnchor } from './types';

/**
 * Owns the transient drag-anchor snapshot and the per-frame translation
 * applied to selected objects during a pointer drag.
 *
 * The store mutates {@link SceneObject} instances directly through their
 * `setPosition` API — they own their own transform, so we don't have to
 * re-emit the entire object list on every pointer move. Anchors are stored
 * as plain `{ startX, startY }` records; rotation / scale are intentionally
 * captured nowhere here because the bed-plane drag only translates in XY.
 */
export class DragStore {
  private readonly _anchors = signal<readonly DragAnchor[]>([]);

  /**
   * Snapshot of where each currently-dragged object started. Empty unless a
   * drag gesture is in progress (between {@link begin} and {@link end}).
   */
  readonly anchors = this._anchors.asReadonly();

  /** `true` while a drag gesture is in progress. */
  readonly isDragging = computed(() => this._anchors().length > 0);

  constructor(
    private readonly getObject: (id: string) => SceneObject | null,
    private readonly selectedObjects: () => readonly SceneObject[],
  ) {}

  /**
   * Snapshot the current XY positions of all selected objects so a
   * subsequent sequence of {@link by} calls can apply a delta from the
   * gesture's starting frame rather than accumulating frame-to-frame error.
   *
   * Returns the captured anchors (also exposed via {@link anchors}). If
   * nothing is selected the snapshot is empty and {@link isDragging} stays
   * `false`.
   */
  begin(): readonly DragAnchor[] {
    const selected = this.selectedObjects();
    const anchors: DragAnchor[] = selected.map((o) => {
      const p = o.position();
      return { id: o.id, startX: p.x, startY: p.y };
    });
    this._anchors.set(anchors);
    return anchors;
  }

  /**
   * Apply a pointer delta (mm in machine space) to every object captured by
   * the most recent {@link begin} call. Each object's new position is
   * `anchor + delta` — independent of how many times this method has been
   * called during the gesture, which keeps the drag visually consistent
   * even when frames are dropped.
   *
   * No-op if no drag is in progress.
   */
  by(dx: number, dy: number): void {
    const anchors = this._anchors();
    if (anchors.length === 0 || !Number.isFinite(dx) || !Number.isFinite(dy)) {
      return;
    }
    for (const a of anchors) {
      const obj = this.getObject(a.id);
      if (!obj) {
        continue;
      }
      // Preserve the object's current Z (objects may sit above the bed).
      obj.setPosition(a.startX + dx, a.startY + dy);
    }
  }

  /** End the active drag gesture and discard the position snapshot. */
  end(): void {
    if (this._anchors().length === 0) {
      return;
    }
    this._anchors.set([]);
  }

  /**
   * Cancel an active drag and restore each affected object to the position
   * captured at {@link begin}. Useful for Esc-to-cancel handling.
   */
  cancel(): void {
    const anchors = this._anchors();
    if (anchors.length === 0) {
      return;
    }
    for (const a of anchors) {
      const obj = this.getObject(a.id);
      if (!obj) {
        continue;
      }
      obj.setPosition(a.startX, a.startY);
    }
    this._anchors.set([]);
  }

  /**
   * Drop an in-flight anchor for the given id without restoring its
   * position — used by the parent service when an object is removed mid
   * drag so the snapshot stays consistent with the tracker.
   */
  forget(id: string): void {
    const anchors = this._anchors();
    if (!anchors.some((a) => a.id === id)) {
      return;
    }
    this._anchors.set(anchors.filter((a) => a.id !== id));
  }

  /** Reset all drag state without touching object positions. */
  reset(): void {
    this._anchors.set([]);
  }
}
