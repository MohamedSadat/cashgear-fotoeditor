import { expect, test } from '@playwright/test';
import { PNG } from 'pngjs';

for (const fault of ['adapter', 'device']) {
  test('falls back when WebGPU ' + fault + ' creation is unavailable', async ({ page }) => {
    await page.goto('/?diagnostic=1&fault=' + fault);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText('WebGL2');
    await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-presented', 'true');
  });
}

test('pauses a zero-sized viewport and resumes rendering', async ({ page }) => {
  await page.goto('/?diagnostic=1');
  const canvas = page.getByTestId('gpu-canvas');
  await expect(canvas).toHaveAttribute('data-presented', 'true');
  await canvas.evaluate((element) => {
    element.parentElement!.style.width = '0px';
    element.parentElement!.style.height = '0px';
  });
  await expect(canvas).toHaveAttribute('data-width', '0');
  await page.waitForTimeout(100);
  const count = await canvas.getAttribute('data-frames');
  await page.waitForTimeout(200);
  await expect(canvas).toHaveAttribute('data-frames', count!);
  await canvas.evaluate((element) => {
    element.parentElement!.style.width = '';
    element.parentElement!.style.height = '';
  });
  await expect.poll(() => canvas.getAttribute('data-frames')).not.toBe(count);
});

test('updates actual backing dimensions when device pixel ratio changes', async ({ page }) => {
  await page.goto('/?diagnostic=1');
  const canvas = page.getByTestId('gpu-canvas');
  await expect(canvas).toHaveAttribute('data-presented', 'true');
  const session = await page.context().newCDPSession(page);
  await session.send('Emulation.setDeviceMetricsOverride', {
    width: 1000,
    height: 700,
    deviceScaleFactor: 2,
    mobile: false,
  });
  await expect
    .poll(() =>
      canvas.evaluate(
        (element) =>
          Number(element.getAttribute('data-render-width')) ===
            Math.round(element.getBoundingClientRect().width * 2) &&
          Number(element.getAttribute('data-render-height')) ===
            Math.round(element.getBoundingClientRect().height * 2),
      ),
    )
    .toBe(true);
  await session.detach();
});

for (const backend of ['WebGPU', 'WebGL2'] as const) {
  const query = '?diagnostic=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(backend + ' presents Rust-uploaded texture samples', async ({ page }, testInfo) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    const response = await page.goto('/' + query);
    expect(response?.headers()['cross-origin-opener-policy']).toBe('same-origin');
    expect(response?.headers()['cross-origin-embedder-policy']).toBe('require-corp');
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    if (testInfo.project.name === 'software-webgl2') {
      await expect(page.getByTestId('gpu-backend')).toHaveAttribute('title', /SwiftShader/);
    }
    if (testInfo.project.name === 'software-webgpu' && backend === 'WebGPU') {
      const vendor = await page.evaluate(async () => {
        const gpu = Reflect.get(navigator, 'gpu') as {
          requestAdapter: () => Promise<{ info: { vendor: string } }>;
        };
        return (await gpu.requestAdapter()).info.vendor;
      });
      expect(vendor).toBe('google');
    }
    await testInfo.attach('adapter', {
      body: JSON.stringify({
        backend,
        adapter: await page.getByTestId('gpu-backend').getAttribute('title'),
        browser: page.context().browser()?.version(),
      }),
      contentType: 'application/json',
    });
    const canvas = page.getByTestId('gpu-canvas');
    await expect(canvas).toHaveAttribute('data-presented', 'true');
    const image = PNG.sync.read(await canvas.screenshot());
    const samples = [
      [0.25, 0.25, [255, 0, 0]],
      [0.75, 0.25, [0, 255, 0]],
      [0.25, 0.75, [0, 0, 255]],
      [0.75, 0.75, [255, 255, 255]],
    ] as const;
    for (const [x, y, expected] of samples) {
      const index = (Math.floor(image.height * y) * image.width + Math.floor(image.width * x)) * 4;
      expected.forEach((value, channel) =>
        expect(Math.abs(image.data[index + channel] - value)).toBeLessThanOrEqual(1),
      );
    }
    expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
    expect(errors).toEqual([]);
  });
  test(backend + ' handles resize, loss and a fresh-canvas retry', async ({ page }) => {
    await page.goto('/' + query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.setViewportSize({ width: 1024, height: 700 });
    const canvas = page.getByTestId('gpu-canvas');
    await expect
      .poll(async () =>
        canvas.evaluate((element) => {
          const rect = element.getBoundingClientRect();
          return (
            element.getAttribute('data-width') === String(Math.round(rect.width * devicePixelRatio))
          );
        }),
      )
      .toBe(true);
    await page.getByRole('button', { name: 'Simulate device loss' }).click();
    await expect(page.getByRole('alert')).toContainText('GPU device lost');
    await page.getByRole('button', { name: 'Retry engine' }).click();
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    await expect(page.getByTestId('gpu-canvas')).toHaveCount(1);
    await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-presented', 'true');
  });
}

test('initialization failure can be retried', async ({ page }) => {
  await page.goto('/?diagnostic=1&fault=init');
  await expect(page.getByRole('alert')).toContainText('Diagnostic initialization failure');
  await page.getByRole('button', { name: 'Retry engine' }).click();
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
});

test('Rust panic is surfaced and its worker is replaced on retry', async ({ page }) => {
  await page.goto('/?diagnostic=1');
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await page.getByRole('button', { name: 'Simulate Rust panic' }).click();
  await expect(page.getByRole('alert')).toContainText('Rust panic');
  await page.getByRole('button', { name: 'Retry engine' }).click();
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
});

test('DPR 2 uses physical pixels and WASM is served with its correct MIME type', async ({
  browser,
  baseURL,
}) => {
  const context = await browser.newContext({
    viewport: { width: 1000, height: 700 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();
  const wasmResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname.endsWith('.wasm'),
  );
  await page.goto(baseURL + '/?diagnostic=1');
  expect((await wasmResponse).headers()['content-type']).toContain('application/wasm');
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await expect
    .poll(() =>
      page
        .getByTestId('gpu-canvas')
        .evaluate(
          (element) =>
            Number(element.getAttribute('data-width')) ===
            Math.round(element.getBoundingClientRect().width * 2),
        ),
    )
    .toBe(true);
  await context.close();
});

test('normal workspace clears its canvas through the GPU', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-presented', 'true');
  await expect(page.getByRole('heading', { name: 'A fresh start.' })).toBeVisible();
});
