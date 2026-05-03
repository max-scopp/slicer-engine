import { RuntimeMode } from '../domain/runtime-mode';

export interface RuntimeSessionState {
  mode: RuntimeMode;
  activeSliceId: string | null;
  lastError: string | null;
}

export class RuntimeSession {
  private state: RuntimeSessionState;

  constructor(mode: RuntimeMode) {
    this.state = {
      mode,
      activeSliceId: null,
      lastError: null,
    };
  }

  getState(): RuntimeSessionState {
    return this.state;
  }

  setActiveSlice(sliceId: string | null): void {
    this.state = {
      ...this.state,
      activeSliceId: sliceId,
    };
  }

  setError(message: string | null): void {
    this.state = {
      ...this.state,
      lastError: message,
    };
  }
}
