import { expect, test } from 'vitest';
import { pointerSamples } from './pointer-input';
test('coalesced pen samples retain pressure/tilt and raw/move duplicates are excluded', () => {
  const a = { clientX: 25, clientY: 37, pressure: 0.25, tiltX: 30, tiltY: -10, timeStamp: 10 };
  const b = { ...a, clientX: 29, pressure: 0.75, timeStamp: 11 };
  const event = {
    ...b,
    pointerType: 'pen',
    getCoalescedEvents: () => [a, b],
  } as unknown as PointerEvent;
  expect(pointerSamples(event, { left: 5, top: 7 }, -1)).toEqual([
    [20, 30, 0.25, 30, -10, 10],
    [24, 30, 0.75, 30, -10, 11],
  ]);
  expect(pointerSamples(event, { left: 5, top: 7 }, 11)).toEqual([]);
});
test('mouse and unavailable coalesced events fall back to a full-pressure sample', () => {
  const event = {
    clientX: 25,
    clientY: 37,
    pressure: 0.5,
    tiltX: 0,
    tiltY: 0,
    timeStamp: 10,
    pointerType: 'mouse',
  } as PointerEvent;
  expect(pointerSamples(event, { left: 5, top: 7 }, 0)).toEqual([[20, 30, 1, 0, 0, 10]]);
});
