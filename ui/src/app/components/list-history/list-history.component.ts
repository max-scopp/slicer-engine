import { Component, inject } from '@angular/core';
import { History, SessionSummary } from '../../services/history';

@Component({
  selector: 'nexus-list-history',
  standalone: true,
  templateUrl: './list-history.component.html',
  styleUrl: './list-history.component.scss',
})
export class ListHistoryComponent {
  protected readonly history = inject(History);

  download(session: SessionSummary): void {
    this.history.download(session);
  }
}
