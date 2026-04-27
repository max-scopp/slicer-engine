export type InfillPattern = 'rectilinear' | 'grid' | 'honeycomb' | 'gyroid';

export interface SliceSettings {
  layerHeight: number;
  printSpeed: number;
  nozzleTemp: number;
  bedTemp: number;
  gcodeFlavor: 'marlin' | 'klipper';
  infillDensity: number;
  infillPattern: InfillPattern;
  infillAngle: number;
}

export const DEFAULT_SETTINGS: SliceSettings = {
  layerHeight: 0.2,
  printSpeed: 60,
  nozzleTemp: 215,
  bedTemp: 60,
  gcodeFlavor: 'marlin',
  infillDensity: 20,
  infillPattern: 'rectilinear',
  infillAngle: 45,
};
