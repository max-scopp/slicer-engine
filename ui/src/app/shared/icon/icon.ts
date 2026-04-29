import { HttpClient } from '@angular/common/http';
import { Component, effect, ElementRef, inject, input } from '@angular/core';

@Component({
  selector: 'nexus-icon',
  template: ``,
  styles: [
    `
      :host {
        display: inline-grid;
        place-items: center;

        ::ng-deep svg {
          width: var(--icon-size, 20px);
          height: var(--icon-size, 20px);
          stroke-width: var(--icon-stroke-width);
        }
      }
    `,
  ],
})
export class Icon {
  readonly name = input.required<string>();
  readonly variant = input<'regular' | 'solid'>('regular');

  private readonly http = inject(HttpClient);
  private readonly el = inject(ElementRef<HTMLElement>);

  constructor() {
    effect(() => {
      const basePath = this.variant() === 'solid' ? '/assets/icons/solid' : '/assets/icons';
      this.http.get(`${basePath}/${this.name()}.svg`, { responseType: 'text' }).subscribe({
        next: (content) => {
          this.el.nativeElement.innerHTML = content;
        },
      });
    });
  }
}
