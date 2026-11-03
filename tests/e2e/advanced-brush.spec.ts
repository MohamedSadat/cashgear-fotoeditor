import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { whenIdle } from './idle';
import { defaults } from '../../apps/web/src/brush-state';
const state = async (p: Page) =>
  JSON.parse(
    (await p.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
async function idle(p: Page, count?: number) {
  await expect
    .poll(async () => {
      const s = await state(p);
      return (
        !s.painting &&
        !s.operation &&
        !s.selection.busy &&
        (count === undefined || s.history.undo === count)
      );
    })
    .toBe(true);
}
async function exported(p: Page) {
  return PNG.sync.read(
    Buffer.from(
      await whenIdle(p, () =>
        p.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      ),
    ),
  );
}
const pixel = (p: PNG, x: number, y: number) => [
  ...p.data.subarray((y * p.width + x) * 4, (y * p.width + x) * 4 + 4),
];
async function range(p: Page, name: string, v: number) {
  await p.getByRole('slider', { name, exact: true }).evaluate((el, v) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(el, String(v));
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }, v);
}
async function setup(p: Page, backend: string) {
  await p.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
  await expect(p.getByTestId('engine-status')).toHaveText('Engine ready');
  await expect(p.getByTestId('gpu-canvas')).toHaveAttribute('data-snapshot', /"revision":/);
  await whenIdle(p, () =>
    p.evaluate(async () => {
      const a = Reflect.get(window, '__fotoeditor') as EngineApi;
      await a.newDocument(512, 256, 300);
      await a.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
    }),
  );
  await expect.poll(async () => (await state(p)).document?.width).toBe(512);
  await expect(p.getByTestId('zoom')).toHaveText('100.0%');
  await idle(p, 0);
  await p.getByRole('textbox', { name: 'Color hex' }).fill('#235dce');
  await p.locator('.advanced-brush summary').click();
  const b = (await p.getByTestId('gpu-canvas').boundingBox())!;
  return { x: b.x + b.width / 2 - 256, y: b.y + b.height / 2 - 128 };
}
async function load(p: Page, kind: 'tip' | 'texture', file = 'brush-asymmetric.png') {
  await p
    .getByLabel(kind === 'tip' ? 'Image tip file' : 'Brush texture file', { exact: true })
    .setInputFiles(resolve('bench/results/fixtures/' + file));
  await expect(p.getByRole('status', { name: 'Brush library status' })).toContainText(
    kind === 'tip' ? 'Image tip loaded' : 'Texture loaded',
  );
}
async function undo(p: Page, count: number) {
  await p.getByRole('button', { name: 'Undo', exact: true }).click();
  await idle(p, count);
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(({ page }) => {
  errors.set(page, []);
  page.on('pageerror', (e) => errors.get(page)!.push(e.message));
});
test.afterEach(({ page }) => expect(errors.get(page)).toEqual([]));
for (const backend of ['WebGPU', 'WebGL2']) {
  for (const mode of ['vertical', 'horizontal', 'both', 'radial'])
    test(`${backend} coincident ${mode} image orientations union coverage without extra flow`, async ({
      page,
    }, info) => {
      const o = await setup(page, backend);
      await load(page, 'tip');
      await load(page, 'texture');
      await range(page, 'Brush size', 40);
      await range(page, 'Brush flow', 0.3);
      await range(page, 'Brush opacity', 0.5);
      await range(page, 'Texture scale', 7);
      const before = await exported(page);
      await page.mouse.click(o.x + 256, o.y + 128);
      await idle(page, 1);
      const reference = await exported(page);
      await undo(page, 0);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      await page.getByRole('combobox', { name: 'Painting symmetry' }).selectOption(mode);
      if (mode === 'radial')
        await page.getByRole('spinbutton', { name: 'Radial sectors' }).fill('4');
      await page.mouse.click(o.x + 256, o.y + 128);
      await idle(page, 1);
      const actual = await exported(page);
      let count = 0;
      for (let y = 100; y < 156; y++)
        for (let x = 228; x < 284; x++) {
          const a = (x: number, y: number) => pixel(reference, x, y)[3];
          const expected =
            mode === 'vertical'
              ? Math.max(a(x, y), a(511 - x, y))
              : mode === 'horizontal'
                ? Math.max(a(x, y), a(x, 255 - y))
                : mode === 'both'
                  ? Math.max(a(x, y), a(511 - x, y), a(x, 255 - y), a(511 - x, 255 - y))
                  : Math.max(
                      a(x, y),
                      a(383 - y, x - 128),
                      a(511 - x, 255 - y),
                      a(128 + y, 383 - x),
                    );
          expect(pixel(actual, x, y)[3], `${mode} ${x},${y}`).toBe(expected);
          if (expected) count++;
        }
      expect(count).toBeGreaterThan(80);
      const prefix = `bench/results/${process.env.E2E_EVIDENCE_PREFIX ?? 'p9-m5'}-coincident-${info.project.name}-${backend}-${mode}`;
      await mkdir('bench/results', { recursive: true });
      await writeFile(prefix + '.png', PNG.sync.write(actual));
      await undo(page, 0);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await idle(page, 1);
      expect((await exported(page)).data.equals(actual.data)).toBe(true);
    });
  test(`${backend} queued image payload refusal preserves pixels redo and later input admission`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    await undo(page, 0);
    const before = await exported(page);
    await page.evaluate(async (defaults) => {
      const a = Reflect.get(window, '__fotoeditor') as EngineApi;
      const asset = { width: 512, height: 512, coverage: 'ff'.repeat(512 * 512) };
      await a.interact(
        Array.from({ length: 8 }, () => ({
          kind: 'stroke-begin' as const,
          x: 0,
          y: 0,
          amount: 0,
          brush: {
            ...defaults,
            tipImage: asset,
            texture: asset,
            color: [0, 0, 0, 255] as [number, number, number, number],
            eraser: false,
          },
        })),
      );
    }, defaults);
    await expect.poll(async () => (await state(page)).editError).toMatch(/16 MiB/);
    await idle(page, 0);
    expect((await state(page)).history.redo).toBe(1);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    expect((await state(page)).editError).toBeUndefined();
  });
  for (const mode of ['vertical', 'horizontal', 'both', 'radial'])
    test(`${backend} image-tip ${mode} symmetry transforms asymmetric coverage with exact undo/redo`, async ({
      page,
    }, info) => {
      const o = await setup(page, backend);
      await load(page, 'tip');
      await range(page, 'Brush size', 36);
      await page.getByRole('combobox', { name: 'Painting symmetry' }).selectOption(mode);
      if (mode === 'radial')
        await page.getByRole('spinbutton', { name: 'Radial sectors' }).fill('4');
      await expect(page.locator('.symmetry-axis')).toHaveCount(
        mode === 'both' ? 2 : mode === 'radial' ? 4 : 1,
      );
      const before = await exported(page);
      await page.mouse.click(o.x + 176, o.y + 68);
      await idle(page, 1);
      const after = await exported(page);
      expect(after.data.equals(before.data)).toBe(false);
      let count = 0;
      for (let y = 45; y < 91; y++)
        for (let x = 153; x < 199; x++) {
          const a = pixel(after, x, y);
          if (a[3]) count++;
          if (['vertical', 'both'].includes(mode)) expect(pixel(after, 511 - x, y)).toEqual(a);
          if (['horizontal', 'both'].includes(mode)) expect(pixel(after, x, 255 - y)).toEqual(a);
          if (mode === 'both') expect(pixel(after, 511 - x, 255 - y)).toEqual(a);
          if (mode === 'radial') {
            expect(pixel(after, 383 - y, x - 128)).toEqual(a);
            expect(pixel(after, 511 - x, 255 - y)).toEqual(a);
            expect(pixel(after, 128 + y, 383 - x)).toEqual(a);
          }
        }
      expect(count).toBeGreaterThan(60);
      const prefix = `bench/results/${process.env.E2E_EVIDENCE_PREFIX ?? 'p9-m5'}-image-${info.project.name}-${backend}-${mode}`;
      await mkdir('bench/results', { recursive: true });
      await writeFile(prefix + '.png', PNG.sync.write(after));
      await page.screenshot({ path: prefix + '-app.png' });
      await undo(page, 0);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await idle(page, 1);
      expect((await exported(page)).data.equals(after.data)).toBe(true);
    });
  test(`${backend} image texture and scatter size angle opacity jitter repeat by seed through real controls`, async ({
    page,
  }, info) => {
    const o = await setup(page, backend);
    await load(page, 'tip', 'brush-leaf.png');
    await load(page, 'texture', 'brush-texture.png');
    await range(page, 'Brush size', 120);
    await range(page, 'Scatter', 0.6);
    await range(page, 'Size jitter', 0.4);
    await range(page, 'Angle jitter', 0.8);
    await range(page, 'Opacity jitter', 0.5);
    await range(page, 'Texture depth', 0.8);
    await range(page, 'Texture scale', 24);
    await page.getByRole('spinbutton', { name: 'Brush random seed' }).fill('71');
    const before = await exported(page);
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    const after = await exported(page);
    expect(after.data.equals(before.data)).toBe(false);
    await undo(page, 0);
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    expect((await exported(page)).data.equals(after.data)).toBe(true);
    await undo(page, 0);
    await page.getByRole('spinbutton', { name: 'Brush random seed' }).fill('72');
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    expect((await exported(page)).data.equals(after.data)).toBe(false);
    const prefix = `bench/results/${process.env.E2E_EVIDENCE_PREFIX ?? 'p9-m5'}-texture-${info.project.name}-${backend}`;
    await writeFile(prefix + '.png', PNG.sync.write(after));
    await page.screenshot({ path: prefix + '-app.png' });
  });
  test(`${backend} CashGear brush library exports imports persists and rejects invalid candidates atomically`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await load(page, 'tip');
    await load(page, 'texture');
    await range(page, 'Scatter', 0.3);
    await page.getByRole('textbox', { name: 'Brush preset name' }).fill('Travel ink');
    await page.getByRole('button', { name: 'Save brush', exact: true }).click();
    await expect(page.getByRole('status', { name: 'Brush library status' })).toHaveText(
      'Brush saved.',
    );
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    await undo(page, 0);
    const before = await exported(page);
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Export CashGear brushes' }).click();
    const text = await readFile((await (await download).path())!, 'utf8');
    const library = JSON.parse(text);
    expect(library.format).toBe('cashgear-brush-presets');
    expect(library.presets[0].settings.tipImage.coverage).toBe('ff0000ffff00');
    expect(library.presets[0].settings).not.toHaveProperty('symmetry');
    for (const invalid of [
      JSON.stringify({ ...library, version: 2 }),
      JSON.stringify({ ...library, presets: [...library.presets, ...library.presets] }),
      '{',
    ]) {
      await page.getByLabel('Import CashGear brushes', { exact: true }).setInputFiles({
        name: 'bad.cgbrush',
        mimeType: 'application/json',
        buffer: Buffer.from(invalid),
      });
      await expect(page.getByRole('status', { name: 'Brush library status' })).not.toHaveText(
        'Brush library imported.',
      );
      expect((await state(page)).history.redo).toBe(1);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      expect(
        await page.evaluate(
          () => JSON.parse(localStorage.getItem('cashgear-brush-presets-v1')!).presets.length,
        ),
      ).toBe(1);
    }
    await page.getByRole('button', { name: 'Delete brush', exact: true }).click();
    await page.getByLabel('Import CashGear brushes', { exact: true }).setInputFiles({
      name: 'travel.cgbrush',
      mimeType: 'application/json',
      buffer: Buffer.from(text),
    });
    await expect(page.getByRole('status', { name: 'Brush library status' })).toHaveText(
      'Brush library imported.',
    );
    await page.reload();
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Travel ink');
    await page.locator('.advanced-brush summary').click();
    await expect(page.getByRole('slider', { name: 'Scatter', exact: true })).toHaveValue('0.3');
    await expect(page.getByRole('button', { name: 'Use round tip', exact: true })).toBeEnabled();
    await expect(page.getByRole('button', { name: 'Remove texture', exact: true })).toBeEnabled();
  });
  test(`${backend} invalid oversized and corrupt image assets retain pixels presets and redo`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await load(page, 'tip');
    await page.getByRole('textbox', { name: 'Brush preset name' }).fill('Keep');
    await page.getByRole('button', { name: 'Save brush', exact: true }).click();
    await page.mouse.click(o.x + 256, o.y + 128);
    await idle(page, 1);
    await undo(page, 0);
    const before = await exported(page);
    await page
      .getByLabel('Image tip file', { exact: true })
      .setInputFiles(resolve('bench/results/fixtures/brush-oversized.png'));
    await expect(page.getByRole('status', { name: 'Brush library status' })).not.toContainText(
      'loaded',
    );
    await page
      .getByLabel('Image tip file', { exact: true })
      .setInputFiles({ name: 'broken.png', mimeType: 'image/png', buffer: Buffer.from('bad') });
    await expect(page.getByRole('status', { name: 'Brush library status' })).toContainText(
      'PNG, JPEG or WebP',
    );
    await page.getByLabel('Image tip file', { exact: true }).setInputFiles({
      name: 'large.png',
      mimeType: 'image/png',
      buffer: Buffer.alloc(4 * 1024 * 1024 + 1),
    });
    await expect(page.getByRole('status', { name: 'Brush library status' })).toContainText('4 MiB');
    expect((await exported(page)).data.equals(before.data)).toBe(true);
    expect((await state(page)).history.redo).toBe(1);
    await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Keep');
    await expect(page.getByRole('button', { name: 'Use round tip', exact: true })).toBeEnabled();
    for (const name of ['brush-texture.jpg', 'brush-texture.webp'])
      await load(page, 'texture', name);
  });
  test(`${backend} symmetric painting clips selection supports masks and Quick Mask with exact undo`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await range(page, 'Brush size', 20);
    await page.getByRole('combobox', { name: 'Painting symmetry' }).selectOption('vertical');
    const selected = await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        return a.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 0, y0: 0, x1: 256, y1: 256 },
          mode: 'new',
          antialias: false,
        });
      }),
    );
    await idle(page, selected.history.undo);
    await page.mouse.click(o.x + 176, o.y + 128);
    await idle(page, selected.history.undo + 1);
    const selectedImage = await exported(page);
    expect(pixel(selectedImage, 176, 128)[3]).toBe(255);
    expect(pixel(selectedImage, 335, 128)).toEqual([0, 0, 0, 0]);
    const masked = await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        const blank = await a.newDocument(512, 256, 300);
        await a.selectionAction({ kind: 'fill', color: [210, 50, 40, 255] });
        const id = blank.layers.nodes[0].id;
        await a.layerAction({ kind: 'mask-add', target: id });
        const s = await a.selectLayer(id, true);
        await a.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
        return s;
      }),
    );
    await idle(page, masked.history.undo);
    await expect(page.getByTestId('zoom')).toHaveText('100.0%');
    await page.getByRole('textbox', { name: 'Color hex' }).fill('#000000');
    const maskBefore = await exported(page);
    await page.mouse.click(o.x + 176, o.y + 128);
    await idle(page, masked.history.undo + 1);
    const maskAfter = await exported(page);
    expect(pixel(maskAfter, 176, 128)[3]).toBe(0);
    expect(pixel(maskAfter, 335, 128)[3]).toBe(0);
    expect(pixel(maskAfter, 10, 10)).toEqual([210, 50, 40, 255]);
    await undo(page, masked.history.undo);
    expect((await exported(page)).data.equals(maskBefore.data)).toBe(true);
    const quick = await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        await a.newDocument(512, 256, 300);
        const s = await a.quickMask(true);
        await a.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
        return s;
      }),
    );
    await idle(page, quick.history.undo);
    await expect(page.getByTestId('zoom')).toHaveText('100.0%');
    await page.mouse.click(o.x + 176, o.y + 128);
    await idle(page, quick.history.undo + 1);
    expect((await state(page)).history.labels.at(-1)).toBe('Paint Quick Mask');
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        await a.quickMask(false);
        await a.selectionAction({ kind: 'fill', color: [0, 220, 60, 255] });
      }),
    );
    await idle(page);
    const quickImage = await exported(page);
    expect(pixel(quickImage, 176, 128)[3]).toBe(0);
    expect(pixel(quickImage, 335, 128)[3]).toBe(0);
    expect(pixel(quickImage, 10, 10)).toEqual([0, 220, 60, 255]);
  });
  test(`${backend} advanced-stroke cancellation no-op and locks preserve committed pixels and redo`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await load(page, 'tip');
    await load(page, 'texture');
    await page.getByRole('combobox', { name: 'Painting symmetry' }).selectOption('radial');
    await page.getByRole('spinbutton', { name: 'Radial sectors' }).fill('16');
    await range(page, 'Brush size', 100);
    await page.mouse.click(o.x + 220, o.y + 128);
    await idle(page, 1);
    await undo(page, 0);
    const before = await exported(page);
    await page.mouse.move(o.x + 220, o.y + 128);
    await page.mouse.down();
    await expect.poll(async () => (await state(page)).painting).toBe(true);
    await page.keyboard.press('Escape');
    await page.mouse.up();
    await idle(page, 0);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
    expect((await state(page)).history.redo).toBe(1);
    await range(page, 'Brush opacity', 0);
    await page.mouse.click(o.x + 220, o.y + 128);
    await idle(page, 0);
    expect((await state(page)).history.redo).toBe(1);
    await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
    await idle(page, 1);
    await range(page, 'Brush opacity', 1);
    await page.mouse.click(o.x + 220, o.y + 128);
    await expect.poll(async () => (await state(page)).editError).toMatch(/lock/i);
    expect((await exported(page)).data.equals(before.data)).toBe(true);
  });
}
