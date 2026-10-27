import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { whenIdle } from './idle';

const state = async (page: Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
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
const pixel = (png: PNG, x: number, y: number) => [
  ...png.data.subarray((y * png.width + x) * 4, (y * png.width + x) * 4 + 4),
];
async function setup(page: Page, backend: string) {
  await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  const prepared = await whenIdle(page, () =>
    page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.newDocument(512, 256, 300);
      for (const [x0, x1, color] of [
        [0, 80, [200, 40, 80, 255]],
        [80, 160, [20, 200, 40, 255]],
      ] as const) {
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0, y0: 0, x1, y1: 256 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({ kind: 'fill', color: [...color] });
      }
      const s = await api.selectionAction({ kind: 'none' });
      await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
      return s;
    }),
  );
  await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
  await idle(page);
  await expect(page.getByTestId('zoom')).toHaveText('100.0%');
  await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Pixel pencil');
  await page.getByRole('button', { name: 'Clone Stamp', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: 'Aligned source' })).toBeChecked();
  const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
  return { x: box.x + box.width / 2 - 256, y: box.y + box.height / 2 - 128 };
}
async function pick(page: Page, origin: { x: number; y: number }, x = 32, y = 128) {
  await idle(page);
  await page.keyboard.down('Alt');
  await page.mouse.click(origin.x + x + 0.5, origin.y + y + 0.5);
  await page.keyboard.up('Alt');
  await expect.poll(async () => (await state(page)).cloneStamp?.source).toEqual([x, y]);
}
async function idle(page: Page, count?: number) {
  if (count !== undefined)
    await expect.poll(async () => (await state(page)).history.undo).toBe(count);
  await expect.poll(async () => (await state(page)).painting).toBe(false);
  await expect.poll(async () => (await state(page)).operation).toBeUndefined();
}
for (const backend of ['WebGPU', 'WebGL2']) {
  test(`${backend} Clone Stamp picks source, preserves alignment across strokes and restarts non-aligned`, async ({
    page,
  }) => {
    const origin = await setup(page, backend);
    const baseline = (await state(page)).history.undo;
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/Alt-click/);
    expect((await state(page)).history.undo).toBe(baseline);
    await pick(page, origin);
    await expect(page.getByLabel('Clone source', { exact: true })).toBeVisible();
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await idle(page, baseline + 1);
    await page.mouse.click(origin.x + 356.5, origin.y + 128.5);
    await idle(page, baseline + 2);
    await page.getByRole('checkbox', { name: 'Aligned source' }).uncheck();
    await page.mouse.click(origin.x + 456.5, origin.y + 128.5);
    await idle(page, baseline + 3);
    const after = await exported(page);
    expect(pixel(after, 256, 128)).toEqual([200, 40, 80, 255]);
    expect(pixel(after, 356, 128)).toEqual([20, 200, 40, 255]);
    expect(pixel(after, 456, 128)).toEqual([200, 40, 80, 255]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, baseline + 2);
    expect(pixel(await exported(page), 456, 128)).toEqual([0, 0, 0, 0]);
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await idle(page, baseline + 3);
    expect((await exported(page)).data.equals(after.data)).toBe(true);
    await page.evaluate(async () => {
      await (Reflect.get(window, '__fotoeditor') as EngineApi).newDocument(512, 256, 72);
    });
    await expect.poll(async () => (await state(page)).cloneStamp?.source).toBeNull();
  });

  test(`${backend} Clone Stamp all-layer source includes mask and group opacity while current blank layer is a no-op`, async ({
    page,
  }) => {
    const origin = await setup(page, backend);
    const prepared = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        let s = await api.layerAction({
          kind: 'add',
          name: 'Masked source',
          parent: null,
          index: 1,
          group: false,
        });
        const target = s.layers.active!;
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 0, y0: 0, x1: 160, y1: 256 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({ kind: 'fill', color: [20, 200, 40, 255] });
        await api.selectionAction({ kind: 'none' });
        await api.layerAction({ kind: 'mask-add', target });
        await api.selectLayer(target, true);
        await api.selectionAction({ kind: 'fill', color: [128, 128, 128, 255] });
        await api.selectLayer(target, false);
        s = await api.layerAction({ kind: 'group', ids: [target], name: 'Source group' });
        const group = s.layers.nodes.find((n) => n.name === 'Source group')!;
        await api.layerAction({
          kind: 'properties',
          target: group.id,
          properties: { ...group.properties, opacity: 0.5 },
        });
        return api.layerAction({
          kind: 'add',
          name: 'Retouch',
          parent: null,
          index: 2,
          group: false,
        });
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBe(prepared.revision);
    await idle(page);
    const before = await exported(page),
      baseline = (await state(page)).history.undo;
    await pick(page, origin);
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await idle(page);
    expect((await state(page)).history.undo).toBe(baseline);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
    await page.getByRole('combobox', { name: 'Clone sample' }).selectOption('all');
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await idle(page, baseline + 1);
    const after = await exported(page);
    expect(pixel(after, 256, 128)).toEqual(pixel(before, 32, 128));
    expect(pixel(after, 256, 128)).toEqual([155, 80, 70, 255]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, baseline);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
  });

  test(`${backend} Clone Stamp selection clips destination and live cancellation preserves samples and redo`, async ({
    page,
  }) => {
    const origin = await setup(page, backend);
    await pick(page, origin);
    await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Hard round');
    const selected = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        return api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 256, y0: 0, x1: 512, y1: 256 },
          mode: 'new',
          antialias: false,
        });
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBe(selected.revision);
    await idle(page);
    const baseline = selected.history.undo;
    const before = await exported(page);
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await idle(page, baseline + 1);
    const painted = await exported(page);
    expect(pixel(painted, 255, 128)).toEqual(pixel(before, 255, 128));
    expect(pixel(painted, 256, 128)).toEqual([200, 40, 80, 255]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, baseline);
    const paintTime = (await state(page)).paintTime;
    await page.mouse.move(origin.x + 256.5, origin.y + 128.5);
    await page.mouse.down();
    await expect.poll(async () => (await state(page)).paintTime).toBeGreaterThan(paintTime);
    expect((await state(page)).history.undo).toBe(baseline);
    await page.keyboard.press('Escape');
    await page.mouse.up();
    await idle(page, baseline);
    expect((await state(page)).history.redo).toBe(1);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await idle(page, baseline + 1);
    expect((await exported(page)).data.equals(painted.data)).toBe(true);
  });

  test(`${backend} Clone Stamp refuses locked and mask targets without losing samples`, async ({
    page,
  }) => {
    const origin = await setup(page, backend);
    await pick(page, origin);
    const before = await exported(page);
    await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
    await expect
      .poll(async () => (await state(page)).layers.nodes[0].properties.locks.pixels)
      .toBe(true);
    await idle(page);
    const baseline = (await state(page)).history.undo;
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/locked|Unlock/i);
    expect((await state(page)).history.undo).toBe(baseline);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
    await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
    await expect
      .poll(async () => (await state(page)).layers.nodes[0].properties.locks.pixels)
      .toBe(false);
    await idle(page);
    const maskTarget = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.selectionAction({ kind: 'none' });
        const target = s.layers.nodes[0].id;
        await api.layerAction({ kind: 'mask-add', target });
        return api.selectLayer(target, true);
      }),
    );
    await expect.poll(async () => (await state(page)).revision).toBe(maskTarget.revision);
    await expect.poll(async () => (await state(page)).layers.maskTarget).toBe(true);
    await idle(page);
    const maskBaseline = (await state(page)).history.undo;
    await page.mouse.click(origin.x + 256.5, origin.y + 128.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/raster pixel target/);
    expect((await state(page)).history.undo).toBe(maskBaseline);
  });
}
