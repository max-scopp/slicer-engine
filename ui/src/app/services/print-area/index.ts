/**
 * Public surface of the print-area service. Internals are split across
 * sibling files (config + orchestration, selection store, drag store,
 * shared types); consumers should only import from this barrel so the
 * internal layout stays free to change.
 *
 * Object identity / position / rotation / scale live with
 * {@link ../object-tracker `ObjectTracker`} — this service is
 * focused on the build volume, selection state, and drag gestures.
 */
export { PrintArea } from './print-area';
export {
  DEFAULT_PRINT_AREA_CONFIG,
  type DragAnchor,
  type PrintAreaBounds,
  type PrintAreaConfig,
  type SelectOptions,
} from './types';
