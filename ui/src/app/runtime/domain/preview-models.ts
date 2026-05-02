export type RuntimePreviewSource =
  | { kind: 'download-url'; url: string }
  | { kind: 'gcode-inline'; gcode: string }
  | { kind: 'none' };
