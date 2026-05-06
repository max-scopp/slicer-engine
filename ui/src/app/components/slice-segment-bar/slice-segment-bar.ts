import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import {
  GcodePreview,
  ROLE_LABELS,
  ROLE_ORDER,
  type RoleName,
} from '../../services/gcode-preview';

@Component({
  selector: 'nexus-slice-segment-bar',
  standalone: true,
  imports: [],
  templateUrl: './slice-segment-bar.html',
  styleUrl: './slice-segment-bar.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SliceSegmentBar {
  protected readonly preview = inject(GcodePreview);

  protected readonly roleCss = this.preview.roleCss;
  protected readonly roleLabels = ROLE_LABELS;
  protected readonly roleOrder: readonly RoleName[] = ROLE_ORDER;

  /** Total move segments in the current top layer derived from its geometry buffers. */
  protected readonly layerSegmentCount = computed(() => {
    const handle = this.preview.gcodeHandle();
    if (!handle) {
      return 0;
    }
    const layer = handle.getLayer(this.preview.layerMax());
    const floatsPerSegment = 8;
    let totalFloats = 0;
    const blocksCount = layer.blocksCount();
    for (let i = 0; i < blocksCount; i++) {
      totalFloats += layer.blockData(i).length;
    }
    return totalFloats / floatsPerSegment;
  });

  /** Segment slider integer value derived from the fractional signal and real segment count. */
  protected readonly segmentSliderValue = computed(() =>
    Math.round(this.preview.segmentProgress() * this.layerSegmentCount()),
  );

  /** CSS `right%` for the scrub track fill (from left edge to thumb). */
  protected readonly scrubFillRight = computed(() => {
    const total = this.layerSegmentCount();
    if (total === 0) {
      return 0;
    }
    return (1 - this.segmentSliderValue() / total) * 100;
  });

  // ── Event handlers ───────────────────────────────────────────────────────

  protected onSegmentInput(event: Event): void {
    const raw = parseInt((event.target as HTMLInputElement).value, 10);
    const total = this.layerSegmentCount();
    this.preview.setSegmentProgress(total > 0 ? raw / total : 1);
  }

  protected onWheelSegment(event: WheelEvent): void {
    event.preventDefault();
    const total = this.layerSegmentCount();
    if (total === 0) {
      return;
    }
    const step = event.deltaY < 0 ? 1 : -1;
    const current = Math.round(this.preview.segmentProgress() * total);
    this.preview.setSegmentProgress((current + step) / total);
  }

  protected toggleRole(role: RoleName): void {
    this.preview.toggleRole(role);
  }
}
