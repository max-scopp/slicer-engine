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
 * reactive signals consumed by both the layer/segment control components and
 * the `Viewer` gcode rendering path.
 *
 * When `showAllLayers` is `true` (default) all layers from 0 to `layerMax` are
 * rendered, giving a cumulative "layers built so far" view.  When `false` only
 * the single layer at `layerMax` is shown.
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
   * Upper bound of the visible layer range (0-based index).
   * The single moving thumb on the vertical layer scrollbar.
   */
  readonly layerMax = signal(0);

  /**
   * When `true` (default) all layers from 0 up to `layerMax` are rendered.
   * When `false` only the single layer at `layerMax` is shown.
   */
  readonly showAllLayers = signal(true);

  /**
   * Lower bound of the visible layer range (0-based index).
   * Derived: always 0 when `showAllLayers` is true, otherwise equals `layerMax`.
   */
  readonly layerMin = computed(() => (this.showAllLayers() ? 0 : this.layerMax()));

  /**
   * Fractional scrub position within the top-most visible layer [0, 1].
   * 0 = nothing shown; 1 = full layer revealed.
   * Automatically resets to 1 when navigating to a different layer via `setLayerMax`.
   */
  readonly segmentProgress = signal(1);

  /** Set of roles to hide in the viewer. */
  readonly hiddenRoles = signal<ReadonlySet<RoleName>>(new Set<RoleName>());

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

  setLayerMax(value: number): void {
    const count = this.layerCount();
    if (count === 0) {
      return;
    }
    const clamped = Math.max(0, Math.min(value, count - 1));
    // Reset segment scrub to fully-revealed whenever the active layer changes
    // so the thumb doesn't visually jump as the new layer's segment count differs.
    if (clamped !== this.layerMax()) {
      this.segmentProgress.set(1);
    }
    this.layerMax.set(clamped);
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

  toggleShowAllLayers(): void {
    this.showAllLayers.set(!this.showAllLayers());
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
      this.layerMax.set(Math.max(0, count - 1));
      this.segmentProgress.set(1);
      this.hiddenRoles.set(new Set<RoleName>());
      this.showAllLayers.set(true);
    } catch (error) {
      console.error('[GcodePreview] Failed to load gcode:', error);
    } finally {
      this.loading.set(false);
    }
  }
}
