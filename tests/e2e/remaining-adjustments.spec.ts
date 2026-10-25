import { test, expect } from '@playwright/test';
import { PNG } from 'pngjs';
import type { Page } from '@playwright/test';
import type { Adjustment, EngineApi } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';

const cases: [Adjustment, number[]][] = [
  [{ kind: 'vibrance', amount: -1 }, [73, 73, 73, 1]],
  [
    {
      kind: 'color-balance',
      shadows: [0, 0, 0],
      midtones: [0.2, 0, -0.2],
      highlights: [0, 0, 0],
      preserveLuminosity: false,
    },
    [157, 64, 0, 1],
  ],
  [{ kind: 'black-white', weights: [0.3, 0.89, 0.59, 0.7, 0.11, 0.41] }, [76, 76, 76, 1]],
  [{ kind: 'invert' }, [127, 191, 255, 1]],
  [{ kind: 'threshold', level: 64 }, [255, 255, 255, 1]],
  [{ kind: 'posterize', levels: 4 }, [170, 85, 0, 1]],
  [
    {
      kind: 'gradient-map',
      stops: [
        [0, 1, 0, 0],
        [1, 0, 0, 1],
      ],
    },
    [182, 0, 73, 1],
  ],
];
const exported = (page: Page) =>
  whenIdle(page, () =>
    page.evaluate(async () =>
      Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
    ),
  );
const pixel = (image: number[], x: number) => {
  const png = PNG.sync.read(Buffer.from(image));
  return Array.from(png.data.subarray(x * 4, x * 4 + 4));
};
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  for (let index = 0; index < 8; index++)
    test(`${backend} remaining adjustment shader ${index} matches masks groups clipping and 16 blends`, async ({
      page,
    }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const errors = await whenIdle(page, () =>
        page.evaluate(
          (index) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).compareAdjustmentCompositors(
              true,
              index,
            ),
          index,
        ),
      );
      expect(errors).toHaveLength(80);
      expect(Math.max(...errors)).toBeLessThanOrEqual(1);
    });
  for (const [adjustment, expected] of cases)
    test(`${backend} ${adjustment.kind} destructive and editable layer retain selected samples undo preview and persistence`, async ({
      page,
    }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const initial = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(513, 3, 72);
          await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 1] });
          return api.selectionAction({
            kind: 'shape',
            shape: { kind: 'rectangle', x0: 255.5, y0: 0, x1: 257.5, y1: 3 },
            mode: 'new',
            antialias: true,
          });
        }),
      );
      const before = await exported(page);
      const changed = await whenIdle(page, () =>
        page.evaluate(
          (a) => (Reflect.get(window, '__fotoeditor') as EngineApi).applyAdjustment(a),
          adjustment,
        ),
      );
      expect(changed.history.undo).toBe(initial.history.undo + 1);
      const destructive = await exported(page);
      expect(pixel(destructive, 254)).toEqual([128, 64, 0, 1]);
      expect(pixel(destructive, 256)).toEqual(expected);
      expect(pixel(destructive, 255)).toEqual(pixel(destructive, 257));
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
          changed.history.undo,
        ),
      );
      expect(await exported(page)).toEqual(destructive);
      await whenIdle(page, () =>
        page.evaluate(
          (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
          initial.history.undo,
        ),
      );
      const layer = await whenIdle(page, () =>
        page.evaluate(
          (adjustment) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).layerAction({
              kind: 'add-adjustment',
              parent: null,
              index: 1,
              adjustment,
            }),
          adjustment,
        ),
      );
      expect(layer.layers.nodes[1].adjustment).toEqual(adjustment);
      const layered = await exported(page);
      for (const x of [0, 254, 255, 256, 257, 258, 512]) {
        const a = pixel(layered, x),
          b = pixel(destructive, x);
        for (let c = 0; c < 4; c++) expect(Math.abs(a[c] - b[c])).toBeLessThanOrEqual(1);
      }
      const preview = await whenIdle(page, () =>
        page.evaluate(
          ({ adjustment, target }) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewAdjustment(adjustment, {
              kind: 'layer',
              target,
            }),
          { adjustment, target: layer.layers.nodes[1].id },
        ),
      );
      expect(preview.serial).toBeGreaterThan(0);
      await page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
      );
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const directory = await navigator.storage.getDirectory();
          const handle = await directory.getFileHandle('remaining.cgfoto', { create: true });
          await (Reflect.get(window, '__fotoeditor') as EngineApi).saveProjectFile(handle);
        }),
      );
      const opened = await whenIdle(page, () =>
        page.evaluate(async () => {
          const directory = await navigator.storage.getDirectory();
          const handle = await directory.getFileHandle('remaining.cgfoto');
          return (Reflect.get(window, '__fotoeditor') as EngineApi).openProjectFile(
            await handle.getFile(),
          );
        }),
      );
      expect(opened.layers.nodes[1].adjustment).toEqual(adjustment);
      expect(await exported(page)).toEqual(layered);
      const edited = await whenIdle(page, () =>
        page.evaluate(
          ({ adjustment, target }) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).layerAction({
              kind: 'set-adjustment',
              target,
              adjustment,
            }),
          { adjustment: { kind: 'invert' } as Adjustment, target: opened.layers.nodes[1].id },
        ),
      );
      expect(edited.layers.nodes[1].adjustment).toEqual({ kind: 'invert' });
    });
}
