import { Injectable, computed, effect, inject, signal } from '@angular/core';
import { RuntimeHistorySession } from '../runtime/domain/history-models';
import { Slicer } from './slicer';

export type HistoryLoadState = 'pending' | 'loaded';

@Injectable({ providedIn: 'root' })
export class History {
  private readonly slicer = inject(Slicer);

  readonly sessions = signal<RuntimeHistorySession[]>([]);
  readonly loadState = signal<HistoryLoadState>('pending');

  /** True only once the first response has come back and the list is genuinely empty. */
  readonly isEmpty = computed(() => this.loadState() === 'loaded' && this.sessions().length === 0);

  constructor() {
    effect(() => {
      const ready = this.slicer.historyReady();
      const _version = this.slicer.historyVersion();
      if (ready) {
        void this.refresh();
      }
    });
  }

  async refresh(): Promise<void> {
    this.loadState.set('pending');
    try {
      const sessions = await this.slicer.getHistory();
      this.sessions.set(sessions);
      this.loadState.set('loaded');
    } catch {
      this.sessions.set([]);
      this.loadState.set('loaded');
    }
  }

  download(session: RuntimeHistorySession): void {
    void this.slicer.downloadHistorySession(session);
  }

  formatDate(dateStr: string): string {
    try {
      return new Date(dateStr).toLocaleString();
    } catch {
      return dateStr;
    }
  }
}
