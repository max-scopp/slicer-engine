import { BufferAttribute, BufferGeometry, Group, LineBasicMaterial, LineSegments } from 'three';
import type { GcodeLayerBuffer } from '../../../generated/scene-wasm/scene_engine';
import { ROLE_COLORS, ROLE_ORDER, type RoleName } from '../../services/gcode-preview.service';

// ── Shared types ─────────────────────────────────────────────────────────────

export interface RoleSegments {
  role: RoleName;
  lines: LineSegments;
  /** Number of line segments (positions.length / 6). */
  count: number;
}

export interface LayerInfo {
  index: number;
  z: number;
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
}

// ── Layer builder ─────────────────────────────────────────────────────────────

interface LayerBuild {
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
}

const ROLE_DATA_KEY: Record<RoleName, keyof GcodeLayerBuffer> = {
  outerWall: 'outer_wall',
  innerWall: 'inner_wall',
  infill: 'infill',
  topSurface: 'top_surface',
  bottomSurface: 'bottom_surface',
  travel: 'travel',
  other: 'other',
};

/**
 * Build a `THREE.Group` containing one `LineSegments` per non-empty role
 * buffer from the given layer, in the canonical display order.
 *
 * Display order determines how segment-progress scrubbing fills the layer:
 * outer walls are revealed first, then inner walls, infill, surfaces, travel.
 */
export function buildLayerGroup(buf: GcodeLayerBuffer): LayerBuild {
  const group = new Group();
  group.userData['handle'] = buf;

  const roleSegments: RoleSegments[] = [];
  let totalSegments = 0;

  for (const role of ROLE_ORDER) {
    const data = buf[ROLE_DATA_KEY[role]] as Float32Array;
    if (data.length === 0) {
      continue;
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(data, 3));
    const material = new LineBasicMaterial({ color: ROLE_COLORS[role] });
    const lines = new LineSegments(geometry, material);
    group.add(lines);
    const count = data.length / 6;
    roleSegments.push({ role, lines, count });
    totalSegments += count;
  }

  return { group, totalSegments, roleSegments };
}

/** Dispose all geometries and materials inside a layer group. */
export function disposeLayerGroup(group: Group): void {
  for (const child of group.children) {
    if (child instanceof LineSegments) {
      child.geometry.dispose();
      if (Array.isArray(child.material)) {
        for (const m of child.material) {
          m.dispose();
        }
      } else {
        child.material.dispose();
      }
    }
  }
}

// ── Visibility helpers ────────────────────────────────────────────────────────

/**
 * Show only layers whose index falls within `[min, max]`.
 * Also restores the draw range of the previously-top layer to `Infinity`
 * before setting the new top layer as the scrubbing target.
 */
export function showLayerRange(
  layers: LayerInfo[],
  min: number,
  max: number,
  prevMax: number,
): void {
  const prevInfo = layers[prevMax];
  if (prevInfo && prevMax !== max) {
    for (const { lines } of prevInfo.roleSegments) {
      lines.geometry.setDrawRange(0, Infinity);
    }
  }

  for (const info of layers) {
    info.group.visible = info.index >= min && info.index <= max;
  }
}

/**
 * Scrub through the segments of the top-most visible layer.
 *
 * `progress` is a fraction [0, 1] of the layer's total segment count.
 * Roles are revealed in display order (outer wall first) so the fill
 * matches the original print sequence.
 */
export function applySegmentProgress(
  layers: LayerInfo[],
  topIndex: number,
  progress: number,
): void {
  const info = layers[topIndex];
  if (!info) {
    return;
  }
  const target = Math.round(progress * info.totalSegments);
  let remaining = target;
  for (const { lines, count } of info.roleSegments) {
    const show = Math.min(remaining, count);
    // `drawRange.count` is in vertices: 2 per segment.
    lines.geometry.setDrawRange(0, show * 2);
    remaining = Math.max(0, remaining - count);
  }
}

/**
 * Apply role visibility across all layers.
 */
export function applyHiddenRoles(layers: LayerInfo[], hiddenRoles: ReadonlySet<RoleName>): void {
  for (const info of layers) {
    for (const rs of info.roleSegments) {
      rs.lines.visible = !hiddenRoles.has(rs.role);
    }
  }
}
