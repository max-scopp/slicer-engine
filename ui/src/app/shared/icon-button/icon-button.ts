import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { Icon } from '../icon/icon';

/**
 * A minimal icon-only button backed by an Iconoir SVG.
 *
 * Usage:
 * ```html
 * <nexus-icon-button icon="trash" label="Delete item" size="sm" />
 * ```
 *
 * Pair with `[tooltip]` from `TooltipDirective` at the call site for
 * accessible hover/focus tooltips, or supply a descriptive `label` for
 * screen-reader `aria-label` alone.
 */
@Component({
  selector: 'nexus-icon-button',
  standalone: true,
  imports: [Icon],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      type="button"
      class="icon-btn"
      [class.icon-btn--xs]="size() === 'xs'"
      [class.icon-btn--sm]="size() === 'sm'"
      [class.icon-btn--md]="size() === 'md'"
      [attr.aria-label]="label()"
    >
      <nexus-icon [name]="icon()" />
    </button>
  `,
  styles: [
    `
      :host {
        display: inline-flex;
      }

      .icon-btn {
        display: inline-grid;
        place-items: center;
        border: none;
        padding: 0;
        background: transparent;
        color: var(--color-text-tertiary);
        cursor: pointer;
        border-radius: var(--radius-sm);
        flex-shrink: 0;
        transition:
          color var(--transition-fast),
          background-color var(--transition-fast);

        &:hover {
          color: var(--color-text-secondary);
          background: var(--color-surface-hover);
        }

        &:focus-visible {
          outline: 2px solid var(--color-focus-ring);
          outline-offset: 1px;
        }
      }

      .icon-btn--xs {
        width: 16px;
        height: 16px;

        nexus-icon {
          --icon-size: 12px;
        }
      }

      .icon-btn--sm {
        width: 20px;
        height: 20px;

        nexus-icon {
          --icon-size: 14px;
        }
      }

      .icon-btn--md {
        width: 28px;
        height: 28px;

        nexus-icon {
          --icon-size: 16px;
        }
      }
    `,
  ],
})
export class IconButton {
  /** Iconoir icon name (matches the SVG filename in `/assets/icons/`). */
  readonly icon = input.required<string>();

  /** Accessible label used as `aria-label` on the inner button. */
  readonly label = input<string>('');

  /** Visual size variant. Defaults to `sm`. */
  readonly size = input<'xs' | 'sm' | 'md'>('sm');
}
