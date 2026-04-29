/**
 * Public surface of the object-tracker service. The viewer and the
 * print-area service consume {@link SceneObject} instances created by
 * {@link ObjectTracker}; the tracker is the single source of
 * truth for what is currently placed in the 3D scene and the live
 * transform of every object.
 */
export { ObjectTracker } from './object-tracker';
export { SceneObject } from './scene-object';
export {
  IDENTITY_TRANSFORM,
  type EulerRotation,
  type SceneObjectInit,
  type Transform,
  type Vec3,
} from './types';
