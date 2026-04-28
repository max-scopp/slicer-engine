import { FocusMonitor } from '@angular/cdk/a11y';
import { Overlay, OverlayRef } from '@angular/cdk/overlay';
import { ComponentPortal } from '@angular/cdk/portal';
import {
  ComponentRef,
  Directive,
  ElementRef,
  HostListener,
  inject,
  input,
  OnDestroy,
  OnInit,
} from '@angular/core';
import { Subscription } from 'rxjs';
import { InputModalityService } from '../input-modality/input-modality.service';
import { TooltipComponent } from './tooltip.component';

const MOUSE_DELAY_MS = 600;

/**
 * Attaches a positioned tooltip to any host element.
 *
 * Usage: <button [tooltip]="'Reset view'">…</button>
 *
 * Behaviour is driven by the active input modality (via InputModalityService):
 *
 *   mouse    — show after a short hover delay; hide on mouse-leave.
 *              Keyboard focus/blur events are ignored.
 *
 *   keyboard — show immediately on focus; hide on blur.
 *              Mouse enter/leave events are ignored.
 *
 *   touch    — tooltips are suppressed entirely.
 *
 * Positioning is handled by the Angular CDK FlexibleConnectedPositionStrategy
 * so the panel stays on-screen even near viewport edges.
 */
@Directive({
  selector: '[tooltip]',
})
export class TooltipDirective implements OnInit, OnDestroy {
  readonly tooltip = input.required<string>();

  private readonly overlay = inject(Overlay);
  private readonly elementRef = inject(ElementRef<HTMLElement>);
  private readonly focusMonitor = inject(FocusMonitor);
  private readonly inputModality = inject(InputModalityService);

  private overlayRef: OverlayRef | null = null;
  private componentRef: ComponentRef<TooltipComponent> | null = null;
  private showTimeout: ReturnType<typeof setTimeout> | null = null;
  private modalitySub: Subscription | null = null;

  ngOnInit(): void {
    // Hide immediately whenever the user switches input method.
    // This covers e.g. reaching for the mouse while a keyboard tooltip is open,
    // or tabbing away while a hover tooltip is pending.
    this.modalitySub = this.inputModality.modalityChanged$.subscribe(() => {
      this.hide();
    });

    // FocusMonitor emits null when focus leaves, or the origin when it arrives.
    // We only act on keyboard-originated focus; mouse clicks that happen to
    // focus an element are ignored here and handled by the hover listeners.
    this.focusMonitor.monitor(this.elementRef).subscribe((origin) => {
      if (origin === null) {
        if (this.inputModality.modality() === 'keyboard') {
          this.hide();
        }
      } else if (origin === 'keyboard') {
        this.show();
      }
    });
  }

  @HostListener('mouseenter')
  onMouseEnter(): void {
    if (this.inputModality.modality() !== 'mouse') {
      return;
    }
    this.showTimeout = setTimeout(() => this.show(), MOUSE_DELAY_MS);
  }

  @HostListener('mouseleave')
  onMouseLeave(): void {
    if (this.inputModality.modality() !== 'mouse') {
      return;
    }
    this.hide();
  }

  @HostListener('keydown.escape')
  onEscape(): void {
    this.hide();
  }

  ngOnDestroy(): void {
    this.hide();
    this.modalitySub?.unsubscribe();
    this.focusMonitor.stopMonitoring(this.elementRef);
  }

  private show(): void {
    if (this.overlayRef) {
      return;
    }

    const positionStrategy = this.overlay
      .position()
      .flexibleConnectedTo(this.elementRef)
      .withPositions([
        {
          originX: 'center',
          originY: 'top',
          overlayX: 'center',
          overlayY: 'bottom',
          offsetY: -6,
        },
        {
          originX: 'center',
          originY: 'bottom',
          overlayX: 'center',
          overlayY: 'top',
          offsetY: 6,
        },
      ]);

    this.overlayRef = this.overlay.create({
      positionStrategy,
      scrollStrategy: this.overlay.scrollStrategies.close(),
      panelClass: 'nexus-tooltip-overlay',
    });

    const portal = new ComponentPortal(TooltipComponent);
    this.componentRef = this.overlayRef.attach(portal);
    this.componentRef.setInput('text', this.tooltip());
  }

  private hide(): void {
    if (this.showTimeout !== null) {
      clearTimeout(this.showTimeout);
      this.showTimeout = null;
    }

    this.overlayRef?.dispose();
    this.overlayRef = null;
    this.componentRef = null;
  }
}
