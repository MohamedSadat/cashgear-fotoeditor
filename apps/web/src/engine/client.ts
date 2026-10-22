import { wrap, releaseProxy, transfer } from 'comlink';
import type { EngineApi, EngineFatalEvent, InitOptions } from './types';

export function createEngineClient(onError: (message: string) => void) {
  const worker = new Worker(new URL('./worker.ts', import.meta.url), {
    type: 'module',
    name: 'fotoeditor-engine',
  });
  const api = wrap<EngineApi>(worker);
  let disposed = false;
  worker.addEventListener('message', (event: MessageEvent<EngineFatalEvent>) => {
    if (!disposed && event.data?.type === 'engine-fatal') onError(event.data.message);
  });
  worker.addEventListener('error', (event) => {
    event.preventDefault();
    if (!disposed) onError(event.message || 'The engine worker stopped unexpectedly.');
  });
  worker.addEventListener('messageerror', () => {
    if (!disposed) onError('Could not read an engine response.');
  });
  return {
    api,
    init(canvas: OffscreenCanvas, options: InitOptions) {
      return api.init(transfer(canvas, [canvas]), options);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      // A crashed WASM instance may never answer RPC. Always terminate its worker.
      const timer = setTimeout(() => worker.terminate(), 200);
      void api
        .dispose()
        .catch(() => {})
        .finally(() => {
          clearTimeout(timer);
          api[releaseProxy]();
          worker.terminate();
        });
    },
  };
}
