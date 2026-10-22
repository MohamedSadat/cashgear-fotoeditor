import { useCallback, useEffect, useRef, useState } from 'react';
import { transfer } from 'comlink';
import { CanvasHost } from './CanvasHost';
import { updateWorkspace, useWorkspace } from './state';
import type { EngineApi, Interaction } from './engine/types';

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
  const [preset, setPreset] = useState('Full HD');
  const [width, setWidth] = useState(1920);
  const [height, setHeight] = useState(1080);
  const [dpi, setDpi] = useState(72);
  const [busy, setBusy] = useState('');
  const [notice, setNotice] = useState('');
  const diagnostic = new URLSearchParams(location.search).get('diagnostic') === '1';
  const fault = useRef<((kind: 'device-loss' | 'panic') => void) | undefined>(undefined);
  const api = useRef<EngineApi | undefined>(undefined);
  const file = useRef<HTMLInputElement>(null);
  const newDialog = useRef<HTMLDialogElement>(null);
  const operation = useRef(0);
  const onFaultReady = useCallback((handler: typeof fault.current) => {
    fault.current = handler;
  }, []);
  const onApiReady = useCallback((engine: EngineApi | undefined) => {
    api.current = engine;
    if (!engine) {
      ++operation.current;
      setBusy('');
    }
  }, []);
  const { status, backend, adapter, error, size, snapshot } = useWorkspace();
  const doc = snapshot?.document;
  useEffect(() => {
    if (dialog) newDialog.current?.showModal();
    else newDialog.current?.close();
  }, [dialog]);
  const navigate = (kind: Interaction['kind'], amount = 0) => {
    void Promise.resolve(api.current?.interact([{ kind, x: 0, y: 0, amount }])).catch((e) =>
      setNotice(String(e)),
    );
  };
  const open = async (selected: File) => {
    if (!api.current || busy) return;
    const token = ++operation.current;
    setBusy('Opening ' + selected.name + '…');
    setNotice('');
    const start = performance.now();
    try {
      if (selected.size > 128 * 1048576) throw new Error('Image files must be at most 128 MiB');
      const bytes = await selected.arrayBuffer();
      if (token !== operation.current) return;
      const next = await api.current.openImage(transfer(bytes, [bytes]), selected.name);
      if (token === operation.current) {
        updateWorkspace({ snapshot: next });
        setNotice('Opened in ' + ((performance.now() - start) / 1000).toFixed(2) + ' s');
      }
    } catch (e) {
      if (token === operation.current) setNotice(String(e));
    } finally {
      if (token === operation.current) setBusy('');
    }
  };
  const exportPng = async () => {
    if (!api.current || !doc || busy) return;
    const token = ++operation.current;
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
    try {
      const next = await api.current?.newDocument(width, height, dpi);
      if (next) updateWorkspace({ snapshot: next });
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
            disabled={status !== 'ready' || !!busy}
            onClick={() => {
              setNotice('');
              setDialog(true);
            }}
          >
            New canvas
          </button>
          <button disabled={status !== 'ready' || !!busy} onClick={() => file.current?.click()}>
            Open image
          </button>
          <button
            className="primary"
            disabled={!doc || status !== 'ready' || !!busy}
            onClick={() => void exportPng()}
          >
            Export PNG <span aria-hidden="true">↗</span>
          </button>
        </nav>
        <input
          ref={file}
          type="file"
          data-testid="open-image"
          accept="image/png,image/jpeg,image/webp,.png,.jpg,.jpeg,.webp"
          hidden
          onChange={(e) => {
            const selected = e.target.files?.[0];
            e.target.value = '';
            if (selected) void open(selected);
          }}
        />
      </header>
      <main className="workspace">
        <aside className="rail" aria-label="View tools">
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
                <p>Drop a PNG, JPEG or WebP, or create a new canvas.</p>
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
                <p>The worker owns your document. Restarting clears the current session.</p>
                <div className="error-actions">
                  <button onClick={() => setGeneration((v) => v + 1)}>Retry engine</button>
                  <a href={'?gpu=webgl2' + (diagnostic ? '&diagnostic=1' : '')}>Try WebGL2</a>
                </div>
              </div>
            )}
          </div>
        </section>
        <aside className="inspector">
          <span className="section-eyebrow">{doc ? 'DOCUMENT' : 'WORKSPACE'}</span>
          <h2>{doc ? 'A closer look.' : 'Room for your ideas.'}</h2>
          <p className="inspector-copy">
            {doc
              ? 'Navigate every detail. Your original image samples stay intact.'
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
      {(busy || notice) && (
        <div className="operation-bar" role="status" data-testid="operation-status">
          <span className="operation-message" title={busy || notice}>
            {busy || notice}
          </span>
          {busy && (
            <button
              onClick={() => {
                void Promise.resolve(api.current?.cancelJob())
                  .then((cancelled) => {
                    if (cancelled) {
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
      )}
      {diagnostic && (
        <div className="diagnostic-bar">
          <strong>GPU test scene</strong>
          <button onClick={() => fault.current?.('device-loss')}>Simulate device loss</button>
          <button onClick={() => fault.current?.('panic')}>Simulate Rust panic</button>
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
      </footer>
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
