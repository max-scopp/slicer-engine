export interface SliceSettings {
  layerHeight: number;
  printSpeed: number;
  nozzleTemp: number;
  bedTemp: number;
  gcodeFlavor: 'marlin' | 'klipper';
}

export const DEFAULT_SETTINGS: SliceSettings = {
  layerHeight: 0.2,
  printSpeed: 60,
  nozzleTemp: 215,
  bedTemp: 60,
  gcodeFlavor: 'marlin',
};
