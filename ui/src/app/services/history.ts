import { Injectable, computed, effect, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { SessionSummary } from '../../generated/slicer-engine-ws-server-message-v1';
import { SlicerConnection } from './slicer-connection';

export type { SessionSummary };

export type HistoryLoadState = 'pending' | 'loaded';

@Injectable({ providedIn: 'root' })
export class History {
  private readonly ws = inject(SlicerConnection);

  readonly sessions = signal<SessionSummary[]>([]);
  readonly loadState = signal<HistoryLoadState>('pending');

  /** True only once the first response has come back and the list is genuinely empty. */
  readonly isEmpty = computed(() => this.loadState() === 'loaded' && this.sessions().length === 0);

  constructor() {
    this.ws.messages$.pipe(takeUntilDestroyed()).subscribe((msg) => {
      if (msg.type === 'SessionsList') {
        this.sessions.set(msg.sessions);
        this.loadState.set('loaded');
      } else if (msg.type === 'SliceComplete') {
        this.refresh();
      }
    });

    effect(() => {
      if (this.ws.isConnected()) {
        this.loadState.set('pending');
        this.refresh();
      }
    });
  }

  refresh(): void {
    this.ws.send({ type: 'ListSessions' });
  }

  download(session: SessionSummary): void {
    const filename =
      (session.original_filename as string | null | undefined)?.replace(/\.stl$/i, '.gcode') ??
      'output.gcode';
    const link = document.createElement('a');
    link.href = session.download_url;
    link.download = filename;
    link.click();
  }

  formatDate(dateStr: string): string {
    try {
      return new Date(dateStr).toLocaleString();
    } catch {
      return dateStr;
    }
  }
}
