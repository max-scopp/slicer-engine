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
        button.setActive(button.radioButtonValue() === current);
      }
    });
  }

  select(value: unknown): void {
    this.value.set(value);
  }
}
