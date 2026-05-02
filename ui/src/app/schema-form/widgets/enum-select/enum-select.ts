import { ChangeDetectionStrategy, Component, EventEmitter, input } from '@angular/core';
import { IconButton } from '../../../shared/icon-button/icon-button';
import { TooltipDirective } from '../../../shared/tooltip/tooltip.directive';
import { FieldDef } from '../../models/field-def';
import { FieldWidget } from '../base-field';

/**
 * Dropdown widget for enum fields with more than 3 options.
 * Each `<option>` shows the enum variant value; the option's `title`
 * attribute carries the per-variant description for browser-native tooltips.
 */
@Component({
  selector: 'se-enum-select',
  standalone: true,
  imports: [IconButton, TooltipDirective],
  changeDetection: ChangeDetectionStrategy.OnPush,
  styles: [
    `
      :host {
        display: flex;
        flex-direction: column;
        gap: 5px;
      }

      label {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        font-weight: 500;
        color: var(--color-text-secondary);
        user-select: none;
        cursor: default;
      }

      select {
        width: 100%;
      }
    `,
  ],
  template: `
    <label [for]="field().key">
      <span>{{ field().title ?? field().key }}</span>
      @if (field().description) {
        <nexus-icon-button
          icon="help-circle"
          label="More info"
          [tooltip]="field().description!"
          [tooltipMode]="'block'"
          [tooltipClickToggle]="true"
        />
      }
    </label>
    <select
      [id]="field().key"
      [value]="value() ?? field().default ?? ''"
      (change)="valueChange.emit($any($event.target).value)"
    >
      @for (opt of field().enumOptions; track opt.value) {
        <option [value]="opt.value" [title]="opt.description ?? ''">
          {{ opt.value }}
        </option>
      }
    </select>
  `,
})
export class EnumSelect implements FieldWidget {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly valueChange = new EventEmitter<unknown>();
}
