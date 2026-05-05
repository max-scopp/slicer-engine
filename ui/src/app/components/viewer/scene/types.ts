import type { GizmoDelta } from '../gizmo';

export interface SceneSelectionHandlers {
  /** A bare click on a selectable object — `additive` for ctrl/⌘/shift. */
  select(id: string, additive: boolean): void;
  /** Click landed on empty space (deselect). */
  clearSelection(): void;
}

export interface SceneGizmoHandlers {
  /** Fired on each frame's incremental delta during a drag. */
  delta(ids: readonly string[], delta: GizmoDelta): void;
  /** Fired when the gesture finishes (pointer-up). Flush history here. */
  end(): void;
  /** Fired when a face has been picked in `pullToFloor` mode. */
  facePicked(objectId: string, faceIndex: number): void;
}

export type ViewerView = 'perspective' | 'ortho';
