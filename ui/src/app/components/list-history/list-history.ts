import { Component, inject } from '@angular/core';
import { Router } from '@angular/router';
import { History, SessionSummary } from '../../services/history';

@Component({
  selector: 'nexus-list-history',
  standalone: true,
  templateUrl: './list-history.component.html',
  styleUrl: './list-history.component.scss',
})
export class ListHistory {
  protected readonly history = inject(History);
  readonly #router = inject(Router);

  navigate(session: SessionSummary): void {
    void this.#router.navigate(['/slice', session.request_uuid]);
  }

  download(session: SessionSummary): void {
    this.history.download(session);
  }
}
