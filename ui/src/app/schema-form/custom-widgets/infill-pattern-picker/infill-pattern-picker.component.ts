import {
  ChangeDetectionStrategy,
  Component,
  EventEmitter,
  input,
  OnChanges,
  OnInit,
} from '@angular/core';
import { Card } from '../../../components/card/card';
import { IconButton } from '../../../shared/icon-button/icon-button';
import { RadioButtonValue } from '../../../shared/radio-group/radio-button-value';
import { RadioGroup } from '../../../shared/radio-group/radio-group';
import { TooltipDirective } from '../../../shared/tooltip/tooltip.directive';
import { FieldDef } from '../../models/field-def';
import { FieldWidget } from '../../widgets/base-field';

interface PatternOption {
  value: string;
  label: string;
  description: string;
}

const PATTERNS: PatternOption[] = [
  {
    value: 'Rectilinear',
    label: 'Lines',
    description: 'Parallel lines alternating direction per layer (default, fastest).',
  },
  {
    value: 'Grid',
    label: 'Grid',
    description: 'Perpendicular lines forming a grid pattern (stronger).',
  },
  {
    value: 'Honeycomb',
    label: 'Hex',
    description: 'Hexagonal cells (good strength-to-weight ratio).',
  },
  {
    value: 'Gyroid',
    label: 'Gyroid',
    description: '3D mathematical pattern (experimental, best strength).',
  },
  {
    value: 'TpmsD',
    label: 'TPMS-D',
    description: 'Triply Periodic Minimal Surface – Diamond (organic, isotropic).',
  },
];

/**
 * Custom widget for `infill_pattern`.
 *
 * Renders the enum options as a segmented button-group using the existing
 * `RadioGroup` / `RadioButtonValue` directives. Each button carries an
 * inline tooltip so the user can discover what each pattern does without
 * leaving the panel.
 */
@Component({
  selector: 'se-infill-pattern-picker',
  standalone: true,
  imports: [Card, RadioGroup, RadioButtonValue, IconButton, TooltipDirective],
  changeDetection: ChangeDetectionStrategy.OnPush,
  styles: [
    `
      :host {
        display: flex;
        flex-direction: column;
        gap: 5px;
      }

      .field-label {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        font-weight: 500;
        color: var(--color-text-secondary);
        user-select: none;
        cursor: default;
      }

      .pattern-group {
        display: flex;
        padding: 3px;
        width: 100%;

        > button {
          flex: 1;
          font-size: 11px;
          font-weight: 500;
          padding: 4px 2px;
          white-space: nowrap;
        }
      }
    `,
  ],
  template: `
    <span class="field-label">
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
    </span>
    <nexus-card
      class="pattern-group"
      [(radioGroup)]="selected"
      (radioGroupChange)="onSelect($event)"
    >
      @for (p of patterns; track p.value) {
        <button [radioButtonValue]="p.value" [tooltip]="p.description">{{ p.label }}</button>
      }
    </nexus-card>
  `,
})
export class InfillPatternPickerComponent implements FieldWidget, OnInit, OnChanges {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly valueChange = new EventEmitter<unknown>();

  readonly patterns = PATTERNS;

  protected selected: unknown = PATTERNS[0].value;

  ngOnInit(): void {
    this.syncFromValue();
  }

  ngOnChanges(): void {
    this.syncFromValue();
  }

  protected onSelect(value: unknown): void {
    this.valueChange.emit(value);
  }

  private syncFromValue(): void {
    const raw = this.value();
    if (raw !== undefined && raw !== null) {
      this.selected = raw;
    }
  }
}
