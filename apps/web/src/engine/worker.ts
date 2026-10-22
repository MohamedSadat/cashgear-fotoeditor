import { expose, transfer } from 'comlink';
import type { EngineApi, EngineInfo, InitOptions, PhysicalSize } from './types';
import initWasm, { Engine } from './pkg/fotoeditor_wasm';
import type { DocumentSnapshot } from './types';

let engine: Engine | undefined;
let initializing = false;
let disposed = false;
let diagnostic = false;
let size: PhysicalSize = { width: 0, height: 0 };
let frames = 0;
let job = 0;
let busy = false;
let presentation: { resolve: () => void; reject: (error: Error) => void } | undefined;
let name = 'Untitled';
let cursorScreen: [number, number] | undefined;
const yieldTask = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
function snapshot(): DocumentSnapshot {
  const p = cursorScreen ? engine!.cursor(...cursorScreen) : undefined;
  return {
    ...(JSON.parse(engine!.snapshot()) as DocumentSnapshot),
    name,
    cursor: p ? [p[0], p[1]] : undefined,
  };
}
function requireEngine() {
  if (!engine || disposed) throw new Error('Engine is unavailable');
  return engine;
}

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
    engine.resize(size.width, size.height, options.dpr ?? 1);
    const backend = engine.backend();
    if (backend !== 'WebGPU' && backend !== 'WebGL2')
      throw new Error('Unexpected GPU backend: ' + backend);
    return { backend, adapter: engine.adapter_name() };
  },
  resize(nextSize) {
    if (disposed) return;
    size = nextSize;
    engine?.resize(size.width, size.height, size.dpr ?? 1);
  },
  frame() {
    if (disposed || !engine) throw new Error('Engine is unavailable');
    const presented = engine.frame();
    if (presented) {
      frames++;
      presentation?.resolve();
      presentation = undefined;
    }
    return {
      presented,
      frames,
      width: engine.width(),
      height: engine.height(),
      snapshot: snapshot(),
    };
  },
  dispose() {
    ++job;
    busy = false;
    presentation?.reject(new Error('Engine disposed'));
    presentation = undefined;
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
  interact(inputs) {
    const e = requireEngine();
    for (const input of inputs) {
      if (input.kind === 'cursor') {
        cursorScreen = [input.x, input.y];
      } else e.navigate(input.kind, input.x, input.y, input.amount);
    }
  },
  newDocument(width, height, dpi) {
    if (busy) throw new Error('Finish or cancel the current operation first');
    requireEngine().new_document(width, height, dpi);
    name = 'Untitled';
    cursorScreen = undefined;
    return snapshot();
  },
  async openImage(bytes, fileName) {
    if (busy) throw new Error('Finish or cancel the current operation first');
    busy = true;
    const token = ++job;
    const e = requireEngine();
    try {
      e.begin_import(new Uint8Array(bytes));
      bytes = new ArrayBuffer(0);
      while (true) {
        await yieldTask();
        if (token !== job || disposed) throw new Error('Operation cancelled');
        if (e.import_step()) break;
      }
      name = fileName;
      cursorScreen = undefined;
      // Only FramePump may present a surface. Presenting here as well could
      // acquire the same browser canvas texture twice within one animation frame.
      busy = false;
      await new Promise<void>((resolve, reject) => {
        presentation = { resolve, reject };
      });
      return snapshot();
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
      }
    }
  },
  async exportPng() {
    if (busy) throw new Error('Finish or cancel the current operation first');
    busy = true;
    const token = ++job;
    const e = requireEngine();
    try {
      e.begin_export();
      while (true) {
        await yieldTask();
        if (token !== job || disposed) throw new Error('Operation cancelled');
        if (e.export_step()) break;
      }
      const bytes = e.finish_export();
      return transfer(bytes, [bytes.buffer]);
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
      }
    }
  },
  cancelJob() {
    // A completed document replacement is atomic and cannot be cancelled after
    // commit while it waits for its first normal viewport frame.
    if (!busy) return false;
    ++job;
    requireEngine().cancel_job();
    busy = false;
    return true;
  },
};
expose(api);
