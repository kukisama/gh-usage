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

function renderUsageChart(from, to, records, lang) {
  const sheets = fs.readFileSync(path.join(__dirname, '../src/report-sheets.js'), 'utf8');
  const chart = { querySelectorAll: () => [] };
  const nodes = { 'daily-chart': chart, 'daily-legend': {}, 'daily-chart-title': {} };
  vm.runInNewContext([
    sheets.split('Object.assign(I18N.en,')[0],
    sheets.slice(sheets.indexOf('const reportLocale'), sheets.indexOf('function historyRows')),
    template.slice(template.indexOf('  function parseDay('), template.indexOf('  function projectLabel(')),
    template.slice(template.indexOf('  function renderDailyChart('), template.indexOf('  function renderModelChart(')),
    'renderDailyChart(records);'
  ].join('\n'), {
    document: { getElementById: id => nodes[id] },
    reportRange: () => [from, to], CURRENT_LANG: lang, records,
    syncUsageChartOverflow: () => {},
    HOSTS: ['desk', 'laptop'], SELECTED_HOSTS: new Set(['desk', 'laptop']),
    HOST_COLOR: { desk: 'blue', laptop: 'green' }, EFFORT_HOST: 'desk', SELECTED_DAY: '',
    t: key => key, usageUnit: () => 'credits', showUsageAmount: r => r.credits,
    fmtNum: (n, digits) => n.toFixed(digits)
  });
  return {
    labels: Array.from(chart.innerHTML.matchAll(/text-anchor="middle"[^>]*>([^<]+)</g), match => match[1]),
    columns: Object.fromEntries(Array.from(chart.innerHTML.matchAll(/data-day="([^"]+)"[^>]*aria-label="([^"]+)"/g),
      match => [match[1], match[2].split(' · ')[1]])),
    svg: chart.innerHTML
  };
}

test('usage shows all seven month labels and groups both machines by Beijing calendar month', () => {
  const rows = [
    { hostname: 'desk', timestamp_ms: Date.parse('2026-07-31T15:59:59.999Z'), credits: 7 },
    { hostname: 'laptop', timestamp_ms: Date.parse('2026-07-31T16:00:00Z'), credits: 11 },
    { hostname: 'desk', timestamp_ms: Date.parse('2026-08-01T00:00:00Z'), credits: 13 }
  ];
  const chart = renderUsageChart('2026-05-17', '2026-11-23', rows, 'zh');
  assert.deepEqual(chart.labels, ['26年5月', '26年6月', '26年7月', '26年8月', '26年9月', '26年10月', '26年11月']);
  assert.equal(chart.columns['2026-07-01'], '7.00 credits');
  assert.equal(chart.columns['2026-08-01'], '24.00 credits');
  assert.equal(chart.columns['2026-09-01'], '0.00 credits');
});

test('multi-year usage keeps empty month labels and expands instead of dropping them', () => {
  const chart = renderUsageChart('2025-12-31', '2027-01-01', [], 'en');
  assert.equal(chart.labels.length, 14);
  assert.equal(chart.labels[0], 'Dec 25');
  assert.equal(chart.labels[1], 'Jan 26');
  assert.equal(chart.labels[13], 'Jan 27');
  assert.equal(Object.keys(chart.columns).length, 14);
  assert.ok(Number(chart.svg.match(/min-width:(\d+)px/)[1]) > 720);
});

test('dragging an overflowing chart pans it without selecting a month; a click still works', () => {
  const sheets = fs.readFileSync(path.join(__dirname, '../src/report-sheets.js'), 'utf8');
  const handlers = new Map();
  const chart = {
    clientWidth: 600, scrollWidth: 600, scrollLeft: 0,
    classList: { add() {}, remove() {} },
    addEventListener: (type, handler) => handlers.set(type, handler),
    setPointerCapture() {}
  };
  const bind = vm.runInNewContext(
    `${sheets.slice(sheets.indexOf('function bindUsageChartPan('), sheets.indexOf('function bindReportSheets('))}; bindUsageChartPan`,
    { ResizeObserver: class { observe() {} } }
  );
  bind(chart);
  const pointer = { isPrimary: true, pointerId: 1, button: 0, pointerType: 'mouse', clientY: 30, target: { closest: () => true } };
  const fire = (type, clientX) => handlers.get(type)({ ...pointer, clientX });
  const clickWasBlocked = () => {
    let blocked = false;
    handlers.get('click')({ detail: 1, preventDefault: () => { blocked = true; }, stopPropagation() {} });
    return blocked;
  };
  fire('pointerdown', 300);
  fire('pointermove', 200);
  fire('pointerup', 200);
  assert.equal(chart.scrollLeft, 0);
  assert.equal(clickWasBlocked(), false);
  chart.scrollWidth = 900;
  fire('pointerdown', 300);
  fire('pointermove', 200);
  fire('pointerup', 200);
  assert.equal(chart.scrollLeft, 100);
  assert.equal(clickWasBlocked(), true);
  fire('pointerdown', 200);
  fire('pointermove', 198);
  fire('pointerup', 198);
  assert.equal(chart.scrollLeft, 100);
  assert.equal(clickWasBlocked(), false);
});

test('Rhythm buckets preserve machine boundaries, Beijing dates and the selected window', () => {
  const sheets = fs.readFileSync(path.join(__dirname, '../src/report-sheets.js'), 'utf8');
  const theme = fs.readFileSync(path.join(__dirname, '../src/report-themes/rhythm.js'), 'utf8');
  const api = vm.runInNewContext([
    sheets.split('Object.assign(I18N.en,')[0],
    template.slice(template.indexOf('  function parseDay('), template.indexOf('  function fmtDateTime(')),
    theme,
    '({ rhythmData, rhythmFocus, dayNumber, offsetDay, offsetMonth, day: dayKey, host: r => r.hostname, amount: r => r.credits })',
  ].join('\n'));
  const records = [
    ['Desk', '2026-09-30T15:59:00Z', 4],
    ['Desk', '2026-09-30T16:00:00Z', 3],
    ['Laptop', '2026-09-30T16:00:00Z', 5],
    ['Laptop', '2026-10-01T16:00:00Z', 9],
  ].map(([hostname, ts, credits]) => ({ hostname, timestamp_ms: Date.parse(ts), credits }));
  const daily = api.rhythmData(['2026-09-30', '2026-10-01'], ['Desk', 'Laptop'], records, api);
  assert.equal(daily.peak, 5);
  assert.equal(daily.totals.get('2026-09-30'), 4);
  assert.equal(daily.totals.get('2026-10-01'), 8);
  assert.equal(daily.lanes.get('Desk').stops.get('2026-10-01').amount, 3);
  assert.equal(daily.lanes.get('Laptop').stops.get('2026-10-01').rows.length, 1);
  assert.deepEqual(Array.from(daily.periods.get('2026-10-01'), row => row.hostname), ['Desk', 'Laptop']);
  const monthly = api.rhythmData(['2026-08-17', '2026-10-01'], ['Laptop'], records, api);
  assert.equal(monthly.monthly, true);
  assert.deepEqual(Array.from(monthly.totals, entry => [...entry]), [['2026-08-01', 0], ['2026-09-01', 0], ['2026-10-01', 5]]);
  assert.deepEqual(Array.from(monthly.periods.get('2026-10-01'), row => row.credits), [5]);
  // The range ends on an empty day: select the latest populated bucket.
  const withGap = api.rhythmData(['2026-09-29', '2026-10-03'], ['Desk', 'Laptop'], records, api);
  assert.deepEqual({ ...api.rhythmFocus(withGap, null) }, { date: '2026-10-02', host: null });
  const deskOnly = api.rhythmData(['2026-09-29', '2026-10-03'], ['Desk'], records, api);
  assert.deepEqual({ ...api.rhythmFocus(deskOnly, { date: '2026-10-01', host: 'Laptop' }) }, { date: '2026-10-01', host: null });
  assert.equal(api.rhythmFocus(api.rhythmData(['2026-09-29', '2026-10-03'], [], records, api), null), null);
});