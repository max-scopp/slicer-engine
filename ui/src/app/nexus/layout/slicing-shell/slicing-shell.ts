import { Component, computed, inject, signal } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ThreeDViewToolbar } from '../../../components/3d-view-toolbar/3d-view-toolbar';
import { Card } from '../../../components/card/card';
import { CodeEditorComponent } from '../../../components/code-editor/code-editor.component';
import { SettingsPanelComponent } from '../../../components/settings-panel/settings-panel.component';
import { ViewportCube } from '../../../components/viewport-cube/viewport-cube';
import { SceneEngineService } from '../../../services/scene-engine.service';
import { Slicer } from '../../../services/slicer';
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
  private readonly sceneEngine = inject(SceneEngineService);
  private readonly slicer = inject(Slicer);

  readonly editorPanelVisible = signal(false);

  /**
   * The current scene snapshot serialised as formatted JSON.
   *
   * This is exactly the payload that would be sent over WebSocket as scene
   * state when a slice job starts. `bigint` ids are serialised as strings so
   * JSON.stringify does not throw.
   */
  readonly snapshotJson = computed(() =>
    JSON.stringify(
      this.sceneEngine.snapshot(),
      (_key, value) => (typeof value === 'bigint' ? String(value) : value),
      2,
    ),
  );

  /**
   * The current slice settings serialised as formatted JSON.
   *
   * This is the `settings` payload that would be sent over WebSocket alongside
   * the scene snapshot when a slice job starts.
   */
  readonly sliceParamsJson = computed(() => JSON.stringify(this.slicer.settings(), null, 2));

  toggleEditorPanel(): void {
    this.editorPanelVisible.update((v) => !v);
  }
}
