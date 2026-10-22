import { expose } from 'comlink';
import type { EngineApi, EngineInfo, InitOptions, PhysicalSize } from './types';
import initWasm, { Engine } from './pkg/fotoeditor_wasm';

let engine: Engine | undefined;
let initializing = false;
let disposed = false;
let diagnostic = false;
let size: PhysicalSize = { width: 0, height: 0 };
let frames = 0;

function fatal(message: string) {
  self.postMessage({ type: 'engine-fatal', message });
}
const api: EngineApi = {
  async init(canvas: OffscreenCanvas, options: InitOptions): Promise<EngineInfo> {
    if (engine || initializing || disposed)
      throw new Error('Engine worker cannot be initialized twice');
    initializing = true;
    diagnostic = options.diagnostic;
    if (diagnostic && options.failInitialization)
      throw new Error('Diagnostic initialization failure');
    // Worker-local fault injection lets browser tests exercise the real Rust
    // fallback path without changing browser flags or the renderer policy.
    if (diagnostic && options.simulateGpuFailure) {
      const gpu = Reflect.get(navigator, 'gpu') as
        { requestAdapter: (...args: unknown[]) => Promise<object | null> } | undefined;
      if (!gpu) throw new Error('GPU failure diagnostics require the WebGPU API');
      const requestAdapter = gpu.requestAdapter.bind(gpu);
      Object.defineProperty(gpu, 'requestAdapter', {
        value: async (...args: unknown[]) => {
          if (options.simulateGpuFailure === 'adapter') return null;
          const adapter = await requestAdapter(...args);
          if (adapter)
            Object.defineProperty(adapter, 'requestDevice', {
              value: () => Promise.reject(new Error('Diagnostic device request failure')),
            });
          return adapter;
        },
      });
    }
    await initWasm({ module_or_path: new URL('./pkg/fotoeditor_wasm_bg.wasm', import.meta.url) });
    const created = await Engine.create(canvas, options.backend, diagnostic, fatal);
    if (disposed) {
      created.dispose();
      created.free();
      throw new Error('Engine disposed during initialization');
    }
    engine = created;
    size = { width: options.width, height: options.height };
    engine.resize(size.width, size.height);
    const backend = engine.backend();
    if (backend !== 'WebGPU' && backend !== 'WebGL2')
      throw new Error('Unexpected GPU backend: ' + backend);
    return { backend, adapter: engine.adapter_name() };
  },
  resize(nextSize) {
    if (disposed) return;
    size = nextSize;
    engine?.resize(size.width, size.height);
  },
  frame() {
    if (disposed || !engine) throw new Error('Engine is unavailable');
    const presented = engine.frame();
    if (presented) frames++;
    return { presented, frames, width: engine.width(), height: engine.height() };
  },
  dispose() {
    disposed = true;
    engine?.dispose();
    engine?.free();
    engine = undefined;
  },
  diagnosticFault(kind) {
    if (!diagnostic || !engine) throw new Error('Diagnostics are disabled');
    if (kind === 'device-loss') engine.diagnostic_device_loss();
    else engine.diagnostic_panic();
  },
};
expose(api);
