import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { whenIdle } from './idle';

const errors = new WeakMap<Page, string[]>();
test.beforeEach(({ page }) => {
  errors.set(page, []);
  page.on('pageerror', (error) => errors.get(page)!.push(error.message));
});
test.afterEach(({ page }) => expect(errors.get(page)).toEqual([]));

const state = async (page: Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
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
async function idle(page: Page, count?: number) {
  if (count !== undefined)
    await expect.poll(async () => (await state(page)).history.undo).toBe(count);
  await expect.poll(async () => (await state(page)).painting).toBe(false);
  await expect.poll(async () => (await state(page)).operation).toBeUndefined();
}
async function size(page: Page, value: number) {
  await page.getByRole('slider', { name: 'Brush size', exact: true }).evaluate((element, value) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
      element,
      String(value),
    );
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}
async function setup(page: Page, backend: string, tool: string) {
  await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  const prepared = await whenIdle(page, () =>
    page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.newDocument(512, 256, 300);
      await api.selectionAction({ kind: 'fill', color: [160, 120, 90, 255] });
      await api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 252, y0: 124, x1: 261, y1: 133 },
        mode: 'new',
        antialias: false,
      });
      await api.selectionAction({ kind: 'fill', color: [200, 40, 70, 255] });
      const s = await api.selectionAction({ kind: 'none' });
      await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
      return s;
    }),
  );
  await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-snapshot', /"revision":/);
  await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
  await idle(page);
  await expect(page.getByTestId('zoom')).toHaveText('100.0%');
  await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Hard round');
  await page.getByRole('button', { name: tool, exact: true }).click();
  await size(page, 30);
  const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
  const origin = { x: box.x + box.width / 2 - 256, y: box.y + box.height / 2 - 128 };
  if (tool === 'Healing Brush') {
    await page.keyboard.down('Alt');
    await page.mouse.click(origin.x + 192.5, origin.y + 128.5);
    await page.keyboard.up('Alt');
    await expect.poll(async () => (await state(page)).cloneStamp?.source).toEqual([192, 128]);
    await expect(page.getByLabel('Healing source', { exact: true })).toBeVisible();
  }
  return origin;
}

for (const backend of ['WebGPU', 'WebGL2']) {
  for (const tool of ['Healing Brush', 'Spot Healing']) {
    test(`${backend} ${tool} removes a blemish with one command and exact undo/redo`, async ({
      page,
    }) => {
      const o = await setup(page, backend, tool),
        before = await exported(page),
        baseline = (await state(page)).history.undo;
      await page.mouse.click(o.x + 256.5, o.y + 128.5);
      await idle(page, baseline + 1);
      expect((await state(page)).editError).toBeUndefined();
      const after = await exported(page),
        image = PNG.sync.read(after);
      expect(pixel(image, 256, 128)).toEqual([160, 120, 90, 255]);
      expect(pixel(image, 240, 128)).toEqual([160, 120, 90, 255]);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, baseline);
      expect((await exported(page)).equals(before)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await idle(page, baseline + 1);
      expect((await exported(page)).equals(after)).toBe(true);
    });
    test(`${backend} ${tool} clips selection and cancellation preserves pixels and redo`, async ({
      page,
    }) => {
      const o = await setup(page, backend, tool);
      const prepared = await whenIdle(page, () =>
        page.evaluate(async () =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
            kind: 'shape',
            shape: { kind: 'rectangle', x0: 256, y0: 0, x1: 512, y1: 256 },
            mode: 'new',
            antialias: false,
          }),
        ),
      );
      await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
      await idle(page);
      const before = await exported(page),
        baseline = prepared.history.undo;
      await page.mouse.click(o.x + 256.5, o.y + 128.5);
      await idle(page, baseline + 1);
      const after = await exported(page),
        image = PNG.sync.read(after);
      expect(pixel(image, 255, 128)).toEqual([200, 40, 70, 255]);
      expect(pixel(image, 256, 128)).toEqual([160, 120, 90, 255]);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, baseline);
      const paintTime = (await state(page)).paintTime;
      await page.mouse.move(o.x + 256.5, o.y + 128.5);
      await page.mouse.down();
      await expect.poll(async () => (await state(page)).paintTime).toBeGreaterThan(paintTime);
      await page.keyboard.press('Escape');
      await page.mouse.up();
      await idle(page, baseline);
      expect((await state(page)).history.redo).toBe(1);
      expect((await exported(page)).equals(before)).toBe(true);
    });
  }
  test(`${backend} healing refuses locked/mask targets and all-visible sampling heals on a separate raster`, async ({
    page,
  }) => {
    const o = await setup(page, backend, 'Healing Brush');
    const before = await exported(page),
      baseline = (await state(page)).history.undo;
    await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
    await expect
      .poll(async () => (await state(page)).layers.nodes[0].properties.locks.pixels)
      .toBe(true);
    await idle(page);
    await page.mouse.click(o.x + 256.5, o.y + 128.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/locked/);
    expect((await exported(page)).equals(before)).toBe(true);
    await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
    await expect
      .poll(async () => (await state(page)).layers.nodes[0].properties.locks.pixels)
      .toBe(false);
    await idle(page);
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
    await page.mouse.click(o.x + 256.5, o.y + 128.5);
    await idle(page);
    expect((await state(page)).history.undo).toBe(prepared.history.undo);
    await page.getByRole('combobox', { name: 'Healing sample' }).selectOption('all');
    await page.mouse.click(o.x + 256.5, o.y + 128.5);
    await idle(page, prepared.history.undo + 1);
    expect(pixel(PNG.sync.read(await exported(page)), 256, 128)).toEqual([160, 120, 90, 255]);
    const masked = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const canvas = document.querySelector('[data-testid="gpu-canvas"]') as HTMLElement;
        const s = JSON.parse(canvas.dataset.snapshot!) as DocumentSnapshot;
        await api.layerAction({ kind: 'mask-add', target: s.layers.active! });
        return api.selectLayer(s.layers.active!, true);
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBe(masked.revision);
    await expect.poll(async () => (await state(page)).layers.maskTarget).toBe(true);
    await idle(page);
    await page.getByRole('button', { name: 'Spot Healing', exact: true }).click();
    await page.mouse.click(o.x + 256.5, o.y + 128.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/raster pixel target/);
    expect((await state(page)).history.undo).toBe(masked.history.undo);
    expect(baseline).toBeGreaterThan(0);
  });

  test(`${backend} healing jobs cancel after pointer release on Escape or focus loss`, async ({
    page,
  }) => {
    for (const tool of ['Healing Brush', 'Spot Healing']) {
      const o = await setup(page, backend, tool),
        before = await exported(page),
        baseline = (await state(page)).history.undo;
      await page.mouse.click(o.x + 256.5, o.y + 128.5);
      await idle(page, baseline + 1);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, baseline);
      await size(page, 128);
      for (const cancellation of ['Escape', 'blur']) {
        await page.mouse.click(o.x + 256.5, o.y + 128.5);
        await expect.poll(async () => (await state(page)).painting).toBe(true);
        if (cancellation === 'Escape') await page.keyboard.press('Escape');
        else await page.evaluate(() => window.dispatchEvent(new Event('blur')));
        await idle(page, baseline);
        expect((await state(page)).history.redo).toBe(1);
        expect((await exported(page)).equals(before)).toBe(true);
      }
    }
  });

  test(`${backend} reference portrait Healing Brush and Spot Healing remove a real blemish`, async ({
    page,
  }, testInfo) => {
    await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page
      .getByTestId('open-image')
      .setInputFiles(resolve('tests/fixtures/retouch/melissa-diaz-usaf.jpg'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    await whenIdle(page, () =>
      page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).interact([
          { kind: 'actual', x: 0, y: 0, amount: 0 },
          { kind: 'pan', x: 952.5 - 1466.5, y: 1444.5 - 1223.5, amount: 0 },
        ]),
      ),
    );
    await expect(page.getByTestId('zoom')).toHaveText('100.0%');
    await idle(page);
    await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Hard round');
    await size(page, 48);
    const box = (await page.getByTestId('gpu-canvas').boundingBox())!,
      x = box.x + box.width / 2,
      y = box.y + box.height / 2;
    const before = await exported(page),
      base = (await state(page)).history.undo;
    await mkdir('bench/results', { recursive: true });
    const prefix = `bench/results/p9-m2-portrait-${testInfo.project.name}-${backend}`;
    await writeFile(prefix + '-before.png', before);
    const original = PNG.sync.read(before);
    for (const tool of ['Healing Brush', 'Spot Healing']) {
      await page.getByRole('button', { name: tool, exact: true }).click();
      if (tool === 'Healing Brush') {
        await page.keyboard.down('Alt');
        await page.mouse.click(x - 46, y - 43);
        await page.keyboard.up('Alt');
        await expect.poll(async () => (await state(page)).cloneStamp?.source).toEqual([1420, 1180]);
      }
      await page.mouse.click(x, y);
      await idle(page, base + 1);
      expect((await state(page)).editError).toBeUndefined();
      const after = await exported(page),
        image = PNG.sync.read(after);
      expect(pixel(image, 1466, 1223)).not.toEqual(pixel(original, 1466, 1223));
      const center = pixel(image, 1466, 1223),
        prior = pixel(original, 1466, 1223);
      expect(center[0] - center[1]).toBeLessThan(prior[0] - prior[1] - 20);
      expect(pixel(image, 1440, 1223)).toEqual(pixel(original, 1440, 1223));
      await writeFile(prefix + (tool === 'Healing Brush' ? '-healing.png' : '-spot.png'), after);
      await page.screenshot({
        path: prefix + (tool === 'Healing Brush' ? '-healing-ui.png' : '-spot-ui.png'),
      });
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, base);
      expect((await exported(page)).equals(before)).toBe(true);
    }
  });
}
