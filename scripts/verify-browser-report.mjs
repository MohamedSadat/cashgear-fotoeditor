import { readFile, writeFile } from 'node:fs/promises';

const path = process.argv[2];
if (!path) throw Error('Usage: node scripts/verify-browser-report.mjs input.json [summary.json]');
const report = JSON.parse(await readFile(path, 'utf8'));
const cases = [];
function visit(suite, parents = []) {
  const trail = [...parents, suite.title];
  for (const spec of suite.specs ?? [])
    for (const test of spec.tests ?? [])
      cases.push({
        title: [...trail, spec.title].filter(Boolean).join(' / '),
        project: test.projectName,
        expectedStatus: test.expectedStatus,
        status: test.status,
        results: test.results,
      });
  for (const child of suite.suites ?? []) visit(child, trail);
}
for (const suite of report.suites ?? []) visit(suite);
const invalid = cases.filter(
  (c) =>
    c.expectedStatus !== 'passed' ||
    c.status !== 'expected' ||
    c.results.length !== 1 ||
    c.results[0].status !== 'passed' ||
    c.results[0].retry !== 0 ||
    c.results[0].errors?.length,
);
const summary = {
  path,
  count: cases.length,
  projects: [...new Set(cases.map((c) => c.project))],
  stats: report.stats,
  globalErrors: report.errors?.length ?? 0,
  singleAttempt: cases.every((c) => c.results.length === 1 && c.results[0].retry === 0),
  invalid,
};
if (process.argv[3]) await writeFile(process.argv[3], JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary, null, 2));
if (
  !cases.length ||
  invalid.length ||
  summary.globalErrors ||
  report.stats.expected !== cases.length ||
  report.stats.skipped ||
  report.stats.unexpected ||
  report.stats.flaky
)
  throw Error('Browser evidence is incomplete, failed, skipped, flaky or retried');
