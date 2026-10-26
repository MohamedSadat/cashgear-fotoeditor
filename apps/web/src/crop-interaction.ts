import type { CanvasRect, EngineApi } from './engine/types';
import type { Point } from './transform-state';
import { useWorkspace } from './state';
import { useBrush } from './brush-state';
import { useGeometry } from './geometry-state';
import { useCrop, startCrop, cancelCrop, applyCrop } from './crop-state';
const corners = (r: CanvasRect): Point[] => [
  [r.x, r.y],
  [r.x + r.width, r.y],
  [r.x + r.width, r.y + r.height],
  [r.x, r.y + r.height],
];
const handles = (r: CanvasRect) => {
  const c = corners(r);
  return c.flatMap((p, i) => [
    p,
    [(p[0] + c[(i + 1) % 4][0]) / 2, (p[1] + c[(i + 1) % 4][1]) / 2] as Point,
  ]);
};
export function bindCropInteractions(
  canvas: HTMLCanvasElement,
  container: HTMLElement,
  api: () => EngineApi,
  documentPoint: (x: number, y: number) => Point,
  screenPoint: (p: Point) => Point,
) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.classList.add('crop-overlay');
  svg.dataset.testid = 'crop-overlay';
  container.appendChild(svg);
  let line: [Point, Point] | undefined;
  let drag:
    | {
        pointer: number;
        start: Point;
        initial: CanvasRect;
        degrees: number;
        handle: number;
        line: boolean;
        move: boolean;
      }
    | undefined;
  const point = (e: PointerEvent): Point => {
    const r = canvas.getBoundingClientRect();
    return documentPoint(e.clientX - r.left, e.clientY - r.top);
  };
  const draw = () => {
    svg.replaceChildren();
    const p = useCrop.getState(),
      s = useWorkspace.getState().snapshot,
      r = p.rect;
    if (!r || !s || useBrush.getState().tool !== 'crop') return;
    svg.setAttribute('viewBox', `0 0 ${s.view.screenWidth} ${s.view.screenHeight}`);
    const path = (d: string, cls: string) => {
      const e = document.createElementNS(svg.namespaceURI, 'path');
      e.setAttribute('d', d);
      e.setAttribute('class', cls);
      svg.appendChild(e);
    };
    const poly = corners(r).map(screenPoint),
      outline = 'M' + poly.map((p) => p.join(' ')).join(' L') + ' Z';
    path(`M0 0 H${s.view.screenWidth} V${s.view.screenHeight} H0 Z ` + outline, 'crop-shade');
    path(outline, 'crop-border');
    for (const n of [1 / 3, 2 / 3]) {
      const a = screenPoint([r.x + r.width * n, r.y]),
        b = screenPoint([r.x + r.width * n, r.y + r.height]),
        c = screenPoint([r.x, r.y + r.height * n]),
        d = screenPoint([r.x + r.width, r.y + r.height * n]);
      path(`M${a.join(' ')} L${b.join(' ')} M${c.join(' ')} L${d.join(' ')}`, 'crop-thirds');
    }
    for (const [i, p] of handles(r).map(screenPoint).entries()) {
      const e = document.createElementNS(svg.namespaceURI, 'circle');
      e.setAttribute('cx', String(p[0]));
      e.setAttribute('cy', String(p[1]));
      e.setAttribute('r', '4');
      e.setAttribute('data-handle', String(i));
      svg.appendChild(e);
    }
    if (line)
      path(
        'M' +
          line
            .map(screenPoint)
            .map((p) => p.join(' '))
            .join(' L'),
        'crop-straighten',
      );
  };
  const snapOffsets = (p: Point): [number | null, number | null] => {
    const s = useWorkspace.getState().snapshot,
      d = s?.document;
    if (!s || !d || !useGeometry.getState().snapping) return [null, null];
    const xs = [d.originX ?? 0, (d.originX ?? 0) + d.width],
      ys = [d.originY ?? 0, (d.originY ?? 0) + d.height];
    if (useGeometry.getState().showGuides)
      for (const g of d.guides ?? []) (g.axis === 'vertical' ? xs : ys).push(g.position);
    return [xs, ys].map((a, i) => {
      let best = 6 / s.view.zoom,
        offset: number | null = null;
      for (const n of a)
        if (Math.abs(n - p[i]) < best) {
          best = Math.abs(n - p[i]);
          offset = n - p[i];
        }
      return offset;
    }) as [number | null, number | null];
  };
  const snap = (p: Point): Point => {
    const d = snapOffsets(p);
    return [p[0] + (d[0] ?? 0), p[1] + (d[1] ?? 0)];
  };
  const update = (e: PointerEvent) => {
    const g = drag,
      s = useWorkspace.getState().snapshot,
      d = s?.document;
    if (!g || !d || !s) return;
    const raw = point(e),
      p = snap(raw),
      ox = d.originX ?? 0,
      oy = d.originY ?? 0;
    if (g.line) {
      line = [g.start, raw];
      if (Math.hypot(raw[0] - g.start[0], raw[1] - g.start[1]) * s.view.zoom > 3) {
        const a = (Math.atan2(raw[1] - g.start[1], raw[0] - g.start[0]) * 180) / Math.PI;
        const angle = ((((a + 45) % 90) + 90) % 90) - 45;
        useCrop.setState({ degrees: -angle });
      }
      draw();
      return;
    }
    if (g.move) {
      let dx = raw[0] - g.start[0],
        dy = raw[1] - g.start[1];
      const r = g.initial;
      const points: Point[] = [
        [r.x + dx, r.y + dy],
        [r.x + r.width + dx, r.y + r.height + dy],
        [r.x + r.width / 2 + dx, r.y + r.height / 2 + dy],
      ];
      const correction = (axis: number) =>
        points
          .map((p) => snapOffsets(p)[axis])
          .filter((v): v is number => v !== null)
          .sort((a, b) => Math.abs(a) - Math.abs(b))[0] ?? 0;
      dx += correction(0);
      dy += correction(1);
      useCrop.setState({
        rect: {
          ...r,
          x: Math.round(Math.max(ox, Math.min(ox + d.width - r.width, r.x + dx))),
          y: Math.round(Math.max(oy, Math.min(oy + d.height - r.height, r.y + dy))),
        },
      });
      return;
    }
    let a = [...g.start] as Point;
    const b = [
      Math.max(ox, Math.min(ox + d.width, p[0])),
      Math.max(oy, Math.min(oy + d.height, p[1])),
    ] as Point;
    if (g.handle >= 0) {
      const c = corners(g.initial),
        i = Math.floor(g.handle / 2);
      if (g.handle % 2 === 0) a = c[(i + 2) % 4];
      else {
        a = c[(i + 2) % 4];
        if (i % 2 === 0) b[0] = c[(i + 3) % 4][0];
        else b[1] = c[(i + 3) % 4][1];
      }
    }
    const pref = useCrop.getState().ratio;
    const ratio =
      pref === 'original'
        ? d.width / d.height
        : pref === 'free'
          ? e.shiftKey
            ? 1
            : 0
          : Number(pref);
    let w = Math.abs(b[0] - a[0]),
      h = Math.abs(b[1] - a[1]);
    const sx = b[0] < a[0] ? -1 : 1,
      sy = b[1] < a[1] ? -1 : 1;
    if (ratio > 0) {
      if (w / h > ratio) h = w / ratio;
      else w = h * ratio;
      const factor = Math.min(
        1,
        (sx > 0 ? ox + d.width - a[0] : a[0] - ox) / Math.max(1, w),
        (sy > 0 ? oy + d.height - a[1] : a[1] - oy) / Math.max(1, h),
      );
      w *= factor;
      h *= factor;
    }
    const x = Math.round(Math.min(a[0], a[0] + sx * w)),
      y = Math.round(Math.min(a[1], a[1] + sy * h));
    const right = Math.round(Math.max(a[0], a[0] + sx * w)),
      bottom = Math.round(Math.max(a[1], a[1] + sy * h));
    if (right > x && bottom > y)
      useCrop.setState({ rect: { x, y, width: right - x, height: bottom - y } });
  };
  const down = (e: PointerEvent) => {
    if (useBrush.getState().tool !== 'crop') return false;
    if (useCrop.getState().busy) return true;
    if (!useCrop.getState().rect) startCrop();
    const r = useCrop.getState().rect;
    if (!r) return true;
    const p = point(e),
      s = useWorkspace.getState().snapshot!,
      d = s.document!,
      ox = d.originX ?? 0,
      oy = d.originY ?? 0;
    const clamped: Point = [
      Math.max(ox, Math.min(ox + d.width, p[0])),
      Math.max(oy, Math.min(oy + d.height, p[1])),
    ];
    let h = -1,
      best = 10;
    const cr = canvas.getBoundingClientRect();
    if (!useCrop.getState().straighten)
      for (const [i, p] of handles(r).map(screenPoint).entries()) {
        const distance = Math.hypot(p[0] - e.clientX + cr.left, p[1] - e.clientY + cr.top);
        if (distance < best) {
          best = distance;
          h = i;
        }
      }
    drag = {
      pointer: e.pointerId,
      start: clamped,
      initial: r,
      degrees: useCrop.getState().degrees,
      handle: h,
      line: useCrop.getState().straighten,
      move: e.altKey && h < 0,
    };
    canvas.setPointerCapture(e.pointerId);
    return true;
  };
  const cancelDrag = () => {
    if (drag) {
      useCrop.setState({ rect: drag.initial, degrees: drag.degrees });
      drag = undefined;
      line = undefined;
      draw();
    }
  };
  const unsubCrop = useCrop.subscribe(draw),
    unsubBrush = useBrush.subscribe((p, prev) => {
      if (prev.tool === 'crop' && p.tool !== 'crop') {
        drag = undefined;
        line = undefined;
        cancelCrop();
      }
      draw();
    }),
    unsubWorkspace = useWorkspace.subscribe(() => {
      const s = useWorkspace.getState().snapshot,
        p = useCrop.getState();
      if (p.rect && s?.contentRevision !== p.sourceRevision) {
        drag = undefined;
        line = undefined;
        cancelCrop();
      }
      draw();
    });
  return {
    down,
    move(e: PointerEvent) {
      if (drag?.pointer !== e.pointerId) return false;
      update(e);
      return true;
    },
    up(e: PointerEvent) {
      if (drag?.pointer !== e.pointerId) return false;
      update(e);
      if (drag.line) useCrop.setState({ straighten: false });
      drag = undefined;
      draw();
      return true;
    },
    cancelDrag,
    dragging: () => !!drag,
    key(e: KeyboardEvent) {
      if (e.code === 'KeyC' && !e.ctrlKey && !e.metaKey && !e.altKey) {
        startCrop();
        return true;
      }
      if (useBrush.getState().tool !== 'crop') return false;
      if (e.code === 'Escape') {
        e.preventDefault();
        drag = undefined;
        line = undefined;
        cancelCrop();
        return true;
      }
      if (e.code === 'Enter' && useCrop.getState().rect) {
        e.preventDefault();
        drag = undefined;
        line = undefined;
        void applyCrop(api());
        return true;
      }
      return false;
    },
    dispose() {
      unsubCrop();
      unsubBrush();
      unsubWorkspace();
      svg.remove();
      cancelCrop();
    },
  };
}
