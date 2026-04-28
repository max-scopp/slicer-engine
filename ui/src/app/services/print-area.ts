import { Injectable, computed, signal } from '@angular/core';

/**
 * Description of the printer's build volume in machine coordinates.
 *
 * The viewer's world origin (0, 0, 0) — where the RGB axis gizmo sits — is
 * always the printer's machine origin. The build plate ("printable area") is
 * a rectangle whose dimensions are given by {@link printableAreaWidth} /
 * {@link printableAreaHeight} and whose lower-left corner is offset from the
 * machine origin by ({@link movableAreaX}, {@link movableAreaY}).
 *
 * That separation lets us model real-world printers correctly: many machines
 * can drive their toolhead to coordinates that lie outside the physical bed
 * (e.g. for purge towers, wipe positions, parking) so the bed itself does
 * not have to start at (0, 0).
 */
export interface PrintAreaConfig {
  /** Width of the bed in millimetres (along the world +X axis). */
  printableAreaWidth: number;
  /** Depth of the bed in millimetres (along the world +Y axis). */
  printableAreaHeight: number;
  /** X offset of the bed's lower-left corner from the machine origin (mm). */
  movableAreaX: number;
  /** Y offset of the bed's lower-left corner from the machine origin (mm). */
  movableAreaY: number;
}

/**
 * Identity + position of an object the user is manipulating on the bed.
 *
 * Positions are given in machine coordinates (millimetres) and refer to the
 * object's anchor point (typically its footprint centre, but the service is
 * agnostic — callers decide what `x` / `y` mean as long as they stay
 * consistent). Optional `name` is purely informational for UI lists.
 */
export interface TrackedObject {
  id: string;
  /** Position in machine coordinates (mm). */
  x: number;
  y: number;
  /** Optional human-readable label for UI. */
  name?: string;
}

/** Snapshot of a single object's position at the moment a drag began. */
export interface DragAnchor {
  id: string;
  startX: number;
  startY: number;
}

/** Options accepted by {@link PrintAreaService.select}. */
export interface SelectOptions {
  /**
   * When `true`, the id is added to (or removed from, if already present)
   * the existing selection — the ctrl/⌘+click semantics. When `false`
   * (default) the selection is replaced with just this id.
   */
  additive?: boolean;
}

const DEFAULT_CONFIG: PrintAreaConfig = {
  printableAreaWidth: 220,
  printableAreaHeight: 220,
  movableAreaX: 0,
  movableAreaY: 0,
};

/**
 * Single source of truth for the build volume shown in the 3D viewer.
 *
 * Owns:
 * - the print-area dimensions and the bed offset within machine space;
 * - the list of objects placed on the bed and their machine-space positions;
 * - which of those objects are currently selected (supports multi-select);
 * - a transient "drag anchor" snapshot that lets the viewer translate a
 *   pointer delta into a position update without losing precision over the
 *   course of a multi-frame drag gesture.
 *
 * The service is intentionally framework-agnostic: it has no knowledge of
 * Three.js, the DOM, or any specific viewer. The viewer reads the signals
 * it needs (`objects`, `selectedIds`, …) and calls the mutation methods
 * (`select`, `beginDragSelected`, `dragSelectedBy`, …) in response to
 * pointer events.
 */
@Injectable({ providedIn: 'root' })
export class PrintAreaService {
  private readonly _config = signal<PrintAreaConfig>({ ...DEFAULT_CONFIG });

  /** Live, read-only view of the current print-area configuration. */
  readonly config = this._config.asReadonly();

  /** Convenience: bed bounds in machine coordinates (lower-left → upper-right). */
  readonly bounds = computed(() => {
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
  // Object tracking
  // ---------------------------------------------------------------------------

  private readonly _objects = signal<readonly TrackedObject[]>([]);
  private readonly _selectedIds = signal<ReadonlySet<string>>(new Set());
  private readonly _dragAnchors = signal<readonly DragAnchor[]>([]);

  /** Read-only list of objects currently placed on the bed. */
  readonly objects = this._objects.asReadonly();

  /** Read-only set of currently selected object ids. */
  readonly selectedIds = this._selectedIds.asReadonly();

  /** Resolved selected objects in their current order on the bed. */
  readonly selectedObjects = computed<readonly TrackedObject[]>(() => {
    const ids = this._selectedIds();
    if (ids.size === 0) {
      return [];
    }
    return this._objects().filter((o) => ids.has(o.id));
  });

  /**
   * Snapshot of where each currently-dragged object started. Empty unless a
   * drag gesture is in progress (between {@link beginDragSelected} and
   * {@link endDrag}). UI overlays can read this to show "from → to" hints.
   */
  readonly dragAnchors = this._dragAnchors.asReadonly();

  /** `true` while a drag gesture is in progress. */
  readonly isDragging = computed(() => this._dragAnchors().length > 0);

  // ---------------------------------------------------------------------------
  // Print-area configuration
  // ---------------------------------------------------------------------------

  /** Replace the entire print-area configuration. */
  setConfig(next: PrintAreaConfig): void {
    const sanitised: PrintAreaConfig = {
      printableAreaWidth: ensurePositive(
        next.printableAreaWidth,
        DEFAULT_CONFIG.printableAreaWidth,
      ),
      printableAreaHeight: ensurePositive(
        next.printableAreaHeight,
        DEFAULT_CONFIG.printableAreaHeight,
      ),
      movableAreaX: ensureFinite(next.movableAreaX, 0),
      movableAreaY: ensureFinite(next.movableAreaY, 0),
    };
    this._config.set(sanitised);
  }

  /** Patch a subset of the print-area configuration. */
  updateConfig(patch: Partial<PrintAreaConfig>): void {
    this.setConfig({ ...this._config(), ...patch });
  }

  /** Reset to the default printer configuration and clear all objects. */
  reset(): void {
    this._config.set({ ...DEFAULT_CONFIG });
    this._objects.set([]);
    this._selectedIds.set(new Set());
    this._dragAnchors.set([]);
  }

  // ---------------------------------------------------------------------------
  // Object CRUD
  // ---------------------------------------------------------------------------

  /** Add or replace an object by id. */
  upsertObject(obj: TrackedObject): void {
    const list = this._objects();
    const idx = list.findIndex((o) => o.id === obj.id);
    if (idx === -1) {
      this._objects.set([...list, { ...obj }]);
      return;
    }
    const next = list.slice();
    next[idx] = { ...obj };
    this._objects.set(next);
  }

  /** Remove an object by id. Also drops it from selection / drag anchors. */
  removeObject(id: string): void {
    const list = this._objects();
    const next = list.filter((o) => o.id !== id);
    if (next.length !== list.length) {
      this._objects.set(next);
    }

    const sel = this._selectedIds();
    if (sel.has(id)) {
      const nextSel = new Set(sel);
      nextSel.delete(id);
      this._selectedIds.set(nextSel);
    }

    const anchors = this._dragAnchors();
    if (anchors.some((a) => a.id === id)) {
      this._dragAnchors.set(anchors.filter((a) => a.id !== id));
    }
  }

  /** Look up an object by id, or `null` if unknown. */
  getObject(id: string): TrackedObject | null {
    return this._objects().find((o) => o.id === id) ?? null;
  }

  // ---------------------------------------------------------------------------
  // Selection
  // ---------------------------------------------------------------------------

  /** `true` if the given object is currently selected. */
  isSelected(id: string): boolean {
    return this._selectedIds().has(id);
  }

  /**
   * Select an object. By default this replaces the current selection; pass
   * `{ additive: true }` (the ctrl/⌘+click case) to extend the selection
   * instead — re-selecting an already-selected id with `additive` removes it.
   *
   * Selecting an unknown id is a no-op.
   */
  select(id: string, options: SelectOptions = {}): void {
    if (!this._objects().some((o) => o.id === id)) {
      return;
    }
    const current = this._selectedIds();
    if (options.additive) {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      this._selectedIds.set(next);
      return;
    }
    if (current.size === 1 && current.has(id)) {
      return;
    }
    this._selectedIds.set(new Set([id]));
  }

  /** Toggle selection state of the given id (equivalent to additive select). */
  toggleSelection(id: string): void {
    this.select(id, { additive: true });
  }

  /** Replace the current selection with the given ids (unknown ids dropped). */
  setSelection(ids: Iterable<string>): void {
    const known = new Set(this._objects().map((o) => o.id));
    const next = new Set<string>();
    for (const id of ids) {
      if (known.has(id)) {
        next.add(id);
      }
    }
    this._selectedIds.set(next);
  }

  /** Remove a single id from the selection. */
  deselect(id: string): void {
    const current = this._selectedIds();
    if (!current.has(id)) {
      return;
    }
    const next = new Set(current);
    next.delete(id);
    this._selectedIds.set(next);
  }

  /** Clear the entire selection (e.g. clicking the empty bed). */
  clearSelection(): void {
    if (this._selectedIds().size === 0) {
      return;
    }
    this._selectedIds.set(new Set());
  }

  /** Select every object currently on the bed. */
  selectAll(): void {
    const all = this._objects();
    if (all.length === 0) {
      this.clearSelection();
      return;
    }
    this._selectedIds.set(new Set(all.map((o) => o.id)));
  }

  // ---------------------------------------------------------------------------
  // Movement
  // ---------------------------------------------------------------------------

  /** Set an object's absolute machine-space position. */
  setObjectPosition(id: string, x: number, y: number): void {
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      return;
    }
    const list = this._objects();
    const idx = list.findIndex((o) => o.id === id);
    if (idx === -1) {
      return;
    }
    const current = list[idx];
    if (current.x === x && current.y === y) {
      return;
    }
    const next = list.slice();
    next[idx] = { ...current, x, y };
    this._objects.set(next);
  }

  /** Translate a single object by a delta (mm). */
  moveObjectBy(id: string, dx: number, dy: number): void {
    const obj = this.getObject(id);
    if (!obj) {
      return;
    }
    this.setObjectPosition(id, obj.x + dx, obj.y + dy);
  }

  /**
   * Snapshot the current positions of all selected objects so a subsequent
   * sequence of {@link dragSelectedBy} calls can apply a delta from the
   * gesture's starting frame rather than accumulating frame-to-frame error.
   *
   * Returns the captured anchors (also exposed via {@link dragAnchors}). If
   * nothing is selected the snapshot is empty and {@link isDragging} stays
   * `false`.
   */
  beginDragSelected(): readonly DragAnchor[] {
    const selected = this.selectedObjects();
    const anchors: DragAnchor[] = selected.map((o) => ({
      id: o.id,
      startX: o.x,
      startY: o.y,
    }));
    this._dragAnchors.set(anchors);
    return anchors;
  }

  /**
   * Apply a pointer delta (mm in machine space) to every object captured by
   * the most recent {@link beginDragSelected} call. Each object's new
   * position is `anchor + delta` — independent of how many times this method
   * has been called during the gesture, which keeps the drag visually
   * consistent even when frames are dropped.
   *
   * No-op if no drag is in progress.
   */
  dragSelectedBy(dx: number, dy: number): void {
    const anchors = this._dragAnchors();
    if (anchors.length === 0 || !Number.isFinite(dx) || !Number.isFinite(dy)) {
      return;
    }
    const list = this._objects();
    const byId = new Map(anchors.map((a) => [a.id, a] as const));
    let changed = false;
    const next = list.map((o) => {
      const a = byId.get(o.id);
      if (!a) {
        return o;
      }
      const nx = a.startX + dx;
      const ny = a.startY + dy;
      if (o.x === nx && o.y === ny) {
        return o;
      }
      changed = true;
      return { ...o, x: nx, y: ny };
    });
    if (changed) {
      this._objects.set(next);
    }
  }

  /** End the active drag gesture and discard the position snapshot. */
  endDrag(): void {
    if (this._dragAnchors().length === 0) {
      return;
    }
    this._dragAnchors.set([]);
  }

  /**
   * Cancel an active drag and restore each affected object to the position
   * captured at {@link beginDragSelected}. Useful for Esc-to-cancel handling.
   */
  cancelDrag(): void {
    const anchors = this._dragAnchors();
    if (anchors.length === 0) {
      return;
    }
    const list = this._objects();
    const byId = new Map(anchors.map((a) => [a.id, a] as const));
    let changed = false;
    const next = list.map((o) => {
      const a = byId.get(o.id);
      if (!a) {
        return o;
      }
      if (o.x === a.startX && o.y === a.startY) {
        return o;
      }
      changed = true;
      return { ...o, x: a.startX, y: a.startY };
    });
    if (changed) {
      this._objects.set(next);
    }
    this._dragAnchors.set([]);
  }
}

function ensurePositive(value: number, fallback: number): number {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function ensureFinite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}
