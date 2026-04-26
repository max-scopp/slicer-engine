import { Injectable, signal } from '@angular/core';
import { Observable, Subject, EMPTY } from 'rxjs';
import { catchError, share } from 'rxjs/operators';
import { WebSocketSubject, webSocket } from 'rxjs/webSocket';
import { ClientMessage, ServerMessage } from '../generated/ws-protocol';

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

@Injectable({ providedIn: 'root' })
export class WebSocketService {
  private subject!: WebSocketSubject<unknown>;

  /** Current WebSocket connection status. */
  readonly status = signal<ConnectionStatus>('connecting');

  /** Stream of messages received from the server. */
  readonly messages$: Observable<ServerMessage>;

  constructor() {
    this.subject = this.createSubject();
    this.messages$ = this.subject.pipe(
      catchError(() => {
        this.status.set('error');
        return EMPTY;
      }),
      share(),
    ) as Observable<ServerMessage>;
  }

  private createSubject(): WebSocketSubject<unknown> {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url = `${protocol}//${window.location.host}/ws`;

    return webSocket<unknown>({
      url,
      openObserver: {
        next: () => this.status.set('connected'),
      },
      closeObserver: {
        next: () => this.status.set('disconnected'),
      },
    });
  }

  /** Send a message to the server. */
  send(msg: ClientMessage): void {
    this.subject.next(msg as unknown);
  }
}
