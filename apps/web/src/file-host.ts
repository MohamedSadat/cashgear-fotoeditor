import { isTauri } from '@tauri-apps/api/core';
import { desktopFileHost } from './desktop-file-host';
import {
  filePermission,
  forgetRecent,
  recentFiles,
  rememberFile,
  type RecentFile as BrowserRecentFile,
} from './file-access';
import type { EngineApi } from './engine/types';

export type FileKind = BrowserRecentFile['kind'];
export type FileLocation =
  | { platform: 'web'; name: string; handle: FileSystemFileHandle }
  | { platform: 'desktop'; name: string; id: number };
export type RecentFile = Pick<BrowserRecentFile, 'id' | 'name' | 'kind'> & {
  handle?: FileSystemFileHandle;
};

export interface FileHost {
  usesInput(): boolean;
  pickOpen(kind: FileKind, recent?: RecentFile): Promise<FileLocation | undefined>;
  open(
    location: FileLocation,
    engine: EngineApi,
    start: () => void,
  ): ReturnType<EngineApi['openProjectFile']>;
  pickProject(suggestedName: string): Promise<FileLocation | undefined>;
  saveProject(location: FileLocation, engine: EngineApi, start: () => void): Promise<number>;
  export(file: Blob, name: string): Promise<boolean>;
  recents(): Promise<RecentFile[]>;
  remember(location: FileLocation, kind: FileKind): Promise<RecentFile[]>;
  forget(id: string): Promise<RecentFile[]>;
  release(location: FileLocation): void;
}

const openPicker = () =>
  Reflect.get(window, 'showOpenFilePicker') as
    ((options: object) => Promise<FileSystemFileHandle[]>) | undefined;

const projectType = {
  description: 'FotoEditor project',
  accept: { 'application/octet-stream': ['.cgfoto'] },
};
const imageType = {
  description: 'Images',
  accept: {
    'image/png': ['.png'],
    'image/jpeg': ['.jpg', '.jpeg'],
    'image/webp': ['.webp'],
    'image/bmp': ['.bmp'],
    'image/gif': ['.gif'],
    'image/tiff': ['.tif', '.tiff'],
    'image/vnd.adobe.photoshop': ['.psd'],
  },
};

export function openBrowserFile(file: File, engine: EngineApi) {
  const project = /\.cgfoto$/i.test(file.name);
  if (file.size > (project ? 4 * 1024 ** 3 : 128 * 1048576))
    throw new Error(
      project ? 'Projects must be at most 4 GiB' : 'Image files must be at most 128 MiB',
    );
  return project ? engine.openProjectFile(file) : engine.openImageFile(file);
}

function browserHandle(location: FileLocation) {
  if (location.platform !== 'web') throw new Error('This file is unavailable in the browser.');
  return location.handle;
}
function browserLocation(handle: FileSystemFileHandle): FileLocation {
  return { platform: 'web', name: handle.name, handle };
}

export function download(file: Blob, name: string) {
  const url = URL.createObjectURL(file);
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

const browser: FileHost = {
  usesInput: () => !openPicker(),
  async pickOpen(kind, recent) {
    if (recent?.handle) {
      await filePermission(recent.handle, 'read');
      return browserLocation(recent.handle);
    }
    const [handle] =
      (await openPicker()?.({
        multiple: false,
        types: [kind === 'project' ? projectType : imageType],
      })) ?? [];
    return handle && browserLocation(handle);
  },
  async open(location, engine, start) {
    const file = await browserHandle(location).getFile();
    start();
    return openBrowserFile(file, engine);
  },
  async pickProject(suggestedName) {
    const picker = Reflect.get(window, 'showSaveFilePicker') as
      ((options: object) => Promise<FileSystemFileHandle>) | undefined;
    const handle = await picker?.({ suggestedName, types: [projectType] });
    return handle && browserLocation(handle);
  },
  async saveProject(location, engine, start) {
    const handle = browserHandle(location);
    await filePermission(handle, 'readwrite');
    start();
    return engine.saveProjectFile(handle);
  },
  async export(file, name) {
    download(file, name);
    return true;
  },
  recents: recentFiles,
  remember: (location, kind) => rememberFile(browserHandle(location), kind),
  async forget(id) {
    await forgetRecent(id);
    return recentFiles();
  },
  release() {},
};

export const isDesktop = isTauri();
export const fileHost: FileHost = isDesktop ? desktopFileHost : browser;
