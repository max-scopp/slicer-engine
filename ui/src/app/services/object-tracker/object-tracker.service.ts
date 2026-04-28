import { Injectable, signal } from '@angular/core';

import { SceneObject } from './scene-object';
import { SceneObjectInit } from './types';

/**
 * Authoritative registry of every {@link SceneObject} currently placed in
 * the 3D scene. Owns each instance's lifecycle (create / remove / clear)
 * and exposes the live list as a signal so consumers (the print-area
 * service for selection + drag, the viewer for mesh mirroring, settings
 * UIs for inspection) can react without holding onto raw references.
 *
 * The tracker is intentionally minimal: it does not know about meshes,
 * the bed, or selection. It just answers "what is on the scene right
 * now and what is each object's transform?".
 */
@Injectable({ providedIn: 'root' })
export class ObjectTrackerService {
  private readonly _objects = signal<readonly SceneObject[]>([]);
  /** Monotonic counter used when the caller doesn't supply an id. */
  private idSeq = 0;

  /** Live, read-only list of every tracked object. */
  readonly objects = this._objects.asReadonly();

  /**
   * Create a new {@link SceneObject} and append it to the tracked list.
   * Returns the instance so callers can keep a strong reference (e.g. the
   * viewer pairs it with the corresponding Three.js mesh).
   */
  create(init: SceneObjectInit = {}): SceneObject {
    const id = init.id ?? this.nextId();
    if (this._objects().some((o) => o.id === id)) {
      throw new Error(`ObjectTrackerService: duplicate object id "${id}"`);
    }
    const obj = new SceneObject(id, init);
    this._objects.set([...this._objects(), obj]);
    return obj;
  }

  /** Look up an object by id, or `null` if unknown. */
  get(id: string): SceneObject | null {
    return this._objects().find((o) => o.id === id) ?? null;
  }

  /** `true` if an object with the given id is currently tracked. */
  has(id: string): boolean {
    return this._objects().some((o) => o.id === id);
  }

  /** Remove an object by id. Returns the dropped instance, or `null`. */
  remove(id: string): SceneObject | null {
    const list = this._objects();
    const idx = list.findIndex((o) => o.id === id);
    if (idx === -1) {
      return null;
    }
    const dropped = list[idx];
    this._objects.set([...list.slice(0, idx), ...list.slice(idx + 1)]);
    return dropped;
  }

  /** Drop every tracked object (e.g. when switching scene sources). */
  clear(): void {
    if (this._objects().length === 0) {
      return;
    }
    this._objects.set([]);
  }

  private nextId(): string {
    return `obj-${++this.idSeq}`;
  }
}
