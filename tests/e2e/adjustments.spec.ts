import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Adjustment, DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { resolve } from 'node:path';
const snap = async (page: Page) => {
  await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-snapshot', /"view":/);
  return JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
};
import { whenIdle } from './idle';
const exported = async (page: Page) =>
  PNG.sync.read(
    Buffer.from(
      await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      ),
    ),
  );
const pixel = (image: PNG, x: number, y = 0) =>
  Array.from(image.data.subarray((y * image.width + x) * 4, (y * image.width + x) * 4 + 4));
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(
    backend +
      ' applies five adjustments across selected tile edges with exact undo histogram and project samples',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
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
      const before = await exported(page);
      const cases: [Adjustment, number[]][] = [
        [{ kind: 'brightness-contrast', brightness: 0.2, contrast: 0 }, [179, 115, 51, 1]],
        [
          { kind: 'levels', black: 0, white: 1, gamma: 1, outputBlack: 1, outputWhite: 1 },
          [255, 255, 255, 1],
        ],
        [
          {
            kind: 'curves',
            points: [
              [0, 1],
              [1, 0],
            ],
          },
          [127, 191, 255, 1],
        ],
        [{ kind: 'exposure', stops: 1, offset: 0, gamma: 1 }, [176, 90, 0, 1]],
        [{ kind: 'hue-saturation', hue: 120, saturation: 0, lightness: 0 }, [0, 128, 64, 1]],
      ];
      for (const [setting, expected] of cases) {
        const state = await whenIdle(page, () =>
          page.evaluate(
            (s) => (Reflect.get(window, '__fotoeditor') as EngineApi).applyAdjustment(s),
            setting,
          ),
        );
        expect(state.history.undo).toBe(3);
        const image = await exported(page);
        expect(pixel(image, 254)).toEqual([128, 64, 0, 1]);
        expect(pixel(image, 256)).toEqual(expected);
        expect(pixel(image, 255)).toEqual(pixel(image, 257));
        expect(pixel(image, 255)[3]).toBe(1);
        const h = await whenIdle(page, () =>
          page.evaluate(async (target) => {
            const api = Reflect.get(window, '__fotoeditor') as EngineApi;
            return api.histogram(target);
          }, state.layers.active ?? state.layers.nodes[0].id),
        );
        expect(h.pixels).toBe(1539);
        expect(h.weight).toBe(1539);
        for (const bins of h.bins) expect(bins.reduce((a, b) => a + b, 0)).toBe(1539);
        if (setting.kind === 'brightness-contrast')
          await page.evaluate(() => {
            Reflect.set(
              window,
              'adjustmentAutosave',
              (Reflect.get(window, '__fotoeditor') as EngineApi).autosave(),
            );
          });
        await whenIdle(page, () =>
          page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(2)),
        );
        if (setting.kind === 'brightness-contrast')
          await page.evaluate(async () => {
            await Reflect.get(window, 'adjustmentAutosave');
          });
        expect((await exported(page)).data.equals(before.data)).toBe(true);
        await whenIdle(page, () =>
          page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(3)),
        );
        expect((await exported(page)).data.equals(image.data)).toBe(true);
        await whenIdle(page, () =>
          page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(2)),
        );
      }
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.applyAdjustment({
            kind: 'curves',
            points: [
              [0, 1],
              [1, 0],
            ],
          });
        }),
      );
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const bytes = await api.saveProject();
          Reflect.set(window, 'adjustedProject', bytes);
        }),
      );
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const bytes = Reflect.get(window, 'adjustedProject') as Uint8Array;
          await api.openProject(
            bytes.buffer.slice(
              bytes.byteOffset,
              bytes.byteOffset + bytes.byteLength,
            ) as ArrayBuffer,
            'adjusted.cgfoto',
          );
        }),
      );
      expect(pixel(await exported(page), 256)).toEqual([127, 191, 255, 1]);
      await expect.poll(async () => (await snap(page)).history.undo).toBe(0);
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/alpha.png'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).applyAdjustment({
          kind: 'curves',
          points: [
            [0, 1],
            [1, 0],
          ],
        }),
      );
      expect(pixel(await exported(page), 0)).toEqual([217, 33, 97, 0]);
    },
  );
  test(
    backend +
      ' rejects invalid locked and over-budget adjustments and cancels before atomic commit',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(520, 2, 72);
        await api.selectionAction({ kind: 'fill', color: [10, 20, 30, 128] });
        await api.selectionAction({ kind: 'all' });
        await api.historyJump(1);
      });
      const before = await exported(page);
      const errors = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const errors: string[] = [];
        for (const settings of [
          { kind: 'levels', black: 0.5, white: 0.5, gamma: 1, outputBlack: 0, outputWhite: 1 },
          {
            kind: 'curves',
            points: [
              [0, 0],
              [0, 1],
            ],
          },
        ] as Adjustment[]) {
          try {
            await api.applyAdjustment(settings);
          } catch (e) {
            errors.push(String(e));
          }
        }
        await api.diagnosticAdjustmentFault('budget');
        try {
          await api.applyAdjustment({ kind: 'exposure', stops: 1, offset: 0, gamma: 1 });
        } catch (e) {
          errors.push(String(e));
        }
        await api.diagnosticAdjustmentFault('pause-before-commit');
        Reflect.set(
          window,
          'adjustmentPending',
          api.applyAdjustment({ kind: 'exposure', stops: 1, offset: 0, gamma: 1 }).then(
            () => '',
            (e) => String(e),
          ),
        );
        return errors;
      });
      expect(errors).toHaveLength(3);
      expect(errors[2]).toMatch(/budget|768 MiB/);
      await expect(page.getByTestId('operation-status')).toHaveText(/Adjustment ready to commit/);
      const cancelled = await page.evaluate(async () => ({
        cancelled: await (Reflect.get(window, '__fotoeditor') as EngineApi).cancelJob(),
        error: await Reflect.get(window, 'adjustmentPending'),
      }));
      expect(cancelled.cancelled).toBe(true);
      expect(cancelled.error).toMatch(/cancelled/);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      expect((await snap(page)).history.redo).toBe(1);
      const locked = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const state = await api.selectionAction({ kind: 'none' });
        const node = state.layers.nodes[0];
        node.properties.locks.pixels = true;
        await api.layerAction({ kind: 'properties', target: node.id, properties: node.properties });
        try {
          await api.applyAdjustment({ kind: 'exposure', stops: 1, offset: 0, gamma: 1 });
          return '';
        } catch (e) {
          return String(e);
        }
      });
      expect(locked).toMatch(/locked/);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
    },
  );
  test(
    backend +
      ' confirms controls restores recovery and refreshes histograms after worker replacement',
    async ({ page, context }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 64, 72);
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
      });
      await expect.poll(async () => (await snap(page)).history.undo).toBe(1);
      await page.getByRole('button', { name: 'Show adjustments section' }).click();
      await page.getByLabel('Adjustment', { exact: true }).selectOption('exposure');
      await page.getByRole('spinbutton', { name: 'Exposure (stops)', exact: true }).fill('1');
      expect(pixel(await exported(page), 0)).toEqual([128, 64, 0, 255]);
      await page.getByRole('button', { name: 'Apply adjustment', exact: true }).click();
      await expect(page.getByTestId('adjustment-notice')).toHaveText(/applied/);
      expect(pixel(await exported(page), 0)).toEqual([176, 90, 0, 255]);
      await expect(page.getByTestId('histogram-total')).toHaveText(
        '4,096 nontransparent pixels · alpha weight 1,044,480',
      );
      await page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).autosave());
      await expect(page.getByTestId('recovery-status')).toHaveText(/^Autosaved /);
      const session = await context.newCDPSession(page);
      const crashed = page.waitForEvent('crash');
      void session.send('Page.crash').catch(() => {});
      await crashed;
      await page.close();
      page = await context.newPage();
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.getByRole('button', { name: 'Restore session', exact: true }).click();
      await expect(page.getByTestId('document-size')).toHaveText('64 × 64');
      expect(pixel(await exported(page), 0)).toEqual([176, 90, 0, 255]);
      await expect.poll(async () => (await snap(page)).history.undo).toBe(0);
      await expect(page.getByTestId('histogram-total')).toHaveText(
        '4,096 nontransparent pixels · alpha weight 1,044,480',
      );
      await page.getByRole('button', { name: 'Simulate device loss' }).click();
      await expect(page.getByRole('alert')).toContainText('GPU device lost');
      await page.getByRole('button', { name: 'Retry engine' }).click();
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).newDocument(64, 64, 72),
      );
      await expect.poll(async () => (await snap(page)).document?.width).toBe(64);
      const cached = await page.getByTestId('histogram-total').allTextContents();
      expect(cached.join('')).not.toContain('4,096');
      await expect(page.getByTestId('histogram-total')).toHaveText(
        '0 nontransparent pixels · alpha weight 0',
      );
    },
  );
}
