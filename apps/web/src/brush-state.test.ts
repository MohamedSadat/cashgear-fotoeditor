import { expect, test } from 'vitest';
import { evaluateCurve, hex, parseHex } from './brush-state';
import { hsvToRgb, rgbToHsv } from './BrushPanel';
test('color controls round-trip RGB and exact alpha without touching document samples', () => {
  for (const rgba of [
    [255, 0, 0, 255],
    [0, 255, 0, 1],
    [39, 87, 201, 0],
    [0, 0, 0, 255],
    [255, 255, 255, 255],
  ] as [number, number, number, number][]) {
    expect(parseHex(hex(rgba))).toEqual(rgba);
    const hsv = rgbToHsv(rgba);
    expect(hsvToRgb(hsv.h, hsv.s, hsv.v, rgba[3])).toEqual(rgba);
  }
  expect(parseHex('#12345')).toBeUndefined();
  expect(parseHex('gg0000')).toBeUndefined();
  expect(evaluateCurve([0, 0.2, 0.6, 0.8, 1], 0.5)).toBe(0.6);
});
