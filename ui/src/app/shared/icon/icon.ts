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
          width: var(--icon-size, 1em);
          height: var(--icon-size, 1em);
        }
      }
    `,
  ],
})
export class Icon {
  readonly name = input.required<string>();

  private readonly http = inject(HttpClient);
  private readonly el = inject(ElementRef<HTMLElement>);

  constructor() {
    effect(() => {
      this.http.get(`/assets/icons/${this.name()}.svg`, { responseType: 'text' }).subscribe({
        next: (content) => {
          this.el.nativeElement.innerHTML = content;
        },
      });
    });
  }
}
