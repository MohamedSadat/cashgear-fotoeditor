import { useCallback, useRef, useState } from 'react';
import { CanvasHost } from './CanvasHost';
import { useWorkspace } from './state';

export function App() {
  const [generation, setGeneration] = useState(0);
  const diagnostic = new URLSearchParams(location.search).get('diagnostic') === '1';
  const fault = useRef<((kind: 'device-loss' | 'panic') => void) | undefined>(undefined);
  const onFaultReady = useCallback((handler: typeof fault.current) => {
    fault.current = handler;
  }, []);
  const { status, backend, adapter, error, size } = useWorkspace();
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
        <span className="phase">
          FOUNDATION <span>01</span>
        </span>
      </header>
      <main className="workspace">
        <aside className="rail" aria-label="Workspace">
          <div className="rail-symbol" title="Canvas workspace" aria-hidden="true">
            ▣
          </div>
          <span className="rail-line" />
          <span className="rail-label">CANVAS</span>
        </aside>
        <section className="viewport" aria-label="Canvas workspace">
          <div className="viewport-heading">
            <span className="document-dot" /> Untitled workspace{' '}
            <span className="viewport-caption">A PLACE TO CREATE</span>
          </div>
          <div className="canvas-area">
            <CanvasHost
              generation={generation}
              diagnostic={diagnostic}
              onFaultReady={onFaultReady}
            />
            {!diagnostic && status === 'ready' && (
              <div className="canvas-welcome">
                <span className="welcome-icon" aria-hidden="true">
                  +
                </span>
                <h1>A fresh start.</h1>
                <p>Your canvas is ready for what comes next.</p>
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
                <div className="error-actions">
                  <button onClick={() => setGeneration((value) => value + 1)}>Retry engine</button>
                  <a href={'?gpu=webgl2' + (diagnostic ? '&diagnostic=1' : '')}>Try WebGL2</a>
                </div>
              </div>
            )}
          </div>
        </section>
        <aside className="inspector">
          <span className="section-eyebrow">WORKSPACE</span>
          <h2>Room for your ideas.</h2>
          <p className="inspector-copy">
            The foundation is ready. Image tools are coming in the next phase.
          </p>
          <div className="section-divider" />
          <span className="section-eyebrow">DISPLAY</span>
          <dl>
            <div>
              <dt>Renderer</dt>
              <dd>{backend ?? 'Starting…'}</dd>
            </div>
            <div>
              <dt>Resolution</dt>
              <dd>{size.width ? size.width + ' × ' + size.height : '—'}</dd>
            </div>
            <div>
              <dt>Engine</dt>
              <dd>Rust / WASM</dd>
            </div>
          </dl>
          <div className="workspace-note">
            <span className="note-dot" />
            <p>Built for a smooth, focused editing experience.</p>
          </div>
        </aside>
      </main>
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
        <span className="status-right">P0 · SCAFFOLD & GPU PIPELINE</span>
      </footer>
    </div>
  );
}
