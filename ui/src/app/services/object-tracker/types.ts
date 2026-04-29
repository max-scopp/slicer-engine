/** Three-component vector in machine-space mm (or any caller-defined unit). */
export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

/**
 * Euler rotation in radians using the XYZ intrinsic order — matching
 * Three.js's default `Euler('XYZ')` so a transform written here can be
 * applied to a Three.js object without conversion.
 */
export interface EulerRotation {
  x: number;
  y: number;
  z: number;
}

/** Full 3D transform: translation, rotation, and uniform-or-anisotropic scale. */
export interface Transform {
  position: Vec3;
  rotation: EulerRotation;
  scale: Vec3;
}

/** Identity transform (no translation, rotation, or scaling). */
export const IDENTITY_TRANSFORM: Transform = {
  position: { x: 0, y: 0, z: 0 },
  rotation: { x: 0, y: 0, z: 0 },
  scale: { x: 1, y: 1, z: 1 },
};

/**
 * Inputs accepted when creating a {@link SceneObject}. Every field is
 * optional and defaults to {@link IDENTITY_TRANSFORM}; only `id` is mandatory
 * (the tracker generates one if the caller does not supply it).
 */
export interface SceneObjectInit {
  id?: string;
  name?: string;
  position?: Partial<Vec3>;
  rotation?: Partial<EulerRotation>;
  scale?: Partial<Vec3>;
}
