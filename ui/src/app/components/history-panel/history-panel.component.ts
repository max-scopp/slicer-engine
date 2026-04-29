import { CommonModule } from '@angular/common';
import { Component, inject, OnInit } from '@angular/core';
import { PreviousSession, Slicer } from '../../services/slicer';

@Component({
  selector: 'nexus-history-panel',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './history-panel.component.html',
  styleUrl: './history-panel.component.scss',
})
export class HistoryPanelComponent implements OnInit {
  private readonly slicerService = inject(Slicer);

  readonly previousSessions = this.slicerService.previousSessions;

  ngOnInit(): void {
    this.loadHistory();
  }

  loadHistory(): void {
    this.slicerService.loadPreviousSessions();
  }

  downloadSession(session: PreviousSession): void {
    this.slicerService.downloadFromHistory(session);
  }

  formatDate(dateStr: string): string {
    try {
      const date = new Date(dateStr);
      return date.toLocaleString();
    } catch {
      return dateStr;
    }
  }

  getFilename(session: PreviousSession): string {
    return session.original_filename?.replace(/\.stl$/i, '.gcode') ?? 'output.gcode';
  }
}
