import { Injectable, isDevMode } from '@angular/core';

/** Severity for {@link LoggerService}. Mapped to the matching `console` method. */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

const LEVEL_ORDER: Record<LogLevel, number> = {
  debug: 10,
  info: 20,
  warn: 30,
  error: 40,
};

/**
 * Lightweight scoped logger used across UI services.
 *
 * `LoggerService.scope('Foo')` returns a `ScopedLogger` that prefixes every
 * line with `[Foo]`, supports `debug`/`info`/`warn`/`error`, and offers a
 * `time()` helper that returns a stop function reporting elapsed milliseconds.
 *
 * The minimum level defaults to `debug` in `ng serve` and `info` in
 * production builds; flip it at runtime via {@link setMinLevel}.
 */
@Injectable({ providedIn: 'root' })
export class LoggerService {
  private minLevel: LogLevel = isDevMode() ? 'debug' : 'info';

  setMinLevel(level: LogLevel): void {
    this.minLevel = level;
  }

  scope(name: string): ScopedLogger {
    return new ScopedLogger(name, this);
  }

  /** @internal — used by {@link ScopedLogger}. */
  shouldLog(level: LogLevel): boolean {
    return LEVEL_ORDER[level] >= LEVEL_ORDER[this.minLevel];
  }
}

/** Result of {@link ScopedLogger.time}. Call to stop the timer and emit a log line. */
export type StopTimer = (extra?: Record<string, unknown>) => number;

/** Maximum samples retained per `(scope, label)` rolling-average window. */
const ROLLING_WINDOW_SIZE = 100;

export class ScopedLogger {
  /** Per-label ring buffer of recent durations (ms) for rolling averages. */
  private readonly samples = new Map<string, number[]>();

  constructor(
    private readonly name: string,
    private readonly parent: LoggerService,
  ) {}

  debug(message: string, ...args: unknown[]): void {
    this.emit('debug', message, args);
  }

  info(message: string, ...args: unknown[]): void {
    this.emit('info', message, args);
  }

  warn(message: string, ...args: unknown[]): void {
    this.emit('warn', message, args);
  }

  error(message: string, ...args: unknown[]): void {
    this.emit('error', message, args);
  }

  /**
   * Start a high-resolution timer. The returned function stops the timer
   * and emits a `debug` line with the elapsed milliseconds, the rolling
   * average over the last {@link ROLLING_WINDOW_SIZE} calls with the same
   * `label`, plus any extra fields supplied at stop time. Returns the
   * elapsed milliseconds.
   */
  time(label: string): StopTimer {
    const start = performance.now();
    return (extra) => {
      const elapsedMs = performance.now() - start;
      const avgMs = this.recordSample(label, elapsedMs);
      const window = this.samples.get(label)!;
      const suffix = `(${elapsedMs.toFixed(2)} ms · avg ${avgMs.toFixed(2)} ms over ${window.length})`;
      // if (extra) {
      //   this.debug(`${label} ${suffix}`, extra);
      // } else {
      //   this.debug(`${label} ${suffix}`);
      // }
      return elapsedMs;
    };
  }

  private recordSample(label: string, elapsedMs: number): number {
    let window = this.samples.get(label);
    if (!window) {
      window = [];
      this.samples.set(label, window);
    }
    window.push(elapsedMs);
    if (window.length > ROLLING_WINDOW_SIZE) {
      window.shift();
    }
    let total = 0;
    for (const ms of window) {
      total += ms;
    }
    return total / window.length;
  }

  /**
   * Snapshot of the rolling window for `label`. Returns `null` if no samples
   * have been recorded yet (e.g. before the first `time()` stop fires).
   */
  stats(label: string): { lastMs: number; avgMs: number; count: number } | null {
    const window = this.samples.get(label);
    if (!window || window.length === 0) {
      return null;
    }
    let total = 0;
    for (const ms of window) {
      total += ms;
    }
    return {
      lastMs: window[window.length - 1],
      avgMs: total / window.length,
      count: window.length,
    };
  }

  private emit(level: LogLevel, message: string, args: unknown[]): void {
    if (!this.parent.shouldLog(level)) {
      return;
    }
    const prefix = `[${this.name}]`;
    // eslint-disable-next-line no-console
    console[level](prefix, message, ...args);
  }
}
