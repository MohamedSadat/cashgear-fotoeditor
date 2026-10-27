import { useCallback, useEffect, useRef, useState } from 'react';
import { CanvasHost } from './CanvasHost';
import { BrushPanel } from './BrushPanel';
import { HistoryPanel } from './HistoryPanel';
import { LayerPanel } from './LayerPanel';
import { SelectionControls } from './SelectionControls';
import { AdjustmentPanel } from './AdjustmentPanel';
import { FilterPanel } from './FilterPanel';
import { GeometryPanel } from './GeometryPanel';
import { TransformPanel } from './TransformPanel';
import { CropPanel } from './CropPanel';
import { useCrop, startCrop, resetCrop } from './crop-state';
import { startTransform, cancelTransform, useTransform, resetTransform } from './transform-state';
import { useSelection } from './selection-state';
import { useBrush } from './brush-state';
import { updateWorkspace, useWorkspace } from './state';
import type { EngineApi, Interaction } from './engine/types';
import {
  fileAccessError,
  filePermission,
  forgetRecent,
  pickerCancelled,
  recentFiles,
  rememberFile,
} from './file-access';
import type { RecentFile } from './file-access';

const presets = [
  { name: 'Full HD', width: 1920, height: 1080, dpi: 72 },
  { name: '4K UHD', width: 3840, height: 2160, dpi: 72 },
  { name: 'Photo · 24 MP', width: 6000, height: 4000, dpi: 300 },
  { name: 'A4 · Print', width: 2480, height: 3508, dpi: 300 },
];
const memory = (n: number) => (n / 1048576).toFixed(1) + ' MiB';
export function App() {
  const [generation, setGeneration] = useState(0);
  const [dialog, setDialog] = useState(false);
  const [showBrush, setShowBrush] = useState(false);
  const [preset, setPreset] = useState('Full HD');
  const [width, setWidth] = useState(1920);
  const [height, setHeight] = useState(1080);
  const [dpi, setDpi] = useState(72);
  const [busy, setBusy] = useState('');
  const [notice, setNotice] = useState('');
  const [recents, setRecents] = useState<RecentFile[]>([]);
  const [fileNotice, setFileNotice] = useState('');
  const projectHandle = useRef<FileSystemFileHandle | undefined>(undefined);
  const diagnostic = new URLSearchParams(location.search).get('diagnostic') === '1';
  const fault = useRef<((kind: 'device-loss' | 'panic') => void) | undefined>(undefined);
  const api = useRef<EngineApi | undefined>(undefined);
  const [engineApi, setEngineApi] = useState<EngineApi>();
  const projectFile = useRef<HTMLInputElement>(null);
  const recoveryDialog = useRef<HTMLDialogElement>(null);
  const [recoveryDismissed, setRecoveryDismissed] = useState(false);
  const file = useRef<HTMLInputElement>(null);
  const newDialog = useRef<HTMLDialogElement>(null);
  const operation = useRef(0);
  const workerJobStarted = useRef(false);
  const onFaultReady = useCallback((handler: typeof fault.current) => {
    fault.current = handler;
  }, []);
  const onApiReady = useCallback((engine: EngineApi | undefined) => {
    resetTransform();
    resetCrop();
    api.current = engine;
    projectHandle.current = undefined;
    setEngineApi(() => engine);
    if (new URLSearchParams(location.search).get('diagnostic') === '1')
      Reflect.set(window, '__fotoeditor', engine);
    setRecoveryDismissed(false);
    if (!engine) {
      ++operation.current;
      setBusy('');
    }
  }, []);
  const { status, backend, adapter, error, size, snapshot } = useWorkspace();
  const doc = snapshot?.document;
  const tool = useBrush((state) => state.tool);
  const selectionBusy = useSelection((state) => state.busy);
  const selectionNotice = useSelection((state) => state.notice);
  const painting = !!snapshot?.painting;
  const transformBusy = useTransform((s) => s.busy || s.pending);
  const cropBusy = useCrop((s) => s.busy);
  const workerOperation = !!snapshot?.operation;
  const working = !!busy || workerOperation || selectionBusy || transformBusy || cropBusy;
  const recovery = snapshot?.recovery;
  const recoverable = recovery?.candidates[0];
  useEffect(() => {
    let active = true;
    void recentFiles()
      .then((files) => {
        if (active) setRecents(files);
      })
      .catch(() => {
        if (active) setFileNotice('Recent files are unavailable in this browser.');
      });
    return () => {
      active = false;
    };
  }, []);
  const remember = async (handle: FileSystemFileHandle, kind: RecentFile['kind']) => {
    try {
      setRecents(await rememberFile(handle, kind));
      setFileNotice('');
    } catch {
      setFileNotice('The file opened or saved, but could not be added to recent files.');
    }
  };
  useEffect(() => {
    if (recoverable && !recoveryDismissed && status === 'ready')
      recoveryDialog.current?.showModal();
    else recoveryDialog.current?.close();
  }, [recoverable, recoveryDismissed, status]);
  useEffect(() => {
    if (!engineApi) return;
    const save = () => {
      if (useWorkspace.getState().snapshot?.painting) return;
      void engineApi.autosave().catch((e) => setNotice(String(e)));
    };
    const hidden = () => {
      if (document.visibilityState === 'hidden') save();
    };
    const timer = setInterval(save, 30000);
    document.addEventListener('visibilitychange', hidden);
    return () => {
      clearInterval(timer);
      document.removeEventListener('visibilitychange', hidden);
    };
  }, [engineApi]);
  useEffect(() => {
    if (dialog) newDialog.current?.showModal();
    else newDialog.current?.close();
  }, [dialog]);
  const navigate = (kind: Interaction['kind'], amount = 0) => {
    void Promise.resolve(api.current?.interact([{ kind, x: 0, y: 0, amount }])).catch((e) =>
      setNotice(String(e)),
    );
  };
  const open = async (selected: File, handle?: FileSystemFileHandle, pickedToken?: number) => {
    if (
      !api.current ||
      (pickedToken === undefined && (working || useWorkspace.getState().snapshot?.painting))
    )
      return;
    const token = pickedToken ?? ++operation.current;
    if (token !== operation.current) return;
    workerJobStarted.current = false;
    setBusy('Opening ' + selected.name + '…');
    setNotice('');
    const start = performance.now();
    try {
      const project = /\.cgfoto$/i.test(selected.name);
      if (selected.size > (project ? 4 * 1024 ** 3 : 128 * 1048576))
        throw new Error(
          project ? 'Projects must be at most 4 GiB' : 'Image files must be at most 128 MiB',
        );
      if (token !== operation.current) return;
      workerJobStarted.current = true;
      const next = project
        ? await api.current.openProjectFile(selected)
        : await api.current.openImageFile(selected);
      if (token === operation.current) {
        projectHandle.current = project ? handle : undefined;
        updateWorkspace({ snapshot: next });
        setNotice('Opened in ' + ((performance.now() - start) / 1000).toFixed(2) + ' s');
        if (handle) await remember(handle, project ? 'project' : 'image');
      }
    } catch (e) {
      if (token === operation.current) setNotice(String(e));
    } finally {
      if (token === operation.current) setBusy('');
    }
  };
  const chooseFile = async (kind: RecentFile['kind'], recent?: RecentFile) => {
    if (!api.current || working || painting) return;
    const picker = Reflect.get(window, 'showOpenFilePicker') as
      ((options: object) => Promise<FileSystemFileHandle[]>) | undefined;
    if (!recent && !picker) {
      (kind === 'project' ? projectFile : file).current?.click();
      return;
    }
    const token = ++operation.current;
    workerJobStarted.current = false;
    setBusy(recent ? 'Opening ' + recent.name + '…' : 'Choose a file…');
    setNotice('');
    try {
      let handle = recent?.handle;
      if (handle) await filePermission(handle, 'read');
      else
        [handle] = await picker!({
          multiple: false,
          types: [
            {
              description: kind === 'project' ? 'FotoEditor project' : 'Images',
              accept:
                kind === 'project'
                  ? { 'application/octet-stream': ['.cgfoto'] }
                  : {
                      'image/png': ['.png'],
                      'image/jpeg': ['.jpg', '.jpeg'],
                      'image/webp': ['.webp'],
                      'image/bmp': ['.bmp'],
                      'image/gif': ['.gif'],
                      'image/tiff': ['.tif', '.tiff'],
                      'image/vnd.adobe.photoshop': ['.psd'],
                    },
            },
          ],
        });
      if (!handle || token !== operation.current) return;
      const selected = await handle.getFile();
      if (token !== operation.current) return;
      await open(selected, handle, token);
    } catch (e) {
      if (token === operation.current && !pickerCancelled(e)) setNotice(fileAccessError(e));
    } finally {
      if (token === operation.current) setBusy('');
    }
  };
  const saveProject = async (saveAs = false) => {
    if (!api.current || !doc || working) return;
    const token = ++operation.current;
    workerJobStarted.current = false;
    setBusy('Saving project…');
    setNotice('');
    try {
      const picker = Reflect.get(window, 'showSaveFilePicker') as
        ((options: object) => Promise<FileSystemFileHandle>) | undefined;
      let handle = saveAs ? undefined : projectHandle.current;
      if (handle) await filePermission(handle, 'readwrite');
      else if (picker)
        handle = await picker({
          suggestedName: (snapshot?.name ?? 'Untitled').replace(/\.[^.]+$/, '') + '.cgfoto',
          types: [
            {
              description: 'FotoEditor project',
              accept: { 'application/octet-stream': ['.cgfoto'] },
            },
          ],
        });
      if (handle) {
        if (token !== operation.current) return;
        workerJobStarted.current = true;
        const size = await api.current.saveProjectFile(handle);
        if (token !== operation.current) return;
        projectHandle.current = handle;
        setNotice('Project saved · document only · ' + (size / 1048576).toFixed(1) + ' MiB');
        await remember(handle, 'project');
        return;
      }
      workerJobStarted.current = true;
      const file = await api.current.saveProjectDownload();
      if (token !== operation.current) return;
      const url = URL.createObjectURL(file);
      const link = document.createElement('a');
      link.href = url;
      link.download = (snapshot?.name ?? 'Untitled').replace(/\.[^.]+$/, '') + '.cgfoto';
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 30000);
      setNotice('Project saved · document only · ' + (file.size / 1048576).toFixed(1) + ' MiB');
    } catch (e) {
      if (token === operation.current && !pickerCancelled(e)) setNotice(fileAccessError(e));
    } finally {
      if (token === operation.current) setBusy('');
    }
  };
  const exportPng = async () => {
    if (!api.current || !doc || working || useWorkspace.getState().snapshot?.painting) return;
    const token = ++operation.current;
    workerJobStarted.current = true;
    setBusy('Exporting PNG…');
    setNotice('');
    try {
      const bytes = await api.current.exportPng();
      if (token !== operation.current) return;
      const url = URL.createObjectURL(
        new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'image/png' }),
      );
      const link = document.createElement('a');
      link.href = url;
      link.download = (snapshot?.name ?? 'Untitled').replace(/\.[^.]+$/, '') + '.png';
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 30000);
      setNotice('PNG exported · ' + (bytes.length / 1024).toFixed(0) + ' KB');
    } catch (e) {
      if (token === operation.current) setNotice(String(e));
    } finally {
      if (token === operation.current) setBusy('');
    }
  };
  const create = async () => {
    if (useWorkspace.getState().snapshot?.painting || working) return;
    try {
      const next = await api.current?.newDocument(width, height, dpi);
      if (next) {
        projectHandle.current = undefined;
        updateWorkspace({ snapshot: next });
      }
      setDialog(false);
      setNotice('');
    } catch (e) {
      setNotice(String(e));
    }
  };
  const cursor = snapshot?.cursor;
  const inside =
    doc &&
    cursor &&
    cursor[0] >= 0 &&
    cursor[1] >= 0 &&
    cursor[0] < doc.width &&
    cursor[1] < doc.height;
  return (
    <div className="app" data-status={status}>
      <header className="topbar">
        <div className="brand-mark" aria-hidden="true">
          f<span>.</span>
        </div>
        <div className="brand">
          <strong>
            CashGear <span>FotoEditor</span>
          </strong>
          <small>YOUR CREATIVE WORKSPACE</small>
        </div>
        <nav className="file-actions" aria-label="File">
          <button
            className="brush-toggle"
            aria-expanded={showBrush}
            onClick={() => setShowBrush((v) => !v)}
          >
            Brush settings
          </button>
          <button
            disabled={status !== 'ready' || working || painting}
            onClick={() => {
              setNotice('');
              setDialog(true);
            }}
          >
            New canvas
          </button>
          <button
            disabled={status !== 'ready' || working || painting}
            onClick={() => void chooseFile('image')}
          >
            Open image
          </button>
          <button
            disabled={status !== 'ready' || working || painting}
            onClick={() => void chooseFile('project')}
          >
            Open project
          </button>
          <button
            disabled={!doc || status !== 'ready' || working || painting}
            onClick={() => void saveProject()}
          >
            Save project
          </button>
          <button
            disabled={!doc || status !== 'ready' || working || painting}
            onClick={() => void saveProject(true)}
          >
            Save project as
          </button>
          <button
            className="primary"
            disabled={!doc || status !== 'ready' || working || painting}
            onClick={() => void exportPng()}
          >
            Export PNG <span aria-hidden="true">↗</span>
          </button>
        </nav>
        <input
          ref={projectFile}
          type="file"
          data-testid="open-project"
          accept=".cgfoto"
          hidden
          onChange={(e) => {
            const selected = e.target.files?.[0];
            e.target.value = '';
            if (selected) void open(selected);
          }}
        />
        <input
          ref={file}
          type="file"
          data-testid="open-image"
          accept="image/png,image/jpeg,image/webp,image/bmp,image/gif,image/tiff,image/vnd.adobe.photoshop,.png,.jpg,.jpeg,.webp,.bmp,.gif,.tif,.tiff,.psd"
          hidden
          onChange={(e) => {
            const selected = e.target.files?.[0];
            e.target.value = '';
            if (selected) void open(selected);
          }}
        />
      </header>
      <details className="recent-files">
        <summary>Recent files</summary>
        <div className="recent-list">
          {recents.length === 0 && <p>No recent files. Open a file to add it here.</p>}
          {recents.map((recent) => (
            <div key={recent.id}>
              <button
                disabled={status !== 'ready' || working || painting}
                onClick={(event) => {
                  event.currentTarget.closest('details')?.removeAttribute('open');
                  void chooseFile(recent.kind, recent);
                }}
              >
                {recent.name}
              </button>
              <span>{recent.kind === 'project' ? 'Project' : 'Image'}</span>
              <button
                aria-label={'Remove ' + recent.name + ' from recent files'}
                onClick={() => {
                  void forgetRecent(recent.id)
                    .then(() => setRecents((files) => files.filter((f) => f.id !== recent.id)))
                    .catch(() => setFileNotice('Could not remove this recent file.'));
                }}
              >
                Remove
              </button>
            </div>
          ))}
          {recents.length > 0 && <p>Opening a recent file may ask for access again.</p>}
          {fileNotice && <p role="status">{fileNotice}</p>}
        </div>
      </details>
      <SelectionControls
        api={engineApi}
        disabled={!doc || status !== 'ready' || working || painting}
      />
      <main className="workspace">
        <aside className="rail" aria-label="Tools">
          <button
            aria-label="Move"
            title="Move · V"
            aria-pressed={tool === 'move'}
            onClick={() => useBrush.setState({ tool: 'move' })}
          >
            V
          </button>
          <button
            aria-label="Free transform"
            title="Free transform · Ctrl+T"
            aria-pressed={tool === 'transform'}
            disabled={!doc || working || painting}
            onClick={() => engineApi && void startTransform(engineApi)}
          >
            T
          </button>
          <button
            aria-label="Crop"
            title="Crop · C"
            aria-pressed={tool === 'crop'}
            disabled={!doc || working || painting}
            onClick={startCrop}
          >
            C
          </button>
          {(
            [
              'brush',
              'eraser',
              'eyedropper',
              'rectangle',
              'ellipse',
              'lasso',
              'polygon',
              'wand',
              'bucket',
            ] as const
          ).map((t, i) => (
            <button
              key={t}
              aria-label={t[0].toUpperCase() + t.slice(1)}
              aria-pressed={tool === t}
              title={`${t} · ${['B', 'E', 'I', 'M', 'Shift+M', 'L', 'Shift+L', 'W', 'G'][i]}`}
              onClick={() => useBrush.setState({ tool: t })}
            >
              {['B', 'E', 'I', '▭', '◯', 'L', '⌁', 'W', 'G'][i]}
            </button>
          ))}
          <span className="rail-line" />
          <button
            aria-label="Undo"
            title="Undo · Ctrl+Z"
            disabled={!snapshot?.history.undo || painting || !!busy}
            onClick={() => navigate('undo')}
          >
            ↶
          </button>
          <button
            aria-label="Redo"
            title="Redo · Ctrl+Shift+Z"
            disabled={!snapshot?.history.redo || painting || !!busy}
            onClick={() => navigate('redo')}
          >
            ↷
          </button>
          <button
            title="Fit canvas"
            aria-label="Fit canvas"
            disabled={!doc}
            onClick={() => navigate('fit')}
          >
            ▣
          </button>
          <button
            title="100% · Actual pixels"
            aria-label="Actual pixels"
            disabled={!doc}
            onClick={() => navigate('actual')}
          >
            1:1
          </button>
          <span className="rail-line" />
          <button
            title="Rotate view 90°"
            aria-label="Rotate view 90 degrees"
            disabled={!doc}
            onClick={() => navigate('rotate', Math.PI / 2)}
          >
            ↻
          </button>
          <button
            title="Flip view horizontally"
            aria-label="Flip view horizontally"
            disabled={!doc}
            onClick={() => navigate('flip')}
          >
            ⇆
          </button>
          <button
            title="Reset view rotation"
            aria-label="Reset view rotation"
            disabled={!doc}
            onClick={() => navigate('reset-rotation')}
          >
            0°
          </button>
          <span className="rail-label">VIEW</span>
        </aside>
        <section className="viewport" aria-label="Canvas workspace">
          <div className="viewport-heading">
            <span className="document-dot" />
            <span data-testid="document-name">{doc ? snapshot?.name : 'Untitled workspace'}</span>
            <span className="viewport-caption">
              {doc ? 'SPACE + DRAG TO PAN · CTRL + WHEEL TO ZOOM' : 'A PLACE TO CREATE'}
            </span>
          </div>
          <div
            className="canvas-area"
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              const selected = e.dataTransfer.files[0];
              if (selected) void open(selected);
            }}
          >
            <CanvasHost
              generation={generation}
              diagnostic={diagnostic}
              onFaultReady={onFaultReady}
              onApiReady={onApiReady}
            />
            {!!snapshot?.refining && (
              <span className="preview-refining" role="status">
                Refining preview…
              </span>
            )}
            {!diagnostic && !doc && status === 'ready' && (
              <div className="canvas-welcome">
                <span className="welcome-icon" aria-hidden="true">
                  +
                </span>
                <h1>A fresh start.</h1>
                <p>Drop a PNG, JPEG, WebP, BMP, GIF or 8-bit TIFF, or create a new canvas.</p>
                <div className="welcome-actions">
                  <button
                    className="primary"
                    onClick={() => file.current?.click()}
                    disabled={!!busy}
                  >
                    Open an image
                  </button>
                  <button onClick={() => setDialog(true)} disabled={!!busy}>
                    Create canvas
                  </button>
                </div>
              </div>
            )}
            {status === 'starting' && (
              <div className="overlay" role="status">
                <span className="spinner" />
                <h1>Preparing your workspace</h1>
                <p>Starting the graphics engine…</p>
              </div>
            )}
            {status === 'error' && (
              <div className="overlay error" role="alert">
                <span className="error-symbol">!</span>
                <h1>Let’s restart the canvas</h1>
                <p>{error}</p>
                <p>
                  Restarting offers any completed autosave. Changes since that snapshot are
                  unavailable.
                </p>
                <div className="error-actions">
                  <button onClick={() => setGeneration((v) => v + 1)}>Retry engine</button>
                  <a href={'?gpu=webgl2' + (diagnostic ? '&diagnostic=1' : '')}>Try WebGL2</a>
                </div>
              </div>
            )}
          </div>
        </section>
        <aside
          className={'inspector' + (showBrush ? ' is-open' : '')}
          aria-label="Brush and document settings"
        >
          <nav className="inspector-index" aria-label="Inspector sections">
            <button
              aria-label="Show canvas section"
              onClick={() =>
                document.querySelector('.geometry-panel')?.scrollIntoView({ block: 'start' })
              }
            >
              Canvas
            </button>
            <button
              aria-label="Show filters section"
              onClick={() =>
                document.querySelector('.filter-panel')?.scrollIntoView({ block: 'start' })
              }
            >
              Filters
            </button>
            <button
              aria-label="Show adjustments section"
              onClick={() =>
                document.querySelector('.adjustment-panel')?.scrollIntoView({ block: 'start' })
              }
            >
              Adjust
            </button>
            <button
              aria-label="Show brush section"
              onClick={() =>
                document.querySelector('.brush-panel')?.scrollIntoView({ block: 'start' })
              }
            >
              Brush
            </button>
            <button
              aria-label="Show history section"
              onClick={() =>
                document.querySelector('.history-panel')?.scrollIntoView({ block: 'start' })
              }
            >
              History
            </button>
            <button
              aria-label="Show recovery section"
              onClick={() =>
                document.querySelector('.recovery-status')?.scrollIntoView({ block: 'start' })
              }
            >
              Recovery
            </button>
          </nav>
          <BrushPanel />
          <div className="section-divider" />
          <TransformPanel
            api={engineApi}
            disabled={status !== 'ready' || working || painting || !doc}
          />
          <div className="section-divider" />
          <GeometryPanel
            api={engineApi}
            disabled={status !== 'ready' || working || painting || !doc}
          />
          <div className="section-divider" />
          <CropPanel
            api={engineApi}
            disabled={
              status !== 'ready' ||
              !!busy ||
              !!snapshot?.operation ||
              selectionBusy ||
              transformBusy ||
              painting ||
              !doc
            }
          />
          <div className="section-divider" />
          <HistoryPanel api={engineApi} disabled={status !== 'ready' || working || painting} />
          <div className="section-divider" />
          <LayerPanel
            api={engineApi}
            disabled={status !== 'ready' || working || painting || !doc}
          />
          <div className="section-divider" />
          <AdjustmentPanel
            api={engineApi}
            disabled={status !== 'ready' || working || painting || !doc}
          />
          <div className="section-divider" />
          <FilterPanel
            api={engineApi}
            disabled={status !== 'ready' || working || painting || !doc}
          />
          <div className="section-divider" />
          <section className="recovery-status" aria-label="Autosave and recovery">
            <span className="section-eyebrow">AUTOSAVE · EVERY 30 SECONDS + TAB HIDE</span>
            <p data-testid="recovery-status">
              {recovery?.state === 'saved'
                ? 'Autosaved ' + new Date(recovery.lastSavedAt!).toLocaleTimeString()
                : recovery?.state === 'error'
                  ? 'Autosave failed'
                  : recovery?.state === 'unavailable'
                    ? 'Autosave unavailable'
                    : recovery?.state === 'saving'
                      ? 'Autosaving…'
                      : 'Waiting for changes'}
            </p>
            <p className="history-note">
              {recovery?.persistent === 'granted'
                ? 'Persistent browser storage granted.'
                : 'Browser storage is not persistent; save project files for a durable copy.'}{' '}
              Tab-hide saves are best effort.
            </p>
            {recovery?.error && <p role="alert">{recovery.error}</p>}
            <button
              disabled={!doc || working || painting || recovery?.state === 'unavailable'}
              onClick={() => void api.current?.autosave()}
            >
              Save recovery now
            </button>
            {recoverable && (
              <button onClick={() => setRecoveryDismissed(false)}>Recover session</button>
            )}
          </section>
          <div className="section-divider" />
          <span className="section-eyebrow">{doc ? 'DOCUMENT' : 'WORKSPACE'}</span>
          <h2>{doc ? 'A closer look.' : 'Room for your ideas.'}</h2>
          <p className="inspector-copy">
            {doc
              ? 'Paint, explore, and undo each stroke.'
              : 'Open an image or start with a transparent canvas.'}
          </p>
          <dl>
            <div>
              <dt>Canvas</dt>
              <dd data-testid="document-size">{doc ? doc.width + ' × ' + doc.height : '—'}</dd>
            </div>
            <div>
              <dt>Resolution</dt>
              <dd>{doc ? doc.dpi.toFixed(0) + ' DPI' : '—'}</dd>
            </div>
            <div>
              <dt>Layer</dt>
              <dd>{doc ? '1 · Raster' : '—'}</dd>
            </div>
          </dl>
          <div className="section-divider" />
          <span className="section-eyebrow">VIEW</span>
          <dl>
            <div>
              <dt>Zoom</dt>
              <dd data-testid="zoom">{doc ? (snapshot!.view.zoom * 100).toFixed(1) + '%' : '—'}</dd>
            </div>
            <div>
              <dt>Rotation</dt>
              <dd>{doc ? ((snapshot!.view.rotation * 180) / Math.PI).toFixed(1) + '°' : '—'}</dd>
            </div>
            <div>
              <dt>Renderer</dt>
              <dd>{backend ?? 'Starting…'}</dd>
            </div>
            <div>
              <dt>Display</dt>
              <dd>{size.width ? size.width + ' × ' + size.height : '—'}</dd>
            </div>
          </dl>
          <div className="section-divider" />
          <span className="section-eyebrow">MEMORY</span>
          <dl
            className="memory-readout"
            title="Live tile, job buffer and GPU atlas allocations. Browser, codec internals and driver overhead are estimates or excluded; see documentation."
          >
            <div>
              <dt>Document</dt>
              <dd>{memory(snapshot?.memory.document ?? 0)}</dd>
            </div>
            <div>
              <dt>History</dt>
              <dd>{memory(snapshot?.memory.history ?? 0)}</dd>
            </div>
            <div>
              <dt>Temporary</dt>
              <dd>{memory(snapshot?.memory.temporary ?? 0)}</dd>
            </div>
            <div>
              <dt>GPU atlas</dt>
              <dd>{memory(snapshot?.memory.gpu ?? 0)}</dd>
            </div>
          </dl>
          <div className="workspace-note">
            <span className="note-dot" />
            <p>
              Space + drag to pan
              <br />R + drag to rotate
              <br />
              Ctrl + wheel or pinch to zoom
            </p>
          </div>
        </aside>
      </main>
      {
        <div className="operation-bar" role="status" data-testid="operation-status">
          <span className="operation-message" title={busy || notice}>
            {busy ||
              snapshot?.operation ||
              snapshot?.editError ||
              (transformBusy ? 'Updating transform…' : '') ||
              (cropBusy ? 'Applying crop…' : '') ||
              selectionNotice ||
              notice ||
              (painting ? 'Painting…' : 'B brush · E eraser · Alt sample · Ctrl+Z undo')}
          </span>
          {(busy || workerOperation || selectionBusy || transformBusy || cropBusy) && (
            <button
              onClick={() => {
                const token = operation.current;
                void Promise.resolve(
                  workerOperation || selectionBusy || transformBusy || cropBusy
                    ? api.current?.cancelJob()
                    : !workerJobStarted.current || api.current?.cancelJob(),
                )
                  .then((cancelled) => {
                    if (cancelled && token === operation.current) {
                      if (transformBusy && api.current) void cancelTransform(api.current);
                      ++operation.current;
                      setBusy('');
                      setNotice('Operation cancelled');
                    }
                  })
                  .catch((e) => setNotice(String(e)));
              }}
            >
              Cancel operation
            </button>
          )}
        </div>
      }
      {diagnostic && (
        <div className="diagnostic-bar">
          <strong>GPU test scene</strong>
          <button onClick={() => fault.current?.('device-loss')}>Simulate device loss</button>
          <button onClick={() => fault.current?.('panic')}>Simulate Rust panic</button>
          {(['quota', 'partial', 'after-flush', 'pause-after-chunk'] as const).map((kind) => (
            <button key={kind} onClick={() => void api.current?.diagnosticRecoveryFault(kind)}>
              Fail next autosave: {kind}
            </button>
          ))}
        </div>
      )}
      <footer className="statusbar">
        <span className={'status-dot ' + status} />
        <span role="status" data-testid="engine-status">
          {status === 'ready'
            ? 'Engine ready'
            : status === 'error'
              ? 'Engine stopped'
              : 'Engine starting'}
        </span>
        <span className="status-separator">/</span>
        <span data-testid="gpu-backend" title={adapter}>
          {backend ?? 'Detecting GPU'}
        </span>
        <span className="cursor-position" data-testid="cursor-position">
          {inside ? Math.floor(cursor[0]) + ', ' + Math.floor(cursor[1]) + ' px' : '—'}
        </span>
        <span className="status-right">
          {doc
            ? doc.width + ' × ' + doc.height + ' · ' + (snapshot!.view.zoom * 100).toFixed(1) + '%'
            : 'READY WHEN YOU ARE'}
        </span>
        <span data-testid="status-memory" title="Logical CPU tile buffers">
          Doc {memory(snapshot?.memory.document ?? 0)} · Undo{' '}
          {memory(snapshot?.memory.history ?? 0)}
        </span>
      </footer>
      <dialog
        ref={recoveryDialog}
        onCancel={() => setRecoveryDismissed(true)}
        aria-labelledby="recovery-title"
      >
        <h2 id="recovery-title">Restore last session?</h2>
        <p>
          {recoverable?.name} · {recoverable ? new Date(recoverable.savedAt).toLocaleString() : ''}
        </p>
        <p className="dialog-note">
          Restore the last completed autosave with fresh undo history. Unsaved changes after that
          snapshot are unavailable.
          {doc && ' Restoring replaces the current canvas; save a project first if you need it.'}
        </p>
        <div className="dialog-actions">
          <button onClick={() => setRecoveryDismissed(true)}>Later</button>
          <button
            disabled={working}
            onClick={() => {
              if (recoverable)
                void api.current
                  ?.discardRecovery(recoverable.file)
                  .catch((e) => setNotice(String(e)));
            }}
          >
            Discard recovery
          </button>
          <button
            className="primary"
            disabled={working}
            onClick={() => {
              if (recoverable)
                void api.current
                  ?.restoreRecovery(recoverable.file)
                  .then((next) => {
                    projectHandle.current = undefined;
                    updateWorkspace({ snapshot: next });
                    setRecoveryDismissed(true);
                    setNotice('Session restored · fresh undo history');
                  })
                  .catch((e) => setNotice(String(e)));
            }}
          >
            Restore session
          </button>
        </div>
      </dialog>
      <dialog
        ref={newDialog}
        onCancel={() => setDialog(false)}
        onClose={() => setDialog(false)}
        aria-labelledby="new-title"
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
        >
          <span className="section-eyebrow">START SOMETHING NEW</span>
          <h2 id="new-title">New canvas</h2>
          <label>
            Preset
            <select
              value={preset}
              onChange={(e) => {
                setPreset(e.target.value);
                const p = presets.find((p) => p.name === e.target.value);
                if (p) {
                  setWidth(p.width);
                  setHeight(p.height);
                  setDpi(p.dpi);
                }
              }}
            >
              {presets.map((p) => (
                <option key={p.name}>{p.name}</option>
              ))}
              <option>Custom</option>
            </select>
          </label>
          <div className="dimensions">
            <label>
              Width <span>px</span>
              <input
                aria-label="Width"
                type="number"
                min="1"
                max="16384"
                step="1"
                required
                value={width}
                onChange={(e) => {
                  setPreset('Custom');
                  setWidth(e.target.valueAsNumber);
                }}
              />
            </label>
            <label>
              Height <span>px</span>
              <input
                aria-label="Height"
                type="number"
                min="1"
                max="16384"
                step="1"
                required
                value={height}
                onChange={(e) => {
                  setPreset('Custom');
                  setHeight(e.target.valueAsNumber);
                }}
              />
            </label>
          </div>
          <label>
            Resolution <span>DPI</span>
            <input
              aria-label="Resolution DPI"
              type="number"
              min="1"
              max="2400"
              step="1"
              required
              value={dpi}
              onChange={(e) => {
                setPreset('Custom');
                setDpi(e.target.valueAsNumber);
              }}
            />
          </label>
          <p className="dialog-note">Transparent background · Maximum 16,384 px per side</p>
          {notice && <p role="alert">{notice}</p>}
          <div className="dialog-actions">
            <button type="button" onClick={() => setDialog(false)}>
              Cancel
            </button>
            <button className="primary" type="submit">
              Create document
            </button>
          </div>
        </form>
      </dialog>
    </div>
  );
}
