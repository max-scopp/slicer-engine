import { Injectable, inject } from '@angular/core';
import { SceneCommand } from '../scene-command/scene-command';
import { SceneHistory } from '../scene-history/scene-history';

/**
 * Registers global keyboard shortcuts for undo/redo and scene operations.
 *
 * Must be eagerly instantiated — inject this class in the root `App`
 * component constructor to ensure shortcuts are active for the entire
 * application lifetime.
 *
 * Shortcuts:
 *   Ctrl+Z           — Undo
 *   Ctrl+Y           — Redo
 *   Ctrl+Shift+Z     — Redo (alternate, common on macOS/Linux)
 *   A (no modifier)  — Auto-orient all objects (or selected if any)
 */
@Injectable({ providedIn: 'root' })
export class KeyboardShortcuts {
  private readonly history = inject(SceneHistory);
  private readonly sceneCommand = inject(SceneCommand);

  constructor() {
    document.addEventListener('keydown', this.onKeyDown);
  }

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    const ctrl = event.ctrlKey || event.metaKey;

    if (ctrl) {
      if (event.key === 'z' && !event.shiftKey && this.history.canUndo()) {
        event.preventDefault();
        this.history.undo();
        return;
      }

      if ((event.key === 'y' || (event.key === 'z' && event.shiftKey)) && this.history.canRedo()) {
        event.preventDefault();
        this.history.redo();
      }
      return;
    }

    // Plain 'a' — auto-orient all objects. Guard against text inputs.
    if (event.key === 'a' && !this.isTextInput(event)) {
      event.preventDefault();
      this.sceneCommand.autoOrient();
    }
  };

  /** Returns true if the event originated from a text-entry element. */
  private isTextInput(event: KeyboardEvent): boolean {
    const target = event.target as HTMLElement | null;
    if (!target) {
      return false;
    }
    const tag = target.tagName.toUpperCase();
    return (
      tag === 'INPUT' ||
      tag === 'TEXTAREA' ||
      target.isContentEditable
    );
  }
}
