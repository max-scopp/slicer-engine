import { ChangeDetectionStrategy, Component, EventEmitter, input } from '@angular/core';
import { IconButton } from '../../../shared/icon-button/icon-button';
import { TooltipDirective } from '../../../shared/tooltip/tooltip.directive';
import { FieldDef } from '../../models/field-def';
import { FieldWidget } from '../base-field';

@Component({
  selector: 'se-integer-field',
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

      input {
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
    <input
      [id]="field().key"
      type="number"
      step="1"
      [min]="field().minimum ?? null"
      [max]="field().maximum ?? null"
      [value]="value()"
      (change)="valueChange.emit(Math.round(+$any($event.target).value))"
    />
  `,
})
export class IntegerField implements FieldWidget {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly valueChange = new EventEmitter<unknown>();

  protected readonly Math = Math;
}
