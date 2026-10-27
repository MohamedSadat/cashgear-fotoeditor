// Reuse the native-input/adapter-verified brush harness, including changed-pixel
// acknowledgement counts so unchanged previews cannot masquerade as throughput.
process.env.CLONE_BENCH = '1';
process.env.PERF_PHASE ??=
  process.env.CLONE_ALL === '1' ? 'p9-m1-clone-all' : 'p9-m1-clone-current';
await import('./brush-bench.mjs');
