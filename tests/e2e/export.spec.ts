import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { PNG } from 'pngjs';
import type { EngineApi, ImageExportOptions } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';
const options: ImageExportOptions = {
  format: 'png',
  source: 'full',
  quality: 85,
  lossless: false,
  embedIcc: true,
  matte: [255, 255, 255],
};
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend + ' export dialog previews actual bytes and downloads PNG JPEG and both WebP modes',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(65, 33, 300);
          await api.selectionAction({ kind: 'fill', color: [179, 53, 77, 128] });
        }),
      );
      await expect(page.getByRole('button', { name: 'Export image…', exact: true })).toBeEnabled();
      await page.getByRole('button', { name: 'Export image…', exact: true }).click();
      const dialog = page.getByRole('dialog', { name: 'Export image' });
      await expect(dialog.getByLabel('Embed sRGB ICC profile')).toBeChecked();
      for (const [format, lossless] of [
        ['png', false],
        ['jpeg', false],
        ['webp', false],
        ['webp', true],
      ] as const) {
        await dialog.getByLabel('Export format').selectOption(format);
        if (format === 'webp') await dialog.getByLabel('Lossless WebP').setChecked(lossless);
        await dialog.getByRole('button', { name: 'Preview encoded size' }).click();
        await expect(dialog.getByTestId('export-size')).toHaveText(/Encoded size: [\d,]+ bytes/);
        const size = Number(
          (await dialog.getByTestId('export-size').innerText()).replace(/\D/g, ''),
        );
        const download = page.waitForEvent('download');
        await dialog.getByRole('button', { name: 'Download image' }).click();
        const downloaded = await download;
        const bytes = await readFile((await downloaded.path())!);
        expect(bytes.length).toBe(size);
        expect(downloaded.suggestedFilename()).toMatch(
          new RegExp('\\.' + (format === 'jpeg' ? 'jpg' : format) + '$'),
        );
        if (format === 'png') {
          const decoded = PNG.sync.read(bytes);
          expect([decoded.width, decoded.height]).toEqual([65, 33]);
          expect(Array.from(decoded.data.subarray(0, 4))).toEqual([179, 53, 77, 128]);
          expect(bytes.includes(Buffer.from('iCCP'))).toBe(true);
        } else
          expect(bytes.includes(Buffer.from(format === 'jpeg' ? 'ICC_PROFILE' : 'ICCP'))).toBe(
            true,
          );
      }
      await dialog.getByRole('button', { name: 'Close', exact: true }).click();
      await expect(dialog).not.toBeVisible();
    },
  );
  test(
    backend + ' selection and isolated raster exports preserve crop and document undo redo',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      const result = await whenIdle(page, () =>
        page.evaluate(async (options) => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(6, 4, 72);
          await api.selectionAction({ kind: 'fill', color: [201, 33, 77, 255] });
          await api.selectionAction({
            kind: 'shape',
            shape: { kind: 'rectangle', x0: 2, y0: 1, x1: 4, y1: 3 },
            mode: 'new',
            antialias: false,
          });
          await api.geometryMetadata({ kind: 'crop', rect: { x: 1, y: 1, width: 4, height: 2 } });
          const before = Array.from(await api.exportPng());
          const selected = Array.from(
            new Uint8Array(
              await (await api.exportImage({ ...options, source: 'selection' })).arrayBuffer(),
            ),
          );
          const isolated = Array.from(
            new Uint8Array(
              await (await api.exportImage({ ...options, source: 'layer' })).arrayBuffer(),
            ),
          );
          const after = Array.from(await api.exportPng());
          const waitWidth = async (width: number) => {
            const deadline = performance.now() + 30_000;
            while (performance.now() < deadline) {
              const state = JSON.parse(
                window.document
                  .querySelector('[data-testid="gpu-canvas"]')!
                  .getAttribute('data-snapshot')!,
              );
              if (state?.document?.width === width) return state.document;
              await new Promise((resolve) => setTimeout(resolve, 5));
            }
            throw new Error('Canvas width did not update');
          };
          await api.interact([{ kind: 'undo', x: 0, y: 0, amount: 0 }]);
          const undo = await waitWidth(6);
          await api.interact([{ kind: 'redo', x: 0, y: 0, amount: 0 }]);
          const redo = await waitWidth(4);
          return { before, after, selected, isolated, undo, redo };
        }, options),
      );
      expect(result.after).toEqual(result.before);
      const isolated = PNG.sync.read(Buffer.from(result.isolated));
      expect([isolated.width, isolated.height]).toEqual([4, 2]);
      expect(isolated.data).toEqual(PNG.sync.read(Buffer.from(result.before)).data);
      const selected = PNG.sync.read(Buffer.from(result.selected));
      expect(Array.from(selected.data.subarray(0, 8))).toEqual([201, 33, 77, 0, 201, 33, 77, 255]);
      expect(result.undo?.width).toBe(6);
      expect(result.redo?.width).toBe(4);
    },
  );
  test(
    backend + ' codec heap refusal and cancellation preserve exact pixels and redo',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      const result = await whenIdle(page, () =>
        page.evaluate(async (options) => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(1024, 1024, 72);
          await api.selectionAction({ kind: 'fill', color: [201, 33, 77, 128] });
          await api.selectionAction({ kind: 'fill', color: [11, 93, 57, 255] });
          await api.interact([{ kind: 'undo', x: 0, y: 0, amount: 0 }]);
          const before = Array.from(await api.exportPng());
          await api.diagnosticExportBudget();
          let refusal = '';
          try {
            await api.exportImage({ ...options, format: 'webp' });
          } catch (error) {
            refusal = String(error);
          }
          const pending = api.exportImage({ ...options, format: 'webp' }).then(
            () => 'unexpected success',
            (error) => String(error),
          );
          const deadline = performance.now() + 30_000;
          while (performance.now() < deadline) {
            const snapshot = JSON.parse(
              window.document
                .querySelector('[data-testid="gpu-canvas"]')!
                .getAttribute('data-snapshot')!,
            );
            if (/Encoding WebP/.test(snapshot?.operation ?? '')) break;
            await new Promise((resolve) => setTimeout(resolve, 5));
          }
          const cancelled = await api.cancelJob();
          const cancellation = await pending;
          const after = Array.from(await api.exportPng());
          await api.interact([{ kind: 'redo', x: 0, y: 0, amount: 0 }]);
          const deadlineRedo = performance.now() + 30_000;
          while (performance.now() < deadlineRedo) {
            const state = JSON.parse(
              window.document
                .querySelector('[data-testid="gpu-canvas"]')!
                .getAttribute('data-snapshot')!,
            );
            if (state?.history.undo === 2)
              return { before, after, refusal, cancelled, cancellation, redo: state.history.undo };
            await new Promise((resolve) => setTimeout(resolve, 5));
          }
          throw new Error('Redo did not update');
        }, options),
      );
      expect(result.refusal).toMatch(/Not enough memory/);
      expect(result.cancelled).toBe(true);
      expect(result.cancellation).toMatch(/cancelled/);
      expect(result.after).toEqual(result.before);
      expect(result.redo).toBe(2);
    },
  );
}
