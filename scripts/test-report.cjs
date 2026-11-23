const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

const template = fs.readFileSync(path.join(__dirname, '../src/report-template.html'), 'utf8');
const start = template.indexOf('  function billingKind(');
const end = template.indexOf('  const PALETTE', start);
assert.ok(start >= 0 && end > start);
const aggregate = vm.runInNewContext(`${template.slice(start, end)}; aggregateReportTurns`);
const base = { hostname: 'host', source: 'copilot.cli.events', session_id: 'session', turn_index: 1, session_total: 2 };

test('several model calls produce one user turn with unchanged credits', () => {
  const input = [
    { ...base, model: 'model-a', credits: 10, timestamp_ms: 3000, duration_ms: 2000 },
    { ...base, model: 'model-b', credits: 20, timestamp_ms: 4000, duration_ms: 2000 },
    { ...base, turn_index: 2, model: 'model-a', credits: 5 }
  ];
  const original = JSON.stringify(input);
  const rows = aggregate(input, 'host');
  assert.equal(rows.length, 2);
  assert.equal(rows[0].credits, 30);
  assert.equal(rows[0].duration_ms, 3000);
  assert.equal(rows[0].model, 'model-a + model-b');
  assert.equal(rows[0].turn_index, 1);
  assert.equal(rows[1].credits, 5);
  assert.equal(JSON.stringify(input), original);
});

test('different sessions, hosts and sources remain separate', () => {
  const rows = aggregate([
    { ...base, credits: 1 },
    { ...base, hostname: 'other-host', credits: 2 },
    { ...base, session_id: 'other-session', credits: 3 },
    { ...base, source: 'vscode.chatSessions', credits: 4 }
  ]);
  assert.equal(rows.length, 4);
  assert.equal(rows.reduce((sum, r) => sum + r.credits, 0), 10);
});

test('legacy summaries keep their unknown turn rather than inventing one', () => {
  const rows = aggregate([{ ...base, turn_index: null, credits: 7 }, { ...base, turn_index: null, credits: 8 }]);
  assert.equal(rows.length, 2);
  assert.ok(rows.every(r => r.turn_index === null));
});

test('VS Code turn total is not multiplied by subagent data', () => {
  const rows = aggregate([{
    ...base, source: 'vscode.chatSessions', credits: 39950.53665,
    response: [{ toolSpecificData: { kind: 'subagent', credits: 1234 } }]
  }]);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].credits, 39950.53665);
});

test('effort uses Beijing half-hour slots, deduplicates projects, and splits days', () => {
  const sheets = fs.readFileSync(path.join(__dirname, '../src/report-sheets.js'), 'utf8');
  const script = fs.readFileSync(path.join(__dirname, '../src/report-effort.js'), 'utf8');
  new vm.Script(script);
  const summarize = vm.runInNewContext(`${sheets.split('Object.assign(I18N.en,')[0]}${script.split('Object.assign(I18N.en,')[0]}; effortAggregate`);
  const events = [
    ['A', '2026-11-23T08:05:00+08:00'], ['B', '2026-11-23T08:29:59.999+08:00'],
    ['A', '2026-11-23T08:30:00+08:00'], ['B', '2026-11-23T23:59:59.999+08:00'],
    ['B', '2026-11-24T00:00:00+08:00'],
  ].map(([project, time]) => ({ project, timestamp_ms: Date.parse(time) }));
  const all = summarize(events);
  assert.equal(all.slots, 4);
  assert.equal(all.segments, 3);
  assert.equal(all.days.get('2026-11-23').slots.get(16).length, 2);
  assert.equal(JSON.stringify(all.days.get('2026-11-23').ranges), '[[16,18],[47,48]]');
  assert.equal(summarize(events.filter(e => e.project === 'B')).slots, 3);
});

test('shared calendar uses Monday weeks, leap days and Beijing midnight', () => {
  const sheets = fs.readFileSync(path.join(__dirname, '../src/report-sheets.js'), 'utf8');
  new vm.Script(sheets);
  const { calendarPeriod, reportDay, reportBrush } = vm.runInNewContext(`${sheets.split('Object.assign(I18N.en,')[0]}; ({calendarPeriod, reportDay, reportBrush})`);
  const bounds = (day, scale, step) => [...calendarPeriod(day, scale, step)];
  assert.deepEqual(bounds('2024-02-29', 'year'), ['2024-01-01', '2024-12-31']);
  assert.deepEqual(bounds('2024-03-31', 'month', -1), ['2024-02-01', '2024-02-29']);
  assert.deepEqual(bounds('2027-01-01', 'week'), ['2026-12-28', '2027-01-03']);
  assert.deepEqual(bounds('2026-12-31', 'day', 1), ['2027-01-01', '2027-01-01']);
  assert.equal(reportDay(Date.parse('2026-11-23T15:59:59.999Z')), '2026-11-23');
  assert.equal(reportDay(Date.parse('2026-11-23T16:00:00Z')), '2026-11-24');
  assert.equal(reportDay(null), null);
  assert.deepEqual([...reportBrush('move', 4, 10, 7, 18, 0, 12)], [6, 12]);
  assert.deepEqual([...reportBrush('start', 4, 10, 4, 15, 0, 20)], [10, 10]);
  assert.deepEqual([...reportBrush('new', 0, 20, 14, 9, 0, 20)], [9, 14]);
});

test('monthly effort bars use a fixed 24h denominator and four thresholds', () => {
  const script = fs.readFileSync(path.join(__dirname, '../src/report-effort.js'), 'utf8');
  const bar = vm.runInNewContext(`${script.split('Object.assign(I18N.en,')[0]}; effortMonthBar`);
  assert.deepEqual([7, 8, 15, 16, 23, 24].map(n => bar(n).band), ['green', 'yellow', 'yellow', 'orange', 'orange', 'red']);
  for (const [n, width] of [[0,0], [12,25], [24,50], [36,75], [48,100]]) assert.equal(bar(n).width, width);
});

test('weekly effort bands change at 30 and 50 hours', () => {
  const script = fs.readFileSync(path.join(__dirname, '../src/report-effort.js'), 'utf8');
  const band = vm.runInNewContext(`${script.split('Object.assign(I18N.en,')[0]}; effortWeekBand`);
  assert.deepEqual([29.5, 30, 49.5, 50].map(band), ['green', 'yellow', 'yellow', 'red']);
});

test('a shared turn cannot coalesce amounts from different billing units', () => {
  const rows = aggregate([{ ...base, credits: 8 }, { ...base, request_count: 3 },
    { ...base, request_count: 1 }, { ...base, unclassified_usage: 4 },
    { ...base, unclassified_usage: 2 }]);
  assert.equal(rows.length, 4);
  assert.equal(rows.find(r => r.credits > 0).credits, 8);
  assert.equal(rows.find(r => r.request_count != null).request_count, 4);
  assert.deepEqual(Array.from(rows.filter(r => r.unclassified_usage != null), r => r.unclassified_usage), [4,2]);
});