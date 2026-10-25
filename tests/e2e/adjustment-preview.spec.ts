import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { whenIdle } from './idle';
const state = async (page: Page) => {
  const canvas = page.getByTestId('gpu-canvas');
  let value = await canvas.getAttribute('data-snapshot');
  if (!value || value === 'null') {
    await expect(canvas).toHaveAttribute('data-snapshot', /"view":/);
    value = await canvas.getAttribute('data-snapshot');
  }
  return JSON.parse(value!) as DocumentSnapshot;
};
const image = async (page: Page) =>
  PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
const pixel = (p: PNG, x: number, y: number) =>
  Array.from(p.data.subarray((y * p.width + x) * 4, (y * p.width + x) * 4 + 4));
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(
    backend + ' keeps the high-zoom pixel grid above an identity viewport preview',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const target = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.newDocument(16, 16, 72);
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
        await api.interact([
          { kind: 'actual', x: 0, y: 0, amount: 0 },
          { kind: 'zoom', x: 439, y: 247, amount: 8 },
          { kind: 'pan', x: 0.25, y: 0.25, amount: 0 },
        ]);
        return s.layers.nodes[0].id;
      });
      await expect.poll(async () => (await state(page)).view.zoom).toBe(8);
      const before = await image(page);
      const cx = Math.floor(before.width / 2),
        cy = Math.floor(before.height / 2);
      expect(pixel(before, cx, cy)).not.toEqual(pixel(before, cx + 3, cy + 3));
      await whenIdle(page, () =>
        page.evaluate(
          async (target) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewAdjustment(
              { kind: 'exposure', stops: 0, offset: 0, gamma: 1 },
              { kind: 'pixels', target },
            ),
          target,
        ),
      );
      const preview = await image(page);
      for (const d of [0, 1, 3, 7, 8, 11])
        expect(pixel(preview, cx + d, cy + d)).toEqual(pixel(before, cx + d, cy + d));
      await page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
      );
    },
  );
  test(
    backend +
      ' cancels refuses and invalidates preview candidates without changing document or redo',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const initial = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.newDocument(257, 3, 72);
        const filled = await api.selectionAction({ kind: 'fill', color: [81, 29, 61, 128] });
        return { id: s.layers.nodes[0].id, history: filled.history };
      });
      const id = initial.id;
      const original = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      const history = initial.history;
      const refused = await whenIdle(page, () =>
        page.evaluate(async (target) => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.diagnosticPreviewFault('budget');
          try {
            await api.previewAdjustment(
              { kind: 'exposure', stops: 1, offset: 0, gamma: 1 },
              { kind: 'pixels', target },
            );
            return '';
          } catch (e) {
            return String(e);
          }
        }, id),
      );
      expect(refused).toMatch(/budget/i);
      await page.evaluate(async (target) => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.diagnosticPreviewFault('pause-before-render');
        Reflect.set(
          window,
          'pausedPreview',
          api
            .previewAdjustment(
              { kind: 'exposure', stops: 1, offset: 0, gamma: 1 },
              { kind: 'pixels', target },
            )
            .then(
              () => '',
              (e) => String(e),
            ),
        );
      }, id);
      await expect
        .poll(async () => (await state(page)).operation)
        .toContain('Preview ready to render');
      await page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
      );
      expect(await page.evaluate(async () => await Reflect.get(window, 'pausedPreview'))).toContain(
        'cancelled',
      );
      await page.evaluate(async (target) => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.diagnosticPreviewFault('hold-gpu-wait');
        Reflect.set(
          window,
          'heldGpuPreview',
          api
            .previewAdjustment(
              { kind: 'exposure', stops: 1, offset: 0, gamma: 1 },
              { kind: 'pixels', target },
            )
            .then(
              () => '',
              (e) => String(e),
            ),
        );
      }, id);
      await expect.poll(async () => (await state(page)).operation).toContain('Preview GPU wait');
      await page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
      );
      expect(
        await page.evaluate(async () => await Reflect.get(window, 'heldGpuPreview')),
      ).toContain('cancelled');
      await expect.poll(async () => (await state(page)).adjustmentPreview?.cached).toBe(false);
      expect((await state(page)).memory.temporary).toBeLessThan(6 * 1048576);
      await whenIdle(page, () =>
        page.evaluate(
          async (target) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewAdjustment(
              { kind: 'exposure', stops: 1, offset: 0, gamma: 1 },
              { kind: 'pixels', target },
            ),
          id,
        ),
      );
      await page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).interact([
          { kind: 'rotate', x: 0, y: 0, amount: 0.4 },
        ]),
      );
      await expect.poll(async () => (await state(page)).adjustmentPreview?.cached).toBe(false);
      const unchanged = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(unchanged).toEqual(original);
      expect((await state(page)).history).toEqual(history);
    },
  );
  test(
    backend +
      ' recovers editable adjustment metadata and selection-derived masks after renderer loss',
    async ({ page, context }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(513, 3, 72);
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 255.5, y0: 0, x1: 257.5, y1: 3 },
          mode: 'new',
          antialias: true,
        });
        await api.layerAction({
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
      });
      const expected = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      await whenIdle(page, () =>
        page.evaluate(async () => (Reflect.get(window, '__fotoeditor') as EngineApi).autosave()),
      );
      await expect(page.getByTestId('recovery-status')).toHaveText(/^Autosaved /);
      const session = await context.newCDPSession(page);
      const crashed = page.waitForEvent('crash');
      void session.send('Page.crash').catch(() => {});
      await crashed;
      await page.close();
      page = await context.newPage();
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await page.getByRole('button', { name: 'Restore session', exact: true }).click();
      await expect.poll(async () => (await state(page)).layers.nodes.length).toBe(2);
      expect((await state(page)).layers.nodes[1].adjustment).toEqual({
        kind: 'curves',
        points: [
          [0, 1],
          [1, 0],
        ],
      });
      await expect.poll(async () => (await state(page)).history.undo).toBe(0);
      const actual = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(actual).toEqual(expected);
      const id = (await state(page)).layers.nodes[1].id;
      await whenIdle(page, () =>
        page.evaluate(
          async (target) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewAdjustment(
              {
                kind: 'curves',
                points: [
                  [0, 0],
                  [1, 1],
                ],
              },
              { kind: 'layer', target },
            ),
          id,
        ),
      );
      await page.getByRole('button', { name: 'Simulate device loss' }).click();
      await expect(page.getByRole('alert')).toContainText('GPU device lost');
      await page.getByRole('button', { name: 'Retry engine' }).click();
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).newDocument(32, 32, 72),
        ),
      );
      await expect.poll(async () => (await state(page))?.adjustmentPreview?.active).toBe(false);
    },
  );
  test(
    backend +
      ' previews selected raster beneath blended layers without mutation and reuses visible sources',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const { base, revision } = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.newDocument(512, 128, 72);
        const base = s.layers.nodes[0].id;
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
        const top = await api.layerAction({
          kind: 'add',
          name: 'Overlay',
          parent: null,
          index: 1,
          group: false,
        });
        await api.selectionAction({ kind: 'fill', color: [32, 16, 208, 128] });
        const node = top.layers.nodes[1];
        await api.layerAction({
          kind: 'properties',
          target: node.id,
          properties: { ...node.properties, blend: 'multiply', opacity: 0.7 },
        });
        await api.selectLayer(base);
        const selected = await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 128, y0: 32, x1: 384, y1: 96 },
          antialias: false,
          mode: 'new',
        });
        await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
        return { base, revision: selected.revision };
      });
      await expect
        .poll(async () => {
          const s = await state(page);
          return [s.revision, s.view.zoom];
        })
        .toEqual([revision, 1]);
      const before = await image(page);
      const history = (await state(page)).history.undo;
      const original = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      const first = await whenIdle(page, () =>
        page.evaluate(
          async (target) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewAdjustment(
              {
                kind: 'curves',
                points: [
                  [0, 1],
                  [1, 0],
                ],
              },
              { kind: 'pixels', target },
            ),
          base,
        ),
      );
      expect(first.width).toBeLessThanOrEqual(2048);
      expect(first.preparationMs).toBeGreaterThan(0);
      await expect.poll(async () => (await state(page)).adjustmentPreview?.active).toBe(true);
      const preview = await image(page);
      const cx = Math.floor(preview.width / 2),
        cy = Math.floor(preview.height / 2);
      expect(pixel(preview, cx, cy)).not.toEqual(pixel(before, cx, cy));
      expect(pixel(preview, cx - 200, cy)).toEqual(pixel(before, cx - 200, cy));
      expect((await state(page)).history.undo).toBe(history);
      const unchanged = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(unchanged).toEqual(original);
      const warm = await whenIdle(page, () =>
        page.evaluate(
          async (target) =>
            (Reflect.get(window, '__fotoeditor') as EngineApi).previewAdjustment(
              {
                kind: 'curves',
                points: [
                  [0, 1],
                  [1, 0],
                ],
              },
              { kind: 'pixels', target },
            ),
          base,
        ),
      );
      expect(warm.preparationMs).toBe(0);
      await page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
      );
      await expect.poll(async () => (await state(page)).adjustmentPreview?.active).toBe(false);
      const cancelled = await image(page);
      expect(pixel(cancelled, cx, cy)).toEqual(pixel(before, cx, cy));
      await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).applyAdjustment({
            kind: 'curves',
            points: [
              [0, 1],
              [1, 0],
            ],
          }),
        ),
      );
      const applied = await image(page);
      for (const dx of [-200, -100, 0, 100, 200]) {
        const actual = pixel(applied, cx + dx, cy),
          expected = pixel(preview, cx + dx, cy);
        expect(Math.max(...actual.map((v, i) => Math.abs(v - expected[i])))).toBeLessThanOrEqual(1);
      }
    },
  );
  test(
    backend + ' creates and edits adjustment layers through live controls with exact metadata undo',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(128, 128, 72);
        await api.selectionAction({ kind: 'fill', color: [64, 32, 16, 255] });
      });
      await page.getByRole('spinbutton', { name: 'Brightness (%)', exact: true }).fill('25');
      await page.getByRole('checkbox', { name: 'Create adjustment layer above selection' }).check();
      await page.getByRole('checkbox', { name: 'Live viewport preview' }).check();
      await expect(page.getByTestId('adjustment-preview-status')).toContainText('Preview ready');
      expect((await state(page)).history.undo).toBe(1);
      await page.getByRole('button', { name: 'Add adjustment layer', exact: true }).click();
      await expect.poll(async () => (await state(page)).layers.nodes.length).toBe(2);
      await expect.poll(async () => (await state(page)).operation).toBeUndefined();
      await page
        .getByRole('checkbox', { name: 'Create adjustment layer above selection' })
        .uncheck();
      await page.getByRole('spinbutton', { name: 'Brightness (%)', exact: true }).fill('50');
      await page.getByRole('checkbox', { name: 'Live viewport preview' }).check();
      await expect(page.getByTestId('adjustment-preview-status')).toContainText('Preview ready');
      await page
        .getByRole('slider', { name: 'Brightness (%) slider', exact: true })
        .evaluate((element) => {
          for (const value of [10, 17, 42, 65, 75]) {
            Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
              element,
              String(value),
            );
            element.dispatchEvent(new Event('input', { bubbles: true }));
          }
        });
      await expect(page.getByTestId('adjustment-preview-status')).toHaveAttribute(
        'data-settings',
        /"brightness":0.75/,
      );
      await page.getByRole('button', { name: 'Save adjustment layer', exact: true }).click();
      await expect.poll(async () => (await state(page)).history.undo).toBe(3);
      const setting = (await state(page)).layers.nodes[1].adjustment;
      expect(setting).toEqual({ kind: 'brightness-contrast', brightness: 0.75, contrast: 0 });
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await expect
        .poll(async () => (await state(page)).layers.nodes[1].adjustment)
        .toEqual({ kind: 'brightness-contrast', brightness: 0.25, contrast: 0 });
      const memory = (await state(page)).memory;
      expect(memory.document + memory.history + memory.temporary).toBeLessThanOrEqual(
        768 * 1048576,
      );
      expect(memory.gpu).toBeLessThanOrEqual(128 * 1048576);
    },
  );
}
