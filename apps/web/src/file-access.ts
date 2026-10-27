// Main-thread file handles and recent-file metadata. The engine still owns bytes.
export interface RecentFile {
  id: string;
  name: string;
  kind: 'image' | 'project';
  openedAt: number;
  handle: FileSystemFileHandle;
}
type PermissionHandle = FileSystemFileHandle & {
  queryPermission?: (descriptor: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>;
  requestPermission?: (descriptor: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>;
};
let database: Promise<IDBDatabase> | undefined;
function db() {
  database ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open('cashgear-recent-files-v1', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('files', { keyPath: 'id' });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error('Recent files storage is busy in another window.'));
  });
  return database;
}
async function catalog<T>(mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest<T>) {
  const database = await db();
  return new Promise<T>((resolve, reject) => {
    const tx = database.transaction('files', mode);
    const request = run(tx.objectStore('files'));
    tx.oncomplete = () => resolve(request.result);
    tx.onerror = () => reject(tx.error ?? request.error);
    tx.onabort = () => reject(tx.error ?? new Error('Recent files storage was interrupted.'));
  });
}
export async function recentFiles(): Promise<RecentFile[]> {
  const rows: unknown[] = await catalog('readonly', (store) => store.getAll());
  return rows
    .filter((row): row is RecentFile => {
      const r = row as Partial<RecentFile> | null;
      return (
        !!r &&
        typeof r.id === 'string' &&
        typeof r.name === 'string' &&
        r.name.length <= 1024 &&
        (r.kind === 'image' || r.kind === 'project') &&
        typeof r.openedAt === 'number' &&
        Number.isFinite(r.openedAt) &&
        r.handle?.kind === 'file' &&
        typeof r.handle.getFile === 'function'
      );
    })
    .sort((a, b) => b.openedAt - a.openedAt)
    .slice(0, 10);
}
export async function forgetRecent(id: string) {
  await catalog('readwrite', (store) => store.delete(id));
}
export async function rememberFile(handle: FileSystemFileHandle, kind: RecentFile['kind']) {
  const update = async () => {
    const files = await recentFiles();
    let id: string | undefined;
    for (const file of files) {
      if (await handle.isSameEntry(file.handle).catch(() => false)) {
        id = file.id;
        break;
      }
    }
    const entry: RecentFile = {
      id: id ?? crypto.randomUUID(),
      name: handle.name,
      kind,
      openedAt: Date.now(),
      handle,
    };
    await catalog('readwrite', (store) => store.put(entry));
    for (const file of files.filter((r) => r.id !== entry.id).slice(9)) await forgetRecent(file.id);
    return recentFiles();
  };
  return navigator.locks ? navigator.locks.request('fotoeditor-recent-files', update) : update();
}
/** Call only from an explicit Open/Save/recent-file action, never at startup. */
export async function filePermission(file: FileSystemFileHandle, mode: 'read' | 'readwrite') {
  const handle = file as PermissionHandle;
  // OPFS handles already belong to this origin and need no permission API.
  if (!handle.queryPermission) return;
  let permission = await handle.queryPermission({ mode });
  if (permission === 'prompt' && handle.requestPermission)
    permission = await handle.requestPermission({ mode });
  if (permission !== 'granted')
    throw new Error(
      mode === 'read'
        ? 'File access was denied. Choose the file again with Open.'
        : 'Write access was denied. Use Save project as to choose another file.',
    );
}
export function fileAccessError(error: unknown) {
  if (error instanceof DOMException && error.name === 'NotFoundError')
    return 'This file is missing or moved. Choose it again with Open.';
  if (error instanceof DOMException && ['NotAllowedError', 'SecurityError'].includes(error.name))
    return 'File access was denied. Choose the file again with Open or Save project as.';
  return String(error);
}
export function pickerCancelled(error: unknown) {
  return error instanceof DOMException && error.name === 'AbortError';
}
