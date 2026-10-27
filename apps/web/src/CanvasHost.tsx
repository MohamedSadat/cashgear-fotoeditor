import { useEffect, useRef } from 'react';
import { createEngineClient } from './engine/client';
import { FramePump } from './engine/frame-pump';
import { pointerSamples } from './engine/pointer-input';
import type { BackendPreference, EngineApi, Interaction } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';
import { useGeometry } from './geometry-state';
import { bindTransformInteractions } from './transform-interaction';
import { bindCropInteractions } from './crop-interaction';
import { cancelTransform } from './transform-state';
import { evaluateCurve, setBrush, useBrush } from './brush-state';
import type { BrushSettings } from './brush-state';
import {
  useSelection,
  performSelection,
  selectionMode,
  copySelection,
  pasteSelection,
} from './selection-state';
import type { SelectionAction } from './engine/types';

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
    const cloneMarker = document.createElement('div');
    cloneMarker.className = 'clone-source-marker';
    cloneMarker.setAttribute('aria-label', 'Clone source');
    cloneMarker.hidden = true;
    container.appendChild(cloneMarker);
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.classList.add('selection-gesture');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    svg.appendChild(path);
    container.appendChild(svg);
    const geometrySvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    geometrySvg.classList.add('geometry-overlay');
    geometrySvg.setAttribute('aria-label', 'Rulers and guides');
    container.appendChild(geometrySvg);
    let geometryKey = '';
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
    let selectionDrag:
      | {
          pointer: number;
          tool: 'rectangle' | 'ellipse' | 'lasso';
          points: [number, number][];
          mode: ReturnType<typeof selectionMode>;
        }
      | undefined;
    let polygon: [number, number][] = [];
    let gestureRevision: number | undefined;
    let polygonMode: ReturnType<typeof selectionMode> = 'new';
    const isSelecting = () =>
      !['brush', 'clone', 'eraser', 'eyedropper', 'move', 'transform', 'crop'].includes(
        useBrush.getState().tool,
      );
    const documentPoint = (x: number, y: number): [number, number] => {
      const s = useWorkspace.getState().snapshot!;
      const d = s.document!;
      const qx = (x - s.view.screenWidth / 2 - s.view.panX) / s.view.zoom;
      const qy = (y - s.view.screenHeight / 2 - s.view.panY) / s.view.zoom;
      const c = Math.cos(s.view.rotation),
        sn = Math.sin(s.view.rotation);
      return [
        (qx * c + qy * sn) * (s.view.flipped ? -1 : 1) + (d.retainedWidth ?? d.width) / 2,
        -qx * sn + qy * c + (d.retainedHeight ?? d.height) / 2,
      ];
    };
    const screenPoint = (p: [number, number]): [number, number] => {
      const s = useWorkspace.getState().snapshot!;
      const d = s.document!;
      const x = (p[0] - (d.retainedWidth ?? d.width) / 2) * (s.view.flipped ? -1 : 1),
        y = p[1] - (d.retainedHeight ?? d.height) / 2;
      const c = Math.cos(s.view.rotation),
        sn = Math.sin(s.view.rotation);
      return [
        s.view.screenWidth / 2 + s.view.panX + (x * c - y * sn) * s.view.zoom,
        s.view.screenHeight / 2 + s.view.panY + (x * sn + y * c) * s.view.zoom,
      ];
    };
    const transform = bindTransformInteractions(
      canvas,
      container,
      () => client.api,
      documentPoint,
      screenPoint,
    );
    const crop = bindCropInteractions(
      canvas,
      container,
      () => client.api,
      documentPoint,
      screenPoint,
    );
    const drawGeometry = () => {
      const s = useWorkspace.getState().snapshot;
      const prefs = useGeometry.getState();
      const d = s?.document;
      const key = JSON.stringify([d, s?.view, prefs]);
      if (key === geometryKey) return;
      geometryKey = key;
      geometrySvg.replaceChildren();
      if (!d || !s) return;
      geometrySvg.setAttribute('viewBox', `0 0 ${s.view.screenWidth} ${s.view.screenHeight}`);
      const x0 = d.originX ?? 0,
        y0 = d.originY ?? 0;
      const line = (a: [number, number], b: [number, number], kind: string) => {
        const node = document.createElementNS(geometrySvg.namespaceURI, 'path');
        node.setAttribute('d', 'M' + screenPoint(a).join(' ') + ' L' + screenPoint(b).join(' '));
        node.setAttribute('class', kind);
        geometrySvg.appendChild(node);
      };
      if (prefs.showGuides)
        for (const g of d.guides ?? []) {
          if (g.axis === 'vertical' && g.position >= x0 && g.position <= x0 + d.width)
            line([g.position, y0], [g.position, y0 + d.height], 'document-guide');
          if (g.axis === 'horizontal' && g.position >= y0 && g.position <= y0 + d.height)
            line([x0, g.position], [x0 + d.width, g.position], 'document-guide');
        }
      if (prefs.showRulers) {
        const desired = 70 / s.view.zoom;
        const power = 10 ** Math.floor(Math.log10(desired));
        const step = [1, 2, 5, 10].map((n) => n * power).find((n) => n >= desired)!;
        const points = [
          [0, 0],
          [s.view.screenWidth, 0],
          [0, s.view.screenHeight],
          [s.view.screenWidth, s.view.screenHeight],
        ].map(([x, y]) => documentPoint(x, y));
        for (const axis of [0, 1] as const) {
          const origin = axis === 0 ? x0 : y0,
            length = axis === 0 ? d.width : d.height;
          const low = Math.max(origin, Math.min(...points.map((p) => p[axis])));
          const high = Math.min(origin + length, Math.max(...points.map((p) => p[axis])));
          for (
            let value = Math.ceil((low - origin) / step) * step;
            value <= high - origin;
            value += step
          ) {
            const a: [number, number] = axis === 0 ? [origin + value, y0] : [x0, origin + value];
            const b: [number, number] =
              axis === 0 ? [a[0], a[1] + 6 / s.view.zoom] : [a[0] + 6 / s.view.zoom, a[1]];
            line(a, b, 'ruler-tick');
            const p = screenPoint(a);
            const label = document.createElementNS(geometrySvg.namespaceURI, 'text');
            label.setAttribute('x', String(p[0] + 8));
            label.setAttribute('y', String(p[1] + 13));
            label.textContent = Number(value.toPrecision(6)).toString();
            geometrySvg.appendChild(label);
          }
        }
      }
    };
    const unsubscribeGeometry = useGeometry.subscribe(drawGeometry);
    const drawGesture = (points: [number, number][]) => {
      path.setAttribute(
        'd',
        points
          .map((p, i) => {
            const q = screenPoint(p);
            return (i ? 'L' : 'M') + q.join(' ');
          })
          .join(' '),
      );
    };
    const clearSelectionGesture = () => {
      selectionDrag = undefined;
      polygon = [];
      gestureRevision = undefined;
      path.setAttribute('d', '');
    };
    const gestureCurrent = () => {
      if (gestureRevision === useWorkspace.getState().snapshot?.revision) return true;
      clearSelectionGesture();
      return false;
    };
    const tooManyPoints = () => {
      clearSelectionGesture();
      useSelection.setState({ notice: 'Selection cancelled: use at most 4096 outline points' });
    };
    function select(action: SelectionAction) {
      void performSelection(client.api, action);
    }
    function finishPolygon() {
      if (!gestureCurrent()) return;
      if (polygon.length >= 3)
        select({
          kind: 'shape',
          shape: { kind: 'polygon', points: polygon },
          mode: polygonMode,
          antialias: useSelection.getState().antialias,
        });
      clearSelectionGesture();
    }
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
      const source = snapshot?.cloneStamp?.source;
      const offset = snapshot?.cloneStamp?.offset;
      cloneMarker.hidden = brush.tool !== 'clone' || !source || !snapshot?.document;
      if (!cloneMarker.hidden && source) {
        const destination = lastScreen ? documentPoint(...lastScreen) : undefined;
        const point: [number, number] =
          (brush.cloneAligned || snapshot?.painting) && offset && destination
            ? [destination[0] + offset[0], destination[1] + offset[1]]
            : source;
        const screen = screenPoint(point.map((v) => Math.floor(v) + 0.5) as [number, number]);
        cloneMarker.style.left = screen[0] + 'px';
        cloneMarker.style.top = screen[1] + 'px';
      }
      outline.hidden =
        !lastScreen ||
        !snapshot?.document ||
        space ||
        rotate ||
        brush.tool === 'eyedropper' ||
        ['move', 'transform', 'crop'].includes(brush.tool) ||
        isSelecting();
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
    const unsubscribeBrush = useBrush.subscribe((state, previous) => {
      if (state.tool !== previous.tool) clearSelectionGesture();
      if (
        previous.tool === 'transform' &&
        state.tool !== 'transform' &&
        useWorkspace.getState().snapshot?.transform
      )
        void cancelTransform(client.api, false);
      refreshOutline();
    });
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
      if (crop.key(event) || transform.key(event)) return;
      if (event.code === 'Space') {
        if (event.target instanceof HTMLButtonElement && !canvas.matches(':hover')) return;
        space = event.type === 'keydown';
        event.preventDefault();
      }
      if (event.code === 'KeyR') rotate = event.type === 'keydown';
      const modifier = event.ctrlKey || event.metaKey;
      if (event.code === 'Escape') clearSelectionGesture();
      if (event.code === 'Enter' && polygon.length) {
        event.preventDefault();
        finishPolygon();
        return;
      }
      if (
        !useSelection.getState().busy &&
        !useWorkspace.getState().snapshot?.painting &&
        useWorkspace.getState().snapshot?.document
      ) {
        if (modifier && ['KeyA', 'KeyD', 'KeyI', 'KeyC', 'KeyX', 'KeyV'].includes(event.code)) {
          event.preventDefault();
          if (event.code === 'KeyA') select({ kind: 'all' });
          else if (event.code === 'KeyD') select({ kind: 'none' });
          else if (event.code === 'KeyI' && event.shiftKey)
            select({ kind: 'modify', operation: 'invert', radius: 0 });
          else if (event.code === 'KeyC' || event.code === 'KeyX')
            void copySelection(
              client.api,
              event.code === 'KeyX',
              event.shiftKey && event.code === 'KeyC',
            );
          else if (event.code === 'KeyV') void pasteSelection(client.api, event.shiftKey);
          return;
        }
        if (event.code === 'Delete' || event.code === 'Backspace') {
          event.preventDefault();
          select({ kind: 'fill', color: useBrush.getState().primary, clear: true });
          return;
        }
        if (!modifier && event.code === 'KeyQ') {
          event.preventDefault();
          void client.api
            .quickMask(!useWorkspace.getState().snapshot?.selection.quickMask)
            .then((snapshot) => updateWorkspace({ snapshot }))
            .catch((e) => useSelection.setState({ notice: String(e) }));
          return;
        }
        if (!modifier && ['KeyM', 'KeyL', 'KeyW', 'KeyG'].includes(event.code)) {
          event.preventDefault();
          clearSelectionGesture();
          useBrush.setState({
            tool:
              event.code === 'KeyM'
                ? event.shiftKey
                  ? 'ellipse'
                  : 'rectangle'
                : event.code === 'KeyL'
                  ? event.shiftKey
                    ? 'polygon'
                    : 'lasso'
                  : event.code === 'KeyW'
                    ? 'wand'
                    : 'bucket',
          });
          refreshOutline();
          return;
        }
      }
      if (modifier && ['KeyZ', 'KeyY'].includes(event.code)) {
        event.preventDefault();
        if (selectionDrag || polygon.length) {
          clearSelectionGesture();
          return;
        }
        if (strokePointer !== undefined) cancelStroke();
        else
          send({
            kind: event.code === 'KeyY' || event.shiftKey ? 'redo' : 'undo',
            x: 0,
            y: 0,
            amount: 0,
          });
      } else if (!modifier) {
        const tool = (
          { KeyB: 'brush', KeyE: 'eraser', KeyI: 'eyedropper', KeyS: 'clone' } as const
        )[event.code as 'KeyB' | 'KeyE' | 'KeyI' | 'KeyS'];
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
      crop.cancelDrag();
      transform.cancelDrag();
      clearSelectionGesture();
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
        if (
          strokePointer !== undefined ||
          selectionDrag ||
          polygon.length ||
          transform.dragging() ||
          crop.dragging()
        )
          return;
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
        !useWorkspace.getState().snapshot?.operation &&
        !useSelection.getState().busy
      ) {
        event.preventDefault();
        if (crop.down(event) || transform.down(event)) return;
        const brush = useBrush.getState();
        const rect = canvas.getBoundingClientRect();
        const x = event.clientX - rect.left,
          y = event.clientY - rect.top;
        if (isSelecting()) {
          const p = documentPoint(x, y),
            settings = useSelection.getState();
          const mode = selectionMode(event.shiftKey, event.altKey);
          if (brush.tool === 'wand' || brush.tool === 'bucket') {
            const doc = useWorkspace.getState().snapshot!.document!;
            const originX = doc.originX ?? 0,
              originY = doc.originY ?? 0;
            if (
              p[0] < originX ||
              p[1] < originY ||
              p[0] >= originX + doc.width ||
              p[1] >= originY + doc.height
            )
              return;
            const options = {
              seed: [Math.floor(p[0]), Math.floor(p[1])] as [number, number],
              tolerance: settings.tolerance,
              contiguous: settings.contiguous,
              antialias: settings.antialias,
              mode,
            };
            select(
              brush.tool === 'wand'
                ? { kind: 'wand', options, allLayers: settings.allLayers }
                : { kind: 'bucket', options, allLayers: settings.allLayers, color: brush.primary },
            );
          } else if (brush.tool === 'polygon') {
            if (polygon.length === 0) {
              polygonMode = mode;
              gestureRevision = useWorkspace.getState().snapshot!.revision;
            } else if (!gestureCurrent()) return;
            if (event.detail >= 2) {
              finishPolygon();
              return;
            }
            if (polygon.length >= 4096) {
              tooManyPoints();
              return;
            }
            polygon.push(p);
            drawGesture(polygon);
          } else if (
            brush.tool === 'rectangle' ||
            brush.tool === 'ellipse' ||
            brush.tool === 'lasso'
          ) {
            selectionDrag = { pointer: event.pointerId, tool: brush.tool, points: [p, p], mode };
            gestureRevision = useWorkspace.getState().snapshot!.revision;
            canvas.setPointerCapture(event.pointerId);
          }
          return;
        }
        if (event.altKey && brush.tool === 'clone') {
          send({ kind: 'clone-source', x, y, amount: 0 });
          return;
        }
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
            ...(brush.tool === 'clone'
              ? { clone: { aligned: brush.cloneAligned, allLayers: brush.cloneAllLayers } }
              : {}),
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
      const rect = canvas.getBoundingClientRect();
      if (strokePointer === event.pointerId) {
        for (const p of pointerSamples(event, rect, lastSampleTime)) {
          lastSample = p;
          lastSampleTime = p[5];
          send({ kind: 'stroke-sample', x: 0, y: 0, amount: 0, sample: lastSample });
        }
      }
      lastScreen = [event.clientX - rect.left, event.clientY - rect.top];
      refreshOutline();
      if ((selectionDrag || polygon.length) && !gestureCurrent()) return;
      if (selectionDrag?.pointer === event.pointerId) {
        const p = documentPoint(event.clientX - rect.left, event.clientY - rect.top);
        if (selectionDrag.tool === 'lasso') {
          const last = selectionDrag.points[selectionDrag.points.length - 1];
          if (
            Math.hypot(p[0] - last[0], p[1] - last[1]) *
              useWorkspace.getState().snapshot!.view.zoom >=
            1
          ) {
            if (selectionDrag.points.length >= 4096) {
              tooManyPoints();
              return;
            }
            selectionDrag.points.push(p);
          }
          drawGesture(selectionDrag.points);
        } else {
          selectionDrag.points[1] = p;
          const a = selectionDrag.points[0];
          if (selectionDrag.tool === 'rectangle')
            drawGesture([a, [p[0], a[1]], p, [a[0], p[1]], a]);
          else {
            const pts: [number, number][] = [];
            for (let i = 0; i <= 64; i++) {
              const angle = (i * Math.PI) / 32;
              pts.push([
                (a[0] + p[0]) / 2 + ((p[0] - a[0]) * Math.cos(angle)) / 2,
                (a[1] + p[1]) / 2 + ((p[1] - a[1]) * Math.sin(angle)) / 2,
              ]);
            }
            drawGesture(pts);
          }
        }
      } else if (polygon.length)
        drawGesture([
          ...polygon,
          documentPoint(event.clientX - rect.left, event.clientY - rect.top),
        ]);
    }
    if (rawInput)
      canvas.addEventListener('pointerrawupdate', (event) => {
        if (event instanceof PointerEvent) strokeSamples(event);
      });
    canvas.addEventListener('pointermove', (event) => {
      const rect = canvas.getBoundingClientRect();
      lastScreen = [event.clientX - rect.left, event.clientY - rect.top];
      if (crop.move(event) || transform.move(event)) {
        input('cursor', ...lastScreen);
        return;
      }
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
      if (crop.up(event) || transform.up(event)) return;
      drag = undefined;
      if (selectionDrag?.pointer === event.pointerId) {
        if (!gestureCurrent()) return;
        const gesture = selectionDrag;
        const rect = canvas.getBoundingClientRect();
        const end = documentPoint(event.clientX - rect.left, event.clientY - rect.top);
        const start = gesture.points[0];
        if (gesture.tool === 'lasso') {
          if (gesture.points.length >= 3)
            select({
              kind: 'shape',
              shape: { kind: 'polygon', points: gesture.points },
              mode: gesture.mode,
              antialias: useSelection.getState().antialias,
            });
        } else
          select({
            kind: 'shape',
            shape: { kind: gesture.tool, x0: start[0], y0: start[1], x1: end[0], y1: end[1] },
            mode: gesture.mode,
            antialias: useSelection.getState().antialias,
          });
        clearSelectionGesture();
        return;
      }
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
    canvas.addEventListener('dblclick', (event) => {
      if (useBrush.getState().tool === 'polygon') {
        event.preventDefault();
        finishPolygon();
      }
    });
    canvas.addEventListener('pointercancel', () => {
      crop.cancelDrag();
      transform.cancelDrag();
      clearSelectionGesture();
      drag = undefined;
      cancelStroke();
    });
    canvas.addEventListener('lostpointercapture', () => {
      if (crop.dragging()) crop.cancelDrag();
      if (transform.dragging()) transform.cancelDrag();
      if (selectionDrag) clearSelectionGesture();
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
              drawGeometry();
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
      unsubscribeGeometry();
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
      svg.remove();
      geometrySvg.remove();
      transform.dispose();
      crop.dispose();
    };
  }, [generation, diagnostic, onFaultReady, onApiReady]);
  return <div className="canvas-host" ref={host} />;
}
