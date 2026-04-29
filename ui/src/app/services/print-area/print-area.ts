import { Injectable, computed, inject, signal } from '@angular/core';

import { ObjectTracker, SceneObject } from '../object-tracker';
import { DragStore } from './drag-store';
import { sanitisePrintAreaConfig } from './sanitise';
import { SelectionStore } from './selection-store';
import {
  DEFAULT_PRINT_AREA_CONFIG,
  PrintAreaBounds,
  PrintAreaConfig,
  SelectOptions,
} from './types';

/**
 * Build-volume orchestrator. Owns the print-area configuration and routes
 * pointer-driven selection / drag gestures from the viewer into the
 * {@link ObjectTracker}.
 *
 * Responsibilities are kept narrow on purpose:
 * - **Configuration** — bed dimensions and the bed offset within machine
 *   space (this file).
 * - **Selection** — which tracked objects are currently selected
 *   ({@link SelectionStore}).
 * - **Drag** — the transient XY snapshot taken at the start of a pointer
 *   drag, plus the per-frame translation it produces ({@link DragStore}).
 *
 * Everything related to *what* objects exist and *what their transforms
 * are* lives in {@link ObjectTracker}; this service merely consumes
 * the tracker through Angular DI and exposes its `objects` signal as a
 * convenience re-export.
 */
@Injectable({ providedIn: 'root' })
export class PrintArea {
  private readonly tracker = inject(ObjectTracker);

  // ---------------------------------------------------------------------------
  // Configuration
  // ---------------------------------------------------------------------------

  private readonly _config = signal<PrintAreaConfig>({ ...DEFAULT_PRINT_AREA_CONFIG });

  /** Live, read-only view of the current print-area configuration. */
  readonly config = this._config.asReadonly();

  /** Convenience: bed bounds in machine coordinates (lower-left → upper-right). */
  readonly bounds = computed<PrintAreaBounds>(() => {
    const c = this._config();
    return {
      minX: c.movableAreaX,
      minY: c.movableAreaY,
      maxX: c.movableAreaX + c.printableAreaWidth,
      maxY: c.movableAreaY + c.printableAreaHeight,
      centerX: c.movableAreaX + c.printableAreaWidth / 2,
      centerY: c.movableAreaY + c.printableAreaHeight / 2,
    };
  });

  // ---------------------------------------------------------------------------
  // Sub-stores (driven from the tracker)
  // ---------------------------------------------------------------------------

  private readonly selection = new SelectionStore(() => this.tracker.objects());
  private readonly drag = new DragStore(
    (id) => this.tracker.get(id),
    () => this.selection.selectedObjects(),
  );

  /** Live, read-only list of every tracked object (mirrors the tracker). */
  readonly objects = this.tracker.objects;
  /** Read-only set of currently selected object ids. */
  readonly selectedIds = this.selection.selectedIds;
  /** Resolved selected objects in their current order. */
  readonly selectedObjects = this.selection.selectedObjects;

  /** Snapshot of where each currently-dragged object started. */
  readonly dragAnchors = this.drag.anchors;
  /** `true` while a drag gesture is in progress. */
  readonly isDragging = this.drag.isDragging;

  // ---------------------------------------------------------------------------
  // Configuration mutations
  // ---------------------------------------------------------------------------

  /** Replace the entire print-area configuration. */
  setConfig(next: PrintAreaConfig): void {
    this._config.set(sanitisePrintAreaConfig(next));
  }

  /** Patch a subset of the print-area configuration. */
  updateConfig(patch: Partial<PrintAreaConfig>): void {
    this.setConfig({ ...this._config(), ...patch });
  }

  /** Reset config to defaults, clear selection / drag state, drop tracker. */
  reset(): void {
    this._config.set({ ...DEFAULT_PRINT_AREA_CONFIG });
    this.selection.clear();
    this.drag.reset();
    this.tracker.clear();
  }

  // ---------------------------------------------------------------------------
  // Object lookup (delegated to the tracker for ergonomics)
  // ---------------------------------------------------------------------------

  /** Look up a tracked object by id, or `null` if unknown. */
  getObject(id: string): SceneObject | null {
    return this.tracker.get(id);
  }

  // ---------------------------------------------------------------------------
  // Selection — thin delegations
  // ---------------------------------------------------------------------------

  isSelected(id: string): boolean {
    return this.selection.isSelected(id);
  }

  select(id: string, options: SelectOptions = {}): void {
    this.selection.select(id, options);
  }

  toggleSelection(id: string): void {
    this.selection.toggle(id);
  }

  setSelection(ids: Iterable<string>): void {
    this.selection.setMany(ids);
  }

  deselect(id: string): void {
    this.selection.deselect(id);
  }

  clearSelection(): void {
    this.selection.clear();
  }

  selectAll(): void {
    this.selection.selectAll();
  }

  /**
   * Forget any references to `id` held by the selection / drag stores.
   * Call this *before* removing the object from the tracker so the signals
   * stay consistent for any subscriber that observes the removal.
   */
  forgetObject(id: string): void {
    this.selection.forget(id);
    this.drag.forget(id);
  }

  // ---------------------------------------------------------------------------
  // Drag — thin delegations to the drag store
  // ---------------------------------------------------------------------------

  beginDragSelected() {
    return this.drag.begin();
  }

  dragSelectedBy(dx: number, dy: number): void {
    this.drag.by(dx, dy);
  }

  endDrag(): void {
    this.drag.end();
  }

  cancelDrag(): void {
    this.drag.cancel();
  }
}
