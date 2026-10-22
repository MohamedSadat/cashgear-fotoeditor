import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
export const bindgenVersion = '0.2.129';
export const toolName =
  'wasm-bindgen-' +
  bindgenVersion +
  '-' +
  (process.platform === 'win32'
    ? 'x86_64-pc-windows-msvc'
    : process.platform === 'darwin'
      ? (process.arch === 'arm64' ? 'aarch64' : 'x86_64') + '-apple-darwin'
      : 'x86_64-unknown-linux-musl');
export function bindgenPath() {
  const exe = process.platform === 'win32' ? 'wasm-bindgen.exe' : 'wasm-bindgen';
  const candidates = [
    process.env.WASM_BINDGEN_PATH,
    join(root, '.tools', toolName, exe),
    process.env.LOCALAPPDATA &&
      join(process.env.LOCALAPPDATA, 'CashGear', 'FotoEditor', 'tools', toolName, exe),
  ].filter(Boolean);
  return candidates.find(existsSync) ?? 'wasm-bindgen';
}
export function run(command, args = [], options = {}) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit', ...options });
    child.on('error', reject);
    child.on('exit', (code, signal) =>
      code === 0 ? resolveRun() : reject(new Error(command + ' failed (' + (signal ?? code) + ')')),
    );
  });
}
export function pnpmArgs(args) {
  if (!process.env.npm_execpath) throw new Error('Run this command through pnpm.');
  return /\.[cm]?js$/.test(process.env.npm_execpath) ? [process.env.npm_execpath, ...args] : args;
}
export function pnpmCommand() {
  if (!process.env.npm_execpath) throw new Error('Run this command through pnpm.');
  return /\.[cm]?js$/.test(process.env.npm_execpath) ? process.execPath : process.env.npm_execpath;
}
