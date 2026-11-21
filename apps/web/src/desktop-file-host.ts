import { invoke, Resource } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { proxy, transfer } from 'comlink';
import { FILE_CHUNK, type ProjectSink } from './engine/file-io';
import type { FileHost, FileLocation } from './file-host';

type PickedFile = { name: string; id: number };
const location = (picked: PickedFile): FileLocation => ({ platform: 'desktop', ...picked });
const locationId = (file: FileLocation) => {
  if (file.platform !== 'desktop') throw new Error('Choose a Windows file.');
  return file.id;
};
const release = (id: number) => new Resource(id).close().catch(() => {});

async function pickSave(suggestedName: string, format: string) {
  const picked = await invoke<PickedFile | null>('pick_save', { suggestedName, format });
  return picked ? location(picked) : undefined;
}

async function beginWrite(file: FileLocation): Promise<ProjectSink> {
  const id = await invoke<number>('begin_native_write', { location: locationId(file) });
  return {
    write: (bytes) =>
      invoke('write_native', bytes as Uint8Array<ArrayBuffer>, {
        headers: { 'x-fotoeditor-file': String(id) },
      }),
    commit: () => invoke('commit_native', { id }),
    abort: () => release(id),
  };
}

export const desktopFileHost: FileHost = {
  usesInput: () => false,
  async pickOpen(kind, recent) {
    const picked = recent
      ? await invoke<PickedFile>('pick_recent', { id: recent.id })
      : await invoke<PickedFile | null>('pick_open', { kind });
    return picked ? location(picked) : undefined;
  },
  async open(file, engine, start) {
    const { id, size } = await invoke<{ id: number; size: number }>('open_native', {
      location: locationId(file),
    });
    try {
      start();
      return await engine.openNativeFile(
        { name: file.name, size },
        proxy(async (offset: number, length: number) => {
          const bytes = await invoke<ArrayBuffer>('read_native', { id, offset, length });
          return transfer(bytes, [bytes]);
        }),
      );
    } finally {
      await release(id);
    }
  },
  async pickProject(name) {
    const picked = await pickSave(name, 'cgfoto');
    if (!picked) throw new DOMException('Save cancelled', 'AbortError');
    return picked;
  },
  async saveProject(file, engine, start) {
    const sink = await beginWrite(file);
    try {
      start();
      return await engine.saveNativeProject(file.name, proxy(sink));
    } finally {
      await sink.abort();
    }
  },
  async export(file, name) {
    const picked = await pickSave(name, name.split('.').at(-1)?.toLowerCase() ?? '');
    if (!picked) return false;
    try {
      const sink = await beginWrite(picked);
      try {
        for (let offset = 0; offset < file.size; offset += FILE_CHUNK) {
          const bytes = await file.slice(offset, offset + FILE_CHUNK).arrayBuffer();
          await sink.write(new Uint8Array(bytes));
        }
        await sink.commit();
        return true;
      } finally {
        await sink.abort();
      }
    } finally {
      await release(locationId(picked));
    }
  },
  recents: () => invoke('recent_files'),
  remember: (file) => invoke('remember_native', { location: locationId(file) }),
  forget: (id) => invoke('forget_native', { id }),
  release: (file) => void release(locationId(file)),
};

export const listenForDesktopOpen = (callback: () => void) =>
  listen('desktop-open-request', callback);
export const pendingDesktopOpen = () => invoke<string | null>('pending_open');
export async function takeDesktopOpen(open: boolean) {
  const picked = await invoke<PickedFile | null>('take_pending_open', { open });
  return picked ? location(picked) : undefined;
}
