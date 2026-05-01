import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { NexusSlicingShell } from '../../nexus/layout/slicing-shell/slicing-shell';
import { GcodePreviewService } from '../../services/gcode-preview.service';
import { Slicer } from '../../services/slicer';
import { ViewerControl } from '../../services/viewer-control';
import { Icon } from '../../shared/icon/icon';
import { RadioButtonValue } from '../../shared/radio-group/radio-button-value';
import { RadioGroup } from '../../shared/radio-group/radio-group';
import { TooltipDirective } from '../../shared/tooltip/tooltip.directive';
import { Card } from '../card/card';

@Component({
  selector: 'nexus-3d-view-toolbar',
  imports: [Card, Icon, RadioGroup, RadioButtonValue, TooltipDirective],
  templateUrl: './3d-view-toolbar.html',
  styleUrl: './3d-view-toolbar.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ThreeDViewToolbar {
  private readonly viewerControl = inject(ViewerControl);
  private readonly slicer = inject(Slicer);
  private readonly gcodePreview = inject(GcodePreviewService);
  protected readonly shell = inject(NexusSlicingShell);

  readonly selectedView = this.viewerControl.view;
  readonly selectedCursorMode = this.viewerControl.cursorMode;
  readonly selectedObjectMode = this.viewerControl.objectMode;
  readonly viewMode = this.viewerControl.viewMode;
  readonly gravityEnabled = this.viewerControl.gravityEnabled;

  toggleGravity(): void {
    this.gravityEnabled.update((v) => !v);
  }

  /** True once a slice result is available (either loading or fully parsed). */
  protected get hasSliceResult(): boolean {
    return this.gcodePreview.gcodeHandle() !== null || this.gcodePreview.loading();
  }

  /** True when a file is loaded and eligible to slice. */
  protected get canSlice(): boolean {
    return this.slicer.selectedFile() !== null;
  }

  resetView(): void {
    this.viewerControl.reset();
  }

  toggleViewMode(): void {
    if (this.viewMode() === 'gcode') {
      this.viewerControl.viewMode.set('model');
      return;
    }

    this.viewerControl.viewMode.set('gcode');

    // If no slice exists yet and we're not already slicing, kick one off.
    const status = this.slicer.status();
    if (!this.gcodePreview.gcodeHandle() && status !== 'slicing' && status !== 'uploading') {
      void this.slicer.slice();
    }
  }
}
