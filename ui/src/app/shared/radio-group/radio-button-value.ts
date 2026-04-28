import { Directive, ElementRef, HostBinding, HostListener, inject, input } from '@angular/core';
import { RadioGroup } from './radio-group';

@Directive({
  selector: '[radioButtonValue]',
  exportAs: 'radioButtonValue',
})
export class RadioButtonValue {
  readonly radioButtonValue = input.required<unknown>();

  private readonly group = inject(RadioGroup);
  private readonly el = inject(ElementRef<HTMLElement>);

  @HostBinding('class.active')
  isActive = false;

  @HostBinding('attr.role')
  readonly role = 'radio';

  @HostListener('click')
  onClick(): void {
    this.group.select(this.radioButtonValue());
  }

  setActive(active: boolean): void {
    this.isActive = active;
    this.el.nativeElement.setAttribute('aria-checked', String(active));
  }
}
