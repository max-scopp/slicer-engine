import { Injectable, computed, effect, inject, signal } from '@angular/core';
import init, { GcodeHandle } from '../../generated/scene-wasm/scene_engine';
import { Slicer } from './slicer';

// ── Role palette — single source of truth shared by the viewer and controls ──

export const ROLE_COLORS = {
  outerWall: 0xff8800, // amber-orange  (legend: outer wall)
  innerWall: 0xffcc00, // golden-yellow (legend: inner wall)
  infill: 0xcc44ff, // violet-purple (legend: sparse infill)
  topSurface: 0xff3355, // crimson-pink  (legend: top surface)
  bottomSurface: 0x00bbff, // vivid cyan  (legend: bottom surface)
  travel: 0x334466, // dark slate    (legend: travel)
  other: 0x44ffaa, // mint-green    (twist: stands apart)
  bridge: 0x00e5ff, // bright teal   (legend: bridge)
  skirt: 0x888888, // mid-gray      (legend: skirt/brim)
  support: 0xffaa00, // warm amber    (legend: support material)
  seam: 0xffffff, // white         (legend: seam point)
} as const;

export type RoleName = keyof typeof ROLE_COLORS;

export const ROLE_LABELS: Record<RoleName, string> = {
  outerWall: 'Outer Wall',
  innerWall: 'Inner Wall',
  infill: 'Infill',
  topSurface: 'Top Surface',
  bottomSurface: 'Bottom Surface',
  travel: 'Travel',
  other: 'Other',
  bridge: 'Bridge',
  skirt: 'Skirt / Brim',
  support: 'Support',
  seam: 'Seam',
};

export const ROLE_CSS = Object.fromEntries(
  Object.entries(ROLE_COLORS).map(([k, v]) => [k, `#${v.toString(16).padStart(6, '0')}`]),
) as Record<RoleName, string>;

export const ROLE_ORDER: readonly RoleName[] = [
  'outerWall',
  'innerWall',
  'infill',
  'topSurface',
  'bottomSurface',
  'bridge',
  'skirt',
  'support',
  'travel',
  'seam',
  'other',
] as const;

// ── Service ───────────────────────────────────────────────────────────────────

/**
 * Owns the parsed `GcodeHandle` for the current slice session and exposes
 * reactive signals consumed by both the `SlicePreviewControlsComponent` and
 * the `Viewer` gcode rendering path.
 *
 * When `isProgressMode` is `true` the UI collapses to a single-thumb slider:
 * `layerMin` is always 0 and `layerMax` sweeps forward as the user drags,
 * simulating the printer building the object layer by layer.
 */
@Injectable({ providedIn: 'root' })
export class GcodePreview {
  private readonly slicer = inject(Slicer);

  /** Parsed handle — `null` until a slice download URL is available. */
  readonly gcodeHandle = signal<GcodeHandle | null>(null);

  /** `true` while bytes are being fetched / parsed. */
  readonly loading = signal(false);

  /** Derived total layer count. */
  readonly layerCount = computed(() => this.gcodeHandle()?.layerCount() ?? 0);

  /**
   * Lower bound of the visible layer range (0-based index).
   * Always 0 when `isProgressMode` is `true`.
   */
  readonly layerMin = signal(0);

  /**
   * Upper bound of the visible layer range (0-based index).
   * The single moving thumb in progress mode.
   */
  readonly layerMax = signal(0);

  /**
   * Fractional scrub position within the top-most visible layer [0, 1].
   * 0 = nothing shown; 1 = full layer revealed.
   */
  readonly segmentProgress = signal(1);

  /** Set of roles to hide in the viewer. */
  readonly hiddenRoles = signal<ReadonlySet<RoleName>>(new Set<RoleName>());

  /**
   * When `false` (range mode) both `layerMin` and `layerMax` are independent
   * thumbs.  When `true` (progress mode) `layerMin` is locked to 0 and only
   * `layerMax` moves.
   */
  readonly isProgressMode = signal(false);

  constructor() {
    // React to every new download URL produced by the slicer service.
    effect(() => {
      const url = this.slicer.gcodeDownloadUrl();
      if (!url) {
        return;
      }
      void this.#loadFromUrl(url);
    });
  }

  // ── Mutators ────────────────────────────────────────────────────────────

  setLayerMin(value: number): void {
    const count = this.layerCount();
    if (count === 0) {
      return;
    }
    const clamped = Math.max(0, Math.min(value, this.layerMax()));
    this.layerMin.set(clamped);
  }

  setLayerMax(value: number): void {
    const count = this.layerCount();
    if (count === 0) {
      return;
    }
    if (this.isProgressMode()) {
      // Progress mode: only one layer visible at a time.
      const clamped = Math.max(0, Math.min(value, count - 1));
      this.layerMin.set(clamped);
      this.layerMax.set(clamped);
    } else {
      const clamped = Math.max(this.layerMin(), Math.min(value, count - 1));
      this.layerMax.set(clamped);
    }
  }

  /** Shift the entire range window by `delta` layers without changing window size. */
  shiftRange(delta: number): void {
    const count = this.layerCount();
    if (count === 0) {
      return;
    }
    const min = this.layerMin();
    const max = this.layerMax();
    const span = max - min;
    const newMin = Math.max(0, Math.min(count - 1 - span, min + delta));
    this.layerMin.set(newMin);
    this.layerMax.set(newMin + span);
  }

  setSegmentProgress(value: number): void {
    this.segmentProgress.set(Math.max(0, Math.min(1, value)));
  }

  toggleRole(role: RoleName): void {
    const current = this.hiddenRoles();
    const next = new Set(current);
    if (next.has(role)) {
      next.delete(role);
    } else {
      next.add(role);
    }
    this.hiddenRoles.set(next);
  }

  toggleProgressMode(): void {
    const next = !this.isProgressMode();
    this.isProgressMode.set(next);
    if (next) {
      // Progress mode: collapse to single layer at current max.
      this.layerMin.set(this.layerMax());
    }
  }

  // ── Private ──────────────────────────────────────────────────────────────

  async #loadFromUrl(url: string): Promise<void> {
    this.loading.set(true);
    this.gcodeHandle.set(null);
    try {
      await init({ module_or_path: 'scene_engine_bg.wasm' });
      const response = await fetch(url);
      const buffer = await response.arrayBuffer();
      const handle = GcodeHandle.parse(new Uint8Array(buffer));
      const count = handle.layerCount();
      this.gcodeHandle.set(handle);
      this.layerMin.set(0);
      this.layerMax.set(Math.max(0, count - 1));
      this.segmentProgress.set(1);
      this.hiddenRoles.set(new Set<RoleName>());
      this.isProgressMode.set(false);
    } catch (error) {
      console.error('[GcodePreview] Failed to load gcode:', error);
    } finally {
      this.loading.set(false);
    }
  }
}
