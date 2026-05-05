import { Injectable, computed, effect, inject, signal } from '@angular/core';
import init, { GcodeHandle } from '../../generated/scene-wasm/scene_engine';
import { AppTheme } from './app-theme';
import { Slicer } from './slicer';

// ── Role palette — single source of truth shared by the viewer and controls ──

/** Keys that identify an extrusion role in the G-code viewer. */
export type RoleName =
  | 'outerWall'
  | 'innerWall'
  | 'infill'
  | 'topSurface'
  | 'bottomSurface'
  | 'travel'
  | 'other'
  | 'bridge'
  | 'overhangPerimeter'
  | 'skirt'
  | 'support'
  | 'seam';

/** Palette mapping every role to a numeric RGB hex color. */
export type RoleColorPalette = Record<RoleName, number>;

export const ROLE_COLORS_DARK: RoleColorPalette = {
  outerWall: 0xff8800, // amber-orange  (legend: outer wall)
  innerWall: 0xffcc00, // golden-yellow (legend: inner wall)
  infill: 0xcc44ff, // violet-purple (legend: sparse infill)
  topSurface: 0xff3355, // crimson-pink  (legend: top surface)
  bottomSurface: 0x00bbff, // vivid cyan    (legend: bottom surface)
  travel: 0x334466, // dark slate    (legend: travel)
  other: 0x44ffaa, // mint-green    (twist: stands apart)
  bridge: 0x0057ff, // vivid azure   (legend: bridge)
  overhangPerimeter: 0x008a4b, // emerald       (legend: overhang perimeter)
  skirt: 0x888888, // mid-gray      (legend: skirt/brim)
  support: 0x7dff00, // neon lime     (legend: support material)
  seam: 0xffffff, // white         (legend: seam point)
};

export const ROLE_COLORS_LIGHT: RoleColorPalette = {
  outerWall: 0xdd5500, // dark-orange    (legend: outer wall)
  innerWall: 0xbb8800, // deep-amber     (legend: inner wall)
  infill: 0x9900cc, // deep-violet    (legend: sparse infill)
  topSurface: 0xcc0033, // dark-crimson   (legend: top surface)
  bottomSurface: 0x0077bb, // ocean-blue     (legend: bottom surface)
  travel: 0x445566, // dark-slate     (legend: travel)
  other: 0x008855, // forest-teal    (twist: stands apart)
  bridge: 0x0044cc, // dark-azure     (legend: bridge)
  overhangPerimeter: 0x005e30, // deep-green     (legend: overhang perimeter)
  skirt: 0x666666, // dark-gray      (legend: skirt/brim)
  support: 0x557700, // dark-lime      (legend: support material)
  seam: 0x111122, // near-black     (legend: seam point — white bg)
};

/** Returns the correct palette for the current theme. */
export function getRoleColors(isDark: boolean): RoleColorPalette {
  return isDark ? ROLE_COLORS_DARK : ROLE_COLORS_LIGHT;
}

/** @deprecated Use `ROLE_COLORS_DARK` or `getRoleColors()` instead. */
export const ROLE_COLORS = ROLE_COLORS_DARK;

export const ROLE_LABELS: Record<RoleName, string> = {
  outerWall: 'Outer Wall',
  innerWall: 'Inner Wall',
  infill: 'Infill',
  topSurface: 'Top Surface',
  bottomSurface: 'Bottom Surface',
  travel: 'Travel',
  other: 'Other',
  bridge: 'Bridge',
  overhangPerimeter: 'Overhang Perimeter',
  skirt: 'Skirt / Brim',
  support: 'Support',
  seam: 'Seam',
};

function makeRoleCss(colors: RoleColorPalette): Record<RoleName, string> {
  return Object.fromEntries(
    Object.entries(colors).map(([k, v]) => [k, `#${v.toString(16).padStart(6, '0')}`]),
  ) as Record<RoleName, string>;
}

export const ROLE_CSS_DARK = makeRoleCss(ROLE_COLORS_DARK);
export const ROLE_CSS_LIGHT = makeRoleCss(ROLE_COLORS_LIGHT);

/** @deprecated Use `ROLE_CSS_DARK` or `GcodePreview.roleCss` signal instead. */
export const ROLE_CSS = ROLE_CSS_DARK;

export const ROLE_ORDER: readonly RoleName[] = [
  'outerWall',
  'innerWall',
  'infill',
  'topSurface',
  'bottomSurface',
  'bridge',
  'overhangPerimeter',
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
  private readonly appTheme = inject(AppTheme);

  /** Parsed handle — `null` until a slice download URL is available. */
  readonly gcodeHandle = signal<GcodeHandle | null>(null);

  /** Active role color palette — switches with the current theme. */
  readonly roleColors = computed<RoleColorPalette>(() => getRoleColors(this.appTheme.isDarkMode()));

  /** Active CSS color map for legend/controls — switches with the current theme. */
  readonly roleCss = computed<Record<RoleName, string>>(() =>
    this.appTheme.isDarkMode() ? ROLE_CSS_DARK : ROLE_CSS_LIGHT,
  );

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
