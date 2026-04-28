import { Injectable, signal } from '@angular/core';
import { Observable, Subject, EMPTY } from 'rxjs';
import { catchError, share } from 'rxjs/operators';
import { WebSocketSubject, webSocket } from 'rxjs/webSocket';
import { ServerMessage } from '../../generated/slicer-engine-ws-server-message-v1';
import { ClientMessage } from '../../generated/slicer-engine-ws-client-message-v1';
import { environment } from '../../environments/environment';

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
      // @ts-ignore
      catchError<any>(() => {
        this.status.set('error');
        return EMPTY;
      }),
      share(),
    ) as Observable<ServerMessage>;
  }

  private createSubject(): WebSocketSubject<unknown> {
    const url = environment.wsUrl;

    return webSocket<unknown>({
      url,
      openObserver: {
        next: () => {
          console.log('[WebSocket] Connected to', url);
          this.status.set('connected');
        },
      },
      closeObserver: {
        next: () => {
          console.log('[WebSocket] Disconnected from', url);
          this.status.set('disconnected');
        },
      },
    });
  }

  /** Send a message to the server. */
  send(msg: ClientMessage): void {
    this.subject.next(msg as unknown);
  }
}
