import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { PNG } from 'pngjs';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';

for (const backend of ['WebGPU', 'WebGL2']) {
  test(`${backend} crops without losing samples, edits guides and reopens retained pixels`, async ({
    page,
  }, info) => {
    await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    await page.getByTestId('open-image').setInputFiles(resolve('bench/results/fixtures/alpha.png'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    const state = async () =>
      JSON.parse(
        (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
      ) as DocumentSnapshot;
    await page.getByRole('button', { name: 'Show canvas section' }).click();
    await page.getByText('Crop canvas', { exact: true }).click();
    for (const [key, value] of Object.entries({ x: 100, y: 40, width: 200, height: 150 }))
      await page.getByLabel('Crop ' + key, { exact: true }).fill(String(value));
    await page.getByRole('button', { name: 'Apply crop', exact: true }).click();
    await expect.poll(async () => (await state()).document?.width).toBe(200);
    const cropped = (await state()).document!;
    expect(cropped).toMatchObject({
      height: 150,
      originX: 100,
      originY: 40,
      retainedWidth: 513,
      retainedHeight: 259,
    });
    const original = PNG.sync.read(await readFile(resolve('bench/results/fixtures/alpha.png')));
    const exported = await whenIdle(page, () =>
      page.evaluate(async () =>
        Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
      ),
    );
    const png = PNG.sync.read(Buffer.from(exported));
    expect([png.width, png.height]).toEqual([200, 150]);
    for (let y = 0; y < 150; y++)
      expect(png.data.subarray(y * 200 * 4, (y + 1) * 200 * 4)).toEqual(
        original.data.subarray(((y + 40) * 513 + 100) * 4, ((y + 40) * 513 + 300) * 4),
      );
    await page.getByText('Guides', { exact: true }).click();
    await page.getByLabel('Guide position', { exact: true }).fill('50');
    await page.getByRole('button', { name: 'Add guide', exact: true }).click();
    await expect
      .poll(async () => (await state()).document?.guides)
      .toEqual([{ axis: 'vertical', position: 150 }]);
    await expect(page.locator('.document-guide')).toHaveCount(1);
    await page.getByLabel('Guide 1 position', { exact: true }).fill('65');
    await page.getByLabel('Guide 1 position', { exact: true }).press('Tab');
    await expect.poll(async () => (await state()).document?.guides?.[0].position).toBe(165);
    await page.getByRole('checkbox', { name: 'Show rulers' }).check();
    await expect.poll(() => page.locator('.geometry-overlay text').count()).toBeGreaterThan(0);
    await expect(page.locator('.geometry-overlay')).toBeVisible();
    const guideGeometry = await page.locator('.document-guide').evaluate((node) => ({
      path: node.getAttribute('d'),
      box: node.getBoundingClientRect().toJSON(),
      svg: node.parentElement?.getBoundingClientRect().toJSON(),
    }));
    await info.attach('guide-geometry', {
      body: JSON.stringify(guideGeometry),
      contentType: 'application/json',
    });
    expect(guideGeometry.path).not.toContain('NaN');
    await page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
    const overlayImage = PNG.sync.read(
      await page.screenshot({
        path: `bench/results/p7-crop-guides-${info.project.name}-${backend}.png`,
      }),
    );
    const guideY = Math.floor(guideGeometry.box.y + guideGeometry.box.height / 3);
    expect(
      [-1, 0, 1].some((offset) => {
        const x = Math.round(guideGeometry.box.x) + offset,
          i = (guideY * overlayImage.width + x) * 4;
        // A one-pixel SVG stroke straddles two columns at fractional x. The
        // fixture here is opaque red; infer coverage from green, then check
        // independent red/blue values against the specified #53c4f4 guide.
        const coverage = overlayImage.data[i + 1] / 196;
        return (
          coverage > 0.2 &&
          Math.abs(overlayImage.data[i] - (255 * (1 - coverage) + 83 * coverage)) <= 2 &&
          Math.abs(overlayImage.data[i + 2] - 244 * coverage) <= 2 &&
          overlayImage.data[i + 3] === 255
        );
      }),
    ).toBe(true);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const state = JSON.parse(
          document.querySelector('[data-testid="gpu-canvas"]')!.getAttribute('data-snapshot')!,
        ) as DocumentSnapshot;
        await api.previewAdjustment(
          { kind: 'exposure', stops: 0, offset: 0, gamma: 1 },
          { kind: 'pixels', target: state.layers.active ?? state.layers.nodes[0].id },
        );
      }),
    );
    const preview = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
    for (const [x, y] of [
      [1, 1],
      [preview.width - 2, 1],
      [1, preview.height - 2],
      [preview.width - 2, preview.height - 2],
    ])
      expect(
        Array.from(
          preview.data.subarray((y * preview.width + x) * 4, (y * preview.width + x) * 4 + 4),
        ),
      ).toEqual([48, 52, 59, 255]);
    await page.evaluate(() =>
      (Reflect.get(window, '__fotoeditor') as EngineApi).endAdjustmentPreview(),
    );
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const dir = await navigator.storage.getDirectory();
        const file = await dir.getFileHandle('p7-crop-roundtrip.cgfoto', { create: true });
        await api.saveProjectFile(file);
      }),
    );
    await whenIdle(page, () =>
      page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).newDocument(8, 8, 72)),
    );
    const reopened = await whenIdle(page, () =>
      page.evaluate(async () => {
        const dir = await navigator.storage.getDirectory();
        const file = await dir.getFileHandle('p7-crop-roundtrip.cgfoto');
        return (Reflect.get(window, '__fotoeditor') as EngineApi).openProjectFile(
          await file.getFile(),
        );
      }),
    );
    expect(reopened.document).toMatchObject({
      width: 200,
      height: 150,
      retainedWidth: 513,
      retainedHeight: 259,
      guides: [{ axis: 'vertical', position: 165 }],
    });
    expect(reopened.history.undo).toBe(0);
    await expect(page.getByRole('button', { name: 'Reveal all pixels' })).toBeEnabled();
    await page.getByRole('button', { name: 'Reveal all pixels' }).click();
    await expect.poll(async () => (await state()).document?.width).toBe(513);
    const revealed = await whenIdle(page, () =>
      page.evaluate(async () =>
        Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
      ),
    );
    expect(PNG.sync.read(Buffer.from(revealed)).data).toEqual(original.data);
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).interact([
          { kind: 'undo', x: 0, y: 0, amount: 0 },
        ]),
      ),
    );
    await expect.poll(async () => (await state()).document?.width).toBe(200);
  });
  test(`${backend} recovers cropped edits and guides while retaining outside pixels`, async ({
    page: initialPage,
    context,
  }) => {
    let page = initialPage;
    const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).newDocument(64, 64, 72),
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
          kind: 'fill',
          color: [200, 10, 20, 255],
        }),
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).geometryMetadata({
          kind: 'crop',
          rect: { x: 16, y: 8, width: 24, height: 32 },
        }),
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).geometryMetadata({
          kind: 'guides',
          guides: [{ axis: 'vertical', position: 20.5 }],
        }),
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({ kind: 'all' }),
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
          kind: 'fill',
          color: [10, 20, 200, 255],
        }),
      ),
    );
    const copied = await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
          kind: 'copy',
          allLayers: false,
        }),
      ),
    );
    expect(copied.selection.clipboard).toEqual({ width: 24, height: 32 });
    await whenIdle(page, () =>
      page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).autosave()),
    );
    const session = await context.newCDPSession(page);
    const crashed = page.waitForEvent('crash');
    void session.send('Page.crash').catch(() => {});
    await crashed;
    await page.close();
    page = await context.newPage();
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.getByRole('button', { name: 'Restore session', exact: true }).click();
    const state = async () =>
      JSON.parse(
        (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
      ) as DocumentSnapshot;
    await expect.poll(async () => (await state()).document?.width).toBe(24);
    expect((await state()).document).toMatchObject({
      height: 32,
      originX: 16,
      originY: 8,
      retainedWidth: 64,
      retainedHeight: 64,
      guides: [{ axis: 'vertical', position: 20.5 }],
    });
    expect((await state()).history.undo).toBe(0);
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).geometryMetadata({
          kind: 'crop',
          rect: null,
        }),
      ),
    );
    const bytes = await whenIdle(page, () =>
      page.evaluate(async () =>
        Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
      ),
    );
    const png = PNG.sync.read(Buffer.from(bytes));
    expect([png.width, png.height]).toEqual([64, 64]);
    for (let y = 0; y < 64; y++)
      for (let x = 0; x < 64; x++) {
        const expected =
          x >= 16 && x < 40 && y >= 8 && y < 40 ? [10, 20, 200, 255] : [200, 10, 20, 255];
        expect(Array.from(png.data.subarray((y * 64 + x) * 4, (y * 64 + x + 1) * 4))).toEqual(
          expected,
        );
      }
  });
}
