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

const GRID_FADE_HIDE = 0.05;
const GRID_FADE_FULL = 0.25;

const DEFAULT_PRINT_AREA: PrintAreaConfig = {
  printableAreaWidth: 220,
  printableAreaHeight: 220,
  movableAreaX: 0,
  movableAreaY: 0,
};

export class SceneGrid {
  private grid: Group;
  private levelMaterials: Map<number, { minor: LineBasicMaterial[]; major: LineBasicMaterial[] }> = new Map();
  private outlineMaterials: LineBasicMaterial[] = [];
  private currentGridSpacingMm = 0;
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
    this.grid = new Group();
    scene.add(this.grid);

    this.themeObserver = new MutationObserver(() => this.refreshGridColor());
    this.themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'style'],
    });

    this.refreshGridColor();
  }

  setPrintArea(config: PrintAreaConfig): void {
    this.printArea = { ...config };
    this.refreshGridColor();
  }

  updateAdaptiveGrid(): void {
    // Moved entirely to updateGridFade for continuous distance-based blending
  }

  updateGridFade(): void {
    const distance = this.camera.position.distanceTo(this.controls.target);
    if (!Number.isFinite(distance) || distance <= 0) return;

    const viewportHeight = Math.max(this.renderer.domElement.clientHeight, 1);
    const fovRad = (this.camera.fov * Math.PI) / 180;
    const worldPerPixel = (2 * Math.tan(fovRad / 2) * distance) / viewportHeight;
    const desiredSpacing = worldPerPixel * TARGET_MINOR_PIXELS;

    const viewDir = this.camera.position.clone().sub(this.controls.target);
    const len = viewDir.length();
    if (len < 1e-6) return;
    viewDir.divideScalar(len);
    const cosAngle = Math.abs(viewDir.z);
    const t = clamp01((cosAngle - GRID_FADE_HIDE) / (GRID_FADE_FULL - GRID_FADE_HIDE));
    const globalFade = t * t * (3 - 2 * t);

    const levelRaw = Math.log10(Math.max(desiredSpacing, 0.1));
    const power = Math.floor(levelRaw);
    const progress = clamp01(levelRaw - power);

    // Apply opacities based on continuous zoom progress
    for (const [spacing, mats] of this.levelMaterials.entries()) {
      const spacingPower = Math.log10(spacing);
      
      let levelFade = 0;
      if (spacingPower === power) {
        levelFade = 1.0 - progress; // currently active, fading out
      } else if (spacingPower === power + 1) {
        levelFade = progress; // next level, fading in
      } else if (spacingPower > power + 1) {
        levelFade = 0.0;
      } else if (spacingPower < power) {
        levelFade = 0.0;
      }

      for (const m of mats.minor) {
        m.opacity = MINOR_OPACITY * globalFade * levelFade;
        m.visible = m.opacity > 0.001;
      }
      for (const m of mats.major) {
        m.opacity = MAJOR_OPACITY * globalFade * levelFade;
        m.visible = m.opacity > 0.001;
      }
    }

    for (const m of this.outlineMaterials) {
      m.opacity = BED_OUTLINE_OPACITY * globalFade;
      m.visible = m.opacity > 0.001;
    }
  }

  dispose(): void {
    this.themeObserver.disconnect();
    this.scene.remove(this.grid);
    disposeObject(this.grid);
  }

  private refreshGridColor(): void {
    this.scene.remove(this.grid);
    disposeObject(this.grid);

    this.grid = new Group();
    this.grid.renderOrder = 0;
    this.levelMaterials.clear();
    this.outlineMaterials = [];

    const { movableAreaX, movableAreaY, printableAreaWidth, printableAreaHeight } = this.printArea;
    const offset = { x: movableAreaX, y: movableAreaY };

    // Build levels 1, 10, 100, 1000
    for (let p = 0; p <= 3; p++) {
      const spacingMm = Math.pow(10, p);
      const minorMats: LineBasicMaterial[] = [];
      const majorMats: LineBasicMaterial[] = [];

      const { minorPositions, majorPositions } = buildBedGridPositions(
        printableAreaWidth,
        printableAreaHeight,
        spacingMm,
        MAJOR_EVERY,
      );

      if (minorPositions.length > 0) {
        const minor = makeLineSegments(minorPositions, offset, readBorderColor(), MINOR_OPACITY, minorMats);
        this.grid.add(minor);
      }
      if (majorPositions.length > 0) {
        const major = makeLineSegments(majorPositions, offset, readBorderColor(), MAJOR_OPACITY, majorMats);
        major.renderOrder = 1;
        this.grid.add(major);
      }

      this.levelMaterials.set(spacingMm, { minor: minorMats, major: majorMats });
    }

    const outlinePositions = buildBedOutlinePositions(printableAreaWidth, printableAreaHeight);
    const outline = makeLineSegments(outlinePositions, offset, readBorderColor(), BED_OUTLINE_OPACITY, this.outlineMaterials);
    outline.renderOrder = 2;
    this.grid.add(outline);

    this.scene.add(this.grid);
  }
}

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
    if (x > width) break;
    const target = i % majorEvery === 0 ? major : minor;
    target.push(x, 0, 0, x, height, 0);
  }
  const yCount = Math.floor(height / spacingMm);
  for (let j = 0; j <= yCount; j++) {
    const y = j * spacingMm;
    if (y > height) break;
    const target = j % majorEvery === 0 ? major : minor;
    target.push(0, y, 0, width, y, 0);
  }
  return {
    minorPositions: new Float32Array(minor),
    majorPositions: new Float32Array(major),
  };
}

function buildBedOutlinePositions(width: number, height: number): Float32Array {
  if (!(width > 0) || !(height > 0)) return new Float32Array(0);
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
  sink: LineBasicMaterial[],
): LineSegments {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new Float32BufferAttribute(positions, 3));
  const material = new LineBasicMaterial({
    color: new Color(color),
    transparent: true,
    opacity,
    depthWrite: false,
  });
  sink.push(material);
  const segments = new LineSegments(geometry, material);
  segments.position.set(offset.x, offset.y, 0);
  return segments;
}

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

function readBorderColor(): number {
  if (typeof document === 'undefined') return 0x333333;
  const raw = getComputedStyle(document.documentElement).getPropertyValue('--border-base').trim();
  if (raw.startsWith('#')) return parseInt(raw.substring(1), 16);
  return 0x333333;
}
