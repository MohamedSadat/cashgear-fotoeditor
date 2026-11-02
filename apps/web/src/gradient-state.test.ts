import { describe, it, expect } from 'vitest';
import { initialGradient, readGradientPresets, validGradient } from './gradient-state';
describe('gradient preferences', () => {
  it('loads valid named multi-stop presets and rejects corrupt or excessive storage', () => {
    const p = [{ name: 'Saved', settings: initialGradient }];
    expect(readGradientPresets(JSON.stringify(p))).toEqual(p);
    for (const text of [
      '{}',
      '[null]',
      'invalid',
      ' '.repeat(65537),
      JSON.stringify(Array(33).fill(p[0])),
    ])
      expect(readGradientPresets(text)).toEqual([]);
  });
  it('rejects unordered stops, missing endpoints, invalid colors and nonfinite controls', () => {
    expect(validGradient(initialGradient)).toBe(true);
    for (const patch of [
      { opacity: NaN },
      { shape: 'bad' },
      { reverse: 0 },
      {
        stops: [
          { position: 0, color: [0, 0, 0, 256] },
          { position: 1, color: [0, 0, 0, 255] },
        ],
      },
      {
        stops: [
          { position: 0, color: [0, 0, 0, 255] },
          { position: 0, color: [0, 0, 0, 255] },
        ],
      },
    ])
      expect(validGradient({ ...initialGradient, ...patch })).toBe(false);
  });
});
