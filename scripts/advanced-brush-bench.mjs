// Share the serial hardware/input/admission/undo observer with the retouch bench.
process.env.ADVANCED_BRUSH_BENCH = '1';
await import('./retouch-gradient-bench.mjs');
