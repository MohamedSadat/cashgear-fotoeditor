// Worker-owned immutable files. Rust validates every hydrated generation and
// every complete project; this adapter only transports bounded byte chunks.
import type { Engine } from './pkg/fotoeditor_wasm';
import { yieldProjectTask } from './yield-task';
export const CHUNK = 1048576;
export interface SyncFile {
  write(data: Uint8Array, options: { at: number }): number;
  read(data: Uint8Array, options: { at: number }): number;
  truncate(size: number): void;
  flush(): void;
  close(): void;
}
export type SyncHandle = FileSystemFileHandle & { createSyncAccessHandle(): Promise<SyncFile> };
type Directory = FileSystemDirectoryHandle & {
  entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
};
interface Request {
  id: string;
  store: string;
  offset: number;
  length: number;
}
export class PagingStore {
  private closed = false;
  private loadedSinceEviction = 32 * CHUNK;
  private directory?: Directory;
  private sources = new Map<string, { handle: FileSystemFileHandle; file?: File }>();
  private release?: () => void;
  private ownership?: Promise<unknown>;
  readonly session = crypto.randomUUID();
  async init() {
    if (this.directory) return;
    if (!navigator.storage?.getDirectory || !navigator.locks)
      throw new Error('Layer paging needs browser file storage.');
    await new Promise<void>((resolve, reject) => {
      this.ownership = navigator.locks
        .request('fotoeditor-pages-' + this.session, async () => {
          resolve();
          await new Promise<void>((release) => {
            this.release = release;
          });
        })
        .catch(reject);
    });
    const root = await navigator.storage.getDirectory();
    const pages = (await root.getDirectoryHandle('cashgear-pages-v2', {
      create: true,
    })) as Directory;
    // Recovery snapshots are self-contained, so abandoned working packs can be
    // collected while holding the former owner's lock. Active tabs keep theirs.
    for await (const [name, handle] of pages.entries()) {
      if (handle.kind !== 'directory' || !/^[\da-f]{8}(-[\da-f]{4}){3}-[\da-f]{12}$/.test(name))
        continue;
      await navigator.locks.request(
        'fotoeditor-pages-' + name,
        { ifAvailable: true },
        async (lock) => {
          if (lock) await pages.removeEntry(name, { recursive: true }).catch(() => {});
        },
      );
    }
    this.directory = (await pages.getDirectoryHandle(this.session, { create: true })) as Directory;
  }
  dispose() {
    this.closed = true;
    this.sources.clear();
    this.release?.();
    void this.ownership;
  }
  async create() {
    await this.init();
    const id = crypto.randomUUID().replaceAll('-', '');
    const handle = (await this.directory!.getFileHandle(id, { create: true })) as SyncHandle;
    return { id, handle };
  }
  async hydrate(
    engine: Engine,
    scope:
      | 'active'
      | 'render'
      | 'write'
      | 'thumbnail'
      | 'adjustment'
      | 'filter'
      | 'geometry'
      | 'transform-source'
      | 'transform-bounds'
      | 'transform-background'
      | 'transform-render'
      | 'hit-test'
      | 'preview'
      | 'preview-render'
      | 'histogram'
      | 'selection'
      | 'selection-seed'
      | 'selection-mask'
      | 'export',
  ) {
    if (this.loadedSinceEviction >= 32 * CHUNK) {
      engine.page_evict(256 * CHUNK, '[]');
      this.loadedSinceEviction = 0;
    }
    const { requests, ticket } = JSON.parse(engine.page_requests(scope)) as {
      requests: Request[];
      ticket: number;
    };
    try {
      for (const request of requests) {
        const source = this.sources.get(request.store);
        if (!source)
          throw new Error(
            'A required layer backing file is unavailable. The edit has not been committed.',
          );
        const file = (source.file ??= await source.handle.getFile());
        const bytes = new Uint8Array(
          await file.slice(request.offset, request.offset + request.length).arrayBuffer(),
        );
        engine.page_hydrate(request.id, bytes);
        this.loadedSinceEviction += bytes.length;
        await yieldProjectTask();
      }
    } finally {
      if (!this.closed) engine.page_io_complete(ticket);
    }
  }
  async collect(engine: Engine) {
    const live = new Set(JSON.parse(engine.page_stores()) as string[]);
    if (!this.directory) return;
    for (const [id] of this.sources)
      if (!live.has(id)) {
        await this.directory.removeEntry(id).catch(() => {});
        this.sources.delete(id);
      }
  }
  async persist(engine: Engine) {
    const { ids, ticket } = JSON.parse(engine.page_unbacked()) as { ids: string[]; ticket: number };
    try {
      if (!ids.length) return;
      const { id, handle } = await this.create();
      const access = await handle.createSyncAccessHandle();
      let offset = 0;
      const extents: { generation: string; offset: number; length: number }[] = [];
      try {
        for (const generation of ids) {
          const bytes = engine.page_encode(generation);
          let written = 0;
          while (written < bytes.length) {
            const count = access.write(bytes.subarray(written), { at: offset + written });
            if (!Number.isInteger(count) || count <= 0 || count > bytes.length - written)
              throw new Error(
                'Layer storage quota or write failure; resident samples are retained',
              );
            written += count;
          }
          extents.push({ generation, offset, length: bytes.length });
          offset += bytes.length;
          await yieldProjectTask();
        }
        access.truncate(offset);
        access.flush();
        this.sources.set(id, { handle });
        for (const extent of extents) {
          const bytes = new Uint8Array(extent.length);
          if (access.read(bytes, { at: extent.offset }) !== bytes.length)
            throw new Error('Incomplete layer backing readback');
          engine.page_acknowledge(extent.generation, id, extent.offset, bytes);
          await yieldProjectTask();
        }
      } finally {
        access.close();
      }
      await this.collect(engine);
    } finally {
      if (!this.closed) engine.page_io_complete(ticket);
    }
  }
  async copy(file: File, cancelled: () => void) {
    if (file.size > 4 * 1024 ** 3) throw new Error('Project exceeds the 4 GiB streamed file limit');
    const { id, handle } = await this.create();
    const access = await handle.createSyncAccessHandle();
    try {
      for (let offset = 0; offset < file.size;) {
        cancelled();
        const bytes = new Uint8Array(await file.slice(offset, offset + CHUNK).arrayBuffer());
        let written = 0;
        while (written < bytes.length) {
          const count = access.write(bytes.subarray(written), { at: offset + written });
          if (!Number.isInteger(count) || count <= 0 || count > bytes.length - written)
            throw new Error(
              'Project storage quota or write failure; the previous document is retained',
            );
          written += count;
        }
        offset += bytes.length;
        await yieldProjectTask();
      }
      access.truncate(file.size);
      access.flush();
    } finally {
      access.close();
    }
    const backing = await handle.getFile();
    if (backing.size !== file.size)
      throw new Error(`Incomplete project backing: ${backing.size} of ${file.size} bytes`);
    this.sources.set(id, { handle, file: backing });
    return { id, file: backing };
  }
}
