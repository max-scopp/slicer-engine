import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { FieldHostComponent } from './field-host/field-host.component';
import { SchemaGroup } from './models/field-def';
import { parseSchema } from './models/schema-parser';

export interface FieldChangeEvent {
  key: string;
  value: unknown;
}

/**
 * Schema-driven form container.
 *
 * Accepts any JSON Schema object and a flat value map, then renders
 * every property as the appropriate widget component. Fields are
 * visually grouped by their `x-group` schema extension value.
 *
 * The component emits `fieldChange` events rather than mutating the
 * value directly, keeping the data flow unidirectional.
 *
 * @example
 * ```html
 * <se-schema-form
 *   [schema]="mySchema"
 *   [value]="currentSettings()"
 *   (fieldChange)="onFieldChange($event)"
 * />
 * ```
 */
@Component({
  selector: 'se-schema-form',
  standalone: true,
  imports: [FieldHostComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './schema-form.component.html',
  styleUrl: './schema-form.component.scss',
})
export class SchemaFormComponent {
  /** Raw JSON Schema object. Changing this input re-parses the schema. */
  readonly schema = input.required<Record<string, unknown>>();

  /**
   * Current values keyed by field name. Pass a partial object — missing
   * keys fall back to the schema default when rendering.
   */
  readonly value = input<Record<string, unknown>>({});

  /** Emitted whenever the user changes a single field. */
  readonly fieldChange = output<FieldChangeEvent>();

  protected readonly groups = computed<SchemaGroup[]>(() => {
    const { groups } = parseSchema(this.schema());
    return groups;
  });

  protected onFieldChange(key: string, value: unknown): void {
    this.fieldChange.emit({ key, value });
  }
}
