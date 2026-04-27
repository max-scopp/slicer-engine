import { Component, signal } from '@angular/core';
import { FileUploadComponent } from './components/file-upload/file-upload.component';
import { SettingsPanelComponent } from './components/settings-panel/settings-panel.component';
import { StatusPanelComponent } from './components/status-panel/status-panel.component';
import { HistoryPanelComponent } from './components/history-panel/history-panel.component';
import { DebugViewerComponent } from './components/debug-viewer/debug-viewer.component';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [FileUploadComponent, SettingsPanelComponent, StatusPanelComponent, HistoryPanelComponent, DebugViewerComponent],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  readonly title = signal('Slicer Engine');
}
