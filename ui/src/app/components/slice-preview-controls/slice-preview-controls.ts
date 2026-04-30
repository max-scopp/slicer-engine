import { DecimalPipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import {
  GcodePreviewService,
  ROLE_CSS,
  ROLE_LABELS,
  ROLE_ORDER,
  type RoleName,
} from '../../services/gcode-preview.service';
import { Card } from '../card/card';

@Component({
  selector: 'nexus-slice-preview-controls',
  standalone: true,
  imports: [DecimalPipe, Card],
  templateUrl: './slice-preview-controls.html',
  styleUrl: './slice-preview-controls.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SlicePreviewControls {
  protected readonly preview = inject(GcodePreviewService);

  protected readonly roleCss = ROLE_CSS;
  protected readonly roleLabels = ROLE_LABELS;
  protected readonly roleOrder: readonly RoleName[] = ROLE_ORDER;

  /** Human-readable layer range label, e.g. "Layers 3–12 / 42". */
  protected readonly layerRangeLabel = computed(() => {
    const count = this.preview.layerCount();
    if (count === 0) {
      return '';
    }
    const min = this.preview.layerMin();
    const max = this.preview.layerMax();
    if (this.preview.isProgressMode()) {
      return `Layer ${max + 1} / ${count}`;
    }
    return `Layers ${min + 1}–${max + 1} / ${count}`;
  });

  /** Z height of the current top layer in mm. */
  protected readonly currentZ = computed(() => {
    const handle = this.preview.gcodeHandle();
    if (!handle) {
      return null;
    }
    return handle.layerZ(this.preview.layerMax());
  });

  // ── Drag event handlers ──────────────────────────────────────────────────

  protected onLayerMinInput(event: Event): void {
    const raw = parseInt((event.target as HTMLInputElement).value, 10);
    this.preview.setLayerMin(raw);
  }

  protected onLayerMaxInput(event: Event): void {
    const raw = parseInt((event.target as HTMLInputElement).value, 10);
    this.preview.setLayerMax(raw);
  }

  protected onSegmentInput(event: Event): void {
    const raw = parseFloat((event.target as HTMLInputElement).value);
    // Slider goes 0–1000; convert to fraction.
    this.preview.setSegmentProgress(raw / 1000);
  }

  // ── Scroll-wheel handlers ────────────────────────────────────────────────

  /** Wheel over the layer slider area. In range mode shifts the window; in
   *  progress mode advances/retreats one layer at a time. */
  protected onWheelLayer(event: WheelEvent): void {
    event.preventDefault();
    // Scroll up (deltaY < 0) = higher layers.
    const step = event.deltaY < 0 ? 1 : -1;
    if (this.preview.isProgressMode()) {
      this.preview.setLayerMax(this.preview.layerMax() + step);
    } else {
      this.preview.shiftRange(step);
    }
  }

  /** Wheel over the nozzle/segment slider. */
  protected onWheelSegment(event: WheelEvent): void {
    event.preventDefault();
    // Scroll up = advance nozzle.
    const step = event.deltaY < 0 ? 25 : -25;
    const current = Math.round(this.preview.segmentProgress() * 1000);
    this.preview.setSegmentProgress((current + step) / 1000);
  }

  // ── Toggle handlers ──────────────────────────────────────────────────────

  protected toggleRole(role: RoleName): void {
    this.preview.toggleRole(role);
  }

  protected toggleProgressMode(): void {
    this.preview.toggleProgressMode();
  }

  // ── Template helpers ─────────────────────────────────────────────────────

  /** CSS `left%` for the range-fill start (layerMin thumb). */
  protected readonly fillLeft = computed(() => {
    const count = this.preview.layerCount();
    if (count <= 1) {
      return 0;
    }
    return (this.preview.layerMin() / (count - 1)) * 100;
  });

  /** CSS `right%` for the range-fill end (layerMax thumb). */
  protected readonly fillRight = computed(() => {
    const count = this.preview.layerCount();
    if (count <= 1) {
      return 0;
    }
    return ((count - 1 - this.preview.layerMax()) / (count - 1)) * 100;
  });

  /** Segment slider integer value (0–1000) derived from the fractional signal. */
  protected readonly segmentSliderValue = computed(() =>
    Math.round(this.preview.segmentProgress() * 1000),
  );
}
