import { Injectable, signal } from '@angular/core';

export type NotificationSeverity = 'info' | 'success' | 'warning' | 'error';

export interface Notification {
  id: string;
  severity: NotificationSeverity;
  title: string;
  message?: string;
  /** When set, renders a progress bar (0–100). */
  progress?: number;
  /** When false the user can dismiss; when true the close button is hidden. */
  dismissible: boolean;
  /** Auto-dismiss after this many ms. Omit to keep until dismissed or updated. */
  autoDismissMs?: number;
}

let _nextId = 1;

@Injectable({ providedIn: 'root' })
export class NotificationService {
  readonly notifications = signal<Notification[]>([]);

  /** Push a simple informational toast and return its id. */
  info(title: string, message?: string, autoDismissMs = 4000): string {
    return this.push({ severity: 'info', title, message, autoDismissMs, dismissible: true });
  }

  success(title: string, message?: string, autoDismissMs = 4000): string {
    return this.push({ severity: 'success', title, message, autoDismissMs, dismissible: true });
  }

  warning(title: string, message?: string, autoDismissMs = 6000): string {
    return this.push({
      severity: 'warning',
      title,
      message,
      autoDismissMs,
      dismissible: true,
    });
  }

  error(title: string, message?: string): string {
    return this.push({ severity: 'error', title, message, dismissible: true });
  }

  /**
   * Push an interactive notification with a progress bar.
   * Returns the id — call `updateProgress` / `dismiss` to manage it.
   */
  progress(title: string, message?: string): string {
    return this.push({ severity: 'info', title, message, progress: 0, dismissible: false });
  }

  /** Update the progress (0–100) and optionally change the message. */
  updateProgress(id: string, progress: number, message?: string): void {
    this.notifications.update((list) =>
      list.map((n) =>
        n.id === id
          ? {
              ...n,
              progress,
              ...(message !== undefined ? { message } : {}),
            }
          : n,
      ),
    );
  }

  /** Complete a progress notification — switches to success and auto-dismisses. */
  completeProgress(id: string, title: string, message?: string): void {
    this.notifications.update((list) =>
      list.map((n) =>
        n.id === id
          ? {
              ...n,
              severity: 'success' as NotificationSeverity,
              title,
              message,
              progress: undefined,
              dismissible: true,
              autoDismissMs: 4000,
            }
          : n,
      ),
    );
    this.scheduleAutoDismiss(id, 4000);
  }

  /** Mark a progress notification as failed and auto-dismiss after a delay. */
  failProgress(id: string, title: string, message?: string): void {
    this.notifications.update((list) =>
      list.map((n) =>
        n.id === id
          ? {
              ...n,
              severity: 'error' as NotificationSeverity,
              title,
              message,
              progress: undefined,
              dismissible: true,
              autoDismissMs: 8000,
            }
          : n,
      ),
    );
    this.scheduleAutoDismiss(id, 8000);
  }

  dismiss(id: string): void {
    this.notifications.update((list) => list.filter((n) => n.id !== id));
  }

  private push(partial: Omit<Notification, 'id'>): string {
    const id = String(_nextId++);
    const notification: Notification = { id, ...partial };
    this.notifications.update((list) => [...list, notification]);

    if (notification.autoDismissMs) {
      this.scheduleAutoDismiss(id, notification.autoDismissMs);
    }

    return id;
  }

  private scheduleAutoDismiss(id: string, delayMs: number): void {
    setTimeout(() => this.dismiss(id), delayMs);
  }
}
