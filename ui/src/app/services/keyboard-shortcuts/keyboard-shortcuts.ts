import { Injectable, inject } from '@angular/core';
import { SceneHistory } from '../scene-history/scene-history';

/**
 * Registers global keyboard shortcuts for undo/redo.
 *
 * Must be eagerly instantiated — inject this class in the root `App`
 * component constructor to ensure shortcuts are active for the entire
 * application lifetime.
 *
 * Shortcuts:
 *   Ctrl+Z           — Undo
 *   Ctrl+Y           — Redo
 *   Ctrl+Shift+Z     — Redo (alternate, common on macOS/Linux)
 */
@Injectable({ providedIn: 'root' })
export class KeyboardShortcuts {
  private readonly history = inject(SceneHistory);

  constructor() {
    document.addEventListener('keydown', this.onKeyDown);
  }

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    const ctrl = event.ctrlKey || event.metaKey;
    if (!ctrl) {
      return;
    }

    if (event.key === 'z' && !event.shiftKey && this.history.canUndo()) {
      event.preventDefault();
      this.history.undo();
      return;
    }

    if ((event.key === 'y' || (event.key === 'z' && event.shiftKey)) && this.history.canRedo()) {
      event.preventDefault();
      this.history.redo();
    }
  };
}
