import { useEffect, useRef } from 'react';
import { createEngineClient } from './engine/client';
import { FramePump } from './engine/frame-pump';
import type { BackendPreference, EngineApi, Interaction } from './engine/types';
import { updateWorkspace } from './state';

interface Props {
  generation: number;
  diagnostic: boolean;
  onApiReady: (api: EngineApi | undefined) => void;
  onFaultReady: (fault: ((kind: 'device-loss' | 'panic') => void) | undefined) => void;
}
export function CanvasHost({ generation, diagnostic, onFaultReady, onApiReady }: Props) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const container = host.current!;
    // Every effect owns a fresh DOM canvas. StrictMode, HMR and Retry must never
    // transfer an already-transferred canvas or reuse a failed GPU context.
    const canvas = document.createElement('canvas');
    canvas.setAttribute('aria-label', 'GPU canvas');
    canvas.dataset.testid = 'gpu-canvas';
    canvas.tabIndex = 0;
    container.appendChild(canvas);
    let disposed = false;
    let animation = 0;
    let pump: FramePump | undefined;
    let dprQuery: MediaQueryList | undefined;
    let failed = false;
    let snapshotKey = '';
    let space = false;
    let rotate = false;
    let drag: { x: number; y: number; rotation: boolean } | undefined;
    function key(event: KeyboardEvent) {
      if (event.type === 'keyup') {
        if (event.code === 'Space') space = false;
        if (event.code === 'KeyR') rotate = false;
        return;
      }
      if (
        event.target instanceof HTMLElement &&
        (['INPUT', 'SELECT', 'TEXTAREA'].includes(event.target.tagName) ||
          event.target.isContentEditable)
      )
        return;
      if (event.code === 'Space') {
        if (event.target instanceof HTMLButtonElement && !canvas.matches(':hover')) return;
        space = event.type === 'keydown';
        event.preventDefault();
      }
      if (event.code === 'KeyR') rotate = event.type === 'keydown';
    }
    function resetKeys() {
      space = false;
      rotate = false;
      drag = undefined;
    }
    window.addEventListener('keydown', key);
    window.addEventListener('keyup', key);
    window.addEventListener('blur', resetKeys);
    const input = (kind: Interaction['kind'], x = 0, y = 0, amount = 0) =>
      pump?.input({ kind, x, y, amount });
    canvas.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault();
        const rect = canvas.getBoundingClientRect();
        const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? rect.height : 1;
        if (event.ctrlKey || event.metaKey)
          input(
            'zoom',
            event.clientX - rect.left,
            event.clientY - rect.top,
            Math.exp(-event.deltaY * unit * 0.002),
          );
        else
          input(
            'pan',
            -(event.deltaX + (event.shiftKey ? event.deltaY : 0)) * unit,
            event.shiftKey ? 0 : -event.deltaY * unit,
          );
      },
      { passive: false },
    );
    canvas.addEventListener('pointerdown', (event) => {
      canvas.focus();
      if (event.button === 1 || (event.button === 0 && (space || rotate))) {
        event.preventDefault();
        drag = { x: event.clientX, y: event.clientY, rotation: rotate };
        canvas.setPointerCapture(event.pointerId);
      }
    });
    canvas.addEventListener('pointermove', (event) => {
      if (drag) {
        const dx = event.clientX - drag.x;
        const dy = event.clientY - drag.y;
        input(drag.rotation ? 'rotate' : 'pan', dx, dy, dx * 0.01);
        drag.x = event.clientX;
        drag.y = event.clientY;
      }
      const rect = canvas.getBoundingClientRect();
      input('cursor', event.clientX - rect.left, event.clientY - rect.top);
    });
    canvas.addEventListener('pointerup', () => {
      drag = undefined;
    });
    canvas.addEventListener('pointercancel', () => {
      drag = undefined;
    });
    canvas.addEventListener('lostpointercapture', () => {
      drag = undefined;
    });
    const fail = (message: string) => {
      if (disposed || failed) return;
      failed = true;
      clearTimeout(timeout);
      pump?.stop();
      cancelAnimationFrame(animation);
      updateWorkspace({ status: 'error', error: message });
      onApiReady(undefined);
      client.dispose();
    };
    const client = createEngineClient(fail);
    const params = new URLSearchParams(location.search);
    const backend: BackendPreference = params.get('gpu') === 'webgl2' ? 'webgl2' : 'auto';
    function measure() {
      const rect = container.getBoundingClientRect();
      const size = {
        width: Math.max(0, Math.round(rect.width * devicePixelRatio)),
        height: Math.max(0, Math.round(rect.height * devicePixelRatio)),
        dpr: devicePixelRatio,
      };
      canvas.dataset.width = String(size.width);
      canvas.dataset.height = String(size.height);
      pump?.resize(size);
      return size;
    }
    function watchDpr() {
      dprQuery?.removeEventListener('change', onDprChange);
      dprQuery = matchMedia('(resolution: ' + devicePixelRatio + 'dppx)');
      dprQuery.addEventListener('change', onDprChange);
    }
    function onDprChange() {
      measure();
      watchDpr();
    }
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    watchDpr();
    window.addEventListener('resize', measure);
    updateWorkspace({
      status: 'starting',
      error: undefined,
      backend: undefined,
      adapter: undefined,
      frames: 0,
      snapshot: undefined,
    });
    const initialSize = measure();
    const offscreen = canvas.transferControlToOffscreen();
    const timeout = setTimeout(
      () => fail('The graphics engine did not start. Retry or try WebGL2.'),
      30000,
    );
    void client
      .init(offscreen, {
        ...initialSize,
        backend,
        diagnostic,
        failInitialization: diagnostic && generation === 0 && params.get('fault') === 'init',
        simulateGpuFailure:
          diagnostic && ['adapter', 'device'].includes(params.get('fault') ?? '')
            ? (params.get('fault') as 'adapter' | 'device')
            : undefined,
      })
      .then((info) => {
        clearTimeout(timeout);
        if (disposed || failed) return;
        canvas.dataset.backend = info.backend;
        updateWorkspace({ ...info, status: 'ready' });
        pump = new FramePump(
          client.api,
          (frame) => {
            canvas.dataset.frames = String(frame.frames);
            canvas.dataset.presented = String(frame.presented);
            canvas.dataset.renderWidth = String(frame.width);
            canvas.dataset.renderHeight = String(frame.height);
            const key = JSON.stringify(frame.snapshot);
            if (key !== snapshotKey) {
              snapshotKey = key;
              canvas.dataset.snapshot = key;
              updateWorkspace({ snapshot: frame.snapshot });
            }
            updateWorkspace((state) =>
              state.size.width === frame.width && state.size.height === frame.height
                ? state
                : { size: { width: frame.width, height: frame.height } },
            );
          },
          fail,
        );
        measure();
        const tick = (timestamp: number) => {
          if (disposed || failed) return;
          pump!.tick(timestamp);
          animation = requestAnimationFrame(tick);
        };
        animation = requestAnimationFrame(tick);
        onFaultReady((kind) => {
          void client.api.diagnosticFault(kind).catch((error) => fail(String(error)));
        });
        onApiReady(client.api);
      })
      .catch((error) => fail(String(error)));
    return () => {
      disposed = true;
      clearTimeout(timeout);
      cancelAnimationFrame(animation);
      pump?.stop();
      observer.disconnect();
      dprQuery?.removeEventListener('change', onDprChange);
      window.removeEventListener('resize', measure);
      onFaultReady(undefined);
      onApiReady(undefined);
      window.removeEventListener('keydown', key);
      window.removeEventListener('keyup', key);
      window.removeEventListener('blur', resetKeys);
      client.dispose();
      canvas.remove();
    };
  }, [generation, diagnostic, onFaultReady, onApiReady]);
  return <div className="canvas-host" ref={host} />;
}
