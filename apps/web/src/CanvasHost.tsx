import { useEffect, useRef } from 'react';
import { createEngineClient } from './engine/client';
import { FramePump } from './engine/frame-pump';
import { pointerSamples } from './engine/pointer-input';
import type { BackendPreference, EngineApi, Interaction } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';
import { evaluateCurve, setBrush, useBrush } from './brush-state';
import type { BrushSettings } from './brush-state';

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
    const rawInput = 'onpointerrawupdate' in window;
    canvas.dataset.pointerInput = rawInput ? 'raw' : 'coalesced-move';
    container.appendChild(canvas);
    const outline = document.createElement('div');
    outline.className = 'brush-outline';
    outline.hidden = true;
    outline.setAttribute('aria-hidden', 'true');
    container.appendChild(outline);
    let disposed = false;
    let animation = 0;
    let pump: FramePump | undefined;
    let dprQuery: MediaQueryList | undefined;
    let failed = false;
    let snapshotKey = '';
    let space = false;
    let rotate = false;
    let drag: { x: number; y: number; rotation: boolean } | undefined;
    let strokePointer: number | undefined;
    let strokeSettings: BrushSettings | undefined;
    let lastSample: Interaction['sample'];
    let lastSampleTime = -1;
    let lastScreen: [number, number] | undefined;
    let spray: ReturnType<typeof setInterval> | undefined;
    const send = (value: Interaction) => pump?.input(value);
    const endSpray = () => {
      clearInterval(spray);
      spray = undefined;
    };
    function cancelStroke() {
      endSpray();
      if (strokePointer !== undefined) send({ kind: 'stroke-cancel', x: 0, y: 0, amount: 0 });
      strokePointer = undefined;
      strokeSettings = undefined;
    }
    function refreshOutline() {
      const brush = useBrush.getState();
      const settings = strokeSettings ?? brush.settings;
      const snapshot = useWorkspace.getState().snapshot;
      outline.hidden =
        !lastScreen || !snapshot?.document || space || rotate || brush.tool === 'eyedropper';
      canvas.style.cursor = outline.hidden ? 'crosshair' : 'none';
      if (!lastScreen || !snapshot) return;
      const pressure = strokePointer !== undefined ? (lastSample?.[2] ?? 1) : 1;
      const tilt =
        strokePointer !== undefined
          ? Math.min(1, Math.hypot(lastSample?.[3] ?? 0, lastSample?.[4] ?? 0) / 90)
          : 0;
      const scale =
        (settings.pressureSize ? evaluateCurve(settings.sizePressure, pressure) : 1) *
        (settings.tiltSize ? evaluateCurve(settings.sizeTilt, tilt) : 1);
      const diameter = (settings.pencil ? 1 : settings.size * scale) * snapshot.view.zoom;
      outline.style.left = lastScreen[0] + 'px';
      outline.style.top = lastScreen[1] + 'px';
      outline.style.width = Math.max(1, diameter) + 'px';
      outline.style.height =
        Math.max(1, diameter * (settings.pencil ? 1 : settings.roundness)) + 'px';
      outline.style.transform =
        'translate(-50%,-50%) rotate(' +
        ((snapshot.view.rotation * 180) / Math.PI +
          (snapshot.view.flipped ? -1 : 1) * settings.angle) +
        'deg)';
    }
    const unsubscribeBrush = useBrush.subscribe(refreshOutline);
    function key(event: KeyboardEvent) {
      if (event.type === 'keyup') {
        if (event.code === 'Space') space = false;
        if (event.code === 'KeyR') rotate = false;
        refreshOutline();
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
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && ['KeyZ', 'KeyY'].includes(event.code)) {
        event.preventDefault();
        if (strokePointer !== undefined) cancelStroke();
        else
          send({
            kind: event.code === 'KeyY' || event.shiftKey ? 'redo' : 'undo',
            x: 0,
            y: 0,
            amount: 0,
          });
      } else if (!modifier) {
        const tool = ({ KeyB: 'brush', KeyE: 'eraser', KeyI: 'eyedropper' } as const)[
          event.code as 'KeyB' | 'KeyE' | 'KeyI'
        ];
        if (tool) useBrush.setState({ tool });
        if (event.code === 'KeyX') useBrush.getState().swap();
        if (event.code === 'BracketLeft' || event.code === 'BracketRight') {
          event.preventDefault();
          const size = useBrush.getState().settings.size;
          setBrush({
            pencil: false,
            size: Math.max(
              1,
              Math.min(2048, Math.round(size * (event.code === 'BracketLeft' ? 0.8 : 1.25))),
            ),
          });
        }
        if (event.code === 'Escape') cancelStroke();
      }
      refreshOutline();
    }
    function resetKeys() {
      space = false;
      rotate = false;
      drag = undefined;
      cancelStroke();
      refreshOutline();
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
        if (strokePointer !== undefined) return;
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
      } else if (
        (event.button === 0 || event.button === 5) &&
        useWorkspace.getState().snapshot?.document &&
        !useWorkspace.getState().snapshot?.operation
      ) {
        event.preventDefault();
        const brush = useBrush.getState();
        const rect = canvas.getBoundingClientRect();
        const x = event.clientX - rect.left,
          y = event.clientY - rect.top;
        if (event.altKey || brush.tool === 'eyedropper') {
          send({ kind: 'sample-color', x, y, amount: 0 });
          return;
        }
        if (strokePointer !== undefined) return;
        updateWorkspace((state) => ({
          snapshot: state.snapshot ? { ...state.snapshot, painting: true } : undefined,
        }));
        strokePointer = event.pointerId;
        strokeSettings = brush.settings;
        canvas.setPointerCapture(event.pointerId);
        lastSample = [
          x,
          y,
          event.pointerType === 'pen' ? event.pressure : 1,
          event.tiltX,
          event.tiltY,
          event.timeStamp,
        ];
        lastSampleTime = event.timeStamp;
        send({
          kind: 'stroke-begin',
          x,
          y,
          amount: 0,
          sample: lastSample,
          brush: {
            ...brush.settings,
            color: brush.primary,
            eraser: brush.tool === 'eraser' || event.button === 5,
          },
        });
        if (brush.settings.flow < 1 && !brush.settings.pencil)
          spray = setInterval(() => {
            if (lastSample) {
              lastSample = [...lastSample];
              lastSample[5] = performance.now();
              send({ kind: 'stroke-sample', x: 0, y: 0, amount: 0, sample: lastSample });
            }
          }, 33);
      }
    });
    function strokeSamples(event: PointerEvent) {
      if (strokePointer !== event.pointerId) return;
      const rect = canvas.getBoundingClientRect();
      for (const p of pointerSamples(event, rect, lastSampleTime)) {
        lastSample = p;
        lastSampleTime = p[5];
        send({ kind: 'stroke-sample', x: 0, y: 0, amount: 0, sample: lastSample });
      }
      lastScreen = [event.clientX - rect.left, event.clientY - rect.top];
      refreshOutline();
    }
    if (rawInput)
      canvas.addEventListener('pointerrawupdate', (event) => {
        if (event instanceof PointerEvent) strokeSamples(event);
      });
    canvas.addEventListener('pointermove', (event) => {
      const rect = canvas.getBoundingClientRect();
      lastScreen = [event.clientX - rect.left, event.clientY - rect.top];
      refreshOutline();
      if (drag) {
        const dx = event.clientX - drag.x;
        const dy = event.clientY - drag.y;
        input(drag.rotation ? 'rotate' : 'pan', dx, dy, dx * 0.01);
        drag.x = event.clientX;
        drag.y = event.clientY;
      }
      strokeSamples(event);
      input('cursor', event.clientX - rect.left, event.clientY - rect.top);
    });
    canvas.addEventListener('pointerup', (event) => {
      drag = undefined;
      if (strokePointer === event.pointerId) {
        const rect = canvas.getBoundingClientRect();
        endSpray();
        const sample: Interaction['sample'] = [
          event.clientX - rect.left,
          event.clientY - rect.top,
          lastSample?.[2] ?? 1,
          event.tiltX,
          event.tiltY,
          event.timeStamp,
        ];
        send({ kind: 'stroke-end', x: 0, y: 0, amount: 0, sample });
        strokePointer = undefined;
        strokeSettings = undefined;
      }
    });
    canvas.addEventListener('pointercancel', () => {
      drag = undefined;
      cancelStroke();
    });
    canvas.addEventListener('lostpointercapture', () => {
      drag = undefined;
      cancelStroke();
    });
    canvas.addEventListener('pointerleave', () => {
      lastScreen = undefined;
      refreshOutline();
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
      60000,
    );
    void client
      .init(offscreen, {
        ...initialSize,
        backend,
        diagnostic,
        diagnosticTexture: diagnostic && params.get('document') !== '1',
        failInitialization: diagnostic && generation === 0 && params.get('fault') === 'init',
        simulateGpuFailure:
          diagnostic && ['adapter', 'device'].includes(params.get('fault') ?? '')
            ? (params.get('fault') as 'adapter' | 'device')
            : undefined,
      })
      .then(async (info) => {
        clearTimeout(timeout);
        if (disposed || failed) return;
        let persistent: 'granted' | 'denied' | 'unavailable' = 'unavailable';
        try {
          // StorageManager.persist() belongs to Window, never the engine worker.
          if (navigator.storage?.persist)
            persistent = (await navigator.storage.persist()) ? 'granted' : 'denied';
        } catch {
          /* Storage denial must not prevent editing. */
        }
        if (disposed || failed) return;
        await client.api.startRecovery(persistent);
        if (disposed || failed) return;
        canvas.dataset.backend = info.backend;
        updateWorkspace({ ...info, status: 'ready' });
        pump = new FramePump(
          client.api,
          (frame) => {
            canvas.dataset.frames = String(frame.frames);
            // This attribute records whether the canvas has presented pixels;
            // idle worker polls do not submit duplicate GPU frames.
            if (frame.presented) canvas.dataset.presented = 'true';
            canvas.dataset.renderWidth = String(frame.width);
            canvas.dataset.renderHeight = String(frame.height);
            canvas.dataset.timing = JSON.stringify(frame.snapshot?.timing);
            const key = JSON.stringify({ ...frame.snapshot, timing: undefined });
            if (key !== snapshotKey) {
              snapshotKey = key;
              canvas.dataset.snapshot = key;
              updateWorkspace({ snapshot: frame.snapshot });
              const picked = frame.snapshot?.sampledColor;
              if (!picked && useBrush.getState().sampledSerial !== 0)
                useBrush.setState({ sampledSerial: 0 });
              if (picked && picked.serial !== useBrush.getState().sampledSerial) {
                useBrush.getState().setColor(picked.rgba);
                useBrush.setState({ sampledSerial: picked.serial });
              }
              refreshOutline();
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
      endSpray();
      unsubscribeBrush();
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
      outline.remove();
    };
  }, [generation, diagnostic, onFaultReady, onApiReady]);
  return <div className="canvas-host" ref={host} />;
}
