import type { EngineApi } from './engine/types';
import { useWorkspace } from './state';
import { useBrush } from './brush-state';
import { useGeometry } from './geometry-state';
import {
  applyTransform,
  cancelTransform,
  startTransform,
  setTransformQuad,
  transformQuad,
  useTransform,
  nudgeLayer,
  selectLayerAtPoint,
  queueTransform,
} from './transform-state';
import type { Point, Quad } from './transform-state';
const add = (a: Point, b: Point): Point => [a[0] + b[0], a[1] + b[1]];
const sub = (a: Point, b: Point): Point => [a[0] - b[0], a[1] - b[1]];
const times = (p: Point, n: number): Point => [p[0] * n, p[1] * n];
const midpoint = (a: Point, b: Point) => times(add(a, b), 0.5);
const center = (q: Quad) =>
  times(
    q.reduce((a, b) => add(a, b), [0, 0]),
    0.25,
  );
const handles = (q: Quad) => q.flatMap((p, i) => [p, midpoint(p, q[(i + 1) % 4])]);
function basis(q: Quad, p: Point): Point {
  const x = sub(q[1], q[0]),
    y = sub(q[3], q[0]),
    d = sub(p, q[0]),
    det = x[0] * y[1] - x[1] * y[0];
  return [(d[0] * y[1] - d[1] * y[0]) / det, (d[1] * x[0] - d[0] * x[1]) / det];
}
function inside(q: Quad, p: Point) {
  const signs = q.map((a, i) => {
    const b = q[(i + 1) % 4];
    return (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
  });
  return signs.every((v) => v >= 0) || signs.every((v) => v <= 0);
}
function snapped(q: Quad, point: Point, move: boolean): Point {
  const state = useWorkspace.getState().snapshot,
    d = state?.document;
  if (!state || !d || !useGeometry.getState().snapping) return point;
  const xs = [d.originX ?? 0, (d.originX ?? 0) + d.width],
    ys = [d.originY ?? 0, (d.originY ?? 0) + d.height];
  if (useGeometry.getState().showGuides)
    for (const g of d.guides ?? []) (g.axis === 'vertical' ? xs : ys).push(g.position);
  const tolerance = 6 / state.view.zoom;
  const axis = (targets: number[], values: number[], delta: number) => {
    let best = tolerance,
      correction = 0;
    for (const t of targets)
      for (const v of values) {
        const distance = t - v - delta;
        if (Math.abs(distance) < best) {
          best = Math.abs(distance);
          correction = distance;
        }
      }
    return delta + correction;
  };
  if (!move) return [axis(xs, [0], point[0]), axis(ys, [0], point[1])];
  const x = q.map((p) => p[0]),
    y = q.map((p) => p[1]);
  return [
    axis(xs, [Math.min(...x), Math.max(...x), (Math.min(...x) + Math.max(...x)) / 2], point[0]),
    axis(ys, [Math.min(...y), Math.max(...y), (Math.min(...y) + Math.max(...y)) / 2], point[1]),
  ];
}
export function bindTransformInteractions(
  canvas: HTMLCanvasElement,
  container: HTMLElement,
  api: () => EngineApi,
  documentPoint: (x: number, y: number) => Point,
  screenPoint: (p: Point) => Point,
) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.classList.add('transform-overlay');
  svg.dataset.testid = 'transform-overlay';
  container.appendChild(svg);
  let drag:
    | {
        pointer: number;
        start: Point;
        last: Point;
        quad?: Quad;
        handle: number;
        move: boolean;
        released: boolean;
        serial: number;
        sourceRevision?: number;
      }
    | undefined;
  let serial = 0;
  let lastView = '',
    needsViewRefresh = false,
    drawKey = '';
  const point = (e: PointerEvent): Point => {
    const r = canvas.getBoundingClientRect();
    return documentPoint(e.clientX - r.left, e.clientY - r.top);
  };
  const rotationHandle = (q: Quad): Point => {
    const mid = midpoint(q[0], q[1]),
      d = sub(mid, center(q));
    return add(mid, times(d, 28 / Math.max(0.001, Math.hypot(...d))));
  };
  const draw = () => {
    const s = useWorkspace.getState().snapshot;
    const q = transformQuad();
    const key = JSON.stringify([s?.view, s?.transform?.sourceRevision, s?.transform?.target, q]);
    if (key === drawKey) return;
    drawKey = key;
    svg.replaceChildren();
    if (!s?.transform || !q) return;
    svg.setAttribute('viewBox', `0 0 ${s.view.screenWidth} ${s.view.screenHeight}`);
    const mapped = q.map(screenPoint) as Quad;
    const path = document.createElementNS(svg.namespaceURI, 'path');
    path.setAttribute('d', 'M' + mapped.map((p) => p.join(' ')).join(' L') + ' Z');
    svg.appendChild(path);
    const mid = midpoint(mapped[0], mapped[1]),
      rotate = rotationHandle(mapped);
    const stem = document.createElementNS(svg.namespaceURI, 'path');
    stem.setAttribute('d', `M${mid.join(' ')} L${rotate.join(' ')}`);
    svg.appendChild(stem);
    for (const [i, p] of [...handles(mapped), rotate].entries()) {
      const h = document.createElementNS(svg.namespaceURI, 'circle');
      h.setAttribute('cx', String(p[0]));
      h.setAttribute('cy', String(p[1]));
      h.setAttribute('r', i === 8 ? '5' : '4');
      h.setAttribute('data-handle', String(i));
      svg.appendChild(h);
    }
  };
  const update = () => {
    const g = drag;
    if (!g?.quad) return;
    const state = useWorkspace.getState().snapshot;
    if (!state?.transform || g.sourceRevision !== state.transform.sourceRevision) {
      drag = undefined;
      return;
    }
    let q = g.quad.map((p) => [...p]) as Quad;
    const delta = sub(g.last, g.start);
    if (g.handle === -1) {
      const d = snapped(q, delta, true);
      q = q.map((p) => add(p, d)) as Quad;
    } else if (g.handle === 8) {
      const c = center(q),
        a = sub(g.start, c),
        b = sub(g.last, c);
      let angle = Math.atan2(b[1], b[0]) - Math.atan2(a[1], a[0]);
      if (shift) angle = (Math.round(angle / (Math.PI / 12)) * Math.PI) / 12;
      const cs = Math.cos(angle),
        sn = Math.sin(angle);
      q = q.map((p) => {
        const v = sub(p, c);
        return add(c, [v[0] * cs - v[1] * sn, v[0] * sn + v[1] * cs]);
      }) as Quad;
    } else {
      const mode = useTransform.getState().mode;
      if (mode === 'scale') {
        const location = [
          [0, 0],
          [0.5, 0],
          [1, 0],
          [1, 0.5],
          [1, 1],
          [0.5, 1],
          [0, 1],
          [0, 0.5],
        ][g.handle];
        const pivot: Point = [1 - location[0], 1 - location[1]];
        const p = basis(q, snapped(q, g.last, false));
        let sx = location[0] === 0.5 ? 1 : (p[0] - pivot[0]) / (location[0] - pivot[0]);
        let sy = location[1] === 0.5 ? 1 : (p[1] - pivot[1]) / (location[1] - pivot[1]);
        if (shift) {
          const factor = Math.max(Math.abs(sx), Math.abs(sy));
          sx = Math.sign(sx) * factor;
          sy = Math.sign(sy) * factor;
        }
        const ex = sub(q[1], q[0]),
          ey = sub(q[3], q[0]),
          origin = q[0];
        q = q.map((v) => {
          const p = basis(q, v);
          return add(
            origin,
            add(
              times(ex, pivot[0] + (p[0] - pivot[0]) * sx),
              times(ey, pivot[1] + (p[1] - pivot[1]) * sy),
            ),
          );
        }) as Quad;
      } else if (mode === 'skew') {
        const edge = Math.floor(g.handle / 2),
          direction = sub(q[(edge + 1) % 4], q[edge]);
        const n = Math.hypot(...direction);
        const amount = (delta[0] * direction[0] + delta[1] * direction[1]) / (n * n);
        const d = times(direction, amount);
        q[edge] = add(q[edge], d);
        q[(edge + 1) % 4] = add(q[(edge + 1) % 4], d);
      } else if (mode === 'perspective' && g.handle % 2 === 0) {
        const index = g.handle / 2;
        const d = sub(snapped(q, g.last, false), g.start);
        if (Math.abs(d[0]) >= Math.abs(d[1])) {
          const other = index === 0 ? 1 : index === 1 ? 0 : index === 2 ? 3 : 2;
          q[index] = add(q[index], [d[0], 0]);
          q[other] = add(q[other], [-d[0], 0]);
        } else {
          const other = index === 0 ? 3 : index === 1 ? 2 : index === 2 ? 1 : 0;
          q[index] = add(q[index], [0, d[1]]);
          q[other] = add(q[other], [0, -d[1]]);
        }
      } else {
        const index = Math.floor(g.handle / 2);
        if (g.handle % 2 === 0) q[index] = snapped(q, g.last, false);
        else {
          q[index] = add(q[index], delta);
          q[(index + 1) % 4] = add(q[(index + 1) % 4], delta);
        }
      }
    }
    void setTransformQuad(api(), q);
    draw();
  };
  let shift = false;
  const begin = async (g: NonNullable<typeof drag>) => {
    try {
      if (g.move && useTransform.getState().autoSelect) {
        const id = await selectLayerAtPoint(api(), g.start);
        if (id === null) {
          drag = undefined;
          return;
        }
      }
      await startTransform(api(), useTransform.getState().selected);
      if (drag?.serial !== g.serial) return;
      const session = useWorkspace.getState().snapshot?.transform;
      if (!session) {
        drag = undefined;
        return;
      }
      g.quad = session.quad as Quad;
      g.sourceRevision = session.sourceRevision;
      update();
      if (g.released) {
        drag = undefined;
        if (g.move) await applyTransform(api());
      }
    } catch (error) {
      drag = undefined;
      useTransform.setState({ notice: String(error) });
    }
  };
  const down = (e: PointerEvent) => {
    const tool = useBrush.getState().tool;
    if (!['move', 'transform'].includes(tool)) return false;
    if (useTransform.getState().busy || drag?.released) return true;
    const p = point(e),
      q = transformQuad(),
      s = useWorkspace.getState().snapshot;
    shift = e.shiftKey;
    if (tool === 'transform' && s?.transform && q) {
      const r = canvas.getBoundingClientRect(),
        pointer: Point = [e.clientX - r.left, e.clientY - r.top];
      const mapped = q.map(screenPoint) as Quad;
      let handle = -1,
        best = 10;
      for (const [i, p] of [...handles(mapped), rotationHandle(mapped)].entries()) {
        const d = Math.hypot(...sub(p, pointer));
        if (d < best) {
          best = d;
          handle = i;
        }
      }
      if (handle === -1 && !inside(q, p)) return true;
      drag = {
        pointer: e.pointerId,
        start: p,
        last: p,
        quad: q.map((p) => [...p]) as Quad,
        handle,
        move: false,
        released: false,
        serial: ++serial,
        sourceRevision: s.transform.sourceRevision,
      };
    } else {
      drag = {
        pointer: e.pointerId,
        start: p,
        last: p,
        handle: -1,
        move: tool === 'move',
        released: false,
        serial: ++serial,
      };
      void begin(drag);
    }
    canvas.setPointerCapture(e.pointerId);
    return true;
  };
  const move = (e: PointerEvent) => {
    if (drag?.pointer !== e.pointerId || drag.released) return false;
    drag.last = point(e);
    shift = e.shiftKey;
    update();
    return true;
  };
  const up = (e: PointerEvent) => {
    if (drag?.pointer !== e.pointerId) return false;
    const g = drag;
    g.last = point(e);
    g.released = true;
    shift = e.shiftKey;
    if (g.quad) {
      update();
      drag = undefined;
      if (g.move) void applyTransform(api());
    }
    return true;
  };
  const cancelDrag = () => {
    if (drag) {
      drag = undefined;
      ++serial;
      void cancelTransform(api());
    }
  };
  const key = (e: KeyboardEvent) => {
    const s = useWorkspace.getState().snapshot;
    if (e.type !== 'keydown' || !s?.document) return false;
    if ((e.ctrlKey || e.metaKey) && e.code === 'KeyT') {
      e.preventDefault();
      void startTransform(api());
      return true;
    }
    if (e.code === 'KeyV' && !e.ctrlKey && !e.metaKey) {
      useBrush.setState({ tool: 'move' });
      return true;
    }
    if (s.transform && ['Enter', 'Escape'].includes(e.code)) {
      e.preventDefault();
      drag = undefined;
      if (e.code === 'Enter') void applyTransform(api());
      else void cancelTransform(api());
      return true;
    }
    if (
      (s.transform || useBrush.getState().tool === 'move') &&
      ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.code)
    ) {
      e.preventDefault();
      const amount = e.shiftKey ? 10 : 1;
      const dx = (e.code === 'ArrowRight' ? 1 : e.code === 'ArrowLeft' ? -1 : 0) * amount,
        dy = (e.code === 'ArrowDown' ? 1 : e.code === 'ArrowUp' ? -1 : 0) * amount;
      const q = transformQuad();
      if (s.transform && q) void setTransformQuad(api(), q.map((p) => add(p, [dx, dy])) as Quad);
      else void nudgeLayer(api(), dx, dy);
      return true;
    }
    return false;
  };
  const refreshView = () => {
    const s = useWorkspace.getState().snapshot,
      prefs = useTransform.getState();
    if (!needsViewRefresh || !s?.transform || prefs.pending || prefs.busy) return;
    needsViewRefresh = false;
    const q = prefs.quad;
    if (q) void queueTransform(api(), { kind: 'quad', points: q });
    else void queueTransform(api(), { kind: 'matrix', matrix: s.transform.matrix });
  };
  const unsubWorkspace = useWorkspace.subscribe(() => {
      draw();
      const s = useWorkspace.getState().snapshot;
      const key = JSON.stringify(s?.view);
      if (key !== lastView) {
        if (lastView && s?.transform) needsViewRefresh = true;
        lastView = key;
      }
      refreshView();
    }),
    unsubTransform = useTransform.subscribe(() => {
      draw();
      refreshView();
    });
  draw();
  return {
    down,
    move,
    up,
    key,
    cancelDrag,
    dragging: () => !!drag && !drag.released,
    draw,
    dispose() {
      unsubWorkspace();
      unsubTransform();
      svg.remove();
    },
  };
}
