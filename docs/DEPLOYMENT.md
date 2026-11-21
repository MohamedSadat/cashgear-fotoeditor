# Static deployment

Run pnpm build and publish apps/web/dist at the HTTPS origin root.
All runtime scripts, fonts, worker modules, and WASM are served from this origin.
No external runtime services are needed.

Set these response headers for HTML and runtime assets:

    Cross-Origin-Opener-Policy: same-origin
    Cross-Origin-Embedder-Policy: require-corp
    Cross-Origin-Resource-Policy: same-origin

Serve .wasm as application/wasm and JavaScript modules with a JavaScript MIME type.
The generated _headers file is suitable for Netlify/Cloudflare Pages-style hosts.
On other hosts configure the same headers explicitly; copying _headers alone does
not configure an arbitrary server. Hosts that cannot set these headers do not meet
the selected deployment requirements.

Vite dev and preview set the same headers. Confirm window.crossOriginIsolated is
true and check the network panel for successful worker/WASM requests after deploy.
Use pnpm test:e2e against the local production preview before deploying.

Keep the entry HTML revalidated (Cache-Control: no-cache). Hashed /assets files may
be cached immutably; deploy the complete new directory atomically.

## Azure Static Web Apps

Production: [CashGear FotoEditor](https://proud-pebble-07b8b4f10.2.azurestaticapps.net).

The Azure workflow installs Node 24, pnpm 12.11.2, Rust 1.99.1, the WASM target,
and the matching wasm-bindgen CLI. It runs pnpm build at the repository root,
including release WASM optimization and the size gate. Azure then uploads
apps/web/dist with skip_app_build enabled and output_location empty; Oryx does
not need to infer this Rust/pnpm workspace's build process.

apps/web/public/staticwebapp.config.json is copied to the distribution root by
Vite. Azure reads it to apply isolation headers, the WASM MIME type, entry HTML
revalidation, and immutable caching for hashed assets. Azure does not use the
Netlify-style _headers configuration.

The existing AZURE_STATIC_WEB_APPS_API_TOKEN_PROUD_PEBBLE_07B8B4F10 repository
secret authorizes deployment. Keep its value in GitHub secrets. Main-branch pushes
publish production; pull requests create preview deployments, and closing a pull
request closes its preview environment.

After deployment, check the Azure action's published URL on both GPU backends,
confirm crossOriginIsolated, and verify worker and WASM asset responses. See
[Azure's build settings](https://learn.microsoft.com/en-us/azure/static-web-apps/build-configuration#skip-building-front-end-app)
and [host configuration](https://learn.microsoft.com/en-us/azure/static-web-apps/configuration).

Set E2E_BASE_URL to the deployed HTTPS origin to run Playwright against production
without starting a local preview. P1 viewport tests open deterministic fixtures,
verify rendered samples and export pixel equality on both installed browsers and
GPU backends. These files are processed locally in the browser worker; no image
upload service is used. Clear E2E_BASE_URL to return to the local preview tests.

## Desktop

Tauri loads the same static files and sets the isolation headers through its
protocol response configuration. CSP permits local WASM compilation and workers;
production does not permit remote scripts. The development server uses its own
headers and the Tauri development CSP.

The Windows shell needs WebView2 Runtime. Test WebGPU and forced WebGL2 inside
WebView2; browser support alone does not prove desktop rendering support.

### NSIS installer and stable updates

Run `pnpm desktop:package` on Windows x64. The per-user NSIS installer is in
`target/release/bundle/nsis`. It uses Microsoft's WebView2 Runtime bootstrapper
when the runtime is absent. The installer is Authenticode unsigned. Explorer
can open .cgfoto projects and lists FotoEditor under Open with for PNG and JPEG.
Uninstall removes the registered application and file handlers. The user's files
stay in their original locations.

The desktop version comes from `apps/desktop/package.json`. Bump that version
for each stable release and run the repository checks on main. The Stable
Windows release workflow builds signed artifacts on main. Run it with publishing
disabled to inspect the artifacts. The publishing option requires a certified
`p10-desktop` ancestor and successful checks and deployment on the exact head.
The workflow publishes a complete draft as the latest stable release under
`desktop-v<VERSION>`.

The workflow uses `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Back up the encrypted signing key and its
password through the release owner's secure storage. The verified public key is
in `apps/desktop/src-tauri/tauri.conf.json`. Keep the same key across releases
so installed copies can verify updates. The signed manifest binds the version,
asset URL, installer signature and size. The installer signature independently
binds the version and installer bytes. The desktop app opens the update dialog
from Updates and commits a recovery snapshot of the current document before
installing. It asks before replacing the current document on a subsequent
Explorer launch.

For a local installer upgrade check, build the web app and generate the small
layer fixture, then run `pnpm exec node scripts/desktop-package-smoke.mjs prepare`
and `pnpm exec node scripts/desktop-package-smoke.mjs verify`. The test builds
two versions with a disposable signing key and stages them under
`bench/results/desktop-acceptance`. The test's loopback feed and install identity
are separate from the stable channel. Run Verify on clean Windows 10 and 11
machines and retain their independent OS, version, tamper, recovery and timing
results. The standard CI includes an x64 installer check on Windows Server
and an x64 emulation run on Windows 11 Arm. The Clean Windows 10 installer
verification workflow accepts the exact CI run ID and exercises its artifacts
in a fresh Windows 10 Enterprise Evaluation guest on a disposable Ubuntu runner.

## Diagnostic routes

/?gpu=webgl2 forces the fallback. /?diagnostic=1 enables the Rust test texture and
explicit diagnostic fault controls. These do not create or modify a document.
