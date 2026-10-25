import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { whenIdle } from './idle';
const snap = async (page: Page) => {
  const canvas = page.getByTestId('gpu-canvas');
  await expect(canvas).toHaveAttribute('data-snapshot', /"view":/);
  return JSON.parse((await canvas.getAttribute('data-snapshot'))!) as DocumentSnapshot;
};
const canvasIdle = async (page: Page) =>
  expect
    .poll(async () => {
      const s = await snap(page);
      return !s.operation && !s.painting && !s.selection.busy;
    })
    .toBe(true);
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
const pixel = (png: PNG, x: number, y: number) =>
  Array.from(png.data.subarray((y * png.width + x) * 4, (y * png.width + x) * 4 + 4));
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend + ' lasso polygon and modifier gestures follow a rotated flipped view',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 64, 72);
        await api.interact([
          { kind: 'actual', x: 0, y: 0, amount: 0 },
          { kind: 'rotate', x: 0, y: 0, amount: Math.PI / 2 },
          { kind: 'flip', x: 0, y: 0, amount: 0 },
        ]);
      });
      await expect.poll(async () => (await snap(page))?.view.flipped).toBe(true);
      await page.getByRole('checkbox', { name: 'Anti-alias', exact: true }).uncheck();
      await page.getByTestId('gpu-canvas').focus();
      const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
      const point = async (x: number, y: number) => {
        const v = (await snap(page)).view;
        const dx = (x - 32) * (v.flipped ? -1 : 1),
          dy = y - 32;
        return [
          box.x +
            v.screenWidth / 2 +
            v.panX +
            (dx * Math.cos(v.rotation) - dy * Math.sin(v.rotation)) * v.zoom,
          box.y +
            v.screenHeight / 2 +
            v.panY +
            (dx * Math.sin(v.rotation) + dy * Math.cos(v.rotation)) * v.zoom,
        ] as const;
      };
      const move = async (x: number, y: number) =>
        page.mouse.move(...(await point(x, y)), { steps: 4 });
      await page.keyboard.press('l');
      await canvasIdle(page);
      await move(8, 8);
      await page.mouse.down();
      await move(56, 8);
      await move(56, 56);
      await move(8, 56);
      await move(8, 8);
      await page.mouse.up();
      await expect.poll(async () => (await snap(page)).history.undo).toBe(1);
      await canvasIdle(page);
      await page.keyboard.press('m');
      await page.keyboard.down('Alt');
      await move(24, 24);
      await page.mouse.down();
      await move(40, 40);
      await page.mouse.up();
      await page.keyboard.up('Alt');
      await expect.poll(async () => (await snap(page)).history.undo).toBe(2);
      await canvasIdle(page);
      await page.keyboard.press('Shift+l');
      for (const p of [
        [0, 0],
        [4, 0],
        [0, 4],
      ]) {
        await canvasIdle(page);
        await page.mouse.click(...(await point(p[0], p[1])));
      }
      await page.keyboard.press('Escape');
      expect((await snap(page)).history.undo).toBe(2);
      await page.keyboard.down('Shift');
      for (const p of [
        [0, 0],
        [4, 0],
        [0, 4],
      ]) {
        await canvasIdle(page);
        await page.mouse.click(...(await point(p[0], p[1])));
      }
      await page.keyboard.up('Shift');
      await canvasIdle(page);
      await page.keyboard.press('Enter');
      await expect.poll(async () => (await snap(page)).history.undo).toBe(3);
      await page.evaluate(async () =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
          kind: 'fill',
          color: [255, 0, 0, 255],
        }),
      );
      const image = await exported(page);
      expect(pixel(image, 1, 1)).toEqual([255, 0, 0, 255]);
      expect(pixel(image, 12, 12)).toEqual([255, 0, 0, 255]);
      expect(pixel(image, 32, 32)).toEqual([0, 0, 0, 0]);
      expect(pixel(image, 60, 60)).toEqual([0, 0, 0, 0]);
    },
  );
  test(
    backend + ' cancelled and refused selection candidates preserve document history and redo',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 64, 72);
        await api.selectionAction({ kind: 'fill', color: [201, 3, 7, 1] });
        await api.selectionAction({ kind: 'all' });
        await api.historyJump(1);
      });
      const before = await exported(page);
      const budget = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.diagnosticSelectionFault('budget');
        try {
          await api.selectionAction({
            kind: 'shape',
            shape: { kind: 'rectangle', x0: 1, y0: 1, x1: 30, y1: 30 },
            mode: 'new',
            antialias: true,
          });
          return '';
        } catch (error) {
          return String(error);
        }
      });
      expect(budget).toMatch(/budget|768 MiB/);
      await expect.poll(async () => (await snap(page)).history.redo).toBe(1);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.diagnosticSelectionFault('pause-before-commit');
        Reflect.set(
          window,
          'selectionPending',
          api
            .selectionAction({
              kind: 'shape',
              shape: { kind: 'ellipse', x0: 1, y0: 1, x1: 30, y1: 30 },
              mode: 'new',
              antialias: true,
            })
            .then(
              () => '',
              (e) => String(e),
            ),
        );
      });
      await expect(page.getByTestId('operation-status')).toHaveText(/ready to commit/);
      const result = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const cancelled = await api.cancelJob();
        return { cancelled, error: await Reflect.get(window, 'selectionPending') };
      });
      expect(result.cancelled).toBe(true);
      expect(result.error).toMatch(/cancelled/);
      expect((await exported(page)).data.equals(before.data)).toBe(true);
      const state = await snap(page);
      expect(state.selection.active).toBe(false);
      expect(state.history.undo).toBe(1);
      expect(state.history.redo).toBe(1);
    },
  );
  test(
    backend + ' fills the active mask and confines a contiguous bucket to one selected island',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const doc = await api.newDocument(7, 1, 72);
        const target = doc.layers.nodes[0].id;
        await api.selectionAction({ kind: 'fill', color: [0, 255, 0, 255] });
        await api.layerAction({ kind: 'mask-add', target });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 2, y0: 0, x1: 4, y1: 1 },
          mode: 'new',
          antialias: false,
        });
        await api.selectLayer(target, true);
        await api.selectionAction({ kind: 'fill', color: [0, 0, 0, 255] });
      });
      let image = await exported(page);
      expect(pixel(image, 1, 0)).toEqual([0, 255, 0, 255]);
      expect(pixel(image, 2, 0)).toEqual([0, 0, 0, 0]);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.historyJump(3);
        const state = await api.selectionAction({ kind: 'none' });
        await api.selectLayer(state.layers.nodes[0].id, false);
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 1, y0: 0, x1: 2, y1: 1 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 5, y0: 0, x1: 6, y1: 1 },
          mode: 'add',
          antialias: false,
        });
        await api.selectionAction({
          kind: 'bucket',
          allLayers: false,
          color: [255, 0, 0, 255],
          options: {
            seed: [1, 0],
            tolerance: 255,
            contiguous: true,
            antialias: false,
            mode: 'new',
          },
        });
      });
      image = await exported(page);
      expect(pixel(image, 1, 0)).toEqual([255, 0, 0, 255]);
      expect(pixel(image, 5, 0)).toEqual([0, 255, 0, 255]);
      expect((await snap(page)).selection.lastWandPixels).toBe(1);
    },
  );
  test(
    backend + ' session selections do not rewrite recovery and projects reopen without a selection',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 64, 72);
        await api.selectionAction({ kind: 'fill', color: [201, 3, 7, 1] });
        await api.autosave();
      });
      await expect.poll(async () => (await snap(page))?.recovery?.state).toBe('saved');
      const savedAt = (await snap(page)).recovery!.lastSavedAt;
      const original = await exported(page);
      const revisions = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const before = JSON.parse(
          document.querySelector<HTMLCanvasElement>('[data-testid="gpu-canvas"]')!.dataset
            .snapshot!,
        ).contentRevision;
        const selected = await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'ellipse', x0: 8, y0: 8, x1: 56, y1: 56 },
          mode: 'new',
          antialias: true,
        });
        await api.historyJump(1);
        await api.historyJump(2);
        await api.autosave();
        return { before, after: selected.contentRevision };
      });
      expect(revisions.after).toBe(revisions.before);
      await expect.poll(async () => (await snap(page)).recovery?.lastSavedAt).toBe(savedAt);
      const reopened = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const bytes = await api.saveProject();
        return api.openProject(bytes.buffer as ArrayBuffer, 'selection-roundtrip.cgfoto');
      });
      expect(reopened.selection.active).toBe(false);
      expect(reopened.history.undo).toBe(0);
      expect((await exported(page)).data.equals(original.data)).toBe(true);
    },
  );
  test(
    backend + ' wand matches opaque colors through unaligned bit rows and samples all layers',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/wand.png'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      const counts = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const options = {
          seed: [1, 1] as [number, number],
          tolerance: 0,
          contiguous: true,
          antialias: false,
          mode: 'new' as const,
        };
        let s = await api.selectionAction({ kind: 'wand', allLayers: false, options });
        const contiguous = s.selection.lastWandPixels;
        s = await api.selectionAction({
          kind: 'wand',
          allLayers: false,
          options: { ...options, contiguous: false },
        });
        const global = s.selection.lastWandPixels;
        await api.selectionAction({ kind: 'none' });
        s = await api.layerAction({
          kind: 'add',
          name: 'Blue overlay',
          parent: null,
          index: 1,
          group: false,
        });
        await api.selectionAction({ kind: 'fill', color: [0, 0, 255, 255] });
        await api.layerAction({
          kind: 'properties',
          target: s.layers.active!,
          properties: { ...s.layers.nodes[1].properties, opacity: 0.5 },
        });
        s = await api.selectionAction({
          kind: 'wand',
          allLayers: false,
          options: { ...options, contiguous: false },
        });
        const active = s.selection.lastWandPixels;
        s = await api.selectionAction({
          kind: 'wand',
          allLayers: true,
          options: { ...options, contiguous: false },
        });
        return { contiguous, global, active, merged: s.selection.lastWandPixels };
      });
      expect(counts).toEqual({
        contiguous: 8192,
        global: 90433,
        active: 1025 * 257,
        merged: 90433,
      });
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
  test(
    backend + ' brush eraser and Quick Mask strokes preserve every sample outside coverage',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(512, 128, 72);
        await api.selectionAction({ kind: 'fill', color: [0, 255, 0, 255] });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 128, y0: 32, x1: 384, y1: 96 },
          mode: 'new',
          antialias: false,
        });
        await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
      });
      await expect.poll(async () => (await snap(page))?.view.zoom).toBe(1);
      await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Pixel pencil');
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#0000ff');
      const canvas = page.getByTestId('gpu-canvas');
      const box = (await canvas.boundingBox())!;
      const click = async (x: number, y: number, mutates = false) => {
        // Autosave/recomposition owns input admission even after a stroke ends.
        await canvasIdle(page);
        const undo = (await snap(page)).history.undo;
        const clientX = box.x + box.width / 2 + x - 256,
          clientY = box.y + box.height / 2 + y - 64;
        if (process.env.GPU_TEST_SOFTWARE === '1') {
          // Keep this single-dab clipping fixture finite even if the driver
          // stalls between native down/up delivery. Hardware retains native input.
          await canvas.evaluate(
            (element, point) => {
              for (const type of ['pointerdown', 'pointerup'])
                element.dispatchEvent(
                  new PointerEvent(type, {
                    ...point,
                    pointerId: 1,
                    pointerType: 'mouse',
                    button: 0,
                    buttons: type === 'pointerdown' ? 1 : 0,
                    pressure: type === 'pointerdown' ? 0.5 : 0,
                    bubbles: true,
                    cancelable: true,
                  }),
                );
            },
            { clientX, clientY },
          );
        } else await page.mouse.click(clientX, clientY);
        if (mutates) await expect.poll(async () => (await snap(page)).history.undo).toBe(undo + 1);
        await expect.poll(async () => (await snap(page)).painting).toBe(false);
        await canvasIdle(page);
      };
      await click(100.5, 64.5);
      await click(256.5, 64.5, true);
      let image = await exported(page);
      expect(pixel(image, 100, 64)).toEqual([0, 255, 0, 255]);
      expect(pixel(image, 256, 64)).toEqual([0, 0, 255, 255]);
      await page.keyboard.press('e');
      await click(100.5, 64.5);
      await click(256.5, 64.5, true);
      image = await exported(page);
      expect(pixel(image, 100, 64)).toEqual([0, 255, 0, 255]);
      expect(pixel(image, 256, 64)).toEqual([0, 0, 255, 0]);
      await page.keyboard.press('b');
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#000000');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          await (Reflect.get(window, '__fotoeditor') as EngineApi).quickMask(true);
        }),
      );
      await click(256.5, 64.5, true);
      await expect
        .poll(async () => (await snap(page)).history.labels.at(-1))
        .toBe('Paint Quick Mask');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          await (Reflect.get(window, '__fotoeditor') as EngineApi).quickMask(false);
        }),
      );
      await whenIdle(page, () =>
        page.evaluate(async () => {
          await (Reflect.get(window, '__fotoeditor') as EngineApi).selectionAction({
            kind: 'fill',
            color: [255, 0, 0, 255],
          });
        }),
      );
      image = await exported(page);
      expect(pixel(image, 256, 64)).toEqual([0, 0, 255, 0]);
      expect(pixel(image, 255, 64)).toEqual([255, 0, 0, 255]);
      expect(pixel(image, 100, 64)).toEqual([0, 255, 0, 255]);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
  test(
    backend + ' system image clipboard and internal fallback preserve copied layer samples',
    async ({ page, context }) => {
      test.setTimeout(180000);
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/alpha.png'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      const source = PNG.sync.read(await readFile('bench/results/fixtures/alpha.png'));
      await page.keyboard.press('Control+c');
      await expect(page.getByTestId('operation-status')).toHaveText(/Copied to clipboard/);
      const types = await page.evaluate(async () => (await navigator.clipboard.read())[0].types);
      expect(types).toContain('image/png');
      await page.keyboard.press('Control+v');
      await expect.poll(async () => (await snap(page)).layers.nodes.length).toBe(2);
      await expect(page.getByTestId('operation-status')).toHaveText(/Pasted as a new layer/);
      const pasted = PNG.sync.read(
        Buffer.from(
          await page.evaluate(async () => {
            const api = Reflect.get(window, '__fotoeditor') as EngineApi;
            await api.selectionAction({ kind: 'copy', allLayers: false });
            return Array.from(await api.clipboardPng());
          }),
        ),
      );
      expect(pasted.data.equals(source.data)).toBe(true);
      const external = new PNG({ width: 1, height: 1 });
      external.data.set([17, 27, 37, 255]);
      await page.evaluate(
        async (bytes) => {
          await navigator.clipboard.write([
            new ClipboardItem({
              'image/png': new Blob([new Uint8Array(bytes)], { type: 'image/png' }),
            }),
          ]);
        },
        Array.from(PNG.sync.write(external)),
      );
      await page.keyboard.press('Control+v');
      await expect.poll(async () => (await snap(page)).layers.nodes.length).toBe(3);
      await expect(page.getByTestId('operation-status')).toHaveText(/Pasted as a new layer/);
      const externalCopy = PNG.sync.read(
        Buffer.from(
          await page.evaluate(async () => {
            const api = Reflect.get(window, '__fotoeditor') as EngineApi;
            await api.selectionAction({ kind: 'copy', allLayers: false });
            return Array.from(await api.clipboardPng());
          }),
        ),
      );
      expect([externalCopy.width, externalCopy.height]).toEqual([513, 259]);
      expect(pixel(externalCopy, 256, 129)).toEqual([17, 27, 37, 255]);
      await page.evaluate(() =>
        Object.defineProperty(navigator, 'clipboard', {
          configurable: true,
          value: {
            write: () => Promise.reject(new DOMException('Denied by test', 'NotAllowedError')),
            read: () => Promise.reject(new DOMException('Denied by test', 'NotAllowedError')),
          },
        }),
      );
      await page.keyboard.press('Control+c');
      await expect(page.getByTestId('operation-status')).toHaveText(/Copied internally/);
      await page.keyboard.press('Control+v');
      await expect.poll(async () => (await snap(page)).layers.nodes.length).toBe(4);
      await expect(page.getByTestId('operation-status')).toHaveText(/Pasted as a new layer/);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
  test(
    backend + ' selection fill cut paste and exact undo retain samples across tile seams',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(520, 64, 72);
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 255, y0: 4, x1: 258, y1: 44 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({ kind: 'fill', color: [255, 0, 0, 255] });
      });
      const red = await exported(page);
      expect(pixel(red, 254, 20)).toEqual([0, 0, 0, 0]);
      expect(pixel(red, 255, 20)).toEqual([255, 0, 0, 255]);
      expect(pixel(red, 257, 20)).toEqual([255, 0, 0, 255]);
      expect(pixel(red, 258, 20)).toEqual([0, 0, 0, 0]);
      const copied = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.selectionAction({ kind: 'cut', allLayers: false });
        return Array.from(await api.clipboardPng());
      });
      const clip = PNG.sync.read(Buffer.from(copied));
      expect([clip.width, clip.height]).toEqual([3, 40]);
      expect(pixel(clip, 0, 10)).toEqual([255, 0, 0, 255]);
      const cleared = await exported(page);
      const expected = Buffer.from(red.data);
      for (let i = 3; i < expected.length; i += 4) expected[i] = 0;
      expect(cleared.data.equals(expected)).toBe(true);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.selectionAction({ kind: 'paste', inPlace: true });
      });
      expect((await exported(page)).data.equals(red.data)).toBe(true);
      await expect.poll(async () => (await snap(page)).layers.nodes.length).toBe(2);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.historyJump(2);
      });
      expect((await exported(page)).data.equals(red.data)).toBe(true);
      await expect.poll(async () => (await snap(page)).selection.active).toBe(true);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
  test(
    backend + ' rectangle gesture and GPU Quick Mask overlay exclude unselected fill pixels',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(128, 128, 72);
        await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
      });
      await expect.poll(async () => (await snap(page))?.view.zoom).toBe(1);
      const canvas = page.getByTestId('gpu-canvas');
      const box = (await canvas.boundingBox())!;
      const cx = box.x + box.width / 2,
        cy = box.y + box.height / 2;
      await page.getByRole('button', { name: 'Rectangle', exact: true }).click();
      await page.mouse.move(cx - 32, cy - 32);
      await page.mouse.down();
      await page.mouse.move(cx + 32, cy + 32, { steps: 8 });
      await page.mouse.up();
      await expect.poll(async () => (await snap(page)).selection.active).toBe(true);
      await page.keyboard.press('q');
      await expect.poll(async () => (await snap(page)).selection.quickMask).toBe(true);
      await expect
        .poll(async () => {
          const image = PNG.sync.read(await canvas.screenshot());
          const outside = pixel(
            image,
            Math.floor(image.width / 2) - 50,
            Math.floor(image.height / 2),
          );
          return outside[0] > outside[1] + 30;
        })
        .toBe(true);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.selectionAction({ kind: 'fill', color: [255, 255, 255, 255] });
      });
      expect((await exported(page)).data.equals(Buffer.alloc(128 * 128 * 4))).toBe(true);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.historyJump(1);
        await api.quickMask(false);
        await api.selectionAction({
          kind: 'fill',
          color: [0, 0, 255, 255],
        });
      });
      const blue = await exported(page);
      expect(pixel(blue, 64, 64)).toEqual([0, 0, 255, 255]);
      expect(pixel(blue, 10, 64)).toEqual([0, 0, 0, 0]);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
}
