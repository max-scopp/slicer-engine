export type { WsSlicingParams as SliceSettings } from '../../generated/slicer-engine-ws-client-message-v1';

export const DEFAULT_SETTINGS: import('../../generated/slicer-engine-ws-client-message-v1').WsSlicingParams =
    {
        layer_height: 0.2,
        print_speed: 60,
        nozzle_temp: 215,
        bed_temp: 60,
        gcode_flavor: 'Marlin',
        infill_density: 20,
        infill_pattern: 'Rectilinear',
        infill_angle: 45,
    };
