import { ChangeDetectionStrategy, Component, EventEmitter, input, signal } from '@angular/core';
import { IconButton } from '../../../shared/icon-button/icon-button';
import { TooltipDirective } from '../../../shared/tooltip/tooltip.directive';
import { FieldDef } from '../../models/field-def';
import { FieldWidget } from '../../widgets/base-field';

const MIN_DENSITY = 0;
const MAX_DENSITY = 100;

/**
 * Custom widget for `infill_density`.
 *
 * The schema represents infill density as a fraction 0.0–1.0, but the
 * WebSocket API expects a percentage (0–100). This widget displays and
 * edits the value as a percentage while keeping the emitted value in the
 * schema's native fraction form (0.0–1.0).
 *
 * Renders a range slider alongside a read-only numeric readout so the
 * user has both tactile control and a precise value in view.
 */
@Component({
  selector: 'se-infill-density-slider',
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

      .infill-slider-row {
        display: flex;
        align-items: center;
        gap: 8px;

        input[type='range'] {
          flex: 1;
          accent-color: var(--color-primary);
        }
      }

      .infill-slider-readout {
        font-size: 12px;
        color: var(--color-text-secondary);
        min-width: 32px;
        text-align: right;
      }
    `,
  ],
  template: `
    <label [for]="field().key">
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
    <div class="infill-slider-row">
      <input
        [id]="field().key"
        type="range"
        [min]="MIN"
        [max]="MAX"
        step="1"
        [value]="displayPercent()"
        (input)="onSliderInput($any($event.target).value)"
      />
      <output class="infill-slider-readout">{{ displayPercent() }}%</output>
    </div>
  `,
})
export class InfillDensitySliderComponent implements FieldWidget {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly valueChange = new EventEmitter<unknown>();

  protected readonly MIN = MIN_DENSITY;
  protected readonly MAX = MAX_DENSITY;

  /** Current value expressed as an integer percentage (0–100). */
  protected readonly displayPercent = signal<number>(20);

  ngOnInit(): void {
    this.syncFromValue();
  }

  ngOnChanges(): void {
    this.syncFromValue();
  }

  protected onSliderInput(raw: string): void {
    const pct = Math.round(Number(raw));
    this.displayPercent.set(pct);
    // Emit as fraction so callers receive the same units as other fields
    this.valueChange.emit(pct / MAX_DENSITY);
  }

  private syncFromValue(): void {
    const raw = this.value();
    if (raw !== undefined && raw !== null) {
      const num = Number(raw);
      // Accept both fraction (0–1) and percent (0–100) gracefully
      const pct = num <= 1 ? Math.round(num * MAX_DENSITY) : Math.round(num);
      this.displayPercent.set(Math.max(MIN_DENSITY, Math.min(MAX_DENSITY, pct)));
    }
  }
}
