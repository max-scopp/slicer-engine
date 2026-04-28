import { DestroyRef, Injectable, computed, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { EMPTY, Observable, Subject, timer } from 'rxjs';
import { catchError, share, switchMap, tap } from 'rxjs/operators';
import { WebSocketSubject, webSocket } from 'rxjs/webSocket';
import { environment } from '../../environments/environment';
import { ClientMessage } from '../../generated/slicer-engine-ws-client-message-v1';
import { ServerMessage } from '../../generated/slicer-engine-ws-server-message-v1';

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'failed';

const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 2000;

@Injectable({ providedIn: 'root' })
export class SlicerConnection {
  readonly #destroyRef = inject(DestroyRef);
  readonly #reconnect$ = new Subject<void>();

  #subject: WebSocketSubject<ServerMessage> | null = null;
  #retryCount = 0;

  readonly status = signal<ConnectionStatus>('connecting');
  readonly retryCount = signal(0);
  readonly isFailed = computed(() => this.status() === 'failed');
  readonly isConnected = computed(() => this.status() === 'connected');

  readonly messages$: Observable<ServerMessage>;

  constructor() {
    const shared$ = this.#reconnect$.pipe(
      switchMap(() => this.#connect()),
      share(),
      takeUntilDestroyed(this.#destroyRef),
    );

    this.messages$ = shared$;
    shared$.subscribe();

    this.#reconnect$.next();
  }

  send(msg: ClientMessage): void {
    this.#subject?.next(msg as unknown as ServerMessage);
  }

  retry(): void {
    if (this.status() !== 'failed') {
      return;
    }

    this.#retryCount = 0;
    this.retryCount.set(0);
    this.status.set('connecting');
    this.#reconnect$.next();
  }

  #connect(): Observable<ServerMessage> {
    this.#subject?.complete();

    this.#subject = webSocket<ServerMessage>({
      url: environment.wsUrl,
      openObserver: {
        next: () => {
          this.#retryCount = 0;
          this.retryCount.set(0);
          this.status.set('connected');
        },
      },
      closeObserver: {
        next: () => {
          if (this.status() === 'connected') {
            this.status.set('disconnected');
          }
        },
      },
    });

    return this.#subject.pipe(
      catchError(() => {
        this.#retryCount++;
        this.retryCount.set(this.#retryCount);

        if (this.#retryCount >= MAX_RETRIES) {
          this.status.set('failed');
          return EMPTY;
        }

        this.status.set('connecting');
        return timer(RETRY_DELAY_MS).pipe(
          tap(() => this.#reconnect$.next()),
          switchMap(() => EMPTY),
        );
      }),
    );
  }
}
