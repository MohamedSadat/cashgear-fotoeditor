import { create } from 'zustand';
export type Rgba = [number, number, number, number];
export type Curve = [number, number, number, number, number];
export function evaluateCurve(points: Curve, input: number) {
  const x = Math.max(0, Math.min(1, input)) * 4,
    i = Math.min(3, Math.floor(x));
  return points[i] + (points[i + 1] - points[i]) * (x - i);
}
export interface BrushSettings {
  size: number;
  hardness: number;
  opacity: number;
  flow: number;
  spacing: number;
  roundness: number;
  angle: number;
  pencil: boolean;
  stabilizer: 'off' | 'pulled' | 'weighted';
  strength: number;
  pressureSize: boolean;
  pressureOpacity: boolean;
  tiltSize: boolean;
  tiltOpacity: boolean;
  sizePressure: Curve;
  opacityPressure: Curve;
  sizeTilt: Curve;
  opacityTilt: Curve;
}
export const defaults: BrushSettings = {
  size: 30,
  hardness: 1,
  opacity: 1,
  flow: 1,
  spacing: 0.1,
  roundness: 1,
  angle: 0,
  pencil: false,
  stabilizer: 'off',
  strength: 0.35,
  pressureSize: true,
  pressureOpacity: false,
  tiltSize: false,
  tiltOpacity: false,
  sizePressure: [0.1, 0.25, 0.5, 0.75, 1],
  opacityPressure: [0, 0.25, 0.5, 0.75, 1],
  sizeTilt: [1, 0.8, 0.6, 0.4, 0.2],
  opacityTilt: [1, 0.8, 0.6, 0.4, 0.2],
};
export const presets: Record<string, Partial<BrushSettings>> = {
  'Hard round': {
    size: 30,
    hardness: 1,
    opacity: 1,
    flow: 1,
    spacing: 0.1,
    roundness: 1,
    angle: 0,
    pencil: false,
  },
  'Soft round': {
    size: 300,
    hardness: 0,
    opacity: 1,
    flow: 1,
    spacing: 0.1,
    roundness: 1,
    angle: 0,
    pencil: false,
  },
  Airbrush: {
    size: 150,
    hardness: 0,
    opacity: 1,
    flow: 0.08,
    spacing: 0.05,
    roundness: 1,
    angle: 0,
    pencil: false,
  },
  'Pixel pencil': {
    size: 1,
    hardness: 1,
    opacity: 1,
    flow: 1,
    spacing: 0.5,
    roundness: 1,
    angle: 0,
    pencil: true,
  },
};
interface BrushState {
  tool:
    | 'brush'
    | 'eraser'
    | 'eyedropper'
    | 'rectangle'
    | 'ellipse'
    | 'lasso'
    | 'polygon'
    | 'wand'
    | 'bucket';
  settings: BrushSettings;
  primary: Rgba;
  secondary: Rgba;
  recent: string[];
  sampledSerial: number;
  setColor: (color: Rgba) => void;
  swap: () => void;
}
export function hex(color: Rgba) {
  return (
    '#' +
    color
      .slice(0, color[3] === 255 ? 3 : 4)
      .map((c) => c.toString(16).padStart(2, '0'))
      .join('')
  );
}
export function parseHex(value: string): Rgba | undefined {
  if (!/^#?[\da-f]{6}([\da-f]{2})?$/i.test(value)) return;
  const text = value.replace('#', '');
  return [
    parseInt(text.slice(0, 2), 16),
    parseInt(text.slice(2, 4), 16),
    parseInt(text.slice(4, 6), 16),
    text.length === 8 ? parseInt(text.slice(6, 8), 16) : 255,
  ];
}
export const useBrush = create<BrushState>((set) => ({
  tool: 'brush',
  settings: defaults,
  primary: [0, 0, 0, 255],
  secondary: [255, 255, 255, 255],
  recent: [],
  sampledSerial: 0,
  setColor: (color) =>
    set((state) => ({
      primary: color,
      recent: [hex(color), ...state.recent.filter((c) => c !== hex(color))].slice(0, 10),
    })),
  swap: () => set((state) => ({ primary: state.secondary, secondary: state.primary })),
}));
export function setBrush(settings: Partial<BrushSettings>) {
  useBrush.setState((state) => ({ settings: { ...state.settings, ...settings } }));
}
