const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

const template = fs.readFileSync(path.join(__dirname, '../src/report-template.html'), 'utf8');
const start = template.indexOf('  function aggregateReportTurns(');
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