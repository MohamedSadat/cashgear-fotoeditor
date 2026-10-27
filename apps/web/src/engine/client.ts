import { wrap, releaseProxy, transfer } from 'comlink';
import type { EngineApi, EngineFatalEvent, InitOptions, CodecHostRequest } from './types';

export function createEngineClient(onError: (message: string) => void) {
  const worker = new Worker(new URL('./worker.ts', import.meta.url), {
    type: 'module',
    name: 'fotoeditor-engine',
  });
  const api = wrap<EngineApi>(worker);
  let disposed = false;
  const codecs = new Map<string, Worker>();
  const stopCodec = (id: string) => {
    codecs.get(id)?.terminate();
    codecs.delete(id);
  };
  const stopCodecs = () => {
    for (const id of codecs.keys()) stopCodec(id);
  };
  worker.addEventListener('message', (event: MessageEvent<EngineFatalEvent | CodecHostRequest>) => {
    const data = event.data;
    if (data?.type === 'codec-stop') stopCodec(data.id);
    if (data?.type === 'codec-start') {
      if (disposed) {
        data.port.close();
        return;
      }
      try {
        const codec = new Worker(new URL('./codec-worker.ts', import.meta.url), {
          type: 'module',
          name: 'fotoeditor-codec',
        });
        codecs.set(data.id, codec);
        codec.onerror = (event) => {
          event.preventDefault();
          // A separate error channel remains owned by the host after the job
          // port moves directly between the two workers.
          worker.postMessage({
            type: 'codec-error',
            id: data.id,
            error: event.message || 'Image codec worker failed',
          });
          stopCodec(data.id);
        };
        codec.postMessage({ port: data.port }, [data.port]);
      } catch (error) {
        data.port.postMessage({ error: String(error) });
        data.port.close();
      }
      return;
    }
    if (!disposed && event.data?.type === 'engine-fatal') {
      stopCodecs();
      onError(event.data.message);
    }
  });
  worker.addEventListener('error', (event) => {
    event.preventDefault();
    stopCodecs();
    if (!disposed) onError(event.message || 'The engine worker stopped unexpectedly.');
  });
  worker.addEventListener('messageerror', () => {
    stopCodecs();
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
      stopCodecs();
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
