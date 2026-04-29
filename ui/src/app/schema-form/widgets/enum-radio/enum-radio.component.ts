import { ChangeDetectionStrategy, Component, EventEmitter, input } from '@angular/core';
import { IconButton } from '../../../shared/icon-button/icon-button';
import { TooltipDirective } from '../../../shared/tooltip/tooltip.directive';
import { FieldDef } from '../../models/field-def';
import { FieldWidget } from '../base-field';

/**
 * Radio-group widget for enum fields with 3 or fewer options.
 * Shows per-option descriptions beneath each radio label so the user can
 * read what each variant does without opening a separate tooltip.
 */
@Component({
  selector: 'se-enum-radio',
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

      fieldset {
        border: none;
        padding: 0;
        margin: 0;
      }

      legend {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        font-weight: 500;
        color: var(--color-text-secondary);
        user-select: none;
        margin-bottom: 6px;
        padding: 0;
      }

      .radio-option {
        display: flex;
        flex-direction: column;
        gap: 2px;
        cursor: pointer;
        font-size: 12px;
        color: var(--color-text-primary);

        & + .radio-option {
          margin-top: 6px;
        }

        input[type='radio'] {
          accent-color: var(--color-primary);
        }
      }

      .radio-label {
        display: flex;
        align-items: center;
        gap: 6px;
      }

      .radio-option-description {
        font-size: 11px;
        color: var(--color-text-tertiary);
        margin-left: 18px;
      }
    `,
  ],
  template: `
    <fieldset>
      <legend>
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
      </legend>
      @for (opt of field().enumOptions; track opt.value) {
        <label class="radio-option">
          <span class="radio-label">
            <input
              type="radio"
              [name]="field().key"
              [value]="opt.value"
              [checked]="(value() ?? field().default) === opt.value"
              (change)="valueChange.emit(opt.value)"
            />
            {{ opt.value }}
          </span>
          @if (opt.description) {
            <span class="radio-option-description">{{ opt.description }}</span>
          }
        </label>
      }
    </fieldset>
  `,
})
export class EnumRadioComponent implements FieldWidget {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly valueChange = new EventEmitter<unknown>();
}
