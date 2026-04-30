import { Injectable, signal } from '@angular/core';

/**
 * Shared toggle state for the code-editor side panel.
 *
 * Lives in the layout shell (visibility) and the toolbar (toggle button),
 * which are in separate component trees, so a root-level service is the
 * lightest bridge between them.
 */
@Injectable({ providedIn: 'root' })
export class EditorPanel {
  readonly visible = signal(false);

  toggle(): void {
    this.visible.update((v) => !v);
  }
}
