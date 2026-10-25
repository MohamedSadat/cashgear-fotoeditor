import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi, SpatialFilter } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { whenIdle } from './idle';
const state = async (page: Page) => {
  await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-snapshot', /"view":/);
  return JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
};
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
const pixel = (p: PNG, x: number, y = 1) =>
  Array.from(p.data.subarray((y * p.width + x) * 4, (y * p.width + x) * 4 + 4));
const filterCases: [SpatialFilter, number, number][] = [
  [{ kind: 'gaussian', radius: 3 }, 109, 79],
  [{ kind: 'box-blur', radius: 1 }, 107, 85],
  [{ kind: 'unsharp-mask', radius: 1, amount: 1, threshold: 0 }, 129, 63],
  [{ kind: 'sharpen', amount: 1 }, 149, 43],
  [{ kind: 'median', radius: 1 }, 96, 64],
];
async function fixture(page: Page, select = true) {
  return whenIdle(page, () =>
    page.evaluate(async (select) => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.newDocument(513, 3, 72);
      await api.selectionAction({ kind: 'fill', color: [64, 64, 64, 255] });
      await api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 255, y0: 0, x1: 256, y1: 3 },
        mode: 'new',
        antialias: false,
      });
      await api.selectionAction({ kind: 'fill', color: [128, 128, 128, 255] });
      return api.selectionAction(
        select
          ? {
              kind: 'shape',
              shape: { kind: 'rectangle', x0: 255.5, y0: 0, x1: 257.5, y1: 3 },
              mode: 'new',
              antialias: true,
            }
          : { kind: 'none' },
      );
    }, select),
  );
}
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(
    backend + ' GPU filters match Rust references at every tile and odd canvas boundary',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const errors = await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).compareFilterCompositors(),
        ),
      );
      expect(errors).toHaveLength(56);
      expect(Math.max(...errors)).toBeLessThanOrEqual(1);
    },
  );
  for (const [filter, at255, at256] of filterCases) {
    test(`${backend} ${filter.kind} clips tile seams and restores exact undo/redo samples`, async ({
      page,
    }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const initial = await fixture(page);
      const before = await exported(page);
      const next = await whenIdle(page, () =>
        page.evaluate(
          (f) => (Reflect.get(window, '__fotoeditor') as EngineApi).applyFilter(f),
          filter,
        ),
      );
      expect(next.history.undo).toBe(initial.history.undo + 1);
      expect(next.canFade).toBe(true);
      const image = await exported(page);
      expect(pixel(image, 254)).toEqual(pixel(before, 254));
      expect(pixel(image, 258)).toEqual(pixel(before, 258));
      expect(pixel(image, 255)).toEqual([at255, at255, at255, 255]);
      expect(pixel(image, 256)).toEqual([at256, at256, at256, 255]);
      await whenIdle(page, () =>
        page.evaluate(
          (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
          initial.history.undo,
        ),
      );
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      await whenIdle(page, () =>
        page.evaluate(
          (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
          next.history.undo,
        ),
      );
      expect((await exported(page)).data.equals(image.data)).toBe(true);
    });
  }
  test(
    backend + ' Fade restores original endpoints, repeats blend settings and persists',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await fixture(page);
      const before = await exported(page);
      await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).applyFilter({
            kind: 'box-blur',
            radius: 1,
          }),
        ),
      );
      const filtered = await exported(page);
      await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).fadeFilter(0, 'normal'),
        ),
      );
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).fadeFilter(1, 'normal'),
        ),
      );
      expect((await exported(page)).data.equals(filtered.data)).toBe(true);
      let quarter: Buffer | undefined;
      for (const opacity of [0.25, 0.75, 0.25]) {
        await whenIdle(page, () =>
          page.evaluate(
            (o) => (Reflect.get(window, '__fotoeditor') as EngineApi).fadeFilter(o, 'multiply'),
            opacity,
          ),
        );
        const image = await exported(page);
        expect(pixel(image, 254)).toEqual(pixel(before, 254));
        if (quarter && opacity === 0.25) expect(image.data.equals(quarter)).toBe(true);
        else if (opacity === 0.25) quarter = image.data;
      }
      const saved = await exported(page);
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const bytes = await api.saveProject();
          await api.openProject(bytes.slice().buffer as ArrayBuffer, 'filters.cgfoto');
        }),
      );
      expect((await exported(page)).data.equals(saved.data)).toBe(true);
      await expect.poll(async () => (await state(page)).canFade).toBe(false);
    },
  );
  test(
    backend + ' refuses invalid locked and over-budget filters and cancels before commit',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const baseline = await fixture(page);
      const before = await exported(page);
      for (const filter of [
        { kind: 'gaussian', radius: 65 },
        { kind: 'median', radius: 6 },
        { kind: 'sharpen', amount: -1 },
      ]) {
        const error = await whenIdle(page, () =>
          page.evaluate(async (f) => {
            try {
              await (Reflect.get(window, '__fotoeditor') as EngineApi).applyFilter(
                f as SpatialFilter,
              );
              return '';
            } catch (e) {
              return String(e);
            }
          }, filter),
        );
        expect(error).toMatch(/radius|amount/);
      }
      const error = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.diagnosticAdjustmentFault('budget');
          try {
            await api.applyFilter({ kind: 'gaussian', radius: 3 });
            return '';
          } catch (e) {
            return String(e);
          }
        }),
      );
      expect(error).toMatch(/budget/);
      const midOperationError = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.diagnosticAdjustmentFault('budget-after-step');
          try {
            await api.applyFilter({ kind: 'gaussian', radius: 3 });
            return '';
          } catch (e) {
            return String(e);
          }
        }),
      );
      expect(midOperationError).toMatch(/budget/);
      await page.evaluate(() => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        void api.diagnosticAdjustmentFault('pause-before-commit').then(() => {
          Reflect.set(
            window,
            'filterPending',
            api.applyFilter({ kind: 'gaussian', radius: 3 }).then(() => '', String),
          );
        });
      });
      await expect(page.getByTestId('operation-status')).toContainText('Filter ready');
      const pausedFrame = await page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).frame(performance.now(), []),
      );
      expect(pausedFrame.presented).toBe(false);
      await page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).cancelJob());
      expect(await page.evaluate(async () => await Reflect.get(window, 'filterPending'))).toMatch(
        /cancelled/,
      );
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      expect((await state(page)).history.undo).toBe(baseline.history.undo);
      const locked = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const canvas = document.querySelector('[data-testid="gpu-canvas"]') as HTMLCanvasElement;
          const s = JSON.parse(canvas.dataset.snapshot!) as DocumentSnapshot;
          const node = s.layers.nodes[0];
          await api.layerAction({
            kind: 'properties',
            target: node.id,
            properties: { ...node.properties, locks: { ...node.properties.locks, pixels: true } },
          });
          try {
            await api.applyFilter({ kind: 'median', radius: 1 });
            return '';
          } catch (e) {
            return String(e);
          }
        }),
      );
      expect(locked).toMatch(/lock/i);
    },
  );
  test(
    backend +
      ' GPU filter preview is temporary, matches committed actual pixels and coalesces UI controls',
    async ({ page }) => {
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      const baseline = await fixture(page, false);
      await whenIdle(page, () =>
        page.evaluate(() =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).interact([
            { kind: 'actual', x: 0, y: 0, amount: 0 },
          ]),
        ),
      );
      const before = await exported(page);
      await page.getByRole('button', { name: 'Show filters section' }).click();
      await page.getByLabel('Filter', { exact: true }).selectOption('box-blur');
      await page.getByLabel('Filter radius (px)', { exact: true }).fill('1');
      await page.getByLabel('Live filter preview', { exact: true }).check();
      await expect(page.getByTestId('filter-preview-status')).toContainText('Filter preview ready');
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      expect((await state(page)).history.undo).toBe(baseline.history.undo);
      const preview = await page.getByTestId('gpu-canvas').screenshot();
      await page.getByRole('button', { name: 'Apply filter', exact: true }).click();
      await expect(page.getByTestId('filter-notice')).toContainText('Filter applied');
      await expect.poll(async () => (await state(page)).refining).toBe(0);
      await page.evaluate(
        () =>
          new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))),
      );
      const final = await page.getByTestId('gpu-canvas').screenshot();
      expect(PNG.sync.read(preview).data.equals(PNG.sync.read(final).data)).toBe(true);
      const committed = await exported(page);
      await page.getByLabel('Filter', { exact: true }).selectOption('gaussian');
      await page.getByLabel('Live filter preview', { exact: true }).check();
      await expect(page.getByTestId('filter-preview-status')).toContainText('Filter preview ready');
      await page.getByLabel('Filter radius (px) slider', { exact: true }).evaluate((e) => {
        const input = e as HTMLInputElement;
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
        for (const v of ['3', '5', '7', '4']) {
          setter.call(input, v);
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
        }
      });
      await expect(page.getByTestId('filter-preview-status')).toHaveAttribute(
        'data-settings',
        JSON.stringify({ kind: 'gaussian', radius: 4 }),
      );
      await page.getByLabel('Live filter preview', { exact: true }).uncheck();
      await expect.poll(async () => (await state(page)).adjustmentPreview?.active).toBe(false);
      expect((await exported(page)).data.equals(committed.data)).toBe(true);
    },
  );
}
