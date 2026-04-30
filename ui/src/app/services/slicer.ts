import { HttpClient } from '@angular/common/http';
import { Injectable, computed, effect, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { environment } from '../../environments/environment';
import {
  SceneObjectSliceDto,
  SlicingParams,
} from '../../generated/slicer-engine-ws-client-message-v1';
import { ServerMessage } from '../../generated/slicer-engine-ws-server-message-v1';
import { DEFAULT_SETTINGS } from '../models/slice-settings.model';
import { History } from './history';
import { NotificationService } from './notifications';
import { SceneEngineService } from './scene-engine.service';
import { SlicerConnection } from './slicer-connection';
import { SlicerFile } from './slicer-file';

/** Human-readable label for each pipeline phase. */
export const PHASE_LABELS: Record<string, string> = {
  total: 'Slicing',
  mesh_load: 'Loading mesh',
  mesh_analysis: 'Analysing mesh',
  slicing: 'Slicing layers',
  arachne_walls: 'Generating walls',
  infill_region_snapshot: 'Mapping infill regions',
  wall_restrictions: 'Applying wall restrictions',
  interior_regions: 'Computing interior regions',
  surfaces: 'Generating surfaces',
  infill: 'Generating infill',
  gcode_generation: 'Generating G-code',
  file_write: 'Writing output',
};

/**
 * Proportional weights per phase derived from typical Benchy timings.
 * `total` is the outer span and excluded from progress accumulation.
 */
const PHASE_WEIGHTS: Record<string, number> = {
  mesh_load: 6,
  mesh_analysis: 1,
  slicing: 46,
  arachne_walls: 11,
  infill_region_snapshot: 4,
  wall_restrictions: 7,
  interior_regions: 4,
  surfaces: 8,
  infill: 2,
  gcode_generation: 13,
  file_write: 1,
};
const PHASE_TOTAL_WEIGHT = Object.values(PHASE_WEIGHTS).reduce((a, b) => a + b, 0);

export type SlicerStatus = 'idle' | 'ready' | 'uploading' | 'slicing' | 'done' | 'error';

export interface PhaseTimingData {
  phase: string;
  startTime?: number;
  endTime?: number;
  elapsedMs?: number;
}

@Injectable({ providedIn: 'root' })
export class Slicer {
  private readonly ws = inject(SlicerConnection);
  private readonly http = inject(HttpClient);
  private readonly slicerFile = inject(SlicerFile);
  private readonly history = inject(History);
  private readonly notifications = inject(NotificationService);
  private readonly sceneEngine = inject(SceneEngineService);

  /**
   * Currently-selected file. Sourced from {@link SlicerFile} so the upload
   * page and the viewer page share a single source of truth.
   */
  readonly selectedFile = this.slicerFile.selectedFile;
  readonly settings = signal<SlicingParams>(DEFAULT_SETTINGS);
  readonly status = signal<SlicerStatus>('idle');
  readonly outputLog = signal<string[]>([]);
  readonly phaseTimings = signal<PhaseTimingData[]>([]);

  /** Resolved download URL for the last completed slice, or `null` when none. */
  readonly gcodeDownloadUrl = signal<string | null>(null);

  /** Name of the pipeline phase currently executing, or `null` when idle. */
  readonly currentPhase = signal<string | null>(null);

  /**
   * Overall slice progress 0–100.
   *
   * - When `status === 'done'`, always returns 100.
   * - Each phase has a known proportional weight. Completed phases (those with
   *   an `endTime`) are stacked in order; their cumulative weight over the
   *   total determines the percentage.
   * - Capped at 99 until `SliceComplete` arrives to avoid a premature 100%.
   */
  readonly sliceProgress = computed(() => {
    if (this.status() === 'done') return 100;

    const timings = this.phaseTimings();
    let completedWeight = 0;

    for (const t of timings) {
      if (t.endTime != null && t.phase !== 'total' && PHASE_WEIGHTS[t.phase] != null) {
        completedWeight += PHASE_WEIGHTS[t.phase];
      }
    }

    return Math.min(99, Math.round((completedWeight / PHASE_TOTAL_WEIGHT) * 100));
  });

  constructor() {
    // Pipe all WebSocket server messages into local state
    this.ws.messages$.pipe(takeUntilDestroyed()).subscribe((msg) => this.handleMessage(msg));

    // Reflect WebSocket connection status in the log
    effect(() => {
      const connStatus = this.ws.status();
      if (connStatus === 'connected') {
        // Will also receive the 'connected' ServerMessage with version from server
      } else if (connStatus === 'disconnected') {
        this.outputLog.update((l) => [...l, '[ws] Disconnected from server.']);
      } else if (connStatus === 'failed') {
        this.outputLog.update((l) => [...l, '[ws] Connection error — is the server running?']);
        if (this.status() === 'slicing') {
          this.status.set('error');
        }
      }
    });
  }

  private handleMessage(msg: ServerMessage): void {
    switch (msg.type) {
      case 'Connected':
        this.outputLog.update((l) => [...l, `[ws] Connected to slicer-engine v${msg.version}`]);
        break;
      case 'Log':
        this.outputLog.update((l) => [...l, `[${msg.level}] ${msg.message}`]);
        break;
      case 'PhaseMarker':
        this.handlePhaseMarker(msg);
        break;
      case 'Progress':
        this.outputLog.update((l) => [
          ...l,
          `Progress: ${msg.current_layer} / ${msg.total_layers} layers`,
        ]);
        break;
      case 'SliceComplete': {
        this.status.set('done');
        this.currentPhase.set(null);

        // Log overall phase timings to the browser console
        const timings = this.phaseTimings();
        const timingLines = timings
          .filter((t) => t.elapsedMs != null)
          .map((t) => `  ${(PHASE_LABELS[t.phase] ?? t.phase).padEnd(28)} ${t.elapsedMs} ms`)
          .join('\n');
        console.log(
          `[slicer] Slice complete — ${msg.layer_count} layers\nPhase timings:\n${timingLines}`,
        );

        const resolvedUrl = msg.download_url.startsWith('/')
          ? `${environment.apiUrl}${msg.download_url}`
          : msg.download_url;
        this.gcodeDownloadUrl.set(resolvedUrl);

        this.notifications.success(
          'Slice complete',
          `${msg.layer_count} layers — click Download to save G-code`,
          6000,
        );

        this.outputLog.update((l) => [
          ...l,
          `Slice complete — ${msg.layer_count} layers generated.`,
        ]);
        break;
      }
      case 'Error':
        this.status.set('error');
        this.outputLog.update((l) => [...l, `[error] ${msg.message}`]);
        break;
    }
  }

  private handlePhaseMarker(msg: {
    phase: string;
    event: string;
    elapsed_ms?: number | null;
  }): void {
    const now = Date.now();

    if (msg.event === 'start') {
      // Phase started - track as the current active phase and add timing entry
      if (msg.phase !== 'total') {
        this.currentPhase.set(msg.phase);
      }
      this.phaseTimings.update((timings) => {
        const existing = timings.find((t) => t.phase === msg.phase);
        if (existing) {
          existing.startTime = now;
          existing.endTime = undefined;
          existing.elapsedMs = undefined;
          return [...timings];
        } else {
          return [...timings, { phase: msg.phase, startTime: now }];
        }
      });
      this.outputLog.update((l) => [...l, `[phase] ${msg.phase} → start`]);
    } else if (msg.event === 'end' && msg.elapsed_ms != null) {
      // Phase ended - update with elapsed time
      this.phaseTimings.update((timings) => {
        const existing = timings.find((t) => t.phase === msg.phase);
        if (existing) {
          existing.endTime = now;
          existing.elapsedMs = msg.elapsed_ms ?? undefined;
          return [...timings];
        } else {
          // Phase end without start (shouldn't happen, but handle it)
          return [
            ...timings,
            { phase: msg.phase, endTime: now, elapsedMs: msg.elapsed_ms ?? undefined },
          ];
        }
      });
      // Clear current phase only if it's the one that just ended
      if (this.currentPhase() === msg.phase) {
        this.currentPhase.set(null);
      }
      this.outputLog.update((l) => [...l, `[phase] ${msg.phase} ✓ ${msg.elapsed_ms} ms`]);
    }
  }

  downloadGcode(): void {
    const url = this.gcodeDownloadUrl();
    if (!url) {
      return;
    }
    const filename = this.selectedFile()?.name.replace(/\.stl$/i, '.gcode') ?? 'output.gcode';
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.click();
  }

  selectFile(file: File): void {
    this.slicerFile.selectFile(file);
    this.status.set('ready');
    this.outputLog.update((log) => [
      ...log,
      `File selected: ${file.name} (${(file.size / 1024 / 1024).toFixed(1)} MB)`,
    ]);
  }

  updateSettings(patch: Partial<SlicingParams>): void {
    this.settings.update((current) => ({ ...current, ...patch }));
  }

  /**
   * Build the wire-format scene that goes alongside a slice request.
   *
   * The frontend owns its scene as a signal of {@link SceneObjectSnapshot}s
   * inside {@link SceneEngineService}. Slicing on the server has to see the
   * exact same objects with the exact same transforms, otherwise the user's
   * translate/rotate/scale/center/drop-to-floor edits silently disappear at
   * slice time. We rebuild the snapshot fresh on every call so transient
   * sync issues are impossible.
   *
   * Each entry references the original upload by `file_id` (the request
   * UUID returned by `POST /api/upload`). The server reads the bytes from
   * the work directory, applies the transform via `apply_transform`, and
   * merges the resulting meshes before `process_mesh`.
   *
   * **Single-upload caveat.** Today every scene object is assumed to have
   * come from the same upload (`uploadFileId`). The wire format already
   * supports per-object `file_id`s for a true multi-file scene, but the
   * `SceneObjectSnapshot` produced by the WASM engine doesn't yet carry
   * the originating upload UUID. When the multi-upload UX lands, replace
   * the constant `uploadFileId` here with `o.file_id` (or whichever field
   * the snapshot grows) — the server side already handles per-object
   * `file_id` correctly.
   */
  private buildSceneSnapshot(uploadFileId: string): SceneObjectSliceDto[] {
    const objects = this.sceneEngine.objects();
    if (objects.length === 0) {
      // Scene engine hasn't been populated (e.g. legacy slice-new flow that
      // hasn't loaded the file into the WASM scene yet). Fall back to a
      // single identity-transform object so the server still slices the
      // uploaded file with the user's chosen settings.
      return [
        {
          file_id: uploadFileId,
          format: 'stl',
          transform: {
            translation: [0, 0, 0],
            euler_xyz_deg: [0, 0, 0],
            scale: [1, 1, 1],
          },
        },
      ];
    }
    return objects.map((o) => ({
      file_id: uploadFileId,
      format: 'stl',
      transform: {
        translation: o.translation,
        euler_xyz_deg: o.euler_xyz_deg,
        scale: o.scale,
      },
    }));
  }

  async slice(): Promise<void> {
    const file = this.selectedFile();
    if (!file) {
      return;
    }

    // Reset phase state for fresh run
    this.phaseTimings.set([]);
    this.currentPhase.set(null);
    this.gcodeDownloadUrl.set(null);

    // If the file was already uploaded (navigated from slice-new), reuse the UUID
    const existingUuid = this.slicerFile.requestUuid();
    if (existingUuid) {
      this.status.set('slicing');
      this.outputLog.update((log) => [...log, `Starting slice job (request: ${existingUuid})…`]);
      this.ws.send({
        type: 'Slice',
        request_uuid: existingUuid,
        scene: this.buildSceneSnapshot(existingUuid),
        settings: this.settings(),
      });
      return;
    }

    this.status.set('uploading');
    this.outputLog.update((log) => [...log, 'Uploading file…']);

    try {
      // Step 1: Upload file via HTTP
      const formData = new FormData();
      formData.append('file', file);

      const uploadResponse = await this.http
        .post<{ request_uuid: string }>(`${environment.apiUrl}/upload`, formData)
        .toPromise();

      if (!uploadResponse) {
        throw new Error('No response from upload');
      }

      const requestUuid = uploadResponse.request_uuid;
      this.outputLog.update((log) => [
        ...log,
        `Upload complete. Request ID: ${requestUuid}`,
        'Starting slice job…',
      ]);

      // Step 2: Send slice request via WebSocket with request_uuid
      this.status.set('slicing');

      this.ws.send({
        type: 'Slice',
        request_uuid: requestUuid,
        scene: this.buildSceneSnapshot(requestUuid),
        settings: this.settings(),
      });
    } catch (error) {
      this.status.set('error');
      this.outputLog.update((log) => [
        ...log,
        `[error] Upload failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      ]);
    }
  }

  reset(): void {
    this.slicerFile.reset();
    this.status.set('idle');
    this.outputLog.set([]);
    this.phaseTimings.set([]);
    this.currentPhase.set(null);
    this.gcodeDownloadUrl.set(null);
    this.ws.send({ type: 'Reset' });
  }

  loadPreviousSessions(): void {
    this.history.refresh();
  }

  downloadFromHistory(session: { download_url: string; original_filename?: string | null }): void {
    this.history.download(session as import('./history').SessionSummary);
  }
}
