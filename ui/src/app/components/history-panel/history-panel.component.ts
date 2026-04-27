import { Component, inject, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { SlicerService, PreviousSession } from '../../services/slicer.service';

@Component({
  selector: 'app-history-panel',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './history-panel.component.html',
  styleUrl: './history-panel.component.scss',
})
export class HistoryPanelComponent implements OnInit {
  private readonly slicerService = inject(SlicerService);

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

  viewDebug(session: PreviousSession): void {
    this.slicerService.requestDebugLoad(session.request_uuid);
    // Scroll to debug viewer
    setTimeout(() => {
      const debugSection = document.querySelector('.debug-section');
      if (debugSection) {
        debugSection.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
    }, 100);
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
