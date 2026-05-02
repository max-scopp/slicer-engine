import { RuntimeEvent, RuntimeEventListener } from '../ports/runtime-events';
import { RuntimeSubscription } from '../ports/runtime-port';

export class RuntimeEventBus {
  private readonly listeners = new Set<RuntimeEventListener>();

  subscribe(listener: RuntimeEventListener): RuntimeSubscription {
    this.listeners.add(listener);
    return {
      unsubscribe: () => {
        this.listeners.delete(listener);
      },
    };
  }

  emit(event: RuntimeEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }

  clear(): void {
    this.listeners.clear();
  }
}
