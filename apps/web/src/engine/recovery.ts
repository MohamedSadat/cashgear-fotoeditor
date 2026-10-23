// OPFS access stays in the engine worker. Payload validation is always Rust.
import type { RecoveryCandidate } from './types';
import { yieldProjectTask } from './yield-task';
const DIRECTORY = 'cashgear-recovery-v1';
const MAX_BYTES = 4 * 1024 ** 3;
interface SyncFile {
  write(data: Uint8Array, options: { at: number }): number;
  truncate(size: number): void;
  flush(): void;
  close(): void;
}
type SyncHandle = FileSystemFileHandle & { createSyncAccessHandle(): Promise<SyncFile> };
type Directory = FileSystemDirectoryHandle & {
  entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
};
export type RecoveryFault = 'quota' | 'partial' | 'after-flush' | 'pause-after-chunk';
export class RecoveryStore {
  readonly session = crypto.randomUUID();
  private directory?: Directory;
  private release?: () => void;
  private ownership?: Promise<unknown>;
  private lastTime = 0;
  constructor(
    private validate: (bytes: Uint8Array) => Promise<void>,
    private admitRead: (size: number) => void,
    private validateFile?: (file: File) => Promise<void>,
  ) {}
  async init() {
    if (!navigator.locks || !navigator.storage?.getDirectory)
      throw new Error('Browser recovery storage is unavailable. Save a project file.');
    const acquired = new Promise<void>((resolve, reject) => {
      this.ownership = navigator.locks
        .request('fotoeditor-session-' + this.session, async () => {
          resolve();
          await new Promise<void>((release) => {
            this.release = release;
          });
        })
        .catch(reject);
    });
    await acquired;
    const root = await navigator.storage.getDirectory();
    this.directory = (await root.getDirectoryHandle(DIRECTORY, { create: true })) as Directory;
  }
  dispose() {
    this.release?.();
    void this.ownership;
  }
  private catalog<T>(fn: () => Promise<T>) {
    return navigator.locks.request('fotoeditor-recovery-catalog', fn);
  }
  private async entries(): Promise<RecoveryCandidate[]> {
    const entries: RecoveryCandidate[] = [];
    for await (const [file, handle] of this.directory!.entries()) {
      if (handle.kind !== 'file' || !file.endsWith('.json')) continue;
      try {
        const f = await (handle as FileSystemFileHandle).getFile();
        if (f.size > 4096) continue;
        const info = JSON.parse(await f.text()) as RecoveryCandidate;
        if (
          info.file + '.json' !== file ||
          !/^[\da-f-]+$/.test(info.session) ||
          !info.file.startsWith(info.session + '-') ||
          !/^[\da-f-]+\.cgfoto$/.test(info.file) ||
          typeof info.name !== 'string' ||
          info.name.length > 512 ||
          !Number.isSafeInteger(info.savedAt) ||
          info.savedAt <= 0 ||
          !Number.isInteger(info.size) ||
          info.size < 40 ||
          info.size > MAX_BYTES
        )
          continue;
        entries.push(info);
      } catch {
        /* Incomplete metadata never commits a generation. */
      }
    }
    return entries.sort((a, b) => b.savedAt - a.savedAt || b.file.localeCompare(a.file));
  }
  private async read(info: RecoveryCandidate) {
    const handle = await this.directory!.getFileHandle(info.file);
    const f = await handle.getFile();
    if (f.size !== info.size) throw new Error('Incomplete recovery snapshot');
    const header = await f.slice(0, 12).arrayBuffer();
    if (header.byteLength === 12 && new DataView(header).getUint32(8, true) === 2) {
      if (!this.validateFile) throw new Error('Layer recovery is unavailable');
      await this.validateFile(f);
      return f;
    }
    if (f.size > 128 * 1048576) throw new Error('Legacy recovery exceeds its 128 MiB limit');
    this.admitRead(f.size);
    const bytes = new Uint8Array(await f.arrayBuffer());
    await this.validate(bytes);
    return bytes;
  }
  async scan() {
    return this.catalog(async () => {
      const candidates: RecoveryCandidate[] = [];
      const seen = new Set<string>();
      let invalid = 0;
      const entries = await this.entries();
      for (const info of entries) {
        if (info.session === this.session || seen.has(info.session)) continue;
        await navigator.locks.request(
          'fotoeditor-session-' + info.session,
          { ifAvailable: true },
          async (lock) => {
            if (!lock) return;
            try {
              await this.read(info);
              candidates.push(info);
              seen.add(info.session);
            } catch {
              invalid++;
            }
          },
        );
      }
      const committed = new Set(entries.map((entry) => entry.file));
      for await (const [file, handle] of this.directory!.entries()) {
        if (handle.kind !== 'file' || !file.endsWith('.cgfoto') || committed.has(file)) continue;
        const session = file.slice(0, 36);
        if (!/^[\da-f]{8}(-[\da-f]{4}){3}-[\da-f]{12}$/.test(session)) continue;
        await navigator.locks.request(
          'fotoeditor-session-' + session,
          { ifAvailable: true },
          async (lock) => {
            if (!lock) return;
            await this.directory!.removeEntry(file + '.json').catch(() => {});
            await this.directory!.removeEntry(file).catch(() => {});
          },
        );
      }
      return { candidates, invalid };
    });
  }
  async restore(info: RecoveryCandidate, commit: (bytes: Uint8Array | File) => Promise<void>) {
    return this.catalog(() =>
      navigator.locks.request(
        'fotoeditor-session-' + info.session,
        { ifAvailable: true },
        async (lock) => {
          if (!lock) throw new Error('That recovery session is active in another tab');
          // Resolve against trusted stored metadata rather than accepting caller paths.
          const stored = (await this.entries()).find((entry) => entry.file === info.file);
          if (!stored) throw new Error('That recovery snapshot is no longer available');
          await commit(await this.read(stored));
        },
      ),
    );
  }
  async discard(info: RecoveryCandidate) {
    return this.catalog(() =>
      navigator.locks.request(
        'fotoeditor-session-' + info.session,
        { ifAvailable: true },
        async (lock) => {
          if (!lock) throw new Error('That recovery session is active in another tab');
          for (const entry of await this.entries())
            if (entry.session === info.session) await this.remove(entry);
        },
      ),
    );
  }
  private async remove(info: RecoveryCandidate) {
    // Metadata first: a kill during cleanup can leave only an uncommitted payload.
    await this.directory!.removeEntry(info.file + '.json');
    await this.directory!.removeEntry(info.file).catch(() => {});
  }
  private async write(file: string, bytes: Uint8Array, fault?: RecoveryFault) {
    const handle = (await this.directory!.getFileHandle(file, { create: true })) as SyncHandle;
    const access = await handle.createSyncAccessHandle();
    try {
      access.truncate(0);
      for (let offset = 0; offset < bytes.length;) {
        if (fault === 'quota')
          throw new DOMException('Diagnostic storage quota exceeded', 'QuotaExceededError');
        const end = Math.min(bytes.length, offset + 1048576);
        const written = access.write(bytes.subarray(offset, end), { at: offset });
        if (!Number.isInteger(written) || written <= 0 || written > end - offset)
          throw new Error('Recovery storage quota or write failure');
        offset += written;
        if (fault === 'pause-after-chunk') await new Promise<void>(() => {});
        if (fault === 'partial') throw new Error('Diagnostic interrupted snapshot write');
        // Yield between bounded file chunks; an actual tab kill may happen here.
        await yieldProjectTask();
      }
      access.truncate(bytes.length);
      access.flush();
      if (fault === 'after-flush')
        throw new Error('Diagnostic interruption before generation commit');
    } finally {
      access.close();
    }
  }
  async save(bytes: Uint8Array, name: string, fault?: RecoveryFault) {
    return this.savePrepared(bytes.length, name, (file) => this.write(file, bytes, fault));
  }
  async saveStream(size: number, name: string, write: (handle: SyncHandle) => Promise<void>) {
    return this.savePrepared(size, name, async (file) => {
      const handle = (await this.directory!.getFileHandle(file, { create: true })) as SyncHandle;
      await write(handle);
    });
  }
  private async savePrepared(size: number, name: string, write: (file: string) => Promise<void>) {
    return this.catalog(async () => {
      const savedAt = Math.max(Date.now(), this.lastTime + 1);
      this.lastTime = savedAt;
      const file =
        this.session + '-' + savedAt.toString(16) + '-' + crypto.randomUUID() + '.cgfoto';
      const info: RecoveryCandidate = {
        session: this.session,
        file,
        name: name.slice(0, 512),
        savedAt,
        size,
      };
      // Unique immutable generations: never truncate a previous valid snapshot.
      try {
        await write(file);
        await this.read(info);
        await this.write(file + '.json', new TextEncoder().encode(JSON.stringify(info)));
      } catch (error) {
        await this.directory!.removeEntry(file + '.json').catch(() => {});
        await this.directory!.removeEntry(file).catch(() => {});
        throw error;
      }
      // Read back the committed metadata as well before reporting success.
      const entries = await this.entries();
      if (!entries.some((entry) => entry.file === file))
        throw new Error('Recovery generation commit failed');
      const valid: RecoveryCandidate[] = [info];
      for (const entry of entries)
        if (entry.session === this.session && entry.file !== info.file) {
          if (valid.length >= 3) break;
          try {
            await this.read(entry);
            valid.push(entry);
          } catch {
            /* Retain older valid generations. */
          }
        }
      // Retain three validated generations; prune only after a new valid commit.
      if (valid.length >= 3)
        for (const old of entries) {
          if (old.session === this.session && !valid.some((entry) => entry.file === old.file))
            await this.remove(old);
        }
      return info;
    });
  }
}
