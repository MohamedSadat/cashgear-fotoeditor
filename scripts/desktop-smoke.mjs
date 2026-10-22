import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { chromium, expect } from '@playwright/test';
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
// Hosted runners launch the trusted test shell in an elevated, non-user session.
// Match Playwright's test-only sandbox configuration; normal app launches do not
// receive these switches or expose a debugging endpoint.
const ciArgs = process.env.CI ? ' --no-sandbox --enable-logging=stderr' : '';
const child = spawn(join(root, 'target/release/fotoeditor-desktop.exe'), [], {
  cwd: root,
  windowsHide: true,
  env: {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: profile,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
      '--remote-debugging-port=' +
      port +
      ' --remote-debugging-address=127.0.0.1' +
      softwareArgs +
      ciArgs,
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
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 30000 });
  const origin = new URL(page.url()).origin;
  const results = [];
  for (const backend of ['WebGPU', 'WebGL2']) {
    await page.goto(origin + '/?diagnostic=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 30000 });
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
    results.push({
      backend,
      webgpuVendor,
      adapter: await page.getByTestId('gpu-backend').getAttribute('title'),
      browser: browser.version(),
      origin,
    });
  }
  await page.goto(origin);
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await mkdir(join(root, 'bench/results'), { recursive: true });
  await page.screenshot({ path: join(root, 'bench/results/desktop.png') });
  await writeFile(
    join(root, 'bench/results/desktop-smoke.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(JSON.stringify(results, null, 2));
} finally {
  await browser?.close();
  child.kill();
  if (profile.startsWith(join(tmpdir(), 'fotoeditor-webview2-smoke-'))) {
    await rm(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }).catch(
      (error) => console.warn('Temporary WebView2 profile cleanup: ' + error.message),
    );
  }
}
