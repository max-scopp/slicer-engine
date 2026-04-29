import { CommonModule } from '@angular/common';
import { Component, inject } from '@angular/core';
import { History, SessionSummary } from '../../services/history';

@Component({
  selector: 'nexus-history-panel',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './history-panel.component.html',
  styleUrl: './history-panel.component.scss',
})
export class HistoryPanelComponent {
  private readonly historyService = inject(History);

  readonly previousSessions = this.historyService.sessions;

  loadHistory(): void {
    this.historyService.refresh();
  }

  downloadSession(session: SessionSummary): void {
    this.historyService.download(session);
  }

  formatDate(dateStr: string): string {
    return this.historyService.formatDate(dateStr);
  }

  getFilename(session: SessionSummary): string {
    return (
      (session.original_filename as string | null | undefined)?.replace(/\.stl$/i, '.gcode') ??
      'output.gcode'
    );
  }
}
