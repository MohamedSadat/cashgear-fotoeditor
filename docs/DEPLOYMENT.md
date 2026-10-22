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
