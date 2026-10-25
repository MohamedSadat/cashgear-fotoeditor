import { test, expect } from '@playwright/test';
import type { EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { whenIdle } from './idle';
import { defaults } from '../../apps/web/src/brush-state';
import type { DocumentSnapshot } from '../../apps/web/src/engine/types';

for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(
    backend + ' paints sparse adjustment masks outside their initial selection with exact undo',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const revision = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(513, 3, 72);
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 255, y0: 0, x1: 258, y1: 3 },
          mode: 'new',
          antialias: false,
        });
        const s = await api.layerAction({
          kind: 'add-adjustment',
          parent: null,
          index: 1,
          adjustment: {
            kind: 'curves',
            points: [
              [0, 1],
              [1, 0],
            ],
          },
        });
        await api.selectionAction({ kind: 'none' });
        const selected = await api.selectLayer(s.layers.nodes[1].id, true);
        await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
        return selected.revision;
      });
      const snapshot = async () =>
        JSON.parse(
          (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
        ) as DocumentSnapshot;
      await expect
        .poll(async () => {
          const s = await snapshot();
          return [s.revision, s.view.zoom];
        })
        .toEqual([revision, 1]);
      await whenIdle(page, () =>
        page.evaluate(
          async (settings) => {
            const canvas = document.querySelector('[data-testid="gpu-canvas"]') as HTMLElement;
            const s = JSON.parse(canvas.dataset.snapshot!) as DocumentSnapshot;
            const x = s.view.screenWidth / 2 - 156,
              y = s.view.screenHeight / 2,
              time = performance.now();
            await (Reflect.get(window, '__fotoeditor') as EngineApi).interact([
              {
                kind: 'stroke-begin',
                x,
                y,
                amount: 0,
                brush: { ...settings, color: [255, 255, 255, 255], eraser: false },
                sample: [x, y, 1, 0, 0, time],
              },
              { kind: 'stroke-end', x, y, amount: 0, sample: [x, y, 1, 0, 0, time + 1] },
            ]);
          },
          { ...defaults, pencil: true, size: 1, pressureSize: false },
        ),
      );
      await expect.poll(async () => (await snapshot()).history.undo).toBe(5);
      await expect.poll(async () => (await snapshot()).painting).toBe(false);
      const painted = PNG.sync.read(
        Buffer.from(
          await whenIdle(page, () =>
            page.evaluate(async () =>
              Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
            ),
          ),
        ),
      );
      const at = (png: PNG, x: number) =>
        Array.from(png.data.subarray((513 + x) * 4, (513 + x) * 4 + 4));
      expect(at(painted, 100)).toEqual([127, 191, 255, 255]);
      expect(at(painted, 101)).toEqual([128, 64, 0, 255]);
      expect(at(painted, 256)).toEqual([127, 191, 255, 255]);
      await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(4),
        ),
      );
      const undone = PNG.sync.read(
        Buffer.from(
          await whenIdle(page, () =>
            page.evaluate(async () =>
              Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
            ),
          ),
        ),
      );
      expect(at(undone, 100)).toEqual([128, 64, 0, 255]);
      expect(at(undone, 256)).toEqual([127, 191, 255, 255]);
    },
  );
  test(
    backend +
      ' compares five adjustment shaders with CPU masks groups clipping and all blend modes',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const errors = await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).compareAdjustmentCompositors(),
        ),
      );
      expect(errors).toHaveLength(400);
      expect(Math.max(...errors)).toBeLessThanOrEqual(1);
    },
  );
  test(
    backend +
      ' keeps adjustment metadata editable with sparse selection masks exact undo and streamed persistence',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(513, 3, 72);
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 1] });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 255.5, y0: 0, x1: 257.5, y1: 3 },
          mode: 'new',
          antialias: true,
        });
      });
      const original = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      const added = await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).layerAction({
            kind: 'add-adjustment',
            parent: null,
            index: 1,
            adjustment: {
              kind: 'curves',
              points: [
                [0, 1],
                [1, 0],
              ],
            },
          }),
        ),
      );
      expect(added.layers.nodes).toHaveLength(2);
      const layer = added.layers.nodes[1];
      expect(layer.adjustment?.kind).toBe('curves');
      expect(layer.mask?.enabled).toBe(true);
      const adjusted = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      const image = PNG.sync.read(Buffer.from(adjusted));
      const pixel = (x: number) => Array.from(image.data.subarray(x * 4, x * 4 + 4));
      expect(pixel(254)).toEqual([128, 64, 0, 1]);
      expect(pixel(256)).toEqual([127, 191, 255, 1]);
      expect(pixel(258)).toEqual([128, 64, 0, 1]);
      await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(2),
        ),
      );
      const undone = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(undone).toEqual(original);
      await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(3),
        ),
      );
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const directory = await navigator.storage.getDirectory();
          const handle = await directory.getFileHandle('adjustment-test.cgfoto', { create: true });
          await (Reflect.get(window, '__fotoeditor') as EngineApi).saveProjectFile(handle);
        }),
      );
      const opened = await whenIdle(page, () =>
        page.evaluate(async () => {
          const directory = await navigator.storage.getDirectory();
          const handle = await directory.getFileHandle('adjustment-test.cgfoto');
          return (Reflect.get(window, '__fotoeditor') as EngineApi).openProjectFile(
            await handle.getFile(),
          );
        }),
      );
      expect(opened.layers.nodes[1].adjustment).toEqual(layer.adjustment);
      expect(opened.history.undo).toBe(0);
      const roundtrip = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(roundtrip).toEqual(adjusted);
      await whenIdle(page, () =>
        page.evaluate(
          async (target) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).layerAction({
              kind: 'set-adjustment',
              target,
              adjustment: { kind: 'brightness-contrast', brightness: 0, contrast: 0 },
            }),
          opened.layers.nodes[1].id,
        ),
      );
      const identity = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(identity).toEqual(original);
    },
  );
}
