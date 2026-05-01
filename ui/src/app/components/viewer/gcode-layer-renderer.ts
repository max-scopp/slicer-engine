import {
  BufferAttribute,
  BufferGeometry,
  CylinderGeometry,
  Group,
  InstancedMesh,
  LineBasicMaterial,
  LineSegments,
  MeshStandardMaterial,
  Object3D,
  SphereGeometry,
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
  /** Number of line segments */
  count: number;
}

export interface LayerInfo {
  index: number;
  z: number;
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
  blockLayout: { role: RoleName; count: number }[];
}

// -- Layer builder ------------------------------------------------------------

interface LayerBuild {
  group: Group;
  totalSegments: number;
  roleSegments: RoleSegments[];
  blockLayout: { role: RoleName; count: number }[];
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

  const blockLayout: { role: RoleName; count: number }[] = [];
  let totalSegments = 0;

  // Pass 1: Tally counts to allocate exactly one buffer/mesh per role
  for (let b = 0; b < numBlocks; b++) {
    const roleId = buf.blockRole(b);
    const dataLen = buf.blockData(b).length;
    if (dataLen === 0) continue;

    const count = dataLen / 8;
    const role = ROLE_ID_TO_NAME[roleId] || 'other';

    roleTotals[role] += count;
    blockLayout.push({ role, count });
    totalSegments += count;
  }

  const roleSegmentsMap: Partial<Record<RoleName, RoleSegments>> = {};

  // Pass 2: Allocate Three.js instances
  for (const role of ROLE_ORDER) {
    const count = roleTotals[role];
    if (count === 0) continue;

    const color = ROLE_COLORS[role];

    if (role === 'travel') {
      const pts = new Float32Array(count * 6);
      const geometry = new BufferGeometry();
      geometry.setAttribute('position', new BufferAttribute(pts, 3));
      const material = new LineBasicMaterial({ color });
      const lines = new LineSegments(geometry, material);
      group.add(lines);
      roleSegmentsMap[role] = { role, lines, count };
    } else {
      const material = new MeshStandardMaterial({ color, roughness: 0.6 });
      const mesh = new InstancedMesh(segmentGeometry, material, count);
      const joints = new InstancedMesh(jointGeometry, material, count * 2);
      mesh.instanceMatrix.setUsage(35044 /* THREE.DynamicDrawUsage */);
      joints.instanceMatrix.setUsage(35044 /* THREE.DynamicDrawUsage */);

      mesh.count = count;
      joints.count = count * 2;
      group.add(mesh);
      group.add(joints);

      roleSegmentsMap[role] = { role, mesh, joints, count };
    }
  }

  // Pass 3: Fill matrices and buffers sequentially per role
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

    const count = data.length / 8;
    const roleId = buf.blockRole(b);
    const role = ROLE_ID_TO_NAME[roleId] || 'other';

    const rs = roleSegmentsMap[role];
    if (!rs) continue;

    const baseOffset = roleOffsets[role];

    if (role === 'travel') {
      const pts = (rs.lines!.geometry.getAttribute('position') as BufferAttribute)
        .array as Float32Array;
      for (let i = 0; i < count; i++) {
        const off = i * 8;
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
        const offset = i * 8;

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

export function disposeLayerGroup(group: Group): void {
  for (const child of group.children) {
    if (child instanceof InstancedMesh || child instanceof LineSegments) {
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

  const visibleCounts: Record<RoleName, number> = {
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
    visibleCounts[block.role] += show;
    remaining -= show;
    if (remaining <= 0) break;
  }

  for (const rs of info.roleSegments) {
    const show = visibleCounts[rs.role] || 0;
    if (rs.mesh) rs.mesh.count = show;
    if (rs.joints) rs.joints.count = show * 2;
    if (rs.lines) rs.lines.geometry.setDrawRange(0, show * 2);
  }
}

export function applyHiddenRoles(layers: LayerInfo[], hiddenRoles: ReadonlySet<RoleName>): void {
  for (const info of layers) {
    for (const rs of info.roleSegments) {
      const visible = !hiddenRoles.has(rs.role);
      if (rs.mesh) rs.mesh.visible = visible;
      if (rs.joints) rs.joints.visible = visible;
      if (rs.lines) rs.lines.visible = visible;
    }
  }
}
