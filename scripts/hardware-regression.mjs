// Full coverage with fresh installed-browser processes per spec, never retries.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { readFile, writeFile, mkdir, copyFile, cp, readdir } from 'node:fs/promises';
import { resolve, basename } from 'node:path';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
const prefix = process.env.HARDWARE_REPORT ?? 'bench/results/p9-m4-final';
const base = process.env.E2E_BASE_URL ?? 'http://127.0.0.1:4191';
const directory = prefix + '-batches';
await mkdir(directory, { recursive: true });
if ((await readdir(directory)).length)
  throw Error('Use a fresh evidence prefix; prior runs are immutable.');
const args = ['test', '--list', '--reporter=json', '--retries=0'];
if (process.env.HARDWARE_SPECS) args.push(...process.env.HARDWARE_SPECS.split(','));
const source = JSON.parse(
  execFileSync(process.execPath, [resolve('node_modules/@playwright/test/cli.js'), ...args], {
    encoding: 'utf8',
    windowsHide: true,
  }),
);
const hash = (b) => createHash('sha256').update(b).digest('hex');
function cases(suites) {
  const out = [];
  function visit(s) {
    for (const spec of s.specs ?? [])
      for (const test of spec.tests ?? [])
        out.push({
          key: [spec.file, spec.line, spec.column, spec.title, test.projectName].join('\u0001'),
          file: spec.file,
          test,
        });
    for (const c of s.suites ?? []) visit(c);
  }
  suites.forEach(visit);
  return out;
}
const expected = cases(source.suites),
  keys = new Set(expected.map((c) => c.key));
if (
  !expected.length ||
  expected.length !== keys.size ||
  expected.some((c) => !['chrome', 'edge'].includes(c.test.projectName))
)
  throw Error('Expected unique hardware Chrome/Edge coverage.');
await writeFile(prefix + '-expected.json', JSON.stringify(source, null, 2) + '\n');
const files = [...new Set(expected.map((c) => c.file))].sort();
const start = Date.now(),
  reports = [],
  seen = new Set();
const manifest = {
  head: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8', windowsHide: true }).trim(),
  startedAt: new Date(start).toISOString(),
  url: base,
  method:
    'Serial spec invocations, fresh installed Chrome/Edge processes per spec, one worker, zero retries, unchanged assertions. Actual non-fallback hardware APIs probed before and after each spec. Exact coverage compared with an unfiltered --list matrix (or explicitly named hosted spec subset).',
  expectedCases: expected.length,
  files: [],
  completed: false,
  runningFile: null,
};
const save = () => writeFile(prefix + '-manifest.json', JSON.stringify(manifest, null, 2) + '\n');
const server = process.env.E2E_BASE_URL
  ? undefined
  : spawn(
      process.execPath,
      [
        resolve('apps/web/node_modules/vite/bin/vite.js'),
        'preview',
        '--host',
        '127.0.0.1',
        '--port',
        '4191',
        '--strictPort',
      ],
      { cwd: resolve('apps/web'), stdio: 'ignore', windowsHide: true },
    );
async function probes() {
  const out = [];
  for (const channel of ['chrome', 'msedge']) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of ['WebGPU', 'WebGL2']) {
        const page = await browser.newPage();
        try {
          await page.goto(base + '/?diagnostic=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
          await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
          await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
          const gpu = await page.evaluate(async () => {
            const a = await navigator.gpu?.requestAdapter();
            return a
              ? {
                  vendor: a.info.vendor,
                  architecture: a.info.architecture,
                  fallback: a.info.isFallbackAdapter,
                }
              : null;
          });
          const adapter = await page.getByTestId('gpu-backend').getAttribute('title');
          if (
            !gpu ||
            gpu.fallback ||
            !gpu.vendor ||
            ['google', 'microsoft'].includes(gpu.vendor) ||
            /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '') ||
            (backend === 'WebGL2' && !adapter)
          )
            throw Error('Hardware unavailable: ' + JSON.stringify({ gpu, adapter }));
          out.push({ channel, backend, browser: browser.version(), gpu, adapter });
        } finally {
          await page.close();
        }
      }
    } finally {
      await browser.close();
    }
  }
  return out;
}
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Wait for the preview server to start. */
    }
    await delay(100);
  }
  for (const file of files) {
    const folder = directory + '/' + file.replace('.spec.ts', '');
    await mkdir(folder, { recursive: true });
    manifest.runningFile = file;
    await save();
    const before = await probes();
    const log = createWriteStream(folder + '/playwright.log');
    const child = spawn(
      process.execPath,
      [resolve('node_modules/@playwright/test/cli.js'), 'test', 'tests/e2e/' + file, '--retries=0'],
      {
        env: { ...process.env, E2E_BASE_URL: base, E2E_EVIDENCE_PREFIX: basename(prefix) },
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
      },
    );
    child.stdout.pipe(log, { end: false });
    child.stderr.pipe(log, { end: false });
    const code = await new Promise((done, reject) => {
      child.once('error', reject);
      child.once('exit', done);
    });
    await new Promise((done) => log.end(done));
    const path = folder + '/browser-tests.json';
    await copyFile('bench/results/browser-tests.json', path);
    const r = JSON.parse(await readFile(path, 'utf8')),
      found = cases(r.suites);
    const invalid = found.filter(
      (c) =>
        c.file !== file ||
        !keys.has(c.key) ||
        seen.has(c.key) ||
        c.test.expectedStatus !== 'passed' ||
        c.test.status !== 'expected' ||
        c.test.results.length !== 1 ||
        c.test.results[0].status !== 'passed' ||
        c.test.results[0].retry !== 0 ||
        c.test.results[0].errors?.length,
    );
    const item = {
      file,
      path,
      exitCode: code,
      cases: found.length,
      expectedCases: expected.filter((c) => c.file === file).length,
      stats: r.stats,
      globalErrors: r.errors?.length ?? 0,
      invalid: invalid.map((c) => c.key),
      sha256: hash(await readFile(path)),
      hardwareBefore: before,
    };
    manifest.files.push(item);
    if (
      code !== 0 ||
      invalid.length ||
      found.length !== item.expectedCases ||
      item.globalErrors ||
      r.stats.expected !== found.length ||
      r.stats.skipped ||
      r.stats.unexpected ||
      r.stats.flaky
    ) {
      await cp('test-results', folder + '/test-results', { recursive: true });
      await save();
      throw Error('Spec did not pass cleanly: ' + file);
    }
    item.hardwareAfter = await probes();
    for (const c of found) seen.add(c.key);
    reports.push(r);
    await save();
    console.log(
      JSON.stringify({
        file,
        completedFiles: reports.length,
        totalFiles: files.length,
        completedCases: seen.size,
        expectedCases: expected.length,
      }),
    );
  }
  if (seen.size !== keys.size || [...keys].some((k) => !seen.has(k)))
    throw Error('Missing coverage');
  const combined = {
    ...reports[0],
    suites: reports.flatMap((r) => r.suites),
    errors: reports.flatMap((r) => r.errors ?? []),
    stats: {
      startTime: new Date(start).toISOString(),
      duration: Date.now() - start,
      expected: seen.size,
      skipped: 0,
      unexpected: 0,
      flaky: 0,
    },
    verificationMethod: manifest.method,
    sourceReports: manifest.files.map((f) => ({ path: f.path, sha256: f.sha256 })),
  };
  await writeFile(prefix + '-browser.json', JSON.stringify(combined, null, 2) + '\n');
  manifest.completed = true;
  manifest.runningFile = null;
  manifest.completedCases = seen.size;
  manifest.completedAt = new Date().toISOString();
  await save();
  execFileSync(
    process.execPath,
    ['scripts/verify-browser-report.mjs', prefix + '-browser.json', prefix + '-verification.json'],
    { stdio: 'inherit', windowsHide: true },
  );
} finally {
  server?.kill();
}
