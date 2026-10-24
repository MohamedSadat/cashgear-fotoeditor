import { create } from 'zustand';
import { transfer } from 'comlink';
import type { EngineApi, SelectionAction, SelectionMode } from './engine/types';
import { useWorkspace, updateWorkspace } from './state';
interface SelectionSettings {
  mode: SelectionMode;
  antialias: boolean;
  tolerance: number;
  contiguous: boolean;
  allLayers: boolean;
  radius: number;
  busy: boolean;
  notice: string;
}
export const useSelection = create<SelectionSettings>(() => ({
  mode: 'new',
  antialias: true,
  tolerance: 32,
  contiguous: true,
  allLayers: false,
  radius: 5,
  busy: false,
  notice: '',
}));
export async function performSelection(api: EngineApi, action: SelectionAction) {
  if (useSelection.getState().busy) return;
  useSelection.setState({ busy: true, notice: '' });
  try {
    const snapshot = await api.selectionAction(action);
    updateWorkspace({ snapshot });
  } catch (error) {
    useSelection.setState({ notice: String(error) });
  } finally {
    useSelection.setState({ busy: false });
  }
}
const customType = 'web application/x-cashgear-fotoeditor-token';
let copiedToken: string | undefined;
let preferInternal = false;
export async function copySelection(api: EngineApi, cut: boolean, allLayers = false) {
  if (
    useWorkspace.getState().snapshot?.selection.quickMask ||
    useWorkspace.getState().snapshot?.layers.maskTarget
  ) {
    useSelection.setState({ notice: 'Return to layer pixels before copying or cutting' });
    return;
  }
  if (useSelection.getState().busy) return;
  useSelection.setState({ busy: true, notice: '' });
  copiedToken = undefined;
  let prepared = false;
  const token = crypto.randomUUID();
  const png = (async () => {
    const snapshot = await api.selectionAction({ kind: cut ? 'cut' : 'copy', allLayers });
    updateWorkspace({ snapshot });
    prepared = true;
    const bytes = await api.clipboardPng();
    return new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'image/png' });
  })();
  // Construct the item and initiate the write in the user's activation task.
  let written: Promise<void>;
  try {
    if (!navigator.clipboard?.write || !globalThis.ClipboardItem)
      throw new Error('Image clipboard API is unavailable');
    const data: Record<string, Blob | Promise<Blob>> = { 'image/png': png };
    if (ClipboardItem.supports?.(customType))
      data[customType] = new Blob([token], { type: customType.slice(4) });
    written = navigator.clipboard.write([new ClipboardItem(data)]);
  } catch (error) {
    written = Promise.reject(error);
  }
  const results = await Promise.allSettled([png, written]);
  preferInternal = results.some((r) => r.status === 'rejected');
  if (!preferInternal) copiedToken = token;
  const failure = results.find((r) => r.status === 'rejected');
  useSelection.setState({
    busy: false,
    notice:
      failure?.status === 'rejected'
        ? (prepared ? 'Copied internally. System clipboard unavailable: ' : '') +
          String(failure.reason)
        : cut
          ? 'Cut to clipboard'
          : 'Copied to clipboard',
  });
}
export async function pasteSelection(api: EngineApi, inPlace: boolean, forceSystem = false) {
  if (useWorkspace.getState().snapshot?.selection.quickMask) {
    useSelection.setState({ notice: 'Exit Quick Mask to paste a new layer' });
    return;
  }
  if (useSelection.getState().busy) return;
  useSelection.setState({ busy: true, notice: '' });
  let note = '';
  try {
    const internal = !!useWorkspace.getState().snapshot?.selection.clipboard;
    let useInternal = internal && (inPlace || (preferInternal && !forceSystem));
    if (!useInternal) {
      try {
        if (!navigator.clipboard?.read) throw new Error('Image clipboard API is unavailable');
        const items = await navigator.clipboard.read();
        const item = items.find((item) => item.types.includes('image/png'));
        if (!item) throw new Error('The system clipboard contains no image');
        if (internal && copiedToken && item.types.includes(customType))
          useInternal = (await (await item.getType(customType)).text()) === copiedToken;
        if (!useInternal) {
          const blob = await item.getType('image/png');
          if (blob.size > 128 * 1048576)
            throw new Error('Clipboard images must be at most 128 MiB');
          const bytes = await blob.arrayBuffer();
          await api.clipboardImport(transfer(bytes, [bytes]));
        }
      } catch (error) {
        if (!internal || forceSystem) throw error;
        note = 'System clipboard unavailable; pasted the internal copy. ' + String(error);
      }
    }
    const snapshot = await api.selectionAction({ kind: 'paste', inPlace });
    updateWorkspace({ snapshot });
    useSelection.setState({ notice: note || 'Pasted as a new layer' });
  } catch (error) {
    useSelection.setState({ notice: String(error) });
  } finally {
    useSelection.setState({ busy: false });
  }
}
export function selectionMode(shift: boolean, alt: boolean): SelectionMode {
  return shift && alt
    ? 'intersect'
    : shift
      ? 'add'
      : alt
        ? 'subtract'
        : useSelection.getState().mode;
}
