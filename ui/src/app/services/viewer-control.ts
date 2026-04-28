import { Injectable, signal } from '@angular/core';

export type ViewerView = '3D' | 'Top' | 'Front';
export type ViewerCursorMode = 'orbit' | 'pan' | 'zoom' | 'rotate' | 'pullToSurface';

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
  readonly view = signal<ViewerView>('3D');

  /** Currently selected pointer interaction mode. */
  readonly cursorMode = signal<ViewerCursorMode>('orbit');

  /**
   * Monotonically increasing counter that is bumped every time the user
   * asks the viewer to reset its camera. The viewer reacts to changes of
   * this signal — the value itself is irrelevant.
   */
  readonly resetTick = signal(0);

  /** Request the viewer to fully reset its camera framing. */
  reset(): void {
    this.view.set('3D');
    this.resetTick.update((v) => v + 1);
  }
}
