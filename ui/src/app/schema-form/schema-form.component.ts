import { AccordionGroup, AccordionPanel, AccordionTrigger } from '@angular/aria/accordion';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { BrowserStorage } from '../services/browser-storage';
import { UserInputModality } from '../shared/input-modality/input-modality';
import { Icon } from '../shared/icon/icon';
import { FieldHostComponent } from './field-host/field-host.component';
import { SchemaGroup } from './models/field-def';
import { parseSchema } from './models/schema-parser';

export interface FieldChangeEvent {
  key: string;
  value: unknown;
}

const ACCORDION_STORAGE_KEY = 'schema-form-accordion';

/**
 * Schema-driven form container.
 *
 * Accepts any JSON Schema object and a flat value map, then renders
 * every property as the appropriate widget component. Fields are
 * visually grouped by their `x-group` schema extension value and
 * presented as collapsible accordion panels.
 *
 * Accordion expansion state is persisted to localStorage via BrowserStorage
 * so panels reopen in the same state after page refresh. Multiple panels
 * can be open simultaneously. All panels start closed by default.
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
  imports: [Icon, FieldHostComponent, AccordionGroup, AccordionPanel, AccordionTrigger],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './schema-form.component.html',
  styleUrl: './schema-form.component.scss',
})
export class SchemaFormComponent {
  private readonly storage = inject(BrowserStorage);
  private readonly inputModality = inject(UserInputModality);

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

  /**
   * Map of group names to their expanded state signals.
   * Created lazily as groups are encountered in the template.
   */
  private readonly expandedSignalMap = new Map<string, ReturnType<typeof signal<boolean>>>();

  protected getExpandedSignal(groupName: string): ReturnType<typeof signal<boolean>> {
    if (!this.expandedSignalMap.has(groupName)) {
      const isExpanded = this.isGroupExpandedInStorage(groupName);
      const sig = signal(isExpanded);
      this.expandedSignalMap.set(groupName, sig);
    }
    return this.expandedSignalMap.get(groupName)!;
  }

  protected onExpandedChange(groupName: string, groupEl: HTMLElement): void {
    const sig = this.getExpandedSignal(groupName);
    if (sig()) {
      // Defer until the expand animation has started so the element has its
      // final height. block:'nearest' scrolls the minimum distance to reveal
      // the whole group; if the panel is taller than the viewport the browser
      // aligns the top (heading) to the viewport top instead.
      setTimeout(() => groupEl.scrollIntoView({ behavior: 'smooth', block: 'nearest' }), 180);
    }
    this.persistExpandedState();
  }

  protected onFieldChange(key: string, value: unknown): void {
    this.fieldChange.emit({ key, value });
  }

  /**
   * On touch devices the AccordionGroup listens to `pointerdown` which fires
   * before the finger is lifted, so the panel opens on touch-start rather
   * than on a tap.  Stopping propagation at the trigger button prevents the
   * event from reaching the group's listener; `onGroupTriggerClick` then
   * handles the tap via the normal `click` event instead.
   */
  protected onGroupTriggerPointerDown(event: PointerEvent): void {
    if (this.inputModality.modality() === 'touch') {
      event.stopPropagation();
    }
  }

  protected onGroupTriggerClick(sig: ReturnType<typeof signal<boolean>>): void {
    if (this.inputModality.modality() === 'touch') {
      sig.set(!sig());
    }
  }

  private isGroupExpandedInStorage(groupName: string): boolean {
    const stored = this.storage.getJson<string[]>(ACCORDION_STORAGE_KEY, 'local');
    return stored ? stored.includes(groupName) : false;
  }

  private persistExpandedState(): void {
    const expanded: string[] = [];
    for (const [groupName, sig] of this.expandedSignalMap.entries()) {
      if (sig()) {
        expanded.push(groupName);
      }
    }
    this.storage.writeJson(ACCORDION_STORAGE_KEY, expanded, 'local');
  }
}
