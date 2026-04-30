import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { Card } from '../../components/card/card';
import { PHASE_LABELS, Slicer } from '../../services/slicer';
import { Icon } from '../../shared/icon/icon';
import { TooltipDirective } from '../../shared/tooltip/tooltip.directive';

@Component({
  selector: 'nexus-slice-control',
  imports: [Card, Icon, TooltipDirective],
  templateUrl: './slice-control.html',
  styleUrl: './slice-control.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SliceControl {
  protected readonly slicer = inject(Slicer);

  protected readonly isActive = computed(() => {
    const s = this.slicer.status();
    return s === 'uploading' || s === 'slicing';
  });

  /**
   * True whenever we should render the progress row.
   * Kept true through 'done' so Angular's batching can't skip it.
   */
  protected readonly showProgress = computed(() => {
    const s = this.slicer.status();
    return s === 'uploading' || s === 'slicing' || s === 'done' || s === 'error';
  });

  protected readonly isDone = computed(() => this.slicer.status() === 'done');

  protected readonly canSlice = computed(() => {
    const s = this.slicer.status();
    return (
      (s === 'idle' || s === 'ready' || s === 'done' || s === 'error') &&
      this.slicer.selectedFile() !== null
    );
  });

  protected readonly phaseLabel = computed(() => {
    const s = this.slicer.status();
    if (s === 'uploading') return 'Uploading…';
    if (s === 'done') return 'Complete';
    const phase = this.slicer.currentPhase();
    if (!phase) return 'Preparing…';
    return PHASE_LABELS[phase] ?? phase;
  });

  slice(): void {
    void this.slicer.slice();
  }

  download(): void {
    this.slicer.downloadGcode();
  }
}
