// Disposable CPU codec host. Only immutable samples cross from the engine worker.
import initWasm, {
  codec_limit,
  codec_encode,
  codec_chunk,
  codec_clear,
  codec_peak,
  codec_refused,
} from './pkg/fotoeditor_wasm';
import type { ImageExportOptions } from './types';
import type { SyncHandle } from './paging';
interface Request {
  pixels: ArrayBuffer;
  width: number;
  height: number;
  dpi: number;
  options: ImageExportOptions;
  handle: SyncHandle;
  budget: number;
}
self.onmessage = ({ data: { port } }: MessageEvent<{ port: MessagePort }>) => {
  port.onmessage = async ({ data }: MessageEvent<Request>) => {
    let initialized = false;
    let access: Awaited<ReturnType<SyncHandle['createSyncAccessHandle']>> | undefined;
    try {
      await initWasm();
      initialized = true;
      codec_limit(data.budget);
      const length = codec_encode(
        new Uint8Array(data.pixels),
        data.width,
        data.height,
        data.dpi,
        JSON.stringify(data.options),
      );
      access = await data.handle.createSyncAccessHandle();
      for (let offset = 0; offset < length;) {
        const bytes = codec_chunk(offset);
        if (!bytes.length) throw new Error('Incomplete encoded image');
        let written = 0;
        while (written < bytes.length) {
          const count = access.write(bytes.subarray(written), { at: offset + written });
          if (!Number.isInteger(count) || count <= 0 || count > bytes.length - written)
            throw new Error('Image export storage write failed');
          written += count;
        }
        offset += bytes.length;
      }
      access.truncate(length);
      access.flush();
      access.close();
      access = undefined;
      const peak = codec_peak();
      codec_clear();
      port.postMessage({ length, peak });
    } catch (error) {
      access?.close();
      const refused = initialized && codec_refused();
      port.postMessage({
        error: refused
          ? 'Not enough memory for this export. Use PNG or a smaller canvas.'
          : String(error),
      });
    }
  };
  port.start();
};
