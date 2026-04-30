import {
  BufferGeometry,
  Color,
  Float32BufferAttribute,
  Group,
  LineBasicMaterial,
  LineSegments,
  type PerspectiveCamera,
  type Scene,
  type WebGLRenderer,
} from 'three';
import type { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { PrintAreaConfig } from '../../../services/print-area';
import { disposeObject } from './utils';

const GRID_MIN_SPACING_MM = 1;
const GRID_MAX_SPACING_MM = 1000;
const MAJOR_EVERY = 10;
const TARGET_MINOR_PIXELS = 14;
const MINOR_OPACITY = 0.25;
const MAJOR_OPACITY = 0.6;
const BED_OUTLINE_OPACITY = 0.9;
const GRID_SCALE_TRANSITION_MS = 350;

const GRID_FADE_HIDE = 0.05;
const GRID_FADE_FULL = 0.25;

const DEFAULT_PRINT_AREA: PrintAreaConfig = {
  printableAreaWidth: 220,
  printableAreaHeight: 220,
  movableAreaX: 0,
  movableAreaY: 0,
};

interface GridTransition {
  outgoing: Group;
  outgoingMaterials: { material: LineBasicMaterial; baseOpacity: number }[];
  startTime: number;
}

/**
 * Builds and maintains the adaptive build-plate grid. Cell spacing snaps to
 * a power of 10 in mm based on the current camera zoom so the user always
 * sees round numbers. The grid cross-fades between levels during zoom and
 * fades out when the camera grazes the XY plane.
 */
export class SceneGrid {
  private grid: Group;
  private gridMaterials: { material: LineBasicMaterial; baseOpacity: number }[] = [];
  private currentGridSpacingMm = 0;
  private gridTransition: GridTransition | null = null;
  private printArea: PrintAreaConfig = { ...DEFAULT_PRINT_AREA };
  private readonly themeObserver: MutationObserver;

  constructor(
    private readonly scene: Scene,
    private readonly camera: PerspectiveCamera,
    private readonly controls: OrbitControls,
    private readonly renderer: WebGLRenderer,
    initialPrintArea: PrintAreaConfig,
  ) {
    this.printArea = { ...initialPrintArea };
    this.grid = this.createGrid(10);
    scene.add(this.grid);
    this.currentGridSpacingMm = 0; // force updateAdaptiveGrid to rebuild on first tick

    this.themeObserver = new MutationObserver(() => this.refreshGridColor());
    this.themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'style'],
    });
  }

  setPrintArea(config: PrintAreaConfig): void {
    this.printArea = { ...config };
    const spacing = this.currentGridSpacingMm > 0 ? this.currentGridSpacingMm : 10;
    this.currentGridSpacingMm = 0;
    const replacement = this.createGrid(spacing);
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    this.scene.add(replacement);
    this.grid = replacement;
  }

  /** Snap the minor-cell spacing to the nearest power of 10 as the user zooms. */
  updateAdaptiveGrid(): void {
    const distance = this.camera.position.distanceTo(this.controls.target);
    if (!Number.isFinite(distance) || distance <= 0) {
      return;
    }
    const viewportHeight = Math.max(this.renderer.domElement.clientHeight, 1);
    const fovRad = (this.camera.fov * Math.PI) / 180;
    const worldPerPixel = (2 * Math.tan(fovRad / 2) * distance) / viewportHeight;
    const desiredSpacing = worldPerPixel * TARGET_MINOR_PIXELS;
    const snapped = snapToPowerOfTen(desiredSpacing, GRID_MIN_SPACING_MM, GRID_MAX_SPACING_MM);
    if (snapped === this.currentGridSpacingMm) {
      return;
    }
    if (this.gridTransition) {
      this.scene.remove(this.gridTransition.outgoing);
      disposeObject(this.gridTransition.outgoing);
      this.gridTransition = null;
    }
    const outgoing = this.grid;
    const outgoingMaterials = this.gridMaterials;
    const incoming = this.createGrid(snapped);
    this.scene.add(incoming);
    this.grid = incoming;
    this.gridTransition = { outgoing, outgoingMaterials, startTime: performance.now() };
  }

  /** Fade the grid based on how oblique the camera angle is to the XY plane. */
  updateGridFade(): void {
    if (this.gridMaterials.length === 0) {
      return;
    }
    const viewDir = this.camera.position.clone().sub(this.controls.target);
    const len = viewDir.length();
    if (len < 1e-6) {
      return;
    }
    viewDir.divideScalar(len);
    const cosAngle = Math.abs(viewDir.z);
    const t = clamp01((cosAngle - GRID_FADE_HIDE) / (GRID_FADE_FULL - GRID_FADE_HIDE));
    const fade = t * t * (3 - 2 * t);

    let crossFadeIn = 1.0;
    if (this.gridTransition) {
      const elapsed = performance.now() - this.gridTransition.startTime;
      const progress = Math.min(elapsed / GRID_SCALE_TRANSITION_MS, 1.0);
      const smooth = progress * progress * (3 - 2 * progress);
      crossFadeIn = smooth;
      const crossFadeOut = 1.0 - smooth;
      for (const entry of this.gridTransition.outgoingMaterials) {
        const opacity = entry.baseOpacity * fade * crossFadeOut;
        entry.material.opacity = opacity;
        entry.material.visible = opacity > 0.001;
      }
      if (progress >= 1.0) {
        this.scene.remove(this.gridTransition.outgoing);
        disposeObject(this.gridTransition.outgoing);
        this.gridTransition = null;
      }
    }

    for (const entry of this.gridMaterials) {
      const opacity = entry.baseOpacity * fade * crossFadeIn;
      if (entry.material.opacity !== opacity) {
        entry.material.opacity = opacity;
        entry.material.visible = opacity > 0.001;
      }
    }
  }

  dispose(): void {
    this.themeObserver.disconnect();
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    if (this.gridTransition) {
      this.scene.remove(this.gridTransition.outgoing);
      disposeObject(this.gridTransition.outgoing);
      this.gridTransition = null;
    }
  }

  private createGrid(spacingMm: number): Group {
    const color = readBorderColor();
    const group = new Group();
    group.renderOrder = 0;

    const materials: { material: LineBasicMaterial; baseOpacity: number }[] = [];
    const { movableAreaX, movableAreaY, printableAreaWidth, printableAreaHeight } = this.printArea;

    const { minorPositions, majorPositions } = buildBedGridPositions(
      printableAreaWidth,
      printableAreaHeight,
      spacingMm,
      MAJOR_EVERY,
    );
    const offset = { x: movableAreaX, y: movableAreaY };

    const minor = makeLineSegments(minorPositions, offset, color, MINOR_OPACITY, materials);
    const major = makeLineSegments(majorPositions, offset, color, MAJOR_OPACITY, materials);
    major.renderOrder = 1;

    const outlinePositions = buildBedOutlinePositions(printableAreaWidth, printableAreaHeight);
    const outline = makeLineSegments(
      outlinePositions,
      offset,
      color,
      BED_OUTLINE_OPACITY,
      materials,
    );
    outline.renderOrder = 2;

    group.add(minor, major, outline);
    this.currentGridSpacingMm = spacingMm;
    this.gridMaterials = materials;
    return group;
  }

  /** Rebuild the grid using the current spacing so CSS theme changes are applied. */
  private refreshGridColor(): void {
    const replacement = this.createGrid(this.currentGridSpacingMm || 10);
    this.scene.remove(this.grid);
    disposeObject(this.grid);
    this.scene.add(replacement);
    this.grid = replacement;
  }
}

// -----------------------------------------------------------------------------
// Grid geometry helpers
// -----------------------------------------------------------------------------

function buildBedGridPositions(
  width: number,
  height: number,
  spacingMm: number,
  majorEvery: number,
): { minorPositions: Float32Array; majorPositions: Float32Array } {
  const minor: number[] = [];
  const major: number[] = [];
  if (!(width > 0) || !(height > 0) || !(spacingMm > 0)) {
    return { minorPositions: new Float32Array(0), majorPositions: new Float32Array(0) };
  }
  const xCount = Math.floor(width / spacingMm);
  for (let i = 0; i <= xCount; i++) {
    const x = i * spacingMm;
    if (x > width) {
      break;
    }
    const target = i % majorEvery === 0 ? major : minor;
    target.push(x, 0, 0, x, height, 0);
  }
  const yCount = Math.floor(height / spacingMm);
  for (let j = 0; j <= yCount; j++) {
    const y = j * spacingMm;
    if (y > height) {
      break;
    }
    const target = j % majorEvery === 0 ? major : minor;
    target.push(0, y, 0, width, y, 0);
  }
  return {
    minorPositions: new Float32Array(minor),
    majorPositions: new Float32Array(major),
  };
}

function buildBedOutlinePositions(width: number, height: number): Float32Array {
  if (!(width > 0) || !(height > 0)) {
    return new Float32Array(0);
  }
  // prettier-ignore
  return new Float32Array([
        0, 0, 0,         width, 0, 0,
        width, 0, 0,     width, height, 0,
        width, height, 0, 0, height, 0,
        0, height, 0,    0, 0, 0,
    ]);
}

function makeLineSegments(
  positions: Float32Array,
  offset: { x: number; y: number },
  color: number,
  opacity: number,
  sink: { material: LineBasicMaterial; baseOpacity: number }[],
): LineSegments {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new Float32BufferAttribute(positions, 3));
  const material = new LineBasicMaterial({
    color: new Color(color),
    transparent: true,
    opacity,
    depthWrite: false,
  });
  sink.push({ material, baseOpacity: opacity });
  const segments = new LineSegments(geometry, material);
  segments.position.set(offset.x, offset.y, 0);
  return segments;
}

function snapToPowerOfTen(value: number, min: number, max: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return min;
  }
  const exponent = Math.round(Math.log10(value));
  const snapped = Math.pow(10, exponent);
  if (snapped < min) {
    return min;
  }
  if (snapped > max) {
    return max;
  }
  return snapped;
}

function clamp01(v: number): number {
  if (v < 0) {
    return 0;
  }
  if (v > 1) {
    return 1;
  }
  return v;
}

function readBorderColor(): number {
  const raw = getComputedStyle(document.documentElement).getPropertyValue('--color-border').trim();
  return parseCssColor(raw) ?? 0x444444;
}

function parseCssColor(value: string): number | null {
  if (!value) {
    return null;
  }
  if (value.startsWith('#')) {
    const hex = value.slice(1);
    if (hex.length === 3) {
      const [r, g, b] = hex;
      return parseInt(`${r}${r}${g}${g}${b}${b}`, 16);
    }
    if (hex.length === 6 || hex.length === 8) {
      return parseInt(hex.slice(0, 6), 16);
    }
    return null;
  }
  const match = value.match(/rgba?\(([^)]+)\)/i);
  if (match) {
    const parts = match[1].split(',').map((p) => parseFloat(p.trim()));
    if (parts.length >= 3 && parts.every((p) => !Number.isNaN(p))) {
      const [r, g, b] = parts;
      return ((r & 0xff) << 16) | ((g & 0xff) << 8) | (b & 0xff);
    }
  }
  return null;
}
