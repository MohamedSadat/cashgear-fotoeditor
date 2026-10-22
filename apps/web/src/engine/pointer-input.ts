import type { Interaction } from './types';
/** Collect input samples, deduplicating the raw/move streams by source time. */
export function pointerSamples(
  event: PointerEvent,
  rect: Pick<DOMRect, 'left' | 'top'>,
  after: number,
): NonNullable<Interaction['sample']>[] {
  const events = event.getCoalescedEvents?.() ?? [];
  return (events.length ? events : [event])
    .filter((p) => p.timeStamp > after)
    .map((p) => [
      p.clientX - rect.left,
      p.clientY - rect.top,
      event.pointerType === 'pen' ? p.pressure : 1,
      p.tiltX,
      p.tiltY,
      p.timeStamp,
    ]);
}
