import { defineConfig } from '@playwright/test';

const software = process.env.GPU_TEST_SOFTWARE === '1';
const deployedUrl = process.env.E2E_BASE_URL;
const port = Number(process.env.E2E_PORT ?? 4187);
const previewUrl = `http://127.0.0.1:${port}`;
export default defineConfig({
  testDir: './tests/e2e',
  timeout: 60000,
  expect: { timeout: 30000 },
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI
    ? [
        ['github'],
        ['html', { open: 'never' }],
        ['json', { outputFile: 'bench/results/browser-tests.json' }],
      ]
    : [['list'], ['json', { outputFile: 'bench/results/browser-tests.json' }]],
  use: {
    baseURL: deployedUrl ?? previewUrl,
    viewport: { width: 1280, height: 850 },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: {
      args: software
        ? [
            '--enable-unsafe-webgpu',
            '--enable-unsafe-swiftshader',
            '--use-angle=d3d11-warp',
            '--use-webgpu-adapter=swiftshader',
          ]
        : [],
    },
  },
  projects: software
    ? [
        { name: 'software-webgpu', use: { channel: 'chromium' } },
        {
          name: 'software-webgl2',
          grep: /WebGL2/,
          use: {
            channel: 'chromium',
            launchOptions: {
              args: [
                '--enable-unsafe-webgpu',
                '--enable-unsafe-swiftshader',
                '--use-angle=swiftshader',
              ],
            },
          },
        },
      ]
    : [
        { name: 'chrome', use: { channel: 'chrome' } },
        { name: 'edge', use: { channel: 'msedge' } },
      ],
  webServer: deployedUrl
    ? undefined
    : {
        command: `pnpm preview --port ${port} --strictPort`,
        url: previewUrl,
        reuseExistingServer: false,
        timeout: 30000,
      },
});
