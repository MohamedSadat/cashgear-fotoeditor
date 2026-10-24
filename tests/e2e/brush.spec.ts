import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { PNG } from 'pngjs';
import type { DocumentSnapshot } from '../../apps/web/src/engine/types';
const snap = async (page: Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
const idle = async (page: Page, count?: number) => {
  if (count !== undefined)
    await expect.poll(async () => (await snap(page)).history.undo).toBe(count);
  await expect.poll(async () => (await snap(page)).painting).toBe(false);
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
};
const range = async (page: Page, label: string, value: number) => {
  await page.getByRole('slider', { name: label, exact: true }).evaluate((element, value) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
      element,
      String(value),
    );
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
};
const exportImage = async (page: Page) => {
  const ready = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export PNG' }).click();
  return PNG.sync.read(await readFile((await (await ready).path())!));
};
const pixel = (png: PNG, x: number, y: number) => [
  ...png.data.subarray((y * png.width + x) * 4, (y * png.width + x) * 4 + 4),
];
async function create(page: Page, query: string) {
  await page.goto('/' + query);
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await page.getByRole('button', { name: 'New canvas', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('512');
  await page.getByRole('spinbutton', { name: 'Height', exact: true }).fill('256');
  await page.getByRole('button', { name: 'Create document' }).click();
  await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
  await expect(page.getByTestId('zoom')).toHaveText('100.0%');
  return (await page.getByTestId('gpu-canvas').boundingBox())!;
}
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = backend === 'WebGL2' ? '?gpu=webgl2' : '';
  test(
    backend + ' uploads changed regions during a live stroke and undoes the completed command',
    async ({ page }) => {
      const box = await create(page, query),
        cx = box.x + box.width / 2,
        cy = box.y + box.height / 2;
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff0000');
      await page.mouse.move(cx - 176, cy);
      await page.mouse.down();
      await expect.poll(async () => (await snap(page)).paintTime).toBeGreaterThan(0);
      const before = await snap(page);
      await page.mouse.move(cx - 56, cy, { steps: 8 });
      await expect.poll(async () => (await snap(page)).paintTime).toBeGreaterThan(before.paintTime);
      await expect.poll(async () => (await snap(page)).pendingSamples).toBe(0);
      const live = await snap(page);
      expect(live.painting).toBe(true);
      expect(live.history.undo).toBe(0);
      expect(live.uploadedBytes - before.uploadedBytes).toBeGreaterThan(0);
      expect(live.uploadedBytes - before.uploadedBytes).toBeLessThan(200000);
      const screen = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
      expect(
        pixel(screen, Math.floor(screen.width / 2) - 56, Math.floor(screen.height / 2)),
      ).toEqual([255, 0, 0, 255]);
      await page.mouse.up();
      await idle(page, 1);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, 0);
      expect((await exportImage(page)).data.equals(Buffer.alloc(512 * 256 * 4))).toBe(true);
    },
  );
  test(
    backend + ' refreshes neighboring gutters when an edited view becomes filtered',
    async ({ page }) => {
      await create(page, query);
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/alpha.png'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
      await expect(page.getByTestId('zoom')).toHaveText('100.0%');
      await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Pixel pencil');
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#0000ff');
      const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
      await page.mouse.click(box.x + box.width / 2 - 1, box.y + box.height / 2 - 1);
      await idle(page, 1);
      const download = page.waitForEvent('download');
      await page.getByRole('button', { name: 'Export PNG' }).click();
      const bytes = await readFile((await (await download).path())!);
      async function filter() {
        await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
        await expect(page.getByTestId('zoom')).toHaveText('100.0%');
        await page.getByTestId('gpu-canvas').evaluate((canvas) => {
          const r = canvas.getBoundingClientRect();
          canvas.dispatchEvent(
            new WheelEvent('wheel', {
              clientX: r.x + r.width / 2,
              clientY: r.y + r.height / 2,
              ctrlKey: true,
              deltaY: -Math.log(0.75) / 0.002,
              bubbles: true,
              cancelable: true,
            }),
          );
          canvas.dispatchEvent(
            new WheelEvent('wheel', { deltaX: -0.75, deltaY: 0, bubbles: true, cancelable: true }),
          );
        });
        await expect(page.getByTestId('zoom')).toHaveText('75.0%');
        await expect.poll(async () => (await snap(page)).view.panX).toBeCloseTo(0.75, 6);
        await expect.poll(async () => (await snap(page)).refining).toBe(0);
        return PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
      }
      const edited = await filter();
      await page
        .getByTestId('open-image')
        .setInputFiles({ name: 'painted.png', mimeType: 'image/png', buffer: bytes });
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      const fresh = await filter();
      for (const [dx, dy] of [
        [0, -1],
        [-1, -1],
        [1, -1],
        [0, -2],
        [0, 0],
        [-2, -1],
      ]) {
        const actual = pixel(
            edited,
            Math.floor(edited.width / 2) + dx,
            Math.floor(edited.height / 2) + dy,
          ),
          expected = pixel(
            fresh,
            Math.floor(fresh.width / 2) + dx,
            Math.floor(fresh.height / 2) + dy,
          );
        actual.forEach((c, i) => expect(Math.abs(c - expected[i])).toBeLessThanOrEqual(1));
      }
    },
  );
  test(
    backend + ' airbrush builds stationary flow and shortcuts keep strokes atomic',
    async ({ page }) => {
      const box = await create(page, query),
        cx = box.x + box.width / 2,
        cy = box.y + box.height / 2;
      await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Airbrush');
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff0000');
      await range(page, 'Brush size', 80);
      await range(page, 'Brush opacity', 0.5);
      await page.mouse.move(cx, cy);
      await page.mouse.down();
      await expect.poll(async () => (await snap(page)).paintTime).toBeGreaterThan(0);
      const first = (await snap(page)).paintTime;
      await expect.poll(async () => (await snap(page)).paintTime).toBeGreaterThan(first + 70);
      expect((await snap(page)).history.undo).toBe(0);
      await page.mouse.up();
      await idle(page, 1);
      const sample = pixel(await exportImage(page), 256, 128);
      expect(sample.slice(0, 3)).toEqual([255, 0, 0]);
      expect(sample[3]).toBeGreaterThan(20);
      expect(sample[3]).toBeLessThanOrEqual(128);
      await page.getByTestId('gpu-canvas').focus();
      await page.keyboard.press(']');
      await expect(page.getByRole('slider', { name: 'Brush size', exact: true })).toHaveValue(
        '100',
      );
      await page.keyboard.press('[');
      await expect(page.getByRole('slider', { name: 'Brush size', exact: true })).toHaveValue('80');
      await page.keyboard.press('i');
      await page.mouse.click(cx, cy);
      await expect(page.getByRole('button', { name: 'Eyedropper', exact: true })).toHaveAttribute(
        'aria-pressed',
        'true',
      );
      await expect(page.getByRole('textbox', { name: 'Color hex' })).toHaveValue(
        /^#ff0000[\da-f]{2}$/,
      );
      expect((await snap(page)).history.undo).toBe(1);
    },
  );
  test(
    backend + ' paints a flick, exports exact undo/redo and preserves stroke boundaries',
    async ({ page }) => {
      const box = await create(page, query);
      const cx = box.x + box.width / 2,
        cy = box.y + box.height / 2;
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff0000');
      await page.mouse.move(cx - 220, cy);
      await page.mouse.down();
      await page.mouse.move(cx + 220, cy, { steps: 2 });
      await page.mouse.up();
      await idle(page, 1);
      const screen = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
      expect(pixel(screen, Math.floor(screen.width / 2), Math.floor(screen.height / 2))).toEqual([
        255, 0, 0, 255,
      ]);
      const first = await exportImage(page);
      for (const x of [36, 128, 255, 256, 384, 475])
        expect(pixel(first, x, 128)).toEqual([255, 0, 0, 255]);
      await page.getByTestId('gpu-canvas').focus();
      await page.keyboard.press('Control+z');
      await idle(page, 0);
      const empty = await exportImage(page);
      expect(empty.data.equals(Buffer.alloc(empty.data.length))).toBe(true);
      await page.getByTestId('gpu-canvas').focus();
      await page.keyboard.press('Control+Shift+z');
      await idle(page, 1);
      expect((await exportImage(page)).data.equals(first.data)).toBe(true);
      await page.mouse.click(cx - 100, cy - 70);
      await page.mouse.click(cx + 100, cy - 70);
      await idle(page, 3);
      await page.getByTestId('gpu-canvas').focus();
      await page.keyboard.press('Control+z');
      await page.keyboard.press('Control+y');
      await idle(page, 3);
      expect((await snap(page)).memory.temporary).toBe(0);
      expect((await snap(page)).editError).toBeUndefined();
      // A long stroke leaves the following quick click queued in the worker.
      // Lifecycle actions must stay blocked while that edit is still waiting.
      await range(page, 'Brush size', 2048);
      await range(page, 'Brush spacing', 0.01);
      const software = process.env.GPU_TEST_SOFTWARE === '1';
      const boundary = await page.evaluateHandle(
        ({ cx, cy, software }) => {
          const canvas = document.querySelector<HTMLCanvasElement>('[data-testid="gpu-canvas"]')!;
          const states: { undo: number; painting: boolean; queued: boolean }[] = [];
          let starts = 0;
          const started = () => {
            starts++;
          };
          canvas.addEventListener('pointerdown', started);
          const observer = new MutationObserver(() => {
            const s = JSON.parse(canvas.dataset.snapshot!);
            states.push({ undo: s.history.undo, painting: s.painting, queued: starts >= 2 });
          });
          observer.observe(canvas, { attributes: true, attributeFilter: ['data-snapshot'] });
          if (software)
            canvas.addEventListener(
              'pointerdown',
              (event) => {
                // Submit both stroke boundaries in one task. Separate protocol
                // calls can stall until the first stroke has already completed,
                // leaving no queued edit for this assertion to observe.
                const pointer = event as PointerEvent;
                let serial = 0;
                const send = (type: string, x: number, buttons: number) => {
                  const input = new PointerEvent(type, {
                    pointerId: pointer.pointerId,
                    pointerType: 'mouse',
                    isPrimary: true,
                    clientX: x,
                    clientY: cy,
                    buttons,
                    button: type === 'pointermove' ? -1 : 0,
                    bubbles: true,
                  });
                  Object.defineProperty(input, 'timeStamp', {
                    value: pointer.timeStamp + ++serial,
                  });
                  canvas.dispatchEvent(input);
                };
                send('pointermove', cx + 220, 1);
                send('pointerup', cx + 220, 0);
                const color = document.querySelector<HTMLInputElement>('#color-hex')!;
                Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
                  color,
                  '#00ff00',
                );
                color.dispatchEvent(new Event('input', { bubbles: true }));
                send('pointerdown', cx, 1);
                send('pointerup', cx, 0);
              },
              { once: true },
            );
          return { states, observer, started };
        },
        { cx, cy, software },
      );
      await page.mouse.move(cx - 220, cy);
      await page.mouse.down();
      if (software) await page.mouse.up();
      else {
        await page.mouse.move(cx + 220, cy);
        await page.mouse.up();
        await page.getByRole('textbox', { name: 'Color hex' }).fill('#00ff00');
        await page.mouse.click(cx, cy);
      }
      await idle(page, 5);
      const intermediate = await boundary.evaluate(({ states, observer, started }) => {
        observer.disconnect();
        document
          .querySelector('[data-testid="gpu-canvas"]')!
          .removeEventListener('pointerdown', started);
        return states.filter((s) => s.undo === 4 && s.queued);
      });
      await boundary.dispose();
      expect(intermediate.length).toBeGreaterThan(0);
      expect(intermediate.every((s) => s.painting)).toBe(true);
      await page.setViewportSize({ width: 700, height: 600 });
      await page.getByRole('button', { name: 'Brush settings', exact: true }).click();
      await expect(page.getByRole('textbox', { name: 'Color hex' })).toBeVisible();
      await page.getByRole('button', { name: 'Brush settings', exact: true }).click();
      await expect(page.getByRole('textbox', { name: 'Color hex' })).toBeHidden();
    },
  );
  test(
    backend + ' caps opacity, erases alpha, samples color and cancels atomically',
    async ({ page }) => {
      const box = await create(page, query);
      const cx = box.x + box.width / 2,
        cy = box.y + box.height / 2;
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#3366cc');
      await range(page, 'Brush size', 60);
      await range(page, 'Brush opacity', 0.4);
      await range(page, 'Brush flow', 0.2);
      await page.mouse.move(cx - 30, cy);
      if (process.env.GPU_TEST_SOFTWARE === '1')
        await page.getByTestId('gpu-canvas').evaluate((canvas) => {
          canvas.addEventListener(
            'pointerdown',
            (event) => {
              const p = event as PointerEvent;
              Reflect.set(window, 'opacityPointer', { id: p.pointerId, time: p.timeStamp });
            },
            { once: true },
          );
        });
      await page.mouse.down();
      if (process.env.GPU_TEST_SOFTWARE === '1') {
        // Keep this finite overlap fixture at 15 samples on software drivers.
        // Slow compositor acknowledgements can otherwise extend a native move
        // into a long stationary spray. Native input and timed spray have their
        // own cases; every pixel/history assertion below still runs here.
        await page.getByTestId('gpu-canvas').evaluate(
          (canvas, { cx, cy }) => {
            const pointer = Reflect.get(window, 'opacityPointer') as { id: number; time: number };
            let previous = -30,
              serial = 0;
            const send = (type: string, x: number, buttons: number) => {
              const event = new PointerEvent(type, {
                pointerId: pointer.id,
                pointerType: 'mouse',
                isPrimary: true,
                clientX: x,
                clientY: cy,
                buttons,
                button: buttons ? -1 : 0,
                bubbles: true,
              });
              Object.defineProperty(event, 'timeStamp', { value: pointer.time + ++serial });
              canvas.dispatchEvent(event);
            };
            for (const target of [30, -30, 30, -30, 30]) {
              for (let step = 1; step <= 3; step++)
                send('pointermove', cx + previous + ((target - previous) * step) / 3, 1);
              previous = target;
            }
            send('pointerup', cx + previous, 0);
          },
          { cx, cy },
        );
      } else {
        for (const x of [30, -30, 30, -30, 30]) await page.mouse.move(cx + x, cy, { steps: 3 });
      }
      await page.mouse.up();
      await idle(page, 1);
      const painted = await exportImage(page);
      expect(pixel(painted, 256, 128).slice(0, 3)).toEqual([51, 102, 204]);
      expect(pixel(painted, 256, 128)[3]).toBeLessThanOrEqual(102);
      expect(pixel(painted, 256, 128)[3]).toBeGreaterThan(90);
      await page.getByTestId('gpu-canvas').focus();
      await page.keyboard.press('e');
      await range(page, 'Brush opacity', 1);
      await range(page, 'Brush flow', 1);
      await page.mouse.click(cx, cy);
      await idle(page, 2);
      const erased = await exportImage(page);
      expect(pixel(erased, 256, 128)).toEqual([51, 102, 204, 0]);
      await page.keyboard.down('Alt');
      await page.mouse.click(cx, cy);
      await page.keyboard.up('Alt');
      await expect(page.getByRole('textbox', { name: 'Color hex' })).toHaveValue('#3366cc00');
      await page.getByTestId('gpu-canvas').focus();
      await page.keyboard.press('x');
      await expect(page.getByRole('textbox', { name: 'Color hex' })).toHaveValue('#ffffff');
      await page.keyboard.press('b');
      await page.mouse.move(cx + 100, cy + 70);
      await page.mouse.down();
      await page.mouse.move(cx + 150, cy + 70);
      await page.keyboard.press('Escape');
      await page.mouse.up();
      await idle(page, 2);
      expect((await exportImage(page)).data.equals(erased.data)).toBe(true);
      await page.getByRole('combobox', { name: 'Stabilizer', exact: true }).selectOption('pulled');
      await page.mouse.move(cx - 180, cy - 70);
      await page.mouse.down();
      await page.mouse.move(cx - 80, cy - 70, { steps: 8 });
      await page.mouse.up();
      await idle(page, 3);
      await page
        .getByRole('combobox', { name: 'Stabilizer', exact: true })
        .selectOption('weighted');
      await page.mouse.move(cx + 80, cy - 70);
      await page.mouse.down();
      await page.mouse.move(cx + 180, cy - 70, { steps: 8 });
      await page.mouse.up();
      await idle(page, 4);
    },
  );
  test(
    backend + ' maps synthetic pen pressure and tilt, and paints through a rotated view',
    async ({ page, context }) => {
      const box = await create(page, query);
      const cx = box.x + box.width / 2,
        cy = box.y + box.height / 2;
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff0000');
      await range(page, 'Brush size', 80);
      const cdp = await context.newCDPSession(page);
      for (const [offset, force] of [
        [-120, 0.25],
        [0, 1],
      ] as const) {
        await cdp.send('Input.dispatchMouseEvent', {
          type: 'mousePressed',
          x: cx + offset,
          y: cy,
          button: 'left',
          buttons: 1,
          pointerType: 'pen',
          force,
        });
        await cdp.send('Input.dispatchMouseEvent', {
          type: 'mouseReleased',
          x: cx + offset,
          y: cy,
          button: 'left',
          buttons: 0,
          pointerType: 'pen',
        });
      }
      await idle(page, 2);
      let png = await exportImage(page);
      expect(pixel(png, 136, 128)).toEqual([255, 0, 0, 255]);
      expect(pixel(png, 156, 128)[3]).toBe(0);
      expect(pixel(png, 276, 128)).toEqual([255, 0, 0, 255]);
      await page.getByText('Pen dynamics & curves', { exact: true }).click();
      await page.getByRole('checkbox', { name: 'Tilt size', exact: true }).check();
      await cdp.send('Input.dispatchMouseEvent', {
        type: 'mousePressed',
        x: cx + 120,
        y: cy,
        button: 'left',
        buttons: 1,
        pointerType: 'pen',
        force: 1,
        tiltX: 90,
      });
      await cdp.send('Input.dispatchMouseEvent', {
        type: 'mouseReleased',
        x: cx + 120,
        y: cy,
        button: 'left',
        buttons: 0,
        pointerType: 'pen',
        tiltX: 90,
      });
      await idle(page, 3);
      png = await exportImage(page);
      expect(pixel(png, 376, 128)[3]).toBe(255);
      expect(pixel(png, 396, 128)[3]).toBe(0);
      await cdp.detach();
      await page.getByRole('button', { name: 'Rotate view 90 degrees' }).click();
      await page.getByRole('button', { name: 'Flip view horizontally' }).click();
      await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Pixel pencil');
      await page.mouse.click(cx, cy + 80);
      await idle(page, 4);
      expect(pixel(await exportImage(page), 176, 128)).toEqual([255, 0, 0, 255]);
      expect((await snap(page)).history.undo).toBe(4);
      expect((await snap(page)).editError).toBeUndefined();
    },
  );
}
