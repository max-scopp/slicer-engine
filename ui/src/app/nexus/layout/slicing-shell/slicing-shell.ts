import { Component, signal } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ThreeDViewToolbar } from '../../../components/3d-view-toolbar/3d-view-toolbar';
import { Card } from '../../../components/card/card';
import { CodeEditorComponent } from '../../../components/code-editor/code-editor.component';
import { SettingsPanelComponent } from '../../../components/settings-panel/settings-panel.component';
import { ViewportCube } from '../../../components/viewport-cube/viewport-cube';
import { PrintEstimates } from '../../print-estimates/print-estimates';
import { Sidebar } from '../../sidebar/sidebar.component';

@Component({
  selector: 'nexus-slicing-shell',
  imports: [
    Sidebar,
    PrintEstimates,
    ThreeDViewToolbar,
    ViewportCube,
    RouterOutlet,
    SettingsPanelComponent,
    CodeEditorComponent,
    Card,
  ],
  templateUrl: './slicing-shell.html',
  styleUrl: './slicing-shell.scss',
})
export class NexusSlicingShell {
  readonly editorPanelVisible = signal(false);

  toggleEditorPanel(): void {
    this.editorPanelVisible.update((v) => !v);
  }
}
