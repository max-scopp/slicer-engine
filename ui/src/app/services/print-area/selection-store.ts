import { Signal, computed, signal } from '@angular/core';

import { SceneObject } from '../object-tracker';
import { SelectOptions } from './types';

/**
 * Owns the "currently selected ids" signal and every operation that mutates
 * it. Lives as a plain class (not an `@Injectable`) so `PrintAreaService`
 * can compose it without polluting Angular's DI graph.
 *
 * The store is decoupled from the source of objects: it asks for the live
 * list through the `objects` accessor passed to the constructor, so the
 * `ObjectTrackerService` can keep ownership of the {@link SceneObject}
 * instances while we just react to whatever it currently holds.
 */
export class SelectionStore {
  private readonly _selectedIds = signal<ReadonlySet<string>>(new Set());

  /** Live, read-only set of currently selected object ids. */
  readonly selectedIds = this._selectedIds.asReadonly();

  /** Resolved selected objects in their current order in the tracker. */
  readonly selectedObjects: Signal<readonly SceneObject[]> = computed(() => {
    const ids = this._selectedIds();
    if (ids.size === 0) {
      return [];
    }
    return this.objects().filter((o) => ids.has(o.id));
  });

  constructor(private readonly objects: () => readonly SceneObject[]) {}

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
    if (!this.objects().some((o) => o.id === id)) {
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
  toggle(id: string): void {
    this.select(id, { additive: true });
  }

  /** Replace the current selection with the given ids (unknown ids dropped). */
  setMany(ids: Iterable<string>): void {
    const known = new Set(this.objects().map((o) => o.id));
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
  clear(): void {
    if (this._selectedIds().size === 0) {
      return;
    }
    this._selectedIds.set(new Set());
  }

  /** Select every object currently tracked. */
  selectAll(): void {
    const all = this.objects();
    if (all.length === 0) {
      this.clear();
      return;
    }
    this._selectedIds.set(new Set(all.map((o) => o.id)));
  }

  /**
   * Drop a single id from the selection without comparing against the
   * objects list. Used by the parent service when an object disappears
   * from the tracker so the selection signal stays consistent.
   */
  forget(id: string): void {
    const current = this._selectedIds();
    if (!current.has(id)) {
      return;
    }
    const next = new Set(current);
    next.delete(id);
    this._selectedIds.set(next);
  }
}
