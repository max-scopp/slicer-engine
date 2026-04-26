import { Injectable, inject, signal, effect } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { DEFAULT_SETTINGS, SliceSettings } from '../models/slice-settings.model';
import { WebSocketService } from './websocket.service';
import { ServerMessage } from '../../generated/schemas/slicer-engine-ws-server-message-v1';

export type SlicerStatus = 'idle' | 'ready' | 'slicing' | 'done' | 'error';

@Injectable({ providedIn: 'root' })
export class SlicerService {
  private readonly ws = inject(WebSocketService);

  readonly selectedFile = signal<File | null>(null);
  readonly settings = signal<SliceSettings>(DEFAULT_SETTINGS);
  readonly status = signal<SlicerStatus>('idle');
  readonly outputLog = signal<string[]>([]);

  constructor() {
    // Pipe all WebSocket server messages into local state
    this.ws.messages$.pipe(takeUntilDestroyed()).subscribe(msg => this.handleMessage(msg));

    // Reflect WebSocket connection status in the log
    effect(() => {
      const connStatus = this.ws.status();
      if (connStatus === 'connected') {
        // Will also receive the 'connected' ServerMessage with version from server
      } else if (connStatus === 'disconnected') {
        this.outputLog.update(l => [...l, '[ws] Disconnected from server.']);
      } else if (connStatus === 'error') {
        this.outputLog.update(l => [...l, '[ws] Connection error — is the server running?']);
        if (this.status() === 'slicing') {
          this.status.set('error');
        }
      }
    });
  }

  private handleMessage(msg: ServerMessage): void {
    switch (msg.type) {
      case 'Connected':
        this.outputLog.update(l => [...l, `[ws] Connected to slicer-engine v${msg.version}`]);
        break;
      case 'Log':
        this.outputLog.update(l => [...l, `[${msg.level}] ${msg.message}`]);
        break;
      case 'Progress':
        this.outputLog.update(l => [
          ...l,
          `Progress: ${msg.current_layer} / ${msg.total_layers} layers`,
        ]);
        break;
      case 'SliceComplete':
        this.status.set('done');
        this.outputLog.update(l => [
          ...l,
          `Slice complete — ${msg.layer_count} layers generated.`,
          'Downloading G-code…',
        ]);
        this.downloadGcode(msg.gcode);
        break;
      case 'Error':
        this.status.set('error');
        this.outputLog.update(l => [...l, `[error] ${msg.message}`]);
        break;
    }
  }

  private downloadGcode(gcode: string): void {
    const filename =
      this.selectedFile()?.name.replace(/\.stl$/i, '.gcode') ?? 'output.gcode';
    const blob = new Blob([gcode], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  selectFile(file: File): void {
    this.selectedFile.set(file);
    this.status.set('ready');
    this.outputLog.update(log => [
      ...log,
      `File selected: ${file.name} (${(file.size / 1024).toFixed(1)} KB)`,
    ]);
  }

  updateSettings(patch: Partial<SliceSettings>): void {
    this.settings.update(current => ({ ...current, ...patch }));
  }

  async slice(): Promise<void> {
    const file = this.selectedFile();
    if (!file) return;

    this.status.set('slicing');
    this.outputLog.update(log => [...log, 'Reading file…']);

    const stl_b64 = await this.readFileAsBase64(file);
    const s = this.settings();

    this.outputLog.update(log => [...log, 'Sending to server over WebSocket…']);
    this.ws.send({
      type: 'Slice',
      stl_b64,
      settings: {
        layer_height: s.layerHeight,
        print_speed: s.printSpeed,
        nozzle_temp: s.nozzleTemp,
        bed_temp: s.bedTemp,
        gcode_flavor: s.gcodeFlavor,
      },
    });
  }

  private readFileAsBase64(file: File): Promise<string> {
    return new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = reader.result as string;
        // Strip the `data:<mime>;base64,` prefix
        resolve(result.split(',')[1]);
      };
      reader.onerror = () => reject(new Error('FileReader error'));
      reader.readAsDataURL(file);
    });
  }

  reset(): void {
    this.selectedFile.set(null);
    this.status.set('idle');
    this.outputLog.set([]);
    this.ws.send({ type: 'Reset' });
  }
}

