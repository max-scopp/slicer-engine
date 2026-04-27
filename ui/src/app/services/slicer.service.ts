import { Injectable, inject, signal, effect } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { HttpClient } from '@angular/common/http';
import { DEFAULT_SETTINGS, SliceSettings } from '../models/slice-settings.model';
import { WebSocketService } from './websocket.service';
import { ServerMessage } from '../../generated/slicer-engine-ws-server-message-v1';

export type SlicerStatus = 'idle' | 'ready' | 'uploading' | 'slicing' | 'done' | 'error';

export interface PreviousSession {
  request_uuid: string;
  original_filename?: string | null;
  layer_count?: number | null;
  created_at: string;
  download_url: string;
}

export interface PhaseTimingData {
  phase: string;
  startTime?: number;
  endTime?: number;
  elapsedMs?: number;
}

@Injectable({ providedIn: 'root' })
export class SlicerService {
  private readonly ws = inject(WebSocketService);
  private readonly http = inject(HttpClient);

  readonly selectedFile = signal<File | null>(null);
  readonly settings = signal<SliceSettings>(DEFAULT_SETTINGS);
  readonly status = signal<SlicerStatus>('idle');
  readonly outputLog = signal<string[]>([]);
  readonly previousSessions = signal<PreviousSession[]>([]);
  readonly phaseTimings = signal<PhaseTimingData[]>([]);

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
      case 'PhaseMarker':
        this.handlePhaseMarker(msg);
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
        this.downloadGcode(msg.download_url);
        // Refresh history to show the newly completed session
        this.loadPreviousSessions();
        break;
      case 'SessionsList':
        this.previousSessions.set(
          msg.sessions.map(s => ({
            request_uuid: s.request_uuid,
            original_filename: s.original_filename,
            layer_count: s.layer_count,
            created_at: s.created_at,
            download_url: s.download_url,
          }))
        );
        break;
      case 'Error':
        this.status.set('error');
        this.outputLog.update(l => [...l, `[error] ${msg.message}`]);
        break;
    }
  }

  private handlePhaseMarker(msg: { phase: string; event: string; elapsed_ms?: number | null }): void {
    const now = Date.now();
    
    if (msg.event === 'start') {
      // Phase started - add or update the timing entry
      this.phaseTimings.update(timings => {
        const existing = timings.find(t => t.phase === msg.phase);
        if (existing) {
          existing.startTime = now;
          existing.endTime = undefined;
          existing.elapsedMs = undefined;
          return [...timings];
        } else {
          return [...timings, { phase: msg.phase, startTime: now }];
        }
      });
      this.outputLog.update(l => [...l, `[phase] ${msg.phase} → start`]);
    } else if (msg.event === 'end' && msg.elapsed_ms != null) {
      // Phase ended - update with elapsed time
      this.phaseTimings.update(timings => {
        const existing = timings.find(t => t.phase === msg.phase);
        if (existing) {
          existing.endTime = now;
          existing.elapsedMs = msg.elapsed_ms ?? undefined;
          return [...timings];
        } else {
          // Phase end without start (shouldn't happen, but handle it)
          return [...timings, { phase: msg.phase, endTime: now, elapsedMs: msg.elapsed_ms ?? undefined }];
        }
      });
      this.outputLog.update(l => [...l, `[phase] ${msg.phase} ✓ ${msg.elapsed_ms} ms`]);
    }
  }

  private downloadGcode(downloadUrl: string): void {
    const filename =
      this.selectedFile()?.name.replace(/\.stl$/i, '.gcode') ?? 'output.gcode';
    const link = document.createElement('a');
    link.href = downloadUrl;
    link.download = filename;
    link.click();
  }

  selectFile(file: File): void {
    this.selectedFile.set(file);
    this.status.set('ready');
    this.outputLog.update(log => [
      ...log,
      `File selected: ${file.name} (${(file.size / 1024 / 1024).toFixed(1)} MB)`,
    ]);
  }

  updateSettings(patch: Partial<SliceSettings>): void {
    this.settings.update(current => ({ ...current, ...patch }));
  }

  async slice(): Promise<void> {
    const file = this.selectedFile();
    if (!file) {
      return;
    }

    this.status.set('uploading');
    this.outputLog.update(log => [...log, 'Uploading file…']);

    try {
      // Step 1: Upload file via HTTP
      const formData = new FormData();
      formData.append('file', file);

      const uploadResponse = await this.http
        .post<{ request_uuid: string }>('/api/upload', formData)
        .toPromise();

      if (!uploadResponse) {
        throw new Error('No response from upload');
      }

      const requestUuid = uploadResponse.request_uuid;
      this.outputLog.update(log => [
        ...log,
        `Upload complete. Request ID: ${requestUuid}`,
        'Starting slice job…',
      ]);

      // Step 2: Send slice request via WebSocket with request_uuid
      const s = this.settings();
      this.status.set('slicing');

      this.ws.send({
        type: 'Slice',
        request_uuid: requestUuid,
        settings: {
          layer_height: s.layerHeight,
          print_speed: s.printSpeed,
          nozzle_temp: s.nozzleTemp,
          bed_temp: s.bedTemp,
          gcode_flavor: s.gcodeFlavor,
          infill_density: s.infillDensity,
          infill_pattern: s.infillPattern,
          infill_angle: s.infillAngle,
        },
      });
    } catch (error) {
      this.status.set('error');
      this.outputLog.update(log => [
        ...log,
        `[error] Upload failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      ]);
    }
  }

  reset(): void {
    this.selectedFile.set(null);
    this.status.set('idle');
    this.outputLog.set([]);
    this.phaseTimings.set([]);
    this.ws.send({ type: 'Reset' });
  }

  loadPreviousSessions(): void {
    this.ws.send({ type: 'ListSessions' });
  }

  downloadFromHistory(session: PreviousSession): void {
    const filename = session.original_filename?.replace(/\.stl$/i, '.gcode') ?? 'output.gcode';
    const link = document.createElement('a');
    link.href = session.download_url;
    link.download = filename;
    link.click();
  }
}

