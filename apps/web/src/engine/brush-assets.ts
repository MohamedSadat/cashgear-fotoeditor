import type { Interaction } from './types';
// Count opaque coverage strings by capacity; never inspect or process their pixels.
// Covers structured-clone UTF-16 payloads and bounded per-brush settings overhead.
export const queuedBrushLimit = 16 * 1024 * 1024;
export function brushInputBytes(input: Interaction) {
  return input.brush
    ? 8192 +
        2 *
          ((input.brush.tipImage?.coverage.length ?? 0) +
            (input.brush.texture?.coverage.length ?? 0))
    : 0;
}
