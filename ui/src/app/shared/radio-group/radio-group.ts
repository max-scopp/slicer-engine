import {
  afterNextRender,
  contentChildren,
  Directive,
  effect,
  ElementRef,
  inject,
  model,
  OnDestroy,
  Renderer2,
} from '@angular/core';
import { RadioButtonValue } from './radio-button-value';

// Trigger stacking this many px before the buttons would visually break.
const STACK_EARLY_PX = 12;

@Directive({
  selector: '[radioGroup]',
  exportAs: 'radioGroup',
})
export class RadioGroup implements OnDestroy {
  readonly value = model<unknown>(null, { alias: 'radioGroup' });

  private readonly buttons = contentChildren(RadioButtonValue, { descendants: true });
  private readonly el = inject(ElementRef<HTMLElement>);
  private readonly renderer = inject(Renderer2);
  private observer: ResizeObserver | null = null;
  private neededWidth = 0;

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

    afterNextRender(() => {
      this.neededWidth = this.measureNaturalWidth();
      this.observer = new ResizeObserver(() => this.updateLayout());
      this.observer.observe(this.el.nativeElement);
      this.updateLayout();
    });
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
  }

  // Measure once at init: temporarily remove flex shrinking/growing so each
  // child reports its natural (no-compression) width, then cache the total.
  private measureNaturalWidth(): number {
    const el: HTMLElement = this.el.nativeElement;
    const children = Array.from(el.children) as HTMLElement[];
    if (children.length === 0) {
      return 0;
    }

    // Measure 1ch in the font context of the first child, then use
    // (trimmed char count × 1ch + horizontal padding) per button.
    // No DOM mutation to flex properties — no reflow side-effects.
    const probe = document.createElement('span');
    probe.style.cssText =
      'position:absolute;visibility:hidden;width:1ch;display:inline-block;pointer-events:none';
    children[0].appendChild(probe);
    const chWidth = probe.offsetWidth;
    children[0].removeChild(probe);

    const gap = parseFloat(getComputedStyle(el).columnGap) || 0;
    const width = children.reduce((sum, c, i) => {
      const chars = (c.textContent ?? '').trim().length;
      const cs = getComputedStyle(c);
      const paddingH = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
      return sum + chars * chWidth + paddingH + STACK_EARLY_PX + (i > 0 ? gap : 0);
    }, 0);

    return width;
  }

  // Hot path: pure numeric compare, no style mutations, no reflow.
  private updateLayout(): void {
    const el: HTMLElement = this.el.nativeElement;
    if (this.neededWidth === 0) {
      return;
    }

    const style = getComputedStyle(el);
    const paddingH = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    const available = el.offsetWidth - paddingH;

    const shouldStack = this.neededWidth > available;
    const isStacked = el.classList.contains('stacked');

    if (shouldStack && !isStacked) {
      this.renderer.addClass(el, 'stacked');
    } else if (!shouldStack && isStacked) {
      this.renderer.removeClass(el, 'stacked');
    }
  }

  select(value: unknown): void {
    this.value.set(value);
  }
}
