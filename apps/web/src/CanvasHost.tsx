import { useEffect, useRef } from 'react';
import { createEngineClient } from './engine/client';
import { FramePump } from './engine/frame-pump';
import type { BackendPreference } from './engine/types';
import { updateWorkspace } from './state';

interface Props {
  generation: number;
  diagnostic: boolean;
  onFaultReady: (fault: ((kind: 'device-loss' | 'panic') => void) | undefined) => void;
}
export function CanvasHost({ generation, diagnostic, onFaultReady }: Props) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const container = host.current!;
    // Every effect owns a fresh DOM canvas. StrictMode, HMR and Retry must never
    // transfer an already-transferred canvas or reuse a failed GPU context.
    const canvas = document.createElement('canvas');
    canvas.setAttribute('aria-label', 'GPU canvas');
    canvas.dataset.testid = 'gpu-canvas';
    container.appendChild(canvas);
    let disposed = false;
    let animation = 0;
    let pump: FramePump | undefined;
    let dprQuery: MediaQueryList | undefined;
    let failed = false;
    const fail = (message: string) => {
      if (disposed || failed) return;
      failed = true;
      clearTimeout(timeout);
      pump?.stop();
      cancelAnimationFrame(animation);
      updateWorkspace({ status: 'error', error: message });
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
      client.dispose();
      canvas.remove();
    };
  }, [generation, diagnostic, onFaultReady]);
  return <div className="canvas-host" ref={host} />;
}
