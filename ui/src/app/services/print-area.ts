import { Injectable, computed, signal } from '@angular/core';

/**
 * Description of the printer's build volume in machine coordinates.
 *
 * The viewer's world origin (0, 0, 0) — where the RGB axis gizmo sits — is
 * always the printer's machine origin. The build plate ("printable area") is
 * a rectangle whose dimensions are given by {@link printableAreaWidth} /
 * {@link printableAreaHeight} and whose lower-left corner is offset from the
 * machine origin by ({@link movableAreaX}, {@link movableAreaY}).
 *
 * That separation lets us model real-world printers correctly: many machines
 * can drive their toolhead to coordinates that lie outside the physical bed
 * (e.g. for purge towers, wipe positions, parking) so the bed itself does
 * not have to start at (0, 0).
 */
export interface PrintAreaConfig {
  /** Width of the bed in millimetres (along the world +X axis). */
  printableAreaWidth: number;
  /** Depth of the bed in millimetres (along the world +Y axis). */
  printableAreaHeight: number;
  /** X offset of the bed's lower-left corner from the machine origin (mm). */
  movableAreaX: number;
  /** Y offset of the bed's lower-left corner from the machine origin (mm). */
  movableAreaY: number;
}

/**
 * Identity + position of an object the user is manipulating on the bed.
 *
 * Currently a placeholder shape: object tracking and per-object transforms
 * are scheduled to land on top of this service later. Keeping the type
 * defined now makes the eventual extension API-additive only.
 */
export interface TrackedObject {
  id: string;
  /** Position in machine coordinates (mm). */
  x: number;
  y: number;
}

const DEFAULT_CONFIG: PrintAreaConfig = {
  printableAreaWidth: 220,
  printableAreaHeight: 220,
  movableAreaX: 0,
  movableAreaY: 0,
};

/**
 * Single source of truth for the build volume shown in the 3D viewer.
 *
 * Owns the print-area dimensions and the bed offset within machine space, so
 * the toolbar / settings UI and the viewer's grid stay in sync without
 * passing config through the component tree. The service is also the future
 * home for object placement on the bed (see {@link TrackedObject}).
 */
@Injectable({ providedIn: 'root' })
export class PrintAreaService {
  private readonly _config = signal<PrintAreaConfig>({ ...DEFAULT_CONFIG });

  /** Live, read-only view of the current print-area configuration. */
  readonly config = this._config.asReadonly();

  /** Convenience: bed bounds in machine coordinates (lower-left → upper-right). */
  readonly bounds = computed(() => {
    const c = this._config();
    return {
      minX: c.movableAreaX,
      minY: c.movableAreaY,
      maxX: c.movableAreaX + c.printableAreaWidth,
      maxY: c.movableAreaY + c.printableAreaHeight,
      centerX: c.movableAreaX + c.printableAreaWidth / 2,
      centerY: c.movableAreaY + c.printableAreaHeight / 2,
    };
  });

  // ---------------------------------------------------------------------------
  // Object tracking (placeholder — implementation lands later)
  // ---------------------------------------------------------------------------

  private readonly _objects = signal<readonly TrackedObject[]>([]);

  /** Read-only list of objects currently placed on the bed. */
  readonly objects = this._objects.asReadonly();

  /** Replace the entire print-area configuration. */
  setConfig(next: PrintAreaConfig): void {
    const sanitised: PrintAreaConfig = {
      printableAreaWidth: ensurePositive(next.printableAreaWidth, DEFAULT_CONFIG.printableAreaWidth),
      printableAreaHeight: ensurePositive(
        next.printableAreaHeight,
        DEFAULT_CONFIG.printableAreaHeight,
      ),
      movableAreaX: ensureFinite(next.movableAreaX, 0),
      movableAreaY: ensureFinite(next.movableAreaY, 0),
    };
    this._config.set(sanitised);
  }

  /** Patch a subset of the print-area configuration. */
  updateConfig(patch: Partial<PrintAreaConfig>): void {
    this.setConfig({ ...this._config(), ...patch });
  }

  /** Reset to the default printer configuration. */
  reset(): void {
    this._config.set({ ...DEFAULT_CONFIG });
    this._objects.set([]);
  }

  // ---------------------------------------------------------------------------
  // Object tracking — minimal CRUD so the eventual placement UI has hooks
  // ---------------------------------------------------------------------------

  /** Add or replace an object by id. */
  upsertObject(obj: TrackedObject): void {
    const list = this._objects();
    const idx = list.findIndex((o) => o.id === obj.id);
    if (idx === -1) {
      this._objects.set([...list, obj]);
    } else {
      const next = list.slice();
      next[idx] = obj;
      this._objects.set(next);
    }
  }

  /** Remove an object by id. */
  removeObject(id: string): void {
    const list = this._objects();
    const next = list.filter((o) => o.id !== id);
    if (next.length !== list.length) {
      this._objects.set(next);
    }
  }
}

function ensurePositive(value: number, fallback: number): number {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function ensureFinite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}
