import { ChangeDetectionStrategy, Component, EventEmitter, input } from '@angular/core';
import { IconButton } from '../../../shared/icon-button/icon-button';
import { TooltipDirective } from '../../../shared/tooltip/tooltip.directive';
import { FieldDef } from '../../models/field-def';
import { FieldWidget } from '../base-field';

@Component({
  selector: 'se-boolean-field',
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
        gap: 6px;
        font-size: 12px;
        font-weight: 500;
        color: var(--color-text-secondary);
        cursor: pointer;
      }

      input[type='checkbox'] {
        accent-color: var(--color-primary);
        width: 14px;
        height: 14px;
        flex-shrink: 0;
        cursor: pointer;
      }
    `,
  ],
  template: `
    <label [for]="field().key">
      <input
        [id]="field().key"
        type="checkbox"
        [checked]="!!value()"
        (change)="valueChange.emit($any($event.target).checked)"
      />
      <span>{{ field().title ?? field().key }}</span>
      @if (field().description) {
        <nexus-icon-button
          icon="info-circle"
          size="xs"
          label="More info"
          [tooltip]="field().description!"
          [tooltipMode]="'block'"
        />
      }
    </label>
  `,
})
export class BooleanFieldComponent implements FieldWidget {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly valueChange = new EventEmitter<unknown>();
}
