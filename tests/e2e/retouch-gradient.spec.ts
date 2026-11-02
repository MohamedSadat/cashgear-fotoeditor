import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { whenIdle } from './idle';
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
  return Buffer.from(
    await whenIdle(p, () =>
      p.evaluate(async () => {
        const a = await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng();
        return new Promise<string>((resolve, reject) => {
          const r = new FileReader();
          r.onload = () => resolve(String(r.result).split(',')[1]);
          r.onerror = () => reject(r.error);
          r.readAsDataURL(new Blob([a as Uint8Array<ArrayBuffer>]));
        });
      }),
    ),
    'base64',
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
  const s = await whenIdle(p, () =>
    p.evaluate(async () => {
      const a = Reflect.get(window, '__fotoeditor') as EngineApi;
      await a.newDocument(512, 128, 300);
      await a.selectionAction({ kind: 'fill', color: [100, 120, 80, 255] });
      await a.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 240, y0: 0, x1: 257, y1: 128 },
        mode: 'new',
        antialias: false,
      });
      await a.selectionAction({ kind: 'fill', color: [180, 60, 90, 255] });
      const s = await a.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 256, y0: 0, x1: 512, y1: 128 },
        mode: 'new',
        antialias: false,
      });
      await a.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
      return s;
    }),
  );
  await expect.poll(async () => (await state(p)).revision).toBe(s.revision);
  await idle(p);
  await expect(p.getByTestId('zoom')).toHaveText('100.0%');
  const b = (await p.getByTestId('gpu-canvas').boundingBox())!;
  return { x: b.x + b.width / 2 - 256, y: b.y + b.height / 2 - 64 };
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(({ page }) => {
  errors.set(page, []);
  page.on('pageerror', (e) => errors.get(page)!.push(e.message));
});
test.afterEach(({ page }) => expect(errors.get(page)).toEqual([]));
for (const backend of ['WebGPU', 'WebGL2']) {
  test(`${backend} real photograph retouch visual evidence and exact undo`, async ({
    page,
  }, info) => {
    test.setTimeout(180000);
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
    await idle(page);
    await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Soft round');
    await range(page, 'Brush size', 48);
    const before = await exported(page),
      base = (await state(page)).history.undo;
    const box = (await page.getByTestId('gpu-canvas').boundingBox())!,
      x = box.x + box.width / 2,
      y = box.y + box.height / 2;
    const prefix = `bench/results/${process.env.E2E_EVIDENCE_PREFIX ?? 'p9-m4'}-portrait-${info.project.name}-${backend}`;
    await mkdir('bench/results', { recursive: true });
    await writeFile(prefix + '-before.png', before);
    await page.screenshot({ path: prefix + '-before-app.png' });
    for (const mode of ['smudge', 'blur', 'sharpen', 'dodge', 'burn', 'sponge']) {
      await page.getByRole('combobox', { name: 'Retouch tool' }).selectOption(mode);
      await range(
        page,
        'Retouch strength',
        ['dodge', 'burn', 'sponge'].includes(mode) ? 0.05 : mode === 'sharpen' ? 0.2 : 0.5,
      );
      await page.mouse.move(x - 8, y);
      await page.mouse.down();
      await page.mouse.move(x + 8, y, { steps: 8 });
      await page.mouse.up();
      await idle(page, base + 1);
      expect((await state(page)).editError).toBeUndefined();
      const after = await exported(page);
      expect(after.equals(before)).toBe(false);
      await writeFile(prefix + '-' + mode + '.png', after);
      await page.screenshot({ path: prefix + '-' + mode + '-app.png' });
      const a = PNG.sync.read(after),
        b = PNG.sync.read(before);
      for (let row = 0; row < a.height; row++) {
        const start = row * a.width * 4;
        if (row < 1190 || row > 1256)
          expect(
            a.data
              .subarray(start, start + a.width * 4)
              .equals(b.data.subarray(start, start + a.width * 4)),
          ).toBe(true);
        else {
          expect(
            a.data
              .subarray(start, start + 1430 * 4)
              .equals(b.data.subarray(start, start + 1430 * 4)),
          ).toBe(true);
          expect(
            a.data
              .subarray(start + 1505 * 4, start + a.width * 4)
              .equals(b.data.subarray(start + 1505 * 4, start + a.width * 4)),
          ).toBe(true);
        }
      }
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, base);
      expect((await exported(page)).equals(before)).toBe(true);
    }
  });
  test(`${backend} retouch tonal range and sponge modes, mask targets and gradient cancellation`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await page.getByRole('combobox', { name: 'Retouch tool' }).selectOption('dodge');
    await range(page, 'Brush size', 30);
    const before = await exported(page),
      base = (await state(page)).history.undo;
    await page.getByRole('combobox', { name: 'Tonal range' }).selectOption('highlights');
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await idle(page, base + 1);
    const highlights = pixel(PNG.sync.read(await exported(page)), 256, 64);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, base);
    await page.getByRole('combobox', { name: 'Tonal range' }).selectOption('all');
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await idle(page, base + 1);
    expect(pixel(PNG.sync.read(await exported(page)), 256, 64)[0]).toBeGreaterThan(highlights[0]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, base);
    await page.getByRole('combobox', { name: 'Retouch tool' }).selectOption('sponge');
    await page.getByRole('combobox', { name: 'Sponge mode' }).selectOption('saturate');
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await idle(page, base + 1);
    const saturated = pixel(PNG.sync.read(await exported(page)), 256, 64);
    expect(saturated[0] - saturated[1]).toBeGreaterThan(120);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, base);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        await a.diagnosticSelectionFault('pause-before-commit');
        Reflect.set(
          window,
          'm4GradientJob',
          a
            .selectionAction({
              kind: 'gradient',
              gradient: {
                shape: 'linear',
                start: [256.5, 64.5],
                end: [320.5, 64.5],
                stops: [
                  { position: 0, color: [0, 0, 0, 255] },
                  { position: 1, color: [255, 255, 255, 255] },
                ],
                opacity: 1,
                reverse: false,
              },
            })
            .then(() => 'committed', String),
        );
      }),
    );
    await expect(page.getByTestId('operation-status')).toContainText('Selection ready to commit');
    const result = await page.evaluate(async () => {
      const a = Reflect.get(window, '__fotoeditor') as EngineApi;
      return { cancelled: await a.cancelJob(), error: await Reflect.get(window, 'm4GradientJob') };
    });
    expect(result.cancelled).toBe(true);
    expect(result.error).toMatch(/cancelled/);
    await idle(page, base);
    expect((await state(page)).history.redo).toBe(1);
    expect((await exported(page)).equals(before)).toBe(true);
    const masked = await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        const target = JSON.parse(
          (document.querySelector('[data-testid="gpu-canvas"]') as HTMLElement).dataset.snapshot!,
        ).layers.nodes[0].id;
        await a.layerAction({
          kind: 'mask-add',
          target,
        });
        return a.selectLayer(target, true);
      }),
    );
    await expect.poll(async () => (await state(page)).layers.maskTarget).toBe(true);
    await idle(page);
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/raster pixel target/);
    await idle(page, masked.history.undo);
    await page.getByRole('button', { name: 'Gradient', exact: true }).click();
    await page.mouse.move(o.x + 256.5, o.y + 64.5);
    await page.mouse.down();
    await page.mouse.move(o.x + 320.5, o.y + 64.5);
    await page.mouse.up();
    await expect(
      page
        .getByRole('status')
        .filter({ hasText: 'Return to layer pixels before applying a gradient' }),
    ).toBeVisible();
    await idle(page, masked.history.undo);
  });
  for (const mode of ['smudge', 'blur', 'sharpen', 'dodge', 'burn', 'sponge'])
    test(`${backend} ${mode} controls clip selection and commit one exact undo/redo command`, async ({
      page,
    }) => {
      const o = await setup(page, backend);
      await page.getByRole('combobox', { name: 'Retouch tool' }).selectOption(mode);
      await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Hard round');
      await range(page, 'Brush size', 30);
      await range(page, 'Retouch strength', 0.5);
      const before = await exported(page),
        base = (await state(page)).history.undo;
      if (mode === 'smudge') {
        await page.mouse.move(o.x + 250.5, o.y + 64.5);
        await page.mouse.down();
        await page.mouse.move(o.x + 265.5, o.y + 64.5, { steps: 5 });
        await page.mouse.up();
      } else await page.mouse.click(o.x + 256.5, o.y + 64.5);
      await idle(page, base + 1);
      expect((await state(page)).editError).toBeUndefined();
      const after = await exported(page),
        a = PNG.sync.read(after),
        b = PNG.sync.read(before);
      expect(after.equals(before)).toBe(false);
      for (let y = 0; y < 128; y++)
        expect(
          a.data
            .subarray(y * 512 * 4, (y * 512 + 256) * 4)
            .equals(b.data.subarray(y * 512 * 4, (y * 512 + 256) * 4)),
        ).toBe(true);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, base);
      expect((await exported(page)).equals(before)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await idle(page, base + 1);
      expect((await exported(page)).equals(after)).toBe(true);
    });
  test(`${backend} retouch cancellation, strength zero and locks preserve committed samples and redo`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await page.getByRole('combobox', { name: 'Retouch tool' }).selectOption('dodge');
    await range(page, 'Brush size', 30);
    const before = await exported(page),
      base = (await state(page)).history.undo;
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await idle(page, base + 1);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await idle(page, base);
    await page.mouse.move(o.x + 256.5, o.y + 64.5);
    await page.mouse.down();
    await expect.poll(async () => (await state(page)).painting).toBe(true);
    await page.keyboard.press('Escape');
    await page.mouse.up();
    await idle(page, base);
    expect((await state(page)).history.redo).toBe(1);
    expect((await exported(page)).equals(before)).toBe(true);
    await range(page, 'Retouch strength', 0);
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await idle(page, base);
    expect((await state(page)).history.redo).toBe(1);
    await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
    await idle(page, base + 1);
    await range(page, 'Retouch strength', 0.5);
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await expect.poll(async () => (await state(page)).editError).toMatch(/lock/i);
    await idle(page, base + 1);
    expect((await exported(page)).equals(before)).toBe(true);
  });
  for (const shape of ['linear', 'radial', 'angle', 'reflected', 'diamond'])
    test(`${backend} ${shape} gradient drag clips selection and has exact undo/redo`, async ({
      page,
    }) => {
      const o = await setup(page, backend);
      await page.getByRole('button', { name: 'Gradient', exact: true }).click();
      await page.getByRole('combobox', { name: 'Gradient shape' }).selectOption(shape);
      const before = await exported(page),
        base = (await state(page)).history.undo;
      await page.mouse.move(o.x + 256.5, o.y + 64.5);
      await page.mouse.down();
      await page.mouse.move(o.x + 320.5, o.y + 64.5, { steps: 4 });
      await expect(page.locator('.selection-gesture path')).not.toHaveAttribute('d', '');
      await page.mouse.up();
      await idle(page, base + 1);
      const after = await exported(page),
        a = PNG.sync.read(after),
        b = PNG.sync.read(before);
      expect(after.equals(before)).toBe(false);
      expect(pixel(a, 255, 64)).toEqual(pixel(b, 255, 64));
      expect(pixel(a, 256, 64)).toEqual([0, 0, 0, 255]);
      expect((await state(page)).editError).toBeUndefined();
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, base);
      expect((await exported(page)).equals(before)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await idle(page, base + 1);
      expect((await exported(page)).equals(after)).toBe(true);
    });
  test(`${backend} editable multi-stop gradient presets persist, reuse alpha and reverse`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await page.getByRole('button', { name: 'Gradient', exact: true }).click();
    await page.getByRole('button', { name: 'Add gradient stop', exact: true }).click();
    await page.getByRole('spinbutton', { name: 'Stop 2 position', exact: true }).fill('0.25');
    await page.getByRole('textbox', { name: 'Stop 1 color', exact: true }).fill('#ff000000');
    await page.getByRole('textbox', { name: 'Stop 2 color', exact: true }).fill('#0000ff80');
    await page.getByRole('textbox', { name: 'Stop 3 color', exact: true }).fill('#00ff00');
    await page.getByRole('textbox', { name: 'Gradient preset name' }).fill('Ocean test');
    await page.getByRole('checkbox', { name: 'Reverse gradient', exact: true }).check();
    await page.getByRole('button', { name: 'Save gradient preset', exact: true }).click();
    await page
      .getByRole('combobox', { name: 'Gradient preset' })
      .selectOption({ label: 'Black to white' });
    await page
      .getByRole('combobox', { name: 'Gradient preset' })
      .selectOption({ label: 'Ocean test' });
    await expect(
      page.getByRole('spinbutton', { name: 'Stop 2 position', exact: true }),
    ).toHaveValue('0.25');
    await expect(
      page.getByRole('checkbox', { name: 'Reverse gradient', exact: true }),
    ).toBeChecked();
    const base = (await state(page)).history.undo;
    await page.mouse.move(o.x + 256.5, o.y + 64.5);
    await page.mouse.down();
    await page.mouse.move(o.x + 320.5, o.y + 64.5);
    await page.mouse.up();
    await idle(page, base + 1);
    const image = PNG.sync.read(await exported(page));
    expect(pixel(image, 256, 64)).toEqual([0, 255, 0, 255]);
    expect(pixel(image, 320, 64)).toEqual([100, 120, 80, 255]);
    await page.reload();
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.getByRole('button', { name: 'Gradient', exact: true }).click();
    await page
      .getByRole('combobox', { name: 'Gradient preset' })
      .selectOption({ label: 'Ocean test' });
    await expect(
      page.getByRole('spinbutton', { name: 'Stop 2 position', exact: true }),
    ).toHaveValue('0.25');
    await page.getByRole('button', { name: 'Delete gradient preset', exact: true }).click();
    await expect(
      page
        .getByRole('combobox', { name: 'Gradient preset' })
        .locator('option')
        .filter({ hasText: 'Ocean test' }),
    ).toHaveCount(0);
  });
  test(`${backend} gradient gesture cancellation, zero drag and job refusal preserve pixels`, async ({
    page,
  }) => {
    const o = await setup(page, backend);
    await page.getByRole('button', { name: 'Gradient', exact: true }).click();
    const before = await exported(page),
      base = (await state(page)).history.undo;
    await page.mouse.move(o.x + 256.5, o.y + 64.5);
    await page.mouse.down();
    await page.mouse.move(o.x + 300.5, o.y + 64.5);
    await page.keyboard.press('Escape');
    await page.mouse.up();
    await idle(page, base);
    expect((await exported(page)).equals(before)).toBe(true);
    await page.mouse.click(o.x + 256.5, o.y + 64.5);
    await expect(page.getByRole('status').filter({ hasText: /nonzero drag/ })).toBeVisible();
    await idle(page, base);
    const refusal = await whenIdle(page, () =>
      page.evaluate(async () => {
        const a = Reflect.get(window, '__fotoeditor') as EngineApi;
        await a.diagnosticSelectionFault('budget');
        try {
          await a.selectionAction({
            kind: 'gradient',
            gradient: {
              shape: 'linear',
              start: [256.5, 64.5],
              end: [320.5, 64.5],
              stops: [
                { position: 0, color: [0, 0, 0, 255] },
                { position: 1, color: [255, 255, 255, 255] },
              ],
              opacity: 1,
              reverse: false,
            },
          });
          return '';
        } catch (e) {
          return String(e);
        }
      }),
    );
    expect(refusal).toMatch(/budget|memory/i);
    expect((await exported(page)).equals(before)).toBe(true);
  });
}
