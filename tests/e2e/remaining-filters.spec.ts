import { test, expect } from '@playwright/test';
import { PNG } from 'pngjs';
import type { Page } from '@playwright/test';
import type { EngineApi, SpatialFilter } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';
const cases: SpatialFilter[] = [
  { kind: 'motion-blur', radius: 7, angle: 31 },
  { kind: 'high-pass', radius: 3 },
  { kind: 'add-noise', amount: 0.4, seed: 123, monochrome: false, distribution: 'gaussian' },
  { kind: 'emboss', radius: 4, angle: -41, amount: 1.3 },
  { kind: 'pixelate', size: 37 },
];
const exported = (page: Page) =>
  whenIdle(page, () =>
    page.evaluate(async () =>
      Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
    ),
  );
const pixel = (bytes: number[], x: number) => {
  const p = PNG.sync.read(Buffer.from(bytes));
  return Array.from(p.data.subarray((p.width + x) * 4, (p.width + x) * 4 + 4));
};
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  for (let index = 0; index < 14; index++)
    test(`${backend} remaining filter shader ${index} matches Rust across all nine odd canvas tiles`, async ({
      page,
    }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const errors = await whenIdle(page, () =>
        page.evaluate(
          (i) => (Reflect.get(window, '__fotoeditor') as EngineApi).compareFilterCompositors(i),
          index,
        ),
      );
      expect(errors).toHaveLength(9);
      expect(Math.max(...errors)).toBeLessThanOrEqual(1);
    });
  for (const filter of cases)
    test(`${backend} ${filter.kind} selection preview Fade exact history and project roundtrip`, async ({
      page,
    }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const initial = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(513, 3, 72);
          await api.selectionAction({ kind: 'fill', color: [64, 64, 64, 128] });
          await api.selectionAction({
            kind: 'shape',
            shape: { kind: 'rectangle', x0: 255, y0: 0, x1: 256, y1: 3 },
            mode: 'new',
            antialias: false,
          });
          await api.selectionAction({ kind: 'fill', color: [128, 128, 128, 128] });
          return api.selectionAction({
            kind: 'shape',
            shape: { kind: 'rectangle', x0: 255.5, y0: 0, x1: 257.5, y1: 3 },
            mode: 'new',
            antialias: true,
          });
        }),
      );
      const before = await exported(page);
      const timing = await whenIdle(page, () =>
        page.evaluate(
          ({ filter, target }) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewFilter(filter, target),
          { filter, target: initial.layers.nodes[0].id },
        ),
      );
      expect(timing.serial).toBeGreaterThan(0);
      expect(await exported(page)).toEqual(before);
      await page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
      );
      const changed = await whenIdle(page, () =>
        page.evaluate(
          (f) => (Reflect.get(window, '__fotoeditor') as EngineApi).applyFilter(f),
          filter,
        ),
      );
      expect(changed.history.undo).toBe(initial.history.undo + 1);
      expect(changed.canFade).toBe(true);
      const result = await exported(page);
      expect(result).not.toEqual(before);
      for (const x of [0, 254, 258, 512]) expect(pixel(result, x)).toEqual(pixel(before, x));
      await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).fadeFilter(0, 'normal'),
        ),
      );
      expect(await exported(page)).toEqual(before);
      await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).fadeFilter(1, 'normal'),
        ),
      );
      expect(await exported(page)).toEqual(result);
      await whenIdle(page, () =>
        page.evaluate(
          (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
          initial.history.undo,
        ),
      );
      expect(await exported(page)).toEqual(before);
      await whenIdle(page, () =>
        page.evaluate(
          (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
          changed.history.undo + 2,
        ),
      );
      expect(await exported(page)).toEqual(result);
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const bytes = await api.saveProject();
          await api.openProject(bytes.slice().buffer as ArrayBuffer, 'remaining-filters.cgfoto');
        }),
      );
      expect(await exported(page)).toEqual(result);
    });
}
