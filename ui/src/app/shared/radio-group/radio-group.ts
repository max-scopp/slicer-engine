import { contentChildren, Directive, effect, model } from '@angular/core';
import { RadioButtonValue } from './radio-button-value';

@Directive({
  selector: '[radioGroup]',
  exportAs: 'radioGroup',
})
export class RadioGroup {
  readonly value = model<unknown>(null, { alias: 'radioGroup' });

  private readonly buttons = contentChildren(RadioButtonValue, { descendants: true });

  constructor() {
    effect(() => {
      const current = this.value();
      for (const button of this.buttons()) {
        try {
          button.setActive(button.radioButtonValue() === current);
        } catch {
          // Required input not yet bound — effect re-runs once bindings settle.
        }
      }
    });
  }

  select(value: unknown): void {
    this.value.set(value);
  }
}
