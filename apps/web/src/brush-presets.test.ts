import { beforeEach, expect, test, vi } from 'vitest';
import { defaults, useBrush } from './brush-state';
import {
  applyBrushPreset,
  brushPresetKey,
  brushPresetLimit,
  decodeBrushPresets,
  deleteBrushPreset,
  encodeBrushPresets,
  importBrushPresets,
  saveBrushPreset,
  useBrushPresets,
  validBrush,
  validCoverage,
} from './brush-presets';
beforeEach(() => {
  vi.stubGlobal('localStorage', { setItem: vi.fn(), getItem: vi.fn() });
  useBrush.setState({ settings: structuredClone(defaults) });
  useBrushPresets.setState({ presets: [], notice: '' });
});
test('CashGear presets roundtrip image assets and complete dynamics without document or symmetry state', () => {
  const settings = {
    ...structuredClone(defaults),
    tipImage: { width: 2, height: 1, coverage: 'ff00' },
    texture: { width: 1, height: 2, coverage: '80ff' },
    scatter: 1.2,
    sizeJitter: 0.3,
    angleJitter: 0.7,
    opacityJitter: 0.2,
    seed: 0xffffffff,
  };
  useBrush.setState({
    settings,
    primary: [53, 42, 99, 127],
    symmetry: { mode: 'both', center: [37, 51], sectors: 6 },
  });
  saveBrushPreset('Ink');
  const encoded = encodeBrushPresets(useBrushPresets.getState().presets);
  expect(decodeBrushPresets(encoded)).toEqual([{ name: 'Ink', settings }]);
  expect(localStorage.setItem).toHaveBeenCalledWith(brushPresetKey, encoded);
  useBrush.setState({ settings: structuredClone(defaults) });
  applyBrushPreset('Ink');
  expect(useBrush.getState().settings).toEqual(settings);
  expect(useBrush.getState().symmetry.center).toEqual([37, 51]);
  expect(useBrush.getState().primary).toEqual([53, 42, 99, 127]);
  applyBrushPreset('Soft round');
  expect(useBrush.getState().settings.tipImage).toBeNull();
  expect(useBrush.getState().settings.scatter).toBe(0);
});
test('malformed, future, duplicate, unknown and oversized preset imports are atomic', () => {
  saveBrushPreset('Keep');
  const before = structuredClone(useBrushPresets.getState().presets);
  const good = JSON.parse(encodeBrushPresets(before)) as {
    format: string;
    version: number;
    presets: unknown[];
  };
  for (const value of [
    '{',
    JSON.stringify({ ...good, version: 2 }),
    JSON.stringify({ ...good, format: 'abr' }),
    JSON.stringify({ ...good, presets: [...before, ...before] }),
    JSON.stringify({
      ...good,
      presets: [{ name: 'Bad', settings: { ...defaults, size: Infinity } }],
    }),
    JSON.stringify({ ...good, extra: true }),
    ' '.repeat(brushPresetLimit + 1),
  ]) {
    importBrushPresets(value);
    expect(useBrushPresets.getState().presets).toEqual(before);
  }
  expect(() =>
    decodeBrushPresets(
      JSON.stringify({
        ...good,
        presets: Array.from({ length: 33 }, (_, i) => ({ name: String(i), settings: defaults })),
      }),
    ),
  ).toThrow();
  expect(validBrush({ ...defaults, tool: 'clone' })).toBe(false);
  expect(validBrush({ ...defaults, sizePressure: [0, 1] })).toBe(false);
  expect(validCoverage({ width: 513, height: 1, coverage: 'ff' })).toBe(false);
  expect(validCoverage({ width: 1, height: 1, coverage: 'GG' })).toBe(false);
});
test('storage failure preserves preset library/settings and successful import replaces once', () => {
  saveBrushPreset('Keep');
  const before = structuredClone(useBrushPresets.getState().presets);
  const incoming = [{ name: 'New', settings: { ...defaults, seed: 71 } }];
  vi.mocked(localStorage.setItem).mockImplementation(() => {
    throw new Error('Storage unavailable');
  });
  const settings = useBrush.getState().settings;
  importBrushPresets(encodeBrushPresets(incoming));
  expect(useBrushPresets.getState().presets).toEqual(before);
  saveBrushPreset('Other');
  deleteBrushPreset('Keep');
  expect(useBrushPresets.getState().presets).toEqual(before);
  expect(useBrushPresets.getState().notice).toMatch(/Storage unavailable/);
  expect(useBrush.getState().settings).toBe(settings);
  vi.mocked(localStorage.setItem).mockImplementation(() => {});
  importBrushPresets(encodeBrushPresets(incoming));
  expect(useBrushPresets.getState().presets).toEqual(incoming);
  deleteBrushPreset('New');
  expect(useBrushPresets.getState().presets).toEqual([]);
});
