import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { NexusSlicingShell } from '../../nexus/layout/slicing-shell/slicing-shell';
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
  protected readonly shell = inject(NexusSlicingShell);

  readonly selectedView = this.viewerControl.view;
  readonly selectedCursorMode = this.viewerControl.cursorMode;

  resetView(): void {
    this.viewerControl.reset();
  }
}
