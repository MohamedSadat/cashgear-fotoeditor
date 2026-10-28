import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { whenIdle } from './idle';

const state = async (page: Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
async function idle(page: Page, count?: number) {
  await expect
    .poll(async () => {
      const s = await state(page);
      return (
        !s.operation &&
        !s.painting &&
        !s.selection.busy &&
        (count === undefined || s.history.undo === count)
      );
    })
    .toBe(true);
}
const exported = async (page: Page) =>
  Buffer.from(
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const bytes = await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng();
        return new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result).split(',')[1]);
          reader.onerror = () => reject(reader.error);
          reader.readAsDataURL(new Blob([bytes as Uint8Array<ArrayBuffer>]));
        });
      }),
    ),
    'base64',
  );
const pixel = (png: PNG, x: number, y: number) => [
  ...png.data.subarray((y * png.width + x) * 4, (y * png.width + x) * 4 + 4),
];
async function setup(page: Page, backend: string) {
  await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-snapshot', /"revision":/);
  const prepared = await whenIdle(page, () =>
    page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.newDocument(512, 128, 300);
      await api.selectionAction({ kind: 'fill', color: [160, 120, 90, 255] });
      await api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 247, y0: 55, x1: 266, y1: 74 },
        mode: 'new',
        antialias: false,
      });
      await api.selectionAction({ kind: 'fill', color: [220, 20, 80, 255] });
      return api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 244, y0: 52, x1: 269, y1: 77 },
        mode: 'new',
        antialias: false,
      });
    }),
  );
  await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
  await idle(page);
  return prepared;
}
async function fill(page: Page) {
  await page
    .locator('summary')
    .filter({ hasText: /^Edit$/ })
    .click();
  await page.getByRole('button', { name: 'Content-aware fill', exact: true }).click();
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(({ page }) => {
  errors.set(page, []);
  page.on('pageerror', (e) => errors.get(page)!.push(e.message));
});
test.afterEach(({ page }) => expect(errors.get(page)).toEqual([]));
for (const backend of ['WebGPU', 'WebGL2']) {
  test(`${backend} content-aware fill uses Edit controls, clips selection and has exact command undo/redo`, async ({
    page,
  }) => {
    const prepared = await setup(page, backend),
      before = await exported(page);
    await fill(page);
    await idle(page, prepared.history.undo + 1);
    const timing = (await state(page)).selection.timing!;
    expect(timing.cpuPeakBytes).toBeGreaterThan(
      prepared.memory.document + prepared.memory.history + prepared.memory.temporary,
    );
    expect(timing.cpuPeakBytes).toBeLessThanOrEqual(768 * 1024 ** 2);
    expect(timing.gpuPeakBytes).toBeGreaterThan(0);
    expect(timing.gpuPeakBytes).toBeLessThanOrEqual(128 * 1024 ** 2);
    expect(timing.maxStepMs).toBeGreaterThanOrEqual(0);
    const after = await exported(page),
      image = PNG.sync.read(after);
    expect(pixel(image, 256, 64)).toEqual([160, 120, 90, 255]);
    expect(pixel(image, 243, 64)).toEqual(pixel(PNG.sync.read(before), 243, 64));
    expect((await state(page)).history.labels.at(-1)).toBe('Content-aware fill');
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, prepared.history.undo);
    expect((await exported(page)).equals(before)).toBe(true);
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await idle(page, prepared.history.undo + 1);
    expect((await exported(page)).equals(after)).toBe(true);
  });
  test(`${backend} content-aware all-visible fills a separate raster and refuses locked or mask targets`, async ({
    page,
  }) => {
    await setup(page, backend);
    const prepared = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        return api.layerAction({
          kind: 'add',
          name: 'Retouch',
          parent: null,
          index: 1,
          group: false,
        });
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
    await idle(page);
    const before = await exported(page);
    await page.getByRole('checkbox', { name: 'All layers', exact: true }).check();
    await fill(page);
    await idle(page, prepared.history.undo + 1);
    expect(pixel(PNG.sync.read(await exported(page)), 256, 64)).toEqual([160, 120, 90, 255]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, prepared.history.undo);
    expect((await exported(page)).equals(before)).toBe(true);
    const locked = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = JSON.parse(
            (document.querySelector('[data-testid="gpu-canvas"]') as HTMLElement).dataset.snapshot!,
          ) as DocumentSnapshot,
          node = s.layers.nodes.find((n) => n.id === s.layers.active)!;
        return api.layerAction({
          kind: 'properties',
          target: node.id,
          properties: { ...node.properties, locks: { ...node.properties.locks, pixels: true } },
        });
      }),
    );
    const refusal = await page.evaluate(async () => {
      try {
        await (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
          kind: 'content-aware',
          allLayers: true,
        });
        return '';
      } catch (e) {
        return String(e);
      }
    });
    expect(refusal).toMatch(/locked/);
    await idle(page, locked.history.undo);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = JSON.parse(
          (document.querySelector('[data-testid="gpu-canvas"]') as HTMLElement).dataset.snapshot!,
        ) as DocumentSnapshot;
        const node = s.layers.nodes.find((n) => n.id === s.layers.active)!;
        await api.layerAction({
          kind: 'properties',
          target: node.id,
          properties: { ...node.properties, locks: { ...node.properties.locks, pixels: false } },
        });
        await api.layerAction({ kind: 'mask-add', target: node.id });
        await api.selectLayer(node.id, true);
      }),
    );
    const mask = await page.evaluate(async () => {
      try {
        await (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
          kind: 'content-aware',
          allLayers: true,
        });
        return '';
      } catch (e) {
        return String(e);
      }
    });
    expect(mask).toMatch(/layer pixels/);
    expect((await exported(page)).equals(before)).toBe(true);
  });
  test(`${backend} content-aware cancellation and budget refusal preserve pixels history and redo`, async ({
    page,
  }) => {
    const prepared = await setup(page, backend);
    await fill(page);
    await idle(page, prepared.history.undo + 1);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, prepared.history.undo);
    const before = await exported(page);
    const refusal = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.diagnosticSelectionFault('budget');
      try {
        await api.selectionAction({ kind: 'content-aware', allLayers: false });
        return '';
      } catch (e) {
        return String(e);
      }
    });
    expect(refusal).toMatch(/budget|768 MiB/);
    for (const cancel of ['Escape', 'button']) {
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.diagnosticSelectionFault('pause-before-commit');
        Reflect.set(
          window,
          'pendingFill',
          api.selectionAction({ kind: 'content-aware', allLayers: false }).then(
            () => '',
            (e) => String(e),
          ),
        );
      });
      await expect(page.getByTestId('operation-status')).toHaveText(/ready to commit/);
      if (cancel === 'Escape') await page.keyboard.press('Escape');
      else await page.getByRole('button', { name: 'Cancel operation', exact: true }).click();
      const result = await page.evaluate(async () => await Reflect.get(window, 'pendingFill'));
      expect(result).toMatch(/cancel/i);
      await idle(page, prepared.history.undo);
      expect((await state(page)).history.redo).toBe(1);
      expect((await exported(page)).equals(before)).toBe(true);
    }
  });
  test(`${backend} content-aware fill cancels during processing before candidate completion`, async ({
    page,
  }) => {
    await setup(page, backend);
    const prepared = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 80, y0: 8, x1: 432, y1: 120 },
          mode: 'new',
          antialias: false,
        });
        await api.resolution(301, 999);
        await api.historyJump(s.history.undo);
        return s;
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBeGreaterThan(prepared.revision);
    await idle(page, prepared.history.undo);
    const before = await exported(page);
    await fill(page);
    await expect(page.getByTestId('operation-status')).toContainText('Content-aware fill…');
    await page.getByRole('button', { name: 'Cancel operation', exact: true }).click();
    await idle(page, prepared.history.undo);
    expect((await state(page)).history.redo).toBe(1);
    expect((await exported(page)).equals(before)).toBe(true);
  });
  test(`${backend} content-aware reference photograph removes a real blemish with exact outside samples`, async ({
    page,
  }, info) => {
    test.setTimeout(180000);
    await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page
      .getByTestId('open-image')
      .setInputFiles(resolve('tests/fixtures/retouch/melissa-diaz-usaf.jpg'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    const prepared = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.interact([
          { kind: 'actual', x: 0, y: 0, amount: 0 },
          { kind: 'pan', x: 952.5 - 1466.5, y: 1444.5 - 1223.5, amount: 0 },
        ]);
        return api.selectionAction({
          kind: 'shape',
          shape: { kind: 'ellipse', x0: 1450, y0: 1205, x1: 1483, y1: 1242 },
          mode: 'new',
          antialias: true,
        });
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
    await idle(page);
    const before = await exported(page);
    await fill(page);
    await idle(page, prepared.history.undo + 1);
    const after = await exported(page),
      a = PNG.sync.read(after),
      b = PNG.sync.read(before);
    expect(pixel(a, 1466, 1223)).not.toEqual(pixel(b, 1466, 1223));
    expect(pixel(a, 1466, 1223)[0] - pixel(a, 1466, 1223)[1]).toBeLessThan(
      pixel(b, 1466, 1223)[0] - pixel(b, 1466, 1223)[1] - 20,
    );
    for (let y = 0; y < a.height; y++) {
      if (y < 1205 || y >= 1242)
        expect(
          a.data
            .subarray(y * a.width * 4, (y + 1) * a.width * 4)
            .equals(b.data.subarray(y * b.width * 4, (y + 1) * b.width * 4)),
        ).toBe(true);
      else
        for (const [x0, x1] of [
          [0, 1450],
          [1483, a.width],
        ])
          expect(
            a.data
              .subarray((y * a.width + x0) * 4, (y * a.width + x1) * 4)
              .equals(b.data.subarray((y * b.width + x0) * 4, (y * b.width + x1) * 4)),
          ).toBe(true);
    }
    await mkdir('bench/results', { recursive: true });
    const prefix = `bench/results/p9-m3-portrait-${info.project.name}-${backend}`;
    await writeFile(prefix + '-before.png', before);
    await writeFile(prefix + '-after.png', after);
    await page.screenshot({ path: prefix + '-ui.png' });
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, prepared.history.undo);
    expect((await exported(page)).equals(before)).toBe(true);
  });
}
