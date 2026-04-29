import { Component, effect, ElementRef, inject, input } from '@angular/core';
import { IconCache } from './icon-cache';

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

  private readonly cache = inject(IconCache);
  private readonly el = inject(ElementRef<HTMLElement>);

  private lastRenderedUrl: string | null = null;

  constructor() {
    effect(() => {
      const basePath = this.variant() === 'solid' ? '/assets/icons/solid' : '/assets/icons';
      const url = `${basePath}/${this.name()}.svg`;

      if (url === this.lastRenderedUrl) {
        return;
      }

      this.cache.get(url).subscribe({
        next: (content) => {
          this.lastRenderedUrl = url;
          this.el.nativeElement.innerHTML = content;
        },
      });
    });
  }
}
