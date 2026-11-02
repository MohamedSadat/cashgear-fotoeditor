import { create } from 'zustand';
import type { GradientSettings } from './engine/types';
export const gradientKey = 'cashgear-gradient-presets-v1';
export interface GradientPreset {
  name: string;
  settings: GradientSettings;
}
export const initialGradient: GradientSettings = {
  shape: 'linear',
  opacity: 1,
  reverse: false,
  stops: [
    { position: 0, color: [0, 0, 0, 255] },
    { position: 1, color: [255, 255, 255, 255] },
  ],
};
export const builtinGradients: GradientPreset[] = [
  { name: 'Black to white', settings: initialGradient },
  {
    name: 'Sunset',
    settings: {
      ...initialGradient,
      stops: [
        { position: 0, color: [42, 25, 91, 255] },
        { position: 0.5, color: [222, 67, 105, 255] },
        { position: 1, color: [255, 211, 136, 255] },
      ],
    },
  },
  {
    name: 'Fade to transparent',
    settings: {
      ...initialGradient,
      stops: [
        { position: 0, color: [0, 0, 0, 255] },
        { position: 1, color: [0, 0, 0, 0] },
      ],
    },
  },
];
export function validGradient(value: unknown): value is GradientSettings {
  if (!value || typeof value !== 'object') return false;
  const g = value as GradientSettings;
  return (
    ['linear', 'radial', 'angle', 'reflected', 'diamond'].includes(g.shape) &&
    typeof g.reverse === 'boolean' &&
    Number.isFinite(g.opacity) &&
    g.opacity >= 0 &&
    g.opacity <= 1 &&
    Array.isArray(g.stops) &&
    g.stops.length >= 2 &&
    g.stops.length <= 32 &&
    g.stops.every(
      (s, i) =>
        s &&
        Number.isFinite(s.position) &&
        s.position >= 0 &&
        s.position <= 1 &&
        (i === 0 || s.position > g.stops[i - 1].position) &&
        Array.isArray(s.color) &&
        s.color.length === 4 &&
        s.color.every((c) => Number.isInteger(c) && c >= 0 && c <= 255),
    ) &&
    g.stops[0].position === 0 &&
    g.stops.at(-1)!.position === 1
  );
}
export function readGradientPresets(text: string | null): GradientPreset[] {
  if (!text || text.length > 65536 || new TextEncoder().encode(text).byteLength > 65536) return [];
  try {
    const p: unknown = JSON.parse(text);
    return Array.isArray(p) &&
      p.length <= 32 &&
      p.every(
        (v) =>
          v &&
          typeof v.name === 'string' &&
          v.name.trim().length > 0 &&
          v.name.length <= 64 &&
          validGradient(v.settings),
      )
      ? p
      : [];
  } catch {
    return [];
  }
}
let saved: GradientPreset[] = [];
try {
  saved = readGradientPresets(localStorage.getItem(gradientKey));
} catch {
  /* Storage can be disabled. */
}
export const useGradient = create<{
  settings: GradientSettings;
  presets: GradientPreset[];
  notice: string;
}>(() => ({ settings: structuredClone(initialGradient), presets: saved, notice: '' }));
export function setGradient(change: Partial<GradientSettings>) {
  useGradient.setState((s) => ({ settings: { ...s.settings, ...change } }));
}
export function saveGradient(name: string) {
  const state = useGradient.getState();
  const clean = name.trim();
  if (!clean || clean.length > 64 || !validGradient(state.settings)) {
    useGradient.setState({ notice: 'Use a name of 1–64 characters and valid gradient stops.' });
    return;
  }
  const presets = state.presets.filter((p) => p.name !== clean);
  if (presets.length >= 32) {
    useGradient.setState({ notice: 'Save at most 32 gradients. Delete one first.' });
    return;
  }
  presets.push({ name: clean, settings: structuredClone(state.settings) });
  const encoded = JSON.stringify(presets);
  if (new TextEncoder().encode(encoded).byteLength > 65536) {
    useGradient.setState({
      notice: 'Gradient presets exceed their 64 KiB limit. Delete one first.',
    });
    return;
  }
  try {
    localStorage.setItem(gradientKey, encoded);
    useGradient.setState({ presets, notice: 'Gradient saved.' });
  } catch {
    useGradient.setState({ notice: 'Could not save gradient: browser storage unavailable.' });
  }
}
export function deleteGradient(name: string) {
  const presets = useGradient.getState().presets.filter((p) => p.name !== name);
  try {
    localStorage.setItem(gradientKey, JSON.stringify(presets));
    useGradient.setState({ presets, notice: 'Gradient deleted.' });
  } catch {
    useGradient.setState({ notice: 'Could not delete gradient: browser storage unavailable.' });
  }
}
