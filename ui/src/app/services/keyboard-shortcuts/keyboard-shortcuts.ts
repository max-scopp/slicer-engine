import { Injectable, inject } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { fromEvent } from 'rxjs';
import { filter, map } from 'rxjs/operators';
import { matchKeyBindingPress, parseKeybinding } from 'tinykeys';
import { GcodePreview } from '../gcode-preview';
import { SceneCommand } from '../scene-command/scene-command';
import { SceneHistory } from '../scene-history/scene-history';
import { Slicer } from '../slicer';
import { ViewerControl } from '../viewer-control';

interface ShortcutConfig {
  actionId: string;
  /** tinykeys-format shortcut string, e.g. `"$mod+z"`, `"$mod+Shift+z"`, `"a"`. */
  shortcut: string;
  /** Human-readable description of what the action does. */
  displayDescription: string;
  canMatch?: () => boolean;
  handleAction: () => void;
}

type ParsedShortcutConfig = ShortcutConfig & {
  _parsed: ReturnType<typeof parseKeybinding>;
};

/**
 * Registers global keyboard shortcuts for undo/redo and scene operations.
 *
 * Must be eagerly instantiated — inject this class in the root `App`
 * component constructor to ensure shortcuts are active for the entire
 * application lifetime.
 */
@Injectable({ providedIn: 'root' })
export class KeyboardShortcuts {
  private readonly history = inject(SceneHistory);
  private readonly sceneCommand = inject(SceneCommand);
  private readonly viewerControl = inject(ViewerControl);
  private readonly slicer = inject(Slicer);
  private readonly gcodePreview = inject(GcodePreview);

  private readonly shortcuts: ParsedShortcutConfig[] = [
    {
      actionId: 'undo',
      shortcut: '$mod+z',
      displayDescription: 'Undo',
      canMatch: () => this.history.canUndo(),
      handleAction: () => this.history.undo(),
    },
    {
      actionId: 'redo',
      shortcut: '$mod+y',
      displayDescription: 'Redo',
      canMatch: () => this.history.canRedo(),
      handleAction: () => this.history.redo(),
    },
    {
      actionId: 'redo-alt',
      shortcut: '$mod+Shift+z',
      displayDescription: 'Redo (alternate)',
      canMatch: () => this.history.canRedo(),
      handleAction: () => this.history.redo(),
    },
    {
      actionId: 'auto-orient',
      shortcut: 'a',
      displayDescription: 'Auto-orient all objects',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.sceneCommand.autoOrient(),
    },
    {
      actionId: 'object-mode-translate',
      shortcut: 'm',
      displayDescription: 'Switch to translate mode',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.viewerControl.objectMode.set('translate'),
    },
    {
      actionId: 'object-mode-rotate',
      shortcut: 'r',
      displayDescription: 'Switch to rotate mode',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.viewerControl.objectMode.set('rotate'),
    },
    {
      actionId: 'object-mode-scale',
      shortcut: 's',
      displayDescription: 'Switch to scale mode',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.viewerControl.objectMode.set('scale'),
    },
    {
      actionId: 'object-mode-pull-to-floor',
      shortcut: 'f',
      displayDescription: 'Switch to pull-face-to-floor mode',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.viewerControl.objectMode.set('pullToFloor'),
    },
    {
      actionId: 'toggle-gravity',
      shortcut: 'g',
      displayDescription: 'Toggle gravity',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.viewerControl.gravityEnabled.update((v) => !v),
    },
    {
      actionId: 'toggle-view-mode',
      shortcut: 'p',
      displayDescription: 'Toggle G-code preview / model view',
      canMatch: () => !this.isTextInputFocused(),
      handleAction: () => this.toggleViewMode(),
    },
  ].map((s) => ({ ...s, _parsed: parseKeybinding(s.shortcut) }));

  constructor() {
    fromEvent<KeyboardEvent>(document, 'keydown')
      .pipe(
        map((event) => ({ event, shortcut: this.findMatch(event) })),
        filter(({ shortcut }) => shortcut !== null),
        takeUntilDestroyed(),
      )
      .subscribe(({ event, shortcut }) => {
        event.preventDefault();
        shortcut!.handleAction();
      });
  }

  /**
   * Returns a human-readable shortcut label for the given action ID,
   * or `undefined` if no shortcut is registered.
   *
   * `$mod` is resolved to `Ctrl` on Windows/Linux and `⌘` on macOS.
   */
  shortcutFor(actionId: string): string | undefined {
    const config = this.shortcuts.find((s) => s.actionId === actionId);
    if (!config) {
      return undefined;
    }
    const isMac = navigator.platform.toUpperCase().includes('MAC');
    return config.shortcut.replace(/\$mod/g, isMac ? '⌘' : 'Ctrl').replace(/\+/g, '+');
  }

  /** Returns all registered shortcuts as plain data for display in a panel. */
  getAll(): { actionId: string; displayText: string; displayDescription: string }[] {
    return this.shortcuts.map(({ actionId, displayDescription }) => ({
      actionId,
      displayText: this.shortcutFor(actionId) ?? '',
      displayDescription,
    }));
  }

  private findMatch(event: KeyboardEvent): ShortcutConfig | null {
    return (
      this.shortcuts.find(
        (s) =>
          s._parsed.every((press) => matchKeyBindingPress(event, press)) &&
          (s.canMatch?.() ?? true),
      ) ?? null
    );
  }

  private toggleViewMode(): void {
    if (this.viewerControl.viewMode() === 'gcode') {
      this.viewerControl.viewMode.set('model');
      return;
    }
    this.viewerControl.viewMode.set('gcode');
    const status = this.slicer.status();
    if (!this.gcodePreview.gcodeHandle() && status !== 'slicing' && status !== 'uploading') {
      void this.slicer.slice();
    }
  }

  private isTextInputFocused(): boolean {
    const target = document.activeElement as HTMLElement | null;
    if (!target) {
      return false;
    }
    const tag = target.tagName.toUpperCase();
    return tag === 'INPUT' || tag === 'TEXTAREA' || target.isContentEditable;
  }
}
