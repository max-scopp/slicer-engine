import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { KeyboardShortcuts } from '../../services/keyboard-shortcuts/keyboard-shortcuts';

@Component({
  selector: 'nexus-keyboard-shortcuts',
  standalone: true,
  imports: [],
  templateUrl: './keyboard-shortcuts.html',
  styleUrl: './keyboard-shortcuts.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class KeyboardShortcutsPanel {
  private readonly keyboardShortcuts = inject(KeyboardShortcuts);

  readonly shortcuts = this.keyboardShortcuts.getAll();
}
