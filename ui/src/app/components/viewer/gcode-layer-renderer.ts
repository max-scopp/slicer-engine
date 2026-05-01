import {
  BufferAttribute,
  BufferGeometry,
  CatmullRomCurve3,
  CylinderGeometry,
  Group,
  InstancedMesh,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshStandardMaterial,
  Object3D,
  SphereGeometry,
  TubeGeometry,
  Vector3,
} from 'three';
import type { GcodeLayerBuffer } from '../../../generated/scene-wasm/scene_engine';
import { ROLE_COLORS, ROLE_ORDER, type RoleName } from '../../services/gcode-preview.service';

// -- Shared types -------------------------------------------------------------

export interface RoleSegments {
  role: RoleName;
  mesh?: InstancedMesh;
  joints?: InstancedMesh;
  lines?: LineSegments;
  /** Tube meshes built from G2/G3 arc blocks, in timeline order for this role. */
  arcMeshes?: Mesh[];
  /** Number of line segments */
  count: number;
  /** Number of arc segments */
  arcCount?: number;
}

export interface LayerInfo {
  index: number;
  z: number;
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
  blockLayout: { role: RoleName; count: number; kind: 'line' | 'arc' }[];
}

// -- Layer builder ------------------------------------------------------------

interface LayerBuild {
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
  blockLayout: { role: RoleName; count: number; kind: 'line' | 'arc' }[];
}

const ROLE_ID_TO_NAME: Record<number, RoleName> = {
  0: 'outerWall',
  1: 'innerWall',
  2: 'infill',
  3: 'topSurface',
  4: 'bottomSurface',
  5: 'travel',
  6: 'other',
};

const _dummy = new Object3D();
const _p0 = new Vector3();
const _p1 = new Vector3();
const _mid = new Vector3();

// Reusable geometries. We will scale instances.
const segmentGeometry = new CylinderGeometry(0.5, 0.5, 1, 8, 1, false);
segmentGeometry.rotateX(Math.PI / 2); // Align along Z
const jointGeometry = new SphereGeometry(0.5, 8, 8);

export function buildLayerGroup(buf: GcodeLayerBuffer): LayerBuild {
  const group = new Group();
  group.userData['handle'] = buf;

  const numBlocks = buf.blocksCount();
  const roleTotals: Record<RoleName, number> = {
    outerWall: 0,
    innerWall: 0,
    infill: 0,
    topSurface: 0,
    bottomSurface: 0,
    travel: 0,
    other: 0,
  };

  const blockLayout: { role: RoleName; count: number; kind: 'line' | 'arc' }[] = [];
  let totalSegments = 0;

  // Pass 1: Tally LINE segment counts so we can pre-allocate instanced meshes.
  // Arc blocks are not pre-allocated — each arc segment is built lazily as its
  // own Mesh below.
  const blockKinds: number[] = [];
  const roleArcTotals: Record<RoleName, number> = {
    outerWall: 0,
    innerWall: 0,
    infill: 0,
    topSurface: 0,
    bottomSurface: 0,
    travel: 0,
    other: 0,
  };
  for (let b = 0; b < numBlocks; b++) {
    const roleId = buf.blockRole(b);
    const kind = (buf as unknown as { blockKind?: (i: number) => number }).blockKind?.(b) ?? 0;
    blockKinds.push(kind);
    const dataLen = buf.blockData(b).length;
    if (dataLen === 0) continue;

    const stride = kind === 1 ? 11 : 8;
    const count = Math.floor(dataLen / stride);
    const role = ROLE_ID_TO_NAME[roleId] || 'other';

    blockLayout.push({ role, count, kind: kind === 1 ? 'arc' : 'line' });
    totalSegments += count;
    if (kind === 0) {
      roleTotals[role] += count;
    } else {
      roleArcTotals[role] += count;
    }
  }

  const roleSegmentsMap: Partial<Record<RoleName, RoleSegments>> = {};

  // Pass 2: Allocate Three.js instances for the line counts.
  for (const role of ROLE_ORDER) {
    const count = roleTotals[role];
    const arcCount = roleArcTotals[role];
    const color = ROLE_COLORS[role];

    if (role === 'travel') {
      if (count === 0 && arcCount === 0) {
        roleSegmentsMap[role] = { role, count: 0, arcCount: 0, arcMeshes: [] };
        continue;
      }
      let lines: LineSegments | undefined;
      if (count > 0) {
        const pts = new Float32Array(count * 6);
        const geometry = new BufferGeometry();
        geometry.setAttribute('position', new BufferAttribute(pts, 3));
        const material = new LineBasicMaterial({ color });
        lines = new LineSegments(geometry, material);
        group.add(lines);
      }
      roleSegmentsMap[role] = { role, lines, count, arcCount, arcMeshes: [] };
    } else {
      if (count === 0 && arcCount === 0) {
        roleSegmentsMap[role] = { role, count: 0, arcCount: 0, arcMeshes: [] };
        continue;
      }
      let mesh: InstancedMesh | undefined;
      let joints: InstancedMesh | undefined;
      if (count > 0) {
        const material = new MeshStandardMaterial({ color, roughness: 0.6 });
        mesh = new InstancedMesh(segmentGeometry, material, count);
        joints = new InstancedMesh(jointGeometry, material, count * 2);
        mesh.instanceMatrix.setUsage(35044 /* THREE.DynamicDrawUsage */);
        joints.instanceMatrix.setUsage(35044 /* THREE.DynamicDrawUsage */);

        mesh.count = count;
        joints.count = count * 2;
        group.add(mesh);
        group.add(joints);
      }

      roleSegmentsMap[role] = { role, mesh, joints, count, arcCount, arcMeshes: [] };
    }
  }

  // Pass 3: Fill line buffers per role and build arc tubes per arc segment.
  const roleOffsets: Record<RoleName, number> = {
    outerWall: 0,
    innerWall: 0,
    infill: 0,
    topSurface: 0,
    bottomSurface: 0,
    travel: 0,
    other: 0,
  };

  for (let b = 0; b < numBlocks; b++) {
    const data = buf.blockData(b);
    if (data.length === 0) continue;

    const roleId = buf.blockRole(b);
    const role = ROLE_ID_TO_NAME[roleId] || 'other';
    const rs = roleSegmentsMap[role];
    if (!rs) continue;

    const kind = blockKinds[b];

    if (kind === 1) {
      // Arc block: 11 floats per segment.
      const stride = 11;
      const count = Math.floor(data.length / stride);
      for (let i = 0; i < count; i++) {
        const off = i * stride;
        const arcMesh = buildArcTube(data, off, role, ROLE_COLORS[role]);
        if (arcMesh) {
          group.add(arcMesh);
          rs.arcMeshes!.push(arcMesh);
        }
      }
      continue;
    }

    // Line block: 8 floats per segment.
    const stride = 8;
    const count = Math.floor(data.length / stride);
    const baseOffset = roleOffsets[role];

    if (role === 'travel') {
      const pts = (rs.lines!.geometry.getAttribute('position') as BufferAttribute)
        .array as Float32Array;
      for (let i = 0; i < count; i++) {
        const off = i * stride;
        const pOff = (baseOffset + i) * 6;
        pts[pOff] = data[off];
        pts[pOff + 1] = data[off + 1];
        pts[pOff + 2] = data[off + 2];
        pts[pOff + 3] = data[off + 3];
        pts[pOff + 4] = data[off + 4];
        pts[pOff + 5] = data[off + 5];
      }
      rs.lines!.geometry.attributes['position'].needsUpdate = true;
    } else {
      const mesh = rs.mesh!;
      const joints = rs.joints!;

      for (let i = 0; i < count; i++) {
        const globalI = baseOffset + i;
        const offset = i * stride;

        _p0.set(data[offset], data[offset + 1], data[offset + 2]);
        _p1.set(data[offset + 3], data[offset + 4], data[offset + 5]);
        const width = data[offset + 6] || 0.4;
        const height = data[offset + 7] || 0.2;

        const length = _p0.distanceTo(_p1);
        _mid.addVectors(_p0, _p1).multiplyScalar(0.5);

        _dummy.position.copy(_mid);
        _dummy.up.set(0, 0, 1);
        _dummy.lookAt(_p1);

        _dummy.scale.set(width, height, length || 0.001);
        _dummy.updateMatrix();
        mesh.setMatrixAt(globalI, _dummy.matrix);

        _dummy.scale.set(width, height, width);

        _dummy.position.copy(_p0);
        _dummy.updateMatrix();
        joints.setMatrixAt(globalI * 2, _dummy.matrix);

        _dummy.position.copy(_p1);
        _dummy.updateMatrix();
        joints.setMatrixAt(globalI * 2 + 1, _dummy.matrix);
      }
      mesh.instanceMatrix.needsUpdate = true;
      joints.instanceMatrix.needsUpdate = true;
    }

    roleOffsets[role] += count;
  }

  const roleSegments: RoleSegments[] = Object.values(roleSegmentsMap) as RoleSegments[];

  return { group, totalSegments, roleSegments, blockLayout };
}

/**
 * Build a Three.js mesh that represents a single G2/G3 arc segment as a
 * tube swept along the circular path on the layer's Z plane.
 */
function buildArcTube(
  data: Float32Array,
  off: number,
  role: RoleName,
  color: number | string,
): Mesh | null {
  const x0 = data[off];
  const y0 = data[off + 1];
  const z0 = data[off + 2];
  const x1 = data[off + 3];
  const y1 = data[off + 4];
  const z1 = data[off + 5];
  const cx = data[off + 6];
  const cy = data[off + 7];
  let isCw = data[off + 8] >= 0.5;
  const width = data[off + 9] || 0.4;
  const height = data[off + 10] || 0.2;

  const dx0 = x0 - cx;
  const dy0 = y0 - cy;
  const dx1 = x1 - cx;
  const dy1 = y1 - cy;
  const r = Math.hypot(dx0, dy0);
  if (!isFinite(r) || r < 1e-5) return null;

  const a0 = Math.atan2(dy0, dx0);
  const a1 = Math.atan2(dy1, dx1);
  const TWO_PI = Math.PI * 2;
  let sweep: number;
  if (isCw) {
    sweep = a0 - a1;
    while (sweep <= 0) sweep += TWO_PI;
  } else {
    sweep = a1 - a0;
    while (sweep <= 0) sweep += TWO_PI;
  }
  // Defensive clamp.  The slicer caps emitted arcs at 100° (5π/9) — anything
  // larger here means our reading of CW/CCW disagrees with the firmware
  // intent (numerical noise on near-collinear endpoints, an upstream bug, or
  // a hand-edited G-code).  Flip to the short arc rather than draw a sweeping
  // diagonal across the print bed, which is what produced the "weird circles"
  // criss-crossing the model in the viewer.
  const SLICER_MAX_SWEEP = (Math.PI * 5) / 9;
  if (sweep > SLICER_MAX_SWEEP) {
    sweep = TWO_PI - sweep;
    isCw = !isCw;
  }
  // After the flip the sweep should be small; if it still exceeds the slicer
  // cap, the data is malformed (e.g. radius mismatch) — skip rather than
  // render junk.
  if (sweep > SLICER_MAX_SWEEP || sweep < 1e-4) return null;

  // Tessellate the arc with enough samples to stay smooth; ~10° per step.
  // Build the curve in *layer-local* space (z = 0) so we can scale the Z axis
  // to squash the cross-section without dragging the curve toward the bed.
  // The layer's true Z is applied via `mesh.position.z` afterwards.
  const steps = Math.max(8, Math.ceil((sweep * 180) / Math.PI / 10));
  const dz = z1 - z0;
  const points: Vector3[] = [];
  for (let s = 0; s <= steps; s++) {
    const t = s / steps;
    const angle = isCw ? a0 - sweep * t : a0 + sweep * t;
    const px = cx + r * Math.cos(angle);
    const py = cy + r * Math.sin(angle);
    points.push(new Vector3(px, py, dz * t));
  }

  // Use a Catmull-Rom curve with `centripetal` parametrisation for stable
  // sweeps without overshoot at endpoints.
  const curve = new CatmullRomCurve3(points, false, 'centripetal', 0.5);
  const tubularSegments = Math.max(steps * 2, 24);
  const radialSegments = 12;
  // Tube radius matches the straight-segment cylinder XY thickness; the Z
  // squash to layer-height is applied via mesh.scale below (safe now because
  // the curve sits at local z = 0, so scaling Z only affects the cross-section
  // and any in-layer ramp, not the layer's elevation).
  const tubeRadius = width * 0.5;
  const geometry = new TubeGeometry(curve, tubularSegments, tubeRadius, radialSegments, false);

  const material = new MeshStandardMaterial({ color, roughness: 0.6 });
  const mesh = new Mesh(geometry, material);
  mesh.position.z = z0;
  mesh.scale.set(1, 1, height / Math.max(width, 1e-4));
  mesh.userData['role'] = role;
  return mesh;
}

export function disposeLayerGroup(group: Group): void {
  for (const child of group.children) {
    if (child instanceof InstancedMesh || child instanceof LineSegments || child instanceof Mesh) {
      child.geometry.dispose();
      if (Array.isArray(child.material)) {
        for (const m of child.material) m.dispose();
      } else {
        child.material.dispose();
      }
    }
  }
}

export function showLayerRange(
  layers: LayerInfo[],
  min: number,
  max: number,
  prevMax: number,
): void {
  const prevInfo = layers[prevMax];
  if (prevInfo && prevMax !== max) {
    for (const rs of prevInfo.roleSegments) {
      if (rs.mesh) rs.mesh.count = rs.count;
      if (rs.joints) rs.joints.count = rs.count * 2;
      if (rs.lines) rs.lines.geometry.setDrawRange(0, Infinity);
      if (rs.arcMeshes) {
        for (const m of rs.arcMeshes) m.visible = true;
      }
    }
  }

  for (const info of layers) {
    info.group.visible = info.index >= min && info.index <= max;
  }
}

export function applySegmentProgress(
  layers: LayerInfo[],
  topIndex: number,
  progress: number,
): void {
  const info = layers[topIndex];
  if (!info) return;

  const target = Math.round(progress * info.totalSegments);

  const visibleLineCounts: Record<RoleName, number> = {
    outerWall: 0,
    innerWall: 0,
    infill: 0,
    topSurface: 0,
    bottomSurface: 0,
    travel: 0,
    other: 0,
  };
  const visibleArcCounts: Record<RoleName, number> = {
    outerWall: 0,
    innerWall: 0,
    infill: 0,
    topSurface: 0,
    bottomSurface: 0,
    travel: 0,
    other: 0,
  };

  let remaining = target;
  for (const block of info.blockLayout) {
    const show = Math.min(remaining, block.count);
    if (block.kind === 'arc') {
      visibleArcCounts[block.role] += show;
    } else {
      visibleLineCounts[block.role] += show;
    }
    remaining -= show;
    if (remaining <= 0) break;
  }

  for (const rs of info.roleSegments) {
    const showLines = visibleLineCounts[rs.role] || 0;
    if (rs.mesh) rs.mesh.count = showLines;
    if (rs.joints) rs.joints.count = showLines * 2;
    if (rs.lines) rs.lines.geometry.setDrawRange(0, showLines * 2);
    if (rs.arcMeshes) {
      const showArcs = visibleArcCounts[rs.role] || 0;
      for (let i = 0; i < rs.arcMeshes.length; i++) {
        rs.arcMeshes[i].visible = i < showArcs;
      }
    }
  }
}

export function applyHiddenRoles(layers: LayerInfo[], hiddenRoles: ReadonlySet<RoleName>): void {
  for (const info of layers) {
    for (const rs of info.roleSegments) {
      const visible = !hiddenRoles.has(rs.role);
      if (rs.mesh) rs.mesh.visible = visible;
      if (rs.joints) rs.joints.visible = visible;
      if (rs.lines) rs.lines.visible = visible;
      if (rs.arcMeshes) {
        for (const m of rs.arcMeshes) m.visible = visible;
      }
    }
  }
}
