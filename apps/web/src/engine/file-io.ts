export interface RemoteFile {
  name: string;
  size: number;
}

export type ReadFileRange = (offset: number, length: number) => Promise<ArrayBuffer>;
export type FileSource = File | (RemoteFile & { read: ReadFileRange });
export interface ProjectSink {
  write(bytes: Uint8Array): Promise<void>;
  commit(): Promise<void>;
  abort(): Promise<void>;
}

export const FILE_CHUNK = 1024 * 1024;

export async function fileRange(file: FileSource, offset: number, length: number) {
  const count = Math.min(length, file.size - offset);
  if (file instanceof File) return file.slice(offset, offset + count).arrayBuffer();
  if (count === 0) return new ArrayBuffer(0);
  const buffer = await file.read(offset, count);
  if (buffer.byteLength !== count) throw new Error('The file changed or could not be read.');
  return buffer;
}

export async function readRemoteFile(file: Exclude<FileSource, File>, check: () => void) {
  const bytes = new Uint8Array(file.size);
  for (let offset = 0; offset < bytes.length; offset += FILE_CHUNK) {
    check();
    bytes.set(new Uint8Array(await fileRange(file, offset, FILE_CHUNK)), offset);
  }
  check();
  return bytes.buffer;
}
