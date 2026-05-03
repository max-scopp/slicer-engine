import { APP_BASE_HREF } from '@angular/common';
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
  private readonly baseHref = inject(APP_BASE_HREF, { optional: true }) ?? '/';

  private lastRenderedUrl: string | null = null;

  constructor() {
    effect(() => {
      const base = this.baseHref.replace(/\/$/, '');
      const basePath =
        this.variant() === 'solid' ? `${base}/assets/icons/solid` : `${base}/assets/icons`;
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
