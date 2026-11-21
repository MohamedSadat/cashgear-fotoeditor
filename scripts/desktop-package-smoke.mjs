import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync } from 'node:fs';
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';
import { installerFor, installerName, tauri, writeReleaseAssets } from './desktop-release.mjs';
import { root, run } from './lib.mjs';

const baseline = '0.90.0';
const upgrade = '0.90.1';
const id = 'com.cashgear.fotoeditor.acceptance';
const feed = 'http://127.0.0.1:4193';
const assets = resolve(process.argv[3] ?? join(root, 'bench/results/desktop-acceptance'));
const binary = 'fotoeditor-desktop.exe';
const extensions = ['cgfoto', 'png', 'jpg', 'jpeg'];
const software = process.env.GPU_TEST_SOFTWARE === '1';
const hosted = process.env.RUNNER_ENVIRONMENT === 'github-hosted';

async function prepare() {
  await mkdir(join(root, '.tools'), { recursive: true });
  const scratch = await mkdtemp(join(root, '.tools', 'desktop-acceptance-'));
  const key = join(scratch, 'acceptance.key');
  const config = join(scratch, 'tauri.conf.json');
  try {
    await tauri(['signer', 'generate', '--ci', '--write-keys', key]);
    const pubkey = (await readFile(key + '.pub', 'utf8')).trim();
    const env = {
      ...process.env,
      TAURI_SIGNING_PRIVATE_KEY: (await readFile(key, 'utf8')).trim(),
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '',
    };
    await mkdir(assets, { recursive: true });
    for (const version of [baseline, upgrade]) {
      await writeFile(
        config,
        JSON.stringify({
          productName: 'CashGear FotoEditor Acceptance',
          version,
          identifier: id,
          bundle: { createUpdaterArtifacts: true },
          plugins: {
            updater: {
              endpoints: [feed + '/latest.json'],
              pubkey,
              dangerousInsecureTransportProtocol: true,
            },
          },
        }),
      );
      await tauri(['build', '--bundles', 'nsis', '--config', config], { env });
      const installer = await installerFor(version);
      if (version === baseline) await copyFile(installer, join(assets, 'baseline.exe'));
      else await writeReleaseAssets(installer, version, assets, feed + '/' + installerName, env);
    }
    await copyFile(
      join(root, 'bench/results/fixtures/layers-small.cgfoto'),
      join(assets, 'source.cgfoto'),
    );
  } finally {
    for (const path of [config, key, key + '.pub']) await rm(path, { force: true });
    await rm(scratch, { recursive: true });
  }
}

function powershell(command, env) {
  return execFileSync('powershell.exe', ['-NoProfile', '-Command', command], {
    env,
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
}

async function waitFor(action, timeout = 60_000) {
  const deadline = performance.now() + timeout;
  for (;;) {
    const result = await action();
    if (result) return result;
    if (performance.now() >= deadline) throw new Error('Timed out waiting for the desktop app.');
    await delay(250);
  }
}

async function verify() {
  const scratch = await mkdtemp(join(tmpdir(), 'fotoeditor-setup-'));
  const install = join(scratch, 'install');
  const exe = join(install, binary);
  const uninstaller = join(install, 'uninstall.exe');
  const document = join(scratch, 'Edited project é.cgfoto');
  const forwarded = join(scratch, 'Forwarded project.cgfoto');
  const packageBytes = await readFile(join(assets, installerName));
  const signedFeed = await readFile(join(assets, 'latest.json'), 'utf8');
  const modified = JSON.parse(signedFeed);
  modified.manifest = modified.manifest.replace(upgrade, '0.91.0');
  const corrupted = Buffer.from(packageBytes);
  corrupted[corrupted.length - 1] ^= 1;
  let mode = 'bad-manifest';
  const server = createServer((request, response) => {
    if (request.url === '/latest.json') {
      response.setHeader('Content-Type', 'application/json');
      response.end(mode === 'bad-manifest' ? JSON.stringify(modified) : signedFeed);
    } else if (request.url === '/' + installerName) {
      response.end(mode === 'bad-package' ? corrupted : packageBytes);
    } else {
      response.statusCode = 404;
      response.end();
    }
  });
  const debug = createServer();
  await new Promise((resolve, reject) => debug.listen(0, '127.0.0.1', resolve).on('error', reject));
  const port = debug.address().port;
  await new Promise((resolve) => debug.close(resolve));
  const extra = software
    ? ' --enable-unsafe-webgpu --enable-unsafe-swiftshader --use-angle=swiftshader --use-webgpu-adapter=swiftshader'
    : '';
  const args = '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1' + extra;
  const env = {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: join(scratch, 'profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: args,
    FOTOEDITOR_TEST_BINARY: exe,
  };
  // RunAsUser can discard the elevated runner's environment at restart. A fresh
  // runner can use the installed application's default profile for both launches.
  if (hosted) delete env.WEBVIEW2_USER_DATA_FOLDER;
  const policy = 'HKLM\\SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2\\AdditionalBrowserArguments';
  const registry = (...data) =>
    execFileSync('reg.exe', data, {
      encoding: 'utf8',
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  let priorPolicy;
  let policyChanged = false;
  const getProcess = () =>
    Number(
      powershell(
        '(Get-Process fotoeditor-desktop -ErrorAction SilentlyContinue | Where-Object Path -eq $env:FOTOEDITOR_TEST_BINARY | Select-Object -First 1).Id; exit 0',
        env,
      ),
    );
  const closeApp = () =>
    powershell(
      'Get-Process fotoeditor-desktop -ErrorAction SilentlyContinue | Where-Object Path -eq $env:FOTOEDITOR_TEST_BINARY | ForEach-Object { if (!$_.CloseMainWindow() -or !$_.WaitForExit(10000)) { $_.Kill(); $_.WaitForExit() } }; exit 0',
      env,
    );
  const fileAssociation = (extension) => 'HKCU\\Software\\Classes\\.' + extension;
  const openWith = (extension) => fileAssociation(extension) + '\\OpenWithProgids';
  const associated = (extension) =>
    registry('query', openWith(extension), '/v', id + '.' + extension);
  const associationsRemoved = () =>
    extensions.every((extension) => {
      try {
        associated(extension);
        return false;
      } catch {
        return true;
      }
    });
  const shellOpen = (path) =>
    powershell('Invoke-Item -LiteralPath $env:FOTOEDITOR_TEST_DOCUMENT', {
      ...env,
      FOTOEDITOR_TEST_DOCUMENT: path,
    });
  const connect = async () => {
    await waitFor(async () => {
      try {
        const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
          signal: AbortSignal.timeout(1000),
        });
        return response.ok;
      } catch {
        return false;
      }
    });
    const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? (await context.waitForEvent('page'));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', { timeout: 60_000 });
    return { browser, page };
  };

  let browser;
  try {
    await new Promise((resolve, reject) =>
      server.listen(4193, '127.0.0.1', resolve).on('error', reject),
    );
    if (hosted) {
      try {
        priorPolicy = registry('query', policy, '/v', binary, '/reg:64').match(
          /REG_SZ\s+(.+)/,
        )?.[1];
      } catch {
        /* No prior WebView2 override on a fresh runner. */
      }
      registry('add', policy, '/v', binary, '/t', 'REG_SZ', '/d', args, '/f', '/reg:64');
      policyChanged = true;
    }
    await copyFile(join(assets, 'source.cgfoto'), document);
    await copyFile(document, forwarded);
    const before = await readFile(document);
    const startInstall = performance.now();
    await run(join(assets, 'baseline.exe'), ['/S', '/D=' + install], {
      env,
      windowsVerbatimArguments: true,
    });
    const installMs = performance.now() - startInstall;
    for (const extension of extensions) associated(extension);
    expect(registry('query', fileAssociation('cgfoto'), '/ve')).toContain(id + '.cgfoto');
    const startOpen = performance.now();
    shellOpen(document);
    let connection = await connect();
    ({ browser } = connection);
    let { page } = connection;
    const pid = getProcess();
    expect(pid).toBeGreaterThan(0);
    await expect(page.getByTestId('document-name')).toHaveText(basename(document));
    const openMs = performance.now() - startOpen;
    const adapter = await page.getByTestId('gpu-backend').getAttribute('title');
    await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
    await page.waitForFunction(() => {
      const canvas = document.querySelector('[data-testid="gpu-canvas"]');
      return JSON.parse(canvas.dataset.snapshot).history.undo === 1;
    });
    const startSave = performance.now();
    await page.getByRole('button', { name: 'Save project', exact: true }).click();
    await expect(page.getByTestId('operation-status')).toContainText('Project saved');
    const saveMs = performance.now() - startSave;
    expect((await readFile(document)).equals(before)).toBe(false);
    shellOpen(forwarded);
    await expect(
      page.getByRole('heading', { name: 'Open ' + basename(forwarded) + '?' }),
    ).toBeVisible();
    await page.getByRole('button', { name: 'Open file', exact: true }).click();
    await expect(page.getByTestId('document-name')).toHaveText(basename(forwarded));
    expect(getProcess()).toBe(pid);

    const updates = page.getByRole('button', { name: 'Updates…', exact: true });
    const dialog = page.getByRole('dialog', { name: 'CashGear FotoEditor updates' });
    await updates.click();
    await expect(dialog.getByRole('status')).toContainText('manifest signature did not verify');
    await dialog.getByRole('button', { name: 'Close' }).click();

    mode = 'bad-package';
    await updates.click();
    await expect(dialog).toContainText('Version ' + upgrade + ' is available');
    await dialog.getByRole('button', { name: 'Install and restart' }).click();
    await expect(dialog.getByRole('status')).toContainText(/signature|verification/i, {
      timeout: 45_000,
    });
    // A status while the package is still downloading cannot satisfy rejection.
    await expect(dialog.getByRole('button', { name: 'Close' })).toBeEnabled();
    expect(getProcess()).toBe(pid);
    await dialog.getByRole('button', { name: 'Close' }).click();

    mode = 'valid';
    await updates.click();
    await expect(dialog).toContainText('Version ' + upgrade + ' is available');
    const startUpdate = performance.now();
    await dialog.getByRole('button', { name: 'Install and restart' }).click();
    await waitFor(() => {
      const next = getProcess();
      return next && next !== pid;
    }, 120_000);
    const updateMs = performance.now() - startUpdate;
    await browser.close().catch(() => {});
    connection = await connect();
    ({ browser, page } = connection);
    const recovery = page.getByRole('dialog', { name: 'Restore last session?' });
    await expect(recovery).toBeVisible({ timeout: 30_000 });
    await recovery.getByRole('button', { name: 'Restore session', exact: true }).click();
    await expect(page.getByTestId('document-name')).toHaveText(basename(forwarded));
    await expect(
      page.getByRole('dialog', { name: 'Open ' + basename(document) + '?' }),
    ).toHaveCount(0);
    await page.getByRole('button', { name: 'Updates…', exact: true }).click();
    const updated = page.getByRole('dialog', { name: 'CashGear FotoEditor updates' });
    await expect(updated).toContainText('Installed version ' + upgrade);
    await expect(updated.getByRole('status')).toHaveText('You have the latest version.');
    const webview = browser.version();
    await page.screenshot({ path: join(assets, 'installed-update.png') });

    closeApp();
    const startUninstall = performance.now();
    await run(uninstaller, ['/S'], { env });
    // NSIS detaches a child uninstaller which finishes registry cleanup after
    // deleting the installed executable and original uninstall launcher.
    await waitFor(() => !existsSync(exe) && !existsSync(uninstaller) && associationsRemoved());
    const uninstallMs = performance.now() - startUninstall;
    expect(existsSync(document)).toBe(true);
    const windows = powershell(
      'Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber | ConvertTo-Json -Compress',
      env,
    );
    const result = {
      windows: JSON.parse(windows),
      architecture: process.arch,
      webview,
      software,
      adapter,
      baseline,
      upgrade,
      installerBytes: packageBytes.length,
      installMs,
      openMs,
      saveMs,
      updateMs,
      uninstallMs,
    };
    await writeFile(join(assets, 'verification.json'), JSON.stringify(result, null, 2) + '\n');
    console.log(result);
  } finally {
    await browser?.close().catch(() => {});
    try {
      closeApp();
    } catch (error) {
      console.error(error);
    }
    if (existsSync(uninstaller)) await run(uninstaller, ['/S'], { env }).catch(() => {});
    if (policyChanged) {
      if (priorPolicy !== undefined)
        registry('add', policy, '/v', binary, '/t', 'REG_SZ', '/d', priorPolicy, '/f', '/reg:64');
      else registry('delete', policy, '/v', binary, '/f', '/reg:64');
    }
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    await rm(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
  }
}

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
if (process.argv[2] === 'prepare') await prepare();
else if (process.argv[2] === 'verify') await verify();
else throw new Error('Use desktop-package-smoke.mjs prepare or verify.');
