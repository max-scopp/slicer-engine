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
}

// -- Layer builder ------------------------------------------------------------

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

  const roleSegments: RoleSegments[] = [];
  let totalSegments = 0;

  for (const role of ROLE_ORDER) {
    const data = buf[ROLE_DATA_KEY[role]] as Float32Array;
    if (data.length === 0) {
      continue;
    }
    const count = data.length / 8;
    const color = ROLE_COLORS[role];

    if (role === 'travel') {
      const pts = new Float32Array(count * 6);
      for (let i = 0; i < count; i++) {
        const off = i * 8;
        const pOff = i * 6;
        pts[pOff] = data[off];
        pts[pOff + 1] = data[off + 1];
        pts[pOff + 2] = data[off + 2];
        pts[pOff + 3] = data[off + 3];
        pts[pOff + 4] = data[off + 4];
        pts[pOff + 5] = data[off + 5];
      }
      const geometry = new BufferGeometry();
      geometry.setAttribute('position', new BufferAttribute(pts, 3));
      const material = new LineBasicMaterial({ color });
      const lines = new LineSegments(geometry, material);
      group.add(lines);
      roleSegments.push({ role, lines, count });
    } else {
      const material = new MeshStandardMaterial({ color, roughness: 0.6 });
      const mesh = new InstancedMesh(segmentGeometry, material, count);
      const joints = new InstancedMesh(jointGeometry, material, count * 2);
      mesh.instanceMatrix.setUsage(35044 /* THREE.DynamicDrawUsage */);
      joints.instanceMatrix.setUsage(35044 /* THREE.DynamicDrawUsage */);

      for (let i = 0; i < count; i++) {
        const offset = i * 8;
        _p0.set(data[offset], data[offset + 1], data[offset + 2]);
        _p1.set(data[offset + 3], data[offset + 4], data[offset + 5]);
        const width = data[offset + 6] || 0.4;
        const height = data[offset + 7] || 0.2;

        const length = _p0.distanceTo(_p1);
        _mid.addVectors(_p0, _p1).multiplyScalar(0.5);

        _dummy.position.copy(_mid);
        _dummy.up.set(0, 0, 1);
        // LookAt sets up Z axis pointing towards _p1, Y axis towards UP, X is right
        _dummy.lookAt(_p1);

        _dummy.scale.set(width, height, length || 0.001);
        _dummy.updateMatrix();
        mesh.setMatrixAt(i, _dummy.matrix);

        _dummy.scale.set(width, height, width);

        _dummy.position.copy(_p0);
        _dummy.updateMatrix();
        joints.setMatrixAt(i * 2, _dummy.matrix);

        _dummy.position.copy(_p1);
        _dummy.updateMatrix();
        joints.setMatrixAt(i * 2 + 1, _dummy.matrix);
      }

      mesh.count = count;
      joints.count = count * 2;
      group.add(mesh);
      group.add(joints);

      roleSegments.push({ role, mesh, joints, count });
    }

    totalSegments += count;
  }

  return { group, totalSegments, roleSegments };
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
  let remaining = target;
  for (const rs of info.roleSegments) {
    const show = Math.min(remaining, rs.count);
    if (rs.mesh) rs.mesh.count = show;
    if (rs.joints) rs.joints.count = show * 2;
    if (rs.lines) rs.lines.geometry.setDrawRange(0, show * 2);
    remaining = Math.max(0, remaining - rs.count);
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
