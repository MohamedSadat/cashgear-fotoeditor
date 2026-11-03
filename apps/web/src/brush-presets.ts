import { create } from 'zustand';
import { defaults, presets, setBrush, useBrush } from './brush-state';
import type { BrushSettings, CoverageImage } from './brush-state';
export const brushPresetKey = 'cashgear-brush-presets-v1';
export const brushPresetLimit = 4 * 1024 * 1024;
export interface BrushPreset {
  name: string;
  settings: BrushSettings;
}
interface Library {
  format: 'cashgear-brush-presets';
  version: 1;
  presets: BrushPreset[];
}
export function validCoverage(value: unknown): value is CoverageImage {
  if (!value || typeof value !== 'object') return false;
  const v = value as CoverageImage;
  return (
    Object.keys(v).length === 3 &&
    Number.isInteger(v.width) &&
    Number.isInteger(v.height) &&
    v.width > 0 &&
    v.height > 0 &&
    v.width <= 512 &&
    v.height <= 512 &&
    typeof v.coverage === 'string' &&
    v.coverage.length === v.width * v.height * 2 &&
    /^[0-9a-f]+$/.test(v.coverage)
  );
}
export function validBrush(value: unknown): value is BrushSettings {
  if (!value || typeof value !== 'object') return false;
  const s = value as BrushSettings;
  if (
    Object.keys(s).length !== Object.keys(defaults).length ||
    Object.keys(s).some((k) => !(k in defaults))
  )
    return false;
  const unit: (keyof BrushSettings)[] = [
    'hardness',
    'opacity',
    'flow',
    'strength',
    'scatter',
    'sizeJitter',
    'angleJitter',
    'opacityJitter',
    'textureDepth',
  ];
  return (
    unit.every(
      (k) =>
        typeof s[k] === 'number' &&
        Number.isFinite(s[k]) &&
        (s[k] as number) >= 0 &&
        (s[k] as number) <= (k === 'scatter' ? 2 : 1),
    ) &&
    Number.isFinite(s.size) &&
    s.size >= 1 &&
    s.size <= 2048 &&
    Number.isFinite(s.angle) &&
    Math.abs(s.angle) <= 360 &&
    Number.isFinite(s.spacing) &&
    s.spacing >= 0.01 &&
    s.spacing <= 1 &&
    Number.isFinite(s.roundness) &&
    s.roundness >= 0.05 &&
    s.roundness <= 1 &&
    Number.isFinite(s.textureScale) &&
    s.textureScale >= 1 &&
    s.textureScale <= 512 &&
    Number.isInteger(s.seed) &&
    s.seed >= 0 &&
    s.seed <= 0xffffffff &&
    ['pencil', 'pressureSize', 'pressureOpacity', 'tiltSize', 'tiltOpacity'].every(
      (k) => typeof s[k as keyof BrushSettings] === 'boolean',
    ) &&
    ['off', 'pulled', 'weighted'].includes(s.stabilizer) &&
    [s.sizePressure, s.opacityPressure, s.sizeTilt, s.opacityTilt].every(
      (c) =>
        Array.isArray(c) &&
        c.length === 5 &&
        c.every((n) => typeof n === 'number' && Number.isFinite(n) && n >= 0 && n <= 1),
    ) &&
    (s.tipImage === null || validCoverage(s.tipImage)) &&
    (s.texture === null || validCoverage(s.texture))
  );
}
export function encodeBrushPresets(entries: BrushPreset[]) {
  const library: Library = { format: 'cashgear-brush-presets', version: 1, presets: entries };
  const text = JSON.stringify(library);
  if (new TextEncoder().encode(text).byteLength > brushPresetLimit)
    throw new Error('Brush presets exceed their 4 MiB limit.');
  return text;
}
export function decodeBrushPresets(text: string): BrushPreset[] {
  if (
    text.length > brushPresetLimit ||
    new TextEncoder().encode(text).byteLength > brushPresetLimit
  )
    throw new Error('Brush presets exceed their 4 MiB limit.');
  const v: Library = JSON.parse(text) as Library;
  if (
    !v ||
    Object.keys(v).length !== 3 ||
    v.format !== 'cashgear-brush-presets' ||
    v.version !== 1 ||
    !Array.isArray(v.presets) ||
    v.presets.length > 32 ||
    !v.presets.every(
      (p) =>
        p &&
        Object.keys(p).length === 2 &&
        typeof p.name === 'string' &&
        p.name === p.name.trim() &&
        p.name.length > 0 &&
        p.name.length <= 64 &&
        !Object.hasOwn(presets, p.name) &&
        validBrush(p.settings),
    ) ||
    new Set(v.presets.map((p) => p.name)).size !== v.presets.length
  )
    throw new Error(
      'Invalid CashGear brush library. Use version 1, up to 32 unique named presets and valid brush settings.',
    );
  return v.presets;
}
let saved: BrushPreset[] = [];
try {
  const text = localStorage.getItem(brushPresetKey);
  if (text) saved = decodeBrushPresets(text);
} catch {
  /* Untrusted/unavailable preferences never affect the document. */
}
export const useBrushPresets = create<{ presets: BrushPreset[]; notice: string }>(() => ({
  presets: saved,
  notice: '',
}));
function persist(entries: BrushPreset[], notice: string) {
  const encoded = encodeBrushPresets(entries);
  decodeBrushPresets(encoded);
  localStorage.setItem(brushPresetKey, encoded);
  useBrushPresets.setState({ presets: entries, notice });
}
export function saveBrushPreset(name: string) {
  try {
    const clean = name.trim();
    const entries = useBrushPresets.getState().presets.filter((p) => p.name !== clean);
    entries.push({ name: clean, settings: structuredClone(useBrush.getState().settings) });
    persist(entries, 'Brush saved.');
  } catch (e) {
    useBrushPresets.setState({ notice: String(e) });
  }
}
export function deleteBrushPreset(name: string) {
  try {
    persist(
      useBrushPresets.getState().presets.filter((p) => p.name !== name),
      'Brush deleted.',
    );
  } catch (e) {
    useBrushPresets.setState({ notice: String(e) });
  }
}
export function importBrushPresets(text: string) {
  try {
    persist(decodeBrushPresets(text), 'Brush library imported.');
  } catch (e) {
    useBrushPresets.setState({ notice: String(e) });
  }
}
export function applyBrushPreset(name: string) {
  const custom = useBrushPresets.getState().presets.find((p) => p.name === name);
  if (custom) setBrush(structuredClone(custom.settings));
  else if (Object.hasOwn(presets, name)) setBrush({ ...defaults, ...presets[name] });
}
