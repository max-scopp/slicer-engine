import {
  ChangeDetectionStrategy,
  Component,
  ComponentRef,
  input,
  OnChanges,
  OnDestroy,
  output,
  SimpleChanges,
  ViewChild,
  ViewContainerRef,
} from '@angular/core';
import { Subscription } from 'rxjs';
import { resolveWidget } from '../field-registry/field-registry';
import { FieldDef } from '../models/field-def';
import { FieldWidget } from '../widgets/base-field';

/**
 * Dynamic host that instantiates the correct widget component for a given
 * `FieldDef` at runtime using `ViewContainerRef.createComponent()`.
 *
 * This approach is required (rather than `NgComponentOutlet`) because
 * `NgComponentOutlet` does not support binding to outputs dynamically.
 */
@Component({
  selector: 'se-field-host',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<ng-container #host />`,
})
export class FieldHostComponent implements OnChanges, OnDestroy {
  readonly field = input.required<FieldDef>();
  readonly value = input<unknown>(undefined);
  readonly fieldChange = output<unknown>();

  @ViewChild('host', { read: ViewContainerRef, static: true })
  private readonly vcr!: ViewContainerRef;

  private componentRef: ComponentRef<FieldWidget> | null = null;
  private changeSubscription: Subscription | null = null;

  ngOnChanges(changes: SimpleChanges): void {
    const fieldChanged = 'field' in changes;
    const valueChanged = 'value' in changes;

    if (fieldChanged) {
      this.destroyWidget();
      this.createWidget();
    } else if (valueChanged && this.componentRef) {
      this.componentRef.setInput('value', this.value());
    }
  }

  ngOnDestroy(): void {
    this.destroyWidget();
  }

  private createWidget(): void {
    const componentClass = resolveWidget(this.field());

    this.componentRef = this.vcr.createComponent(componentClass);
    this.componentRef.setInput('field', this.field());
    this.componentRef.setInput('value', this.value());

    this.changeSubscription = this.componentRef.instance.valueChange.subscribe((val) => {
      this.fieldChange.emit(val);
    });
  }

  private destroyWidget(): void {
    this.changeSubscription?.unsubscribe();
    this.changeSubscription = null;
    this.componentRef?.destroy();
    this.componentRef = null;
  }
}
