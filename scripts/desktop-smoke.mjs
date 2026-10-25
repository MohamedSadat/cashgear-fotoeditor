import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { chromium, expect } from '@playwright/test';
import { PNG } from 'pngjs';
import { root } from './lib.mjs';

if (process.platform !== 'win32') throw new Error('Desktop smoke currently targets Windows.');
const runtimeKeys = [
  'HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
  'HKLM\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
  'HKCU\\Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
];
const runtimeVersion = runtimeKeys
  .map(
    (key) =>
      spawnSync('reg', ['query', key, '/v', 'pv'], {
        encoding: 'utf8',
        windowsHide: true,
      }).stdout?.match(/REG_SZ\s+([\d.]+)/)?.[1],
  )
  .find((version) => version && version !== '0.0.0.0');
if (!runtimeVersion)
  throw new Error('WebView2 Runtime is missing. Run scripts/setup-webview2.ps1 first.');
console.log('WebView2 Runtime: ' + runtimeVersion);
const listener = createServer();
await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
const port = listener.address().port;
await new Promise((resolve) => listener.close(resolve));
const profile = await mkdtemp(join(tmpdir(), 'fotoeditor-webview2-smoke-'));
const softwareArgs =
  process.env.GPU_TEST_SOFTWARE === '1'
    ? ' --enable-unsafe-webgpu --enable-unsafe-swiftshader --use-angle=d3d11-warp --use-webgpu-adapter=swiftshader'
    : '';
const browserArguments =
  '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1' + softwareArgs;
// Elevated WebView2 hosts ignore environment browser arguments. Use the documented
// HKLM override only on disposable GitHub-hosted runners, scoped to our executable.
// https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security#for-an-elevated-host-app-use-appropriate-override-flags
const policyKey = 'HKLM\\SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2\\AdditionalBrowserArguments';
const policyName = 'fotoeditor-desktop.exe';
const hostedRunner =
  process.env.GITHUB_ACTIONS === 'true' && process.env.RUNNER_ENVIRONMENT === 'github-hosted';
let previousPolicy;
function reg(args) {
  return spawnSync('reg.exe', args, { encoding: 'utf8', windowsHide: true });
}
if (hostedRunner) {
  const existing = reg(['query', policyKey, '/v', policyName, '/reg:64']);
  if (existing.status === 0) {
    previousPolicy = existing.stdout.match(/REG_SZ\s+(.+)/)?.[1];
    if (previousPolicy === undefined) throw new Error('Unexpected existing WebView2 test policy');
  }
  const added = reg([
    'add',
    policyKey,
    '/v',
    policyName,
    '/t',
    'REG_SZ',
    '/d',
    browserArguments,
    '/f',
    '/reg:64',
  ]);
  if (added.status !== 0)
    throw new Error('Could not configure WebView2 test policy: ' + added.stderr);
}
const child = spawn(join(root, 'target/release/fotoeditor-desktop.exe'), [], {
  cwd: root,
  windowsHide: true,
  env: {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: profile,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: browserArguments,
  },
  stdio: 'inherit',
});
let launchError;
child.on('error', (error) => {
  launchError = error;
});
let browser;
try {
  const endpoint = 'http://127.0.0.1:' + port;
  const deadline = Date.now() + 45000;
  while (true) {
    if (launchError) throw launchError;
    if (child.exitCode !== null)
      throw new Error('Desktop exited before debugging was ready: ' + child.exitCode);
    try {
      const response = await fetch(endpoint + '/json/version', {
        signal: AbortSignal.timeout(1000),
      });
      if (response.ok) break;
    } catch {
      /* Wait for WebView2 to create its debugging endpoint. */
    }
    if (Date.now() > deadline) {
      if (process.env.CI) {
        const details = spawnSync(
          'powershell.exe',
          [
            '-NoProfile',
            '-Command',
            '$app = Get-Process -Id ' +
              child.pid +
              ' -ErrorAction SilentlyContinue; ' +
              '$children = Get-CimInstance Win32_Process -Filter "ParentProcessId=' +
              child.pid +
              '"; ' +
              '@{app=($app | Select-Object ProcessName,MainWindowTitle,Responding,SessionId); children=($children | Select-Object Name,ProcessId,CommandLine)} | ConvertTo-Json -Depth 4',
          ],
          { encoding: 'utf8', windowsHide: true },
        );
        await mkdir(join(root, 'bench/results'), { recursive: true });
        await writeFile(
          join(root, 'bench/results/desktop-startup.json'),
          details.stdout || details.stderr || 'No startup diagnostics',
        );
        console.error(details.stdout || details.stderr);
      }
      throw new Error('WebView2 debugging endpoint timed out');
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  browser = await chromium.connectOverCDP(endpoint);
  const context = browser.contexts()[0];
  const page = context.pages()[0] ?? (await context.waitForEvent('page'));
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60000 });
  const origin = new URL(page.url()).origin;
  const dismissRecoveryPrompt = async () => {
    // Engine readiness precedes the asynchronous storage/recovery scan. Decide
    // from its completed catalog, then let click wait for React's dialog effect.
    await page.waitForFunction(() => {
      const snapshot = JSON.parse(
        document.querySelector('[data-testid="gpu-canvas"]')?.dataset.snapshot ?? 'null',
      );
      return snapshot?.recovery && snapshot.recovery.state !== 'starting';
    });
    const count = await page.evaluate(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot).recovery
          .candidates.length,
    );
    if (count > 0) await page.getByRole('button', { name: 'Later', exact: true }).click();
  };
  const results = [];
  for (const backend of ['WebGPU', 'WebGL2']) {
    await page.goto(origin + '/?diagnostic=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60000 });
    await dismissRecoveryPrompt();
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    await expect(page.getByTestId('gpu-canvas')).toHaveAttribute('data-presented', 'true');
    if (!(await page.evaluate(() => crossOriginIsolated)))
      throw new Error('Desktop is not cross-origin isolated');
    const webgpuVendor =
      backend === 'WebGPU'
        ? await page.evaluate(async () => {
            const adapter = await navigator.gpu.requestAdapter();
            return adapter?.info.vendor;
          })
        : undefined;
    if (process.env.GPU_TEST_SOFTWARE === '1' && backend === 'WebGPU') {
      if (webgpuVendor !== 'google') throw new Error('Expected the SwiftShader WebGPU adapter');
    }
    const result = {
      backend,
      webgpuVendor,
      adapter: await page.getByTestId('gpu-backend').getAttribute('title'),
      browser: browser.version(),
      origin,
    };
    await page.goto(origin + (backend === 'WebGL2' ? '/?gpu=webgl2' : '/'));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60000 });
    await dismissRecoveryPrompt();
    await page
      .getByTestId('open-image')
      .setInputFiles(join(root, 'bench/results/fixtures/alpha.png'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
    await expect(page.getByTestId('zoom')).toHaveText('100.0%');
    await page.waitForFunction(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
          .refining === 0,
    );
    const pixels = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
    const sample =
      (Math.floor(pixels.height / 2 - 50) * pixels.width + Math.floor(pixels.width / 2 - 50)) * 4;
    expect([...pixels.data.subarray(sample, sample + 4)]).toEqual([255, 0, 0, 255]);
    result.document = JSON.parse(
      await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'),
    ).document;
    await page.getByRole('button', { name: 'Brush', exact: true }).click();
    await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff00ff');
    const canvasBox = await page.getByTestId('gpu-canvas').boundingBox();
    await page.mouse.move(
      canvasBox.x + canvasBox.width / 2 - 20,
      canvasBox.y + canvasBox.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      canvasBox.x + canvasBox.width / 2 + 20,
      canvasBox.y + canvasBox.height / 2,
    );
    await page.mouse.up();
    await page.waitForFunction(() => {
      const s = JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot);
      return s.history.undo === 1 && !s.painting;
    });
    // Compare document presentation without the pointer's brush-outline overlay.
    await page.mouse.move(20, 20);
    const painted = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
    const middle =
      (Math.floor(painted.height / 2) * painted.width + Math.floor(painted.width / 2)) * 4;
    expect([...painted.data.subarray(middle, middle + 4)]).toEqual([255, 0, 255, 255]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await page.waitForFunction(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot).history
          .undo === 0,
    );
    const undone = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
    expect(undone.data.equals(pixels.data)).toBe(true);
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await page.waitForFunction(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot).history
          .undo === 1,
    );
    const redone = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
    expect(redone.data.equals(painted.data)).toBe(true);
    result.brushUndoRedo = true;
    // Capture the normal download Blob without opening WebView2's native save UI.
    // This instruments browser download APIs only; the actual Rust save/load runs.
    await page.evaluate(() => {
      const create = URL.createObjectURL.bind(URL),
        click = HTMLAnchorElement.prototype.click;
      URL.createObjectURL = (blob) => {
        if (blob.type !== 'application/octet-stream') return create(blob);
        const reader = new FileReader();
        reader.onload = () => {
          window.__projectPayload = reader.result;
        };
        reader.readAsDataURL(blob);
        return create(blob);
      };
      HTMLAnchorElement.prototype.click = function () {
        if (!this.download.endsWith('.cgfoto')) click.call(this);
      };
    });
    async function savedProject() {
      await page.evaluate(() => {
        window.__projectPayload = undefined;
      });
      await page.getByRole('button', { name: 'Save project', exact: true }).click();
      await page.waitForFunction(() => typeof window.__projectPayload === 'string');
      const data = await page.evaluate(() => window.__projectPayload);
      const bytes = Buffer.from(data.split(',')[1], 'base64');
      expect(bytes.subarray(0, 8).toString()).toBe('CGFOTO\r\n');
      return bytes;
    }
    const project = await savedProject();
    const projectPath = join(root, 'bench/results/desktop-project-' + backend + '.cgfoto');
    await writeFile(projectPath, project);
    await page.getByTestId('open-project').setInputFiles(projectPath);
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    await page.waitForFunction(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot).history
          .undo === 0,
    );
    expect((await savedProject()).equals(project)).toBe(true);
    result.projectRoundtrip = true;
    await page.getByRole('button', { name: 'Save recovery now', exact: true }).click();
    await expect(page.getByTestId('recovery-status')).toHaveText(/^Autosaved /);
    await page.reload();
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60000 });
    await page.getByRole('button', { name: 'Restore session', exact: true }).click();
    await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
    await expect(page.getByTestId('zoom')).toHaveText('100.0%');
    await page.waitForFunction(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
          .refining === 0,
    );
    const restored = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot());
    expect(restored.data.equals(painted.data)).toBe(true);
    result.autosaveRestore = true;
    await page.goto(
      origin + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
    );
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60000 });
    await dismissRecoveryPrompt();
    const golden = await page.evaluate(async () =>
      Reflect.get(window, '__fotoeditor').compareCompositors(),
    );
    expect(golden).toHaveLength(64);
    expect(Math.max(...golden)).toBeLessThanOrEqual(1);
    result.gpuGoldenMaxError = Math.max(...golden);
    await page
      .getByTestId('open-image')
      .setInputFiles(join(root, 'bench/results/fixtures/layers-small.cgfoto'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    result.layerStream = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor');
      const directory = await navigator.storage.getDirectory();
      const handle = await directory.getFileHandle('desktop-layer-project.cgfoto', {
        create: true,
      });
      const size = await api.saveProjectFile(handle);
      const first = new Uint8Array(await (await handle.getFile()).arrayBuffer());
      const state = await api.openProjectFile(await handle.getFile());
      await api.saveProjectFile(handle);
      const second = new Uint8Array(await (await handle.getFile()).arrayBuffer());
      return {
        size,
        layers: state.layers.nodes.length,
        history: state.history.undo,
        exact: first.length === second.length && first.every((b, i) => b === second[i]),
      };
    });
    expect(result.layerStream.layers).toBe(3);
    expect(result.layerStream.history).toBe(0);
    expect(result.layerStream.exact).toBe(true);
    await page.waitForFunction(
      () =>
        JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
          .visibleTiles > 0,
    );
    result.selection = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor');
      const hash = async () =>
        Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())))
          .map((b) => b.toString(16).padStart(2, '0'))
          .join('');
      await api.newDocument(520, 64, 72);
      await api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 255, y0: 4, x1: 258, y1: 44 },
        mode: 'new',
        antialias: false,
      });
      await api.selectionAction({ kind: 'fill', color: [255, 0, 0, 255] });
      const before = await hash();
      await api.quickMask(true);
      await api.selectionAction({ kind: 'fill', color: [255, 255, 255, 255] });
      const quickMaskPreservesPixels = before === (await hash());
      await api.historyJump(2);
      await api.quickMask(false);
      await api.selectionAction({ kind: 'cut', allLayers: false });
      const cut = await hash();
      const pasted = await api.selectionAction({ kind: 'paste', inPlace: true });
      const exactPaste = before === (await hash());
      await api.historyJump(2);
      const restoredSelection = await api.quickMask(false);
      return {
        quickMaskPreservesPixels,
        cutChangesPixels: before !== cut,
        exactPaste,
        layers: pasted.layers.nodes.length,
        exactUndo: before === (await hash()),
        selectionRestored: restoredSelection.selection.active,
      };
    });
    expect(result.selection).toEqual({
      quickMaskPreservesPixels: true,
      cutChangesPixels: true,
      exactPaste: true,
      layers: 2,
      exactUndo: true,
      selectionRestored: true,
    });
    result.adjustments = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor');
      const hash = async () =>
        Array.from(
          new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())),
        ).join(',');
      await api.newDocument(513, 3, 72);
      await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 128] });
      await api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 255.5, y0: 0, x1: 257.5, y1: 3 },
        mode: 'new',
        antialias: true,
      });
      const before = await hash();
      await api.applyAdjustment({
        kind: 'curves',
        points: [
          [0, 1],
          [1, 0],
        ],
      });
      const after = await hash();
      await api.historyJump(2);
      const exactUndo = before === (await hash());
      await api.historyJump(3);
      const exactRedo = after === (await hash());
      const h = await api.histogram(
        (await api.selectionAction({ kind: 'none' })).layers.nodes[0].id,
      );
      return {
        changed: before !== after,
        exactUndo,
        exactRedo,
        pixels: h.pixels,
        weight: h.weight,
      };
    });
    expect(result.adjustments).toEqual({
      changed: true,
      exactUndo: true,
      exactRedo: true,
      pixels: 1539,
      weight: 1539 * 128,
    });
    result.adjustmentLayers = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor');
      const hash = async () =>
        Array.from(
          new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())),
        ).join(',');
      const state = await api.newDocument(128, 128, 72);
      await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
      const original = await hash();
      const timing = await api.previewAdjustment(
        { kind: 'exposure', stops: 1, offset: 0, gamma: 1 },
        { kind: 'pixels', target: state.layers.nodes[0].id },
      );
      const previewDoesNotMutate = (await hash()) === original;
      await api.endAdjustmentPreview();
      const added = await api.layerAction({
        kind: 'add-adjustment',
        parent: null,
        index: 1,
        adjustment: { kind: 'exposure', stops: 1, offset: 0, gamma: 1 },
      });
      const adjusted = await hash();
      const edited = await api.layerAction({
        kind: 'set-adjustment',
        target: added.layers.nodes[1].id,
        adjustment: { kind: 'exposure', stops: 0, offset: 0, gamma: 1 },
      });
      const identity = (await hash()) === original;
      await api.historyJump(2);
      const exactUndo = (await hash()) === adjusted;
      const directory = await navigator.storage.getDirectory();
      const handle = await directory.getFileHandle('desktop-adjustment.cgfoto', { create: true });
      await api.saveProjectFile(handle);
      const reopened = await api.openProjectFile(await handle.getFile());
      return {
        previewDoesNotMutate,
        previewWidth: timing.width,
        changed: adjusted !== original,
        identity,
        exactUndo,
        settings: reopened.layers.nodes[1].adjustment,
        exactRoundtrip: (await hash()) === adjusted,
        steps: edited.history.undo,
      };
    });
    expect(result.adjustmentLayers.previewDoesNotMutate).toBe(true);
    expect(result.adjustmentLayers.previewWidth).toBeGreaterThan(0);
    expect(result.adjustmentLayers.changed).toBe(true);
    expect(result.adjustmentLayers.identity).toBe(true);
    expect(result.adjustmentLayers.exactUndo).toBe(true);
    expect(result.adjustmentLayers.exactRoundtrip).toBe(true);
    expect(result.adjustmentLayers.settings).toEqual({
      kind: 'exposure',
      stops: 1,
      offset: 0,
      gamma: 1,
    });
    result.remainingAdjustments = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor');
      const hash = async () =>
        Array.from(
          new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())),
        ).join(',');
      const checks = [];
      for (const adjustment of [
        { kind: 'vibrance', amount: -0.7 },
        {
          kind: 'color-balance',
          shadows: [-0.3, 0.1, 0.4],
          midtones: [0.2, -0.4, 0.1],
          highlights: [0.3, 0.1, -0.2],
          preserveLuminosity: true,
        },
        { kind: 'black-white', weights: [0.3, 0.89, 0.59, 0.7, 0.11, 0.41] },
        { kind: 'invert' },
        { kind: 'threshold', level: 128 },
        { kind: 'posterize', levels: 4 },
        {
          kind: 'gradient-map',
          stops: [
            [0, 1, 0, 0],
            [1, 0, 0, 1],
          ],
        },
      ]) {
        const doc = await api.newDocument(128, 128, 72);
        await api.selectionAction({ kind: 'fill', color: [128, 64, 0, 255] });
        const original = await hash();
        await api.previewAdjustment(adjustment, { kind: 'pixels', target: doc.layers.nodes[0].id });
        const previewDoesNotMutate = (await hash()) === original;
        await api.endAdjustmentPreview();
        await api.applyAdjustment(adjustment);
        const destructive = await hash();
        await api.historyJump(1);
        const exactUndo = (await hash()) === original;
        const added = await api.layerAction({
          kind: 'add-adjustment',
          parent: null,
          index: 1,
          adjustment,
        });
        const layered = await hash();
        const directory = await navigator.storage.getDirectory();
        const handle = await directory.getFileHandle('desktop-remaining.cgfoto', { create: true });
        await api.saveProjectFile(handle);
        const reopened = await api.openProjectFile(await handle.getFile());
        checks.push({
          kind: adjustment.kind,
          previewDoesNotMutate,
          changed: destructive !== original,
          exactUndo,
          layerMatches: layered === destructive,
          exactRoundtrip: (await hash()) === layered,
          settingsPreserved:
            JSON.stringify(reopened.layers.nodes[1].adjustment) ===
            JSON.stringify(added.layers.nodes[1].adjustment),
        });
      }
      return checks;
    });
    expect(result.remainingAdjustments).toHaveLength(7);
    for (const check of result.remainingAdjustments)
      for (const key of [
        'previewDoesNotMutate',
        'changed',
        'exactUndo',
        'layerMatches',
        'exactRoundtrip',
        'settingsPreserved',
      ])
        expect(check[key], check.kind + ' ' + key).toBe(true);
    result.filters = await page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor');
      const hash = async () =>
        Array.from(
          new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())),
        ).join(',');
      const s = await api.newDocument(513, 3, 72);
      await api.selectionAction({ kind: 'fill', color: [64, 64, 64, 255] });
      await api.selectionAction({
        kind: 'shape',
        shape: { kind: 'rectangle', x0: 255, y0: 0, x1: 256, y1: 3 },
        mode: 'new',
        antialias: false,
      });
      await api.selectionAction({ kind: 'fill', color: [128, 128, 128, 255] });
      const baseline = await api.selectionAction({ kind: 'none' });
      const original = await hash();
      const checks = [];
      for (const filter of [
        { kind: 'gaussian', radius: 20 },
        { kind: 'box-blur', radius: 3 },
        { kind: 'unsharp-mask', radius: 3, amount: 1, threshold: 0 },
        { kind: 'sharpen', amount: 1 },
        { kind: 'median', radius: 1 },
        { kind: 'motion-blur', radius: 7, angle: 31 },
        { kind: 'high-pass', radius: 3 },
        { kind: 'add-noise', amount: 0.4, seed: 123, monochrome: false, distribution: 'gaussian' },
        { kind: 'emboss', radius: 4, angle: -41, amount: 1.3 },
        { kind: 'pixelate', size: 37 },
      ]) {
        await api.previewFilter(filter, s.layers.nodes[0].id);
        const previewDoesNotMutate = (await hash()) === original;
        await api.endAdjustmentPreview();
        const applied = await api.applyFilter(filter);
        const filtered = await hash();
        await api.fadeFilter(0, 'normal');
        const exactFadeZero = (await hash()) === original;
        await api.fadeFilter(1, 'normal');
        const exactFadeOne = (await hash()) === filtered;
        await api.historyJump(baseline.history.undo);
        const exactUndo = (await hash()) === original;
        checks.push({
          kind: filter.kind,
          previewDoesNotMutate,
          changed: filtered !== original,
          canFade: applied.canFade,
          exactFadeZero,
          exactFadeOne,
          exactUndo,
        });
      }
      await api.applyFilter({ kind: 'gaussian', radius: 3 });
      await api.fadeFilter(0.5, 'multiply');
      const faded = await hash();
      const bytes = await api.saveProject();
      const reopened = await api.openProject(bytes.slice().buffer, 'desktop-filters.cgfoto');
      return { checks, exactRoundtrip: (await hash()) === faded, clearedFade: !reopened.canFade };
    });
    expect(result.filters.checks).toHaveLength(10);
    for (const check of result.filters.checks) {
      for (const key of [
        'previewDoesNotMutate',
        'changed',
        'canFade',
        'exactFadeZero',
        'exactFadeOne',
        'exactUndo',
      ])
        expect(check[key], check.kind + ' ' + key).toBe(true);
    }
    expect(result.filters.exactRoundtrip).toBe(true);
    expect(result.filters.clearedFade).toBe(true);
    if (process.env.P4_DESKTOP_DENSE === '1' && backend === 'WebGPU') {
      page.setDefaultTimeout(360000);
      const started = performance.now();
      // CDP-connected WebView2 is local, but Playwright's remote-upload helper
      // refuses files above 50 MB. Let the local browser read the actual file.
      const session = await context.newCDPSession(page);
      const { root: dom } = await session.send('DOM.getDocument');
      const { nodeId } = await session.send('DOM.querySelector', {
        nodeId: dom.nodeId,
        selector: '[data-testid="open-image"]',
      });
      await session.send('DOM.setFileInputFiles', {
        nodeId,
        files: [join(root, 'bench/results/fixtures/layers-50.cgfoto')],
      });
      await session.detach();
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/, {
        timeout: 360000,
      });
      result.denseOpenMs = performance.now() - started;
      result.denseStream = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor');
        const digest = async () =>
          Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())))
            .map((b) => b.toString(16).padStart(2, '0'))
            .join('');
        const before = await digest();
        await api.autosave();
        const deadline = performance.now() + 360000;
        for (;;) {
          const state = JSON.parse(
            document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
          );
          if (state.recovery?.state !== 'saving' && !state.operation) break;
          if (performance.now() > deadline) throw new Error('Dense autosave did not become idle');
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
        const directory = await navigator.storage.getDirectory();
        const handle = await directory.getFileHandle('desktop-dense-output.cgfoto', {
          create: true,
        });
        const start = performance.now();
        const size = await api.saveProjectFile(handle);
        const saveMs = performance.now() - start;
        const opened = await api.openProjectFile(await handle.getFile());
        const after = await digest();
        return {
          size,
          saveMs,
          layers: opened.layers.nodes.length,
          tiles: opened.document.tiles,
          memory: opened.memory,
          pngSha256: after,
          exactComposite: before === after,
        };
      });
      expect(result.denseStream.layers).toBe(50);
      expect(result.denseStream.tiles).toBe(9600);
      expect(result.denseStream.exactComposite).toBe(true);
      expect(result.denseStream.size).toBeGreaterThan(2400 * 1048576);
    }
    await page.goto(
      origin +
        (backend === 'WebGL2'
          ? '/?diagnostic=1&document=1&gpu=webgl2'
          : '/?diagnostic=1&document=1'),
    );
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60000 });
    await dismissRecoveryPrompt();
    result.canvas = await page.evaluate(async () => {
      const api = window.__fotoeditor;
      const digest = async () =>
        Array.from(
          new Uint8Array(await crypto.subtle.digest('SHA-256', await api.exportPng())),
        ).join(',');
      await api.newDocument(32, 24, 72);
      await api.selectionAction({ kind: 'fill', color: [50, 100, 150, 128] });
      const before = await digest();
      const cropped = await api.geometryMetadata({
        kind: 'crop',
        rect: { x: 3, y: 5, width: 17, height: 13 },
      });
      await api.geometryMetadata({
        kind: 'guides',
        guides: [{ axis: 'horizontal', position: 7.25 }],
      });
      const dir = await navigator.storage.getDirectory();
      const handle = await dir.getFileHandle('desktop-crop.cgfoto', { create: true });
      await api.saveProjectFile(handle);
      await api.newDocument(1, 1, 72);
      const reopened = await api.openProjectFile(await handle.getFile());
      const revealed = await api.geometryMetadata({ kind: 'crop', rect: null });
      return {
        cropped: cropped.document,
        reopened: reopened.document,
        history: reopened.history.undo,
        revealed: revealed.document,
        exactReveal: before === (await digest()),
      };
    });
    expect(result.canvas.cropped).toMatchObject({
      width: 17,
      height: 13,
      retainedWidth: 32,
      retainedHeight: 24,
    });
    expect(result.canvas.reopened).toMatchObject({
      width: 17,
      height: 13,
      originX: 3,
      originY: 5,
      guides: [{ axis: 'horizontal', position: 7.25 }],
    });
    expect(result.canvas.history).toBe(0);
    expect(result.canvas.revealed).toMatchObject({ width: 32, height: 24, cropped: false });
    expect(result.canvas.exactReveal).toBe(true);
    result.software = process.env.GPU_TEST_SOFTWARE === '1';
    await mkdir(join(root, 'bench/results'), { recursive: true });
    await page.screenshot({ path: join(root, 'bench/results/desktop-p4-' + backend + '.png') });
    results.push(result);
  }
  await mkdir(join(root, 'bench/results'), { recursive: true });
  await page.screenshot({ path: join(root, 'bench/results/desktop.png') });
  await writeFile(
    join(root, 'bench/results/desktop-smoke.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(JSON.stringify(results, null, 2));
} finally {
  await browser?.close().catch((error) => {
    console.error('Could not close the desktop browser: ' + error.message);
    process.exitCode = 1;
  });
  child.kill();
  if (hostedRunner) {
    const restored = reg(
      previousPolicy === undefined
        ? ['delete', policyKey, '/v', policyName, '/f', '/reg:64']
        : [
            'add',
            policyKey,
            '/v',
            policyName,
            '/t',
            'REG_SZ',
            '/d',
            previousPolicy,
            '/f',
            '/reg:64',
          ],
    );
    if (restored.status !== 0) {
      console.error('Could not restore WebView2 test policy: ' + restored.stderr);
      process.exitCode = 1;
    }
  }
  if (profile.startsWith(join(tmpdir(), 'fotoeditor-webview2-smoke-'))) {
    await rm(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }).catch(
      (error) => console.warn('Temporary WebView2 profile cleanup: ' + error.message),
    );
  }
}
