import { test, expect } from '@playwright/test';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(`${backend} edits tone arrays and gradient stops through adjustment controls`, async ({
    page,
  }, info) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    const initial = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 64, 72);
        return api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
      }),
    );
    const state = async () => {
      const canvas = page.getByTestId('gpu-canvas');
      await expect(canvas).toHaveAttribute('data-snapshot', /"view":/);
      return JSON.parse((await canvas.getAttribute('data-snapshot'))!) as DocumentSnapshot;
    };
    await expect.poll(async () => (await state()).revision).toBe(initial.revision);
    await page.getByRole('button', { name: 'Show adjustments section' }).click();
    await page.getByLabel('Adjustment', { exact: true }).selectOption('color-balance');
    await page.getByLabel('midtones Cyan / Red (%)', { exact: true }).fill('40');
    await page.getByRole('checkbox', { name: 'Preserve luminosity' }).uncheck();
    await page.getByRole('checkbox', { name: 'Create adjustment layer above selection' }).check();
    await page.getByRole('button', { name: 'Add adjustment layer', exact: true }).click();
    await expect
      .poll(async () => (await state()).layers.nodes[1]?.adjustment)
      .toEqual({
        kind: 'color-balance',
        shadows: [0, 0, 0],
        midtones: [0.4, 0, 0],
        highlights: [0, 0, 0],
        preserveLuminosity: false,
      });
    const layer = (await state()).layers.nodes[1].id;
    await whenIdle(page, () =>
      page.evaluate(
        (id) => (Reflect.get(window, '__fotoeditor') as EngineApi).selectLayer(id),
        layer,
      ),
    );
    await page.getByRole('checkbox', { name: 'Create adjustment layer above selection' }).uncheck();
    await expect(
      page.getByRole('button', { name: 'Save adjustment layer', exact: true }),
    ).toBeEnabled();
    await page.getByLabel('Adjustment', { exact: true }).selectOption('gradient-map');
    await page.getByRole('button', { name: 'Add gradient stop', exact: true }).click();
    await page.getByLabel('Gradient red 1', { exact: true }).fill('100');
    await page.getByLabel('Gradient position 2', { exact: true }).fill('40');
    await page.getByLabel('Gradient blue 2', { exact: true }).fill('80');
    await page.getByRole('checkbox', { name: 'Live viewport preview' }).check();
    await expect(page.getByTestId('adjustment-preview-status')).toContainText('Preview ready');
    await page.screenshot({ path: info.outputPath('gradient-controls.png') });
    await page.getByRole('button', { name: 'Save adjustment layer', exact: true }).click();
    await expect
      .poll(async () => (await state()).layers.nodes[1].adjustment)
      .toEqual({
        kind: 'gradient-map',
        stops: [
          [0, 1, 0, 0],
          [0.4, 0.5, 0.5, 0.8],
          [1, 1, 1, 1],
        ],
      });
    await page.getByRole('button', { name: 'Remove gradient stop 2' }).click();
    await page.getByRole('button', { name: 'Save adjustment layer', exact: true }).click();
    await expect
      .poll(async () => (await state()).layers.nodes[1].adjustment)
      .toEqual({
        kind: 'gradient-map',
        stops: [
          [0, 1, 0, 0],
          [1, 1, 1, 1],
        ],
      });
  });
  test(`${backend} applies seeded monochrome Gaussian noise through filter controls`, async ({
    page,
  }, info) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 64, 72);
        return api.selectionAction({ kind: 'fill', color: [128, 128, 128, 255] });
      }),
    );
    await page.getByRole('button', { name: 'Show filters section' }).click();
    await page.getByLabel('Filter', { exact: true }).selectOption('add-noise');
    await page.getByLabel('Noise amount', { exact: true }).fill('0.4');
    await page.getByLabel('Noise seed', { exact: true }).fill('4294967295');
    await page.getByRole('checkbox', { name: 'Monochrome noise' }).check();
    await page.getByLabel('Noise distribution', { exact: true }).selectOption('gaussian');
    await page.getByRole('checkbox', { name: 'Live filter preview' }).check();
    await expect(page.getByTestId('filter-preview-status')).toContainText('Filter preview ready');
    const settings = JSON.parse(
      (await page.getByTestId('filter-preview-status').getAttribute('data-settings'))!,
    );
    expect(settings).toEqual({
      kind: 'add-noise',
      amount: 0.4,
      seed: 4294967295,
      monochrome: true,
      distribution: 'gaussian',
    });
    await page.screenshot({ path: info.outputPath('noise-controls.png') });
    await page.getByRole('button', { name: 'Apply filter', exact: true }).click();
    await expect(page.getByTestId('filter-notice')).toContainText('Filter applied');
    await expect(page.getByRole('button', { name: 'Apply Fade', exact: true })).toBeEnabled();
  });
}
