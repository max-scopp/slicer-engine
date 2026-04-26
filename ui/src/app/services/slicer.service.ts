import { Injectable, signal } from '@angular/core';
import { DEFAULT_SETTINGS, SliceSettings } from '../models/slice-settings.model';

export type SlicerStatus = 'idle' | 'ready' | 'slicing' | 'done' | 'error';

@Injectable({ providedIn: 'root' })
export class SlicerService {
  readonly selectedFile = signal<File | null>(null);
  readonly settings = signal<SliceSettings>(DEFAULT_SETTINGS);
  readonly status = signal<SlicerStatus>('idle');
  readonly outputLog = signal<string[]>([]);

  selectFile(file: File): void {
    this.selectedFile.set(file);
    this.status.set('ready');
    this.outputLog.update(log => [...log, `File selected: ${file.name} (${(file.size / 1024).toFixed(1)} KB)`]);
  }

  updateSettings(patch: Partial<SliceSettings>): void {
    this.settings.update(current => ({ ...current, ...patch }));
  }

  slice(): void {
    if (!this.selectedFile()) return;

    this.status.set('slicing');
    this.outputLog.update(log => [...log, 'Starting slice operation...']);

    // Simulate async slicing
    setTimeout(() => {
      const s = this.settings();
      this.outputLog.update(log => [
        ...log,
        `Layer height: ${s.layerHeight}mm`,
        `Print speed: ${s.printSpeed}mm/s`,
        `Nozzle temp: ${s.nozzleTemp}°C`,
        `Bed temp: ${s.bedTemp}°C`,
        `GCode flavor: ${s.gcodeFlvor}`,
        'Slice complete.',
      ]);
      this.status.set('done');
    }, 1500);
  }

  reset(): void {
    this.selectedFile.set(null);
    this.status.set('idle');
    this.outputLog.set([]);
  }
}
