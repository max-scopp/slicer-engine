import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { ViewerControl } from '../../services/viewer-control';
import { Icon } from '../../shared/icon/icon';
import { RadioButtonValue } from '../../shared/radio-group/radio-button-value';
import { RadioGroup } from '../../shared/radio-group/radio-group';
import { Card } from '../card/card';

@Component({
  selector: 'nexus-3d-view-toolbar',
  imports: [Card, Icon, RadioGroup, RadioButtonValue],
  templateUrl: './3d-view-toolbar.html',
  styleUrl: './3d-view-toolbar.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ThreeDViewToolbar {
  private readonly viewerControl = inject(ViewerControl);

  readonly selectedView = this.viewerControl.view;
  readonly selectedCursorMode = this.viewerControl.cursorMode;

  resetView(): void {
    this.viewerControl.reset();
  }
}
