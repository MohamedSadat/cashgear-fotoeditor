import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { pnpmArgs, pnpmCommand, root, run } from './lib.mjs';

export const installerName = 'CashGearFotoEditor-Setup-x64.exe';
export const desktopVersion = JSON.parse(
  await readFile(join(root, 'apps/desktop/package.json'), 'utf8'),
).version;

export function tauri(args, options) {
  return run(
    pnpmCommand(),
    pnpmArgs(['--filter', '@fotoeditor/desktop', 'exec', 'tauri', ...args]),
    options,
  );
}

export async function installerFor(version) {
  const directory = join(root, 'target/release/bundle/nsis');
  const matches = (await readdir(directory)).filter((name) =>
    name.endsWith(`_${version}_x64-setup.exe`),
  );
  if (matches.length !== 1) throw new Error(`Expected one NSIS x64 installer for ${version}.`);
  return join(directory, matches[0]);
}

export async function writeReleaseAssets(installer, version, destination, url, env = process.env) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Expected a stable desktop version.');
  await mkdir(destination, { recursive: true });
  const setup = join(destination, installerName);
  const payload = join(destination, 'manifest-payload.json');
  const signature = (await readFile(installer + '.sig', 'utf8')).trim();
  const bytes = (await stat(installer)).size;
  const manifest = JSON.stringify({
    schema: 1,
    version,
    bytes,
    platforms: { 'windows-x86_64': { url, signature } },
  });
  await writeFile(payload, manifest);
  try {
    await tauri(['signer', 'sign', '--app-version', version, payload], { env });
    const envelope = { manifest, signature: (await readFile(payload + '.sig', 'utf8')).trim() };
    await copyFile(installer, setup);
    await writeFile(join(destination, 'latest.json'), JSON.stringify(envelope) + '\n');
    const digest = createHash('sha256')
      .update(await readFile(setup))
      .digest('hex');
    await writeFile(join(destination, 'SHA256SUMS.txt'), digest + '  ' + installerName + '\n');
  } finally {
    await rm(payload, { force: true });
    await rm(payload + '.sig', { force: true });
  }
  return { setup, bytes };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const destination = resolve(process.argv[2] ?? join(root, 'bench/results/desktop-release'));
  const url =
    'https://github.com/MohamedSadat/cashgear-fotoeditor/releases/download/' +
    `desktop-v${desktopVersion}/${installerName}`;
  await writeReleaseAssets(await installerFor(desktopVersion), desktopVersion, destination, url);
  console.log(`Prepared signed desktop release ${desktopVersion}.`);
}
