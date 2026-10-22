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

## Desktop

Tauri loads the same static files and sets the isolation headers through its
protocol response configuration. CSP permits local WASM compilation and workers;
production does not permit remote scripts. The development server uses its own
headers and the Tauri development CSP.

The Windows shell needs WebView2 Runtime. Test WebGPU and forced WebGL2 inside
WebView2; browser support alone does not prove desktop rendering support.

## Diagnostic routes

/?gpu=webgl2 forces the fallback. /?diagnostic=1 enables the Rust test texture and
explicit diagnostic fault controls. These do not create or modify a document.
