// Calendar arithmetic uses UTC civil dates. The report day is UTC+08:00.
const REPORT_DAY_MS = 86400000;
const REPORT_OFFSET_MS = 8 * 3600000;

function reportDay(ms) {
  if (!Number.isFinite(ms)) return null;
  const date = new Date(ms + 8 * 3600000);
  return Number.isFinite(date.getTime()) ? date.toISOString().slice(0, 10) : null;
}

function dayNumber(day) { return Date.parse(`${day}T00:00:00Z`) / 86400000; }
function dayFromNumber(n) { return new Date(n * 86400000).toISOString().slice(0, 10); }
function calendarPeriod(day, scale, step = 0) {
  const date = new Date(dayNumber(day) * 86400000);
  if (scale === "year" || scale === "month") {
    const year = date.getUTCFullYear(), month = date.getUTCMonth();
    const first = scale === "year" ? Date.UTC(year + step, 0, 1) : Date.UTC(year, month + step, 1);
    const next = scale === "year" ? Date.UTC(year + step + 1, 0, 1) : Date.UTC(year, month + step + 1, 1);
    return [dayFromNumber(first / 86400000), dayFromNumber(next / 86400000 - 1)];
  }
  const week = scale === "week";
  const start = dayNumber(day) - (week ? (date.getUTCDay() + 6) % 7 : 0) + step * (week ? 7 : 1);
  return [dayFromNumber(start), dayFromNumber(start + (week ? 6 : 0))];
}

function reportBrush(mode, start, end, anchor, pointer, min, max) {
  const clamp = (n, a, b) => Math.min(b, Math.max(a, n));
  if (mode === "start") return [clamp(pointer, min, end), end];
  if (mode === "end") return [start, clamp(pointer, start, max)];
  if (mode === "new") return [clamp(Math.min(anchor, pointer), min, max), clamp(Math.max(anchor, pointer), min, max)];
  const shift = clamp(pointer - anchor, min - start, max - end);
  return [start + shift, end + shift];
}

Object.assign(I18N.en, {
  "sheet.history": "Full timeline", "sheet.full": "Full range", "sheet.views": "Report sheets",
  "sheet.start": "Start of time window", "sheet.end": "End of time window",
  "sheet.brushHint": "Drag to select or move · drag either edge to resize",
  "sheet.usage": "Usage", "sheet.effort": "Effort time", "sheet.details": "Related details",
  "sheet.breakdown": "Usage breakdown", "sheet.rules": "How usage is counted",
  "sheet.toEffort": "View effort on the selected day →", "sheet.toUsage": "View usage on the selected day →",
  "sheet.noDates": "No dated records", "sheet.dateZone": "Beijing time · UTC+08:00",
  "sheet.monthly": "Monthly totals", "sheet.dayTotals": "Daily totals", "sheet.focus": "Selected day",
  "cols.timeLocal": "Time (UTC+08:00)",
  "ledger.credits": "AI credits",
  "ledger.noMoney": "Currency estimates apply only to GitHub AI Credits.",
  "history.title": "Historical requests", "history.note": "Legacy request-era logs only. VS Code counts recorded requests; CLI counts recorded model calls. No multipliers, no AI credits.",
  "history.vscode": "VS Code requests", "history.cli": "CLI model calls", "history.audit": "Unverified values",
  "history.coverage": "Observed historical range: {range}", "history.months": "Monthly count", "history.auditNote": "The original request count is unavailable for these rows. Values are not included in request totals.",
  "history.header": "{n} historical log rows",
});
Object.assign(I18N.zh, {
  "sheet.history": "全时段时间轴", "sheet.full": "返回全时段", "sheet.views": "报表工作表",
  "sheet.start": "时间窗起点", "sheet.end": "时间窗终点",
  "sheet.brushHint": "拖拽框选或平移 · 拖动边缘缩放",
  "sheet.usage": "用量", "sheet.effort": "精力时间", "sheet.details": "关联明细",
  "sheet.breakdown": "用量分布", "sheet.rules": "用量统计口径",
  "sheet.toEffort": "查看当天精力 →", "sheet.toUsage": "查看当天用量 →",
  "sheet.noDates": "没有带日期的记录", "sheet.dateZone": "北京时间 · UTC+08:00",
  "sheet.monthly": "按月汇总", "sheet.dayTotals": "按日汇总", "sheet.focus": "选中日期",
  "cols.timeLocal": "时间（北京时间）",
  "ledger.credits": "AI 积分",
  "ledger.noMoney": "金额估算仅适用于 GitHub AI Credits。",
  "history.title": "历史请求", "history.note": "只分析旧制时期的记录。VS Code 按请求条目计数，CLI 按日志里的模型调用计数；不乘倍率、不计 AI 积分。",
  "history.vscode": "VS Code 请求数", "history.cli": "CLI 模型调用数", "history.audit": "待核对原始值",
  "history.coverage": "可见历史范围：{range}", "history.months": "按月请求数", "history.auditNote": "这些记录无法还原原始请求次数，数值只供核对，不计入请求总数。",
  "history.header": "{n} 条历史日志记录",
});

const reportUi = { sheet: "usage", detail: "usage", breakdown: "model", scale: "", slot: null, slotDay: "" };
const reportWindows = { current: null, history: null };
const historyKind = r => r.request_count == null ? "audit" : r.source?.startsWith("copilot.cli") ? "cli" : "vscode";
const historyKinds = ["vscode", "cli", "audit"].filter(kind => HISTORY.some(r => historyKind(r) === kind));
const historyUi = { source: historyKinds[0], page: 1 };
const reportEl = id => document.getElementById(id);
const reportLocale = () => CURRENT_LANG === "zh" ? "zh-CN" : "en-US";
const reportDateFormats = new Map();
function reportDate(day, options = { dateStyle: "medium" }) {
  if (!day) return "";
  const locale = reportLocale(), key = locale + JSON.stringify(options);
  if (!reportDateFormats.has(key)) reportDateFormats.set(key, new Intl.DateTimeFormat(locale, { ...options, timeZone: "UTC" }));
  return reportDateFormats.get(key).format(dayNumber(day) * REPORT_DAY_MS);
}
function reportDateRange(from, to) {
  return !from || !to ? t("sheet.noDates") : from === to ? reportDate(from) : `${reportDate(from)} – ${reportDate(to)}`;
}
function historyRows(limited = true) {
  return HISTORY.filter(r => historyKind(r) === historyUi.source && SELECTED_HOSTS.has(r.hostname || EFFORT_HOST)
    && SELECTED_PROJECTS.has(projectOf(r)) && (!limited || reportInRange(dayKey(r.timestamp_ms))));
}

function observedBounds(dates) {
  const sorted = dates.filter(Boolean).sort();
  return [sorted[0] || "", sorted.at(-1) || ""];
}
const historyBounds = Object.fromEntries(historyKinds.map(kind => [kind, observedBounds(HISTORY.filter(r => historyKind(r) === kind).map(r => dayKey(r.timestamp_ms)))]));
const effortBounds = observedBounds((EFFORT_EVENTS || []).map(e => reportDay(e.timestamp_ms)));
function reportDataBounds() {
  if (reportUi.sheet === "history") return historyBounds[historyUi.source] || ["", ""];
  if (reportUi.sheet === "effort") return effortBounds;
  return [DATA_MIN_DAY, DATA_MAX_DAY];
}

function renderHistory() {
  const rows = historyRows(), audit = historyUi.source === "audit";
  const label = t(`history.${historyUi.source}`);
  reportEl("history-coverage").textContent = t("history.coverage", { range: reportDateRange(...reportDataBounds()) });
  reportEl("history-sources").innerHTML = historyKinds.map(kind => `<button type="button" data-history-source="${kind}" aria-pressed="${kind === historyUi.source}">${t(`history.${kind}`)}</button>`).join("");
  reportEl("history-summary").innerHTML = audit ? `<p class="usage-note">${t("history.auditNote")}</p>` :
    `<div class="kpi"><div class="label">${label}</div><div class="value" id="history-count">${fmtNum(rows.reduce((n, r) => n + r.request_count, 0), 0)}</div></div>`;
  const months = new Map(), hosts = new Map();
  for (const r of rows) {
    const day = dayKey(r.timestamp_ms), host = r.hostname || EFFORT_HOST;
    if (day) months.set(day.slice(0, 7), (months.get(day.slice(0, 7)) || 0) + r.request_count);
    hosts.set(host, (hosts.get(host) || 0) + r.request_count);
  }
  const max = Math.max(1, ...months.values());
  reportEl("history-months").innerHTML = audit ? "" : [...months].sort(([a], [b]) => a.localeCompare(b)).map(([month, n]) =>
    `<button type="button" data-history-month="${month}-01"><span>${reportDate(`${month}-01`, { month: "long", year: "numeric" })}</span><strong>${fmtNum(n, 0)}</strong><span class="track"><i style="width:${100 * n / max}%"></i></span></button>`).join("");
  reportEl("history-machines").innerHTML = audit ? "" : [...hosts].map(([host, n]) => `<span class="chip"><span class="mask">${escapeHtml(host)}</span> · <b>${fmtNum(n, 0)}</b></span>`).join("");
  reportEl("history-detail-heading").textContent = t("sheet.details");
  const detailRows = rows.slice().sort((a, b) => (b.timestamp_ms || 0) - (a.timestamp_ms || 0));
  const pageSize = 20, pages = Math.max(1, Math.ceil(detailRows.length / pageSize));
  historyUi.page = Math.min(historyUi.page, pages);
  reportEl("history-table").tHead.innerHTML = `<tr>${["cols.timeLocal", "cols.machine", "cols.project", "cols.model", `history.${historyUi.source}`, "cols.source"].map(key => `<th>${t(key)}</th>`).join("")}</tr>`;
  reportEl("history-table").tBodies[0].innerHTML = detailRows.slice((historyUi.page - 1) * pageSize, historyUi.page * pageSize).map(r => `<tr><td>${fmtDateTime(r.timestamp_ms)}</td>
    <td><span class="mask">${escapeHtml(r.hostname || EFFORT_HOST)}</span></td><td><span class="mask">${escapeHtml(projectLabel(projectOf(r)))}</span></td>
    <td>${escapeHtml(r.model)}</td><td class="num">${fmtNum(billingNumber(r), audit ? 2 : 0)}</td><td>${escapeHtml(r.source)}${r.file ? ` · <a class="effort-source-link" href="${escapeHtml(vscodeFileUrl(r.file, r.line))}">${t("source.open")}</a>` : ""}</td></tr>`).join("")
    || `<tr><td colspan="6" class="empty">${t("empty.records")}</td></tr>`;
  reportEl("history-pages").innerHTML = pages > 1 ? `<button class="pg-btn" type="button" data-history-page="-1" ${historyUi.page === 1 ? "disabled" : ""}>${t("pagination.prev")}</button>
    <span>${t("pagination.page", { current: historyUi.page, total: pages })}</span><button class="pg-btn" type="button" data-history-page="1" ${historyUi.page === pages ? "disabled" : ""}>${t("pagination.next")}</button>` : "";
}

function reportRange() {
  const [min, max] = reportDataBounds();
  const from = DATE_FROM || min || DATE_TO;
  const to = DATE_TO || max || DATE_FROM;
  return [DATE_FROM ? from : from < to ? from : to, DATE_TO ? to : to > from ? to : from];
}
function reportInRange(day) { return (!DATE_FROM || day && day >= DATE_FROM) && (!DATE_TO || day && day <= DATE_TO); }

function reportSetRange(from, to, { scale = "", preset = "custom", focus = SELECTED_DAY } = {}) {
  if (from && to && from > to) [from, to] = [to, from];
  if (reportUi.sheet === "history") {
    const [min, max] = reportDataBounds();
    const clamp = d => !d || !min ? d : d < min ? min : d > max ? max : d;
    from = clamp(from); to = clamp(to);
    historyUi.page = 1;
  }
  DATE_FROM = from;
  DATE_TO = to;
  DATE_PRESET = preset;
  reportUi.scale = scale;
  SELECTED_DAY = focus && reportInRange(focus) ? focus : "";
  CURRENT_PAGE = 1;
  renderAll();
}

function reportPickDay(day, slot = null, detail = reportUi.sheet) {
  SELECTED_DAY = day;
  reportUi.slot = slot;
  reportUi.slotDay = day;
  reportUi.detail = detail;
  reportEl("related-drawer").open = true;
  CURRENT_PAGE = 1;
  renderAll();
}

function reportSwitchSheet(sheet) {
  if (sheet === "effort" && EFFORT_EVENTS === null) return;
  if (sheet === "history" && !historyKinds.length) return;
  const scope = name => name === "history" ? "history" : "current";
  const previous = scope(reportUi.sheet), next = scope(sheet);
  if (previous !== next) {
    reportWindows[previous] = [DATE_FROM, DATE_TO, DATE_PRESET, SELECTED_DAY];
    [DATE_FROM, DATE_TO, DATE_PRESET, SELECTED_DAY] = reportWindows[next] || ["", "", "all", ""];
  }
  reportUi.sheet = reportUi.detail = sheet;
  if (SELECTED_DAY) reportEl("related-drawer").open = true;
  renderAll();
}

function reportSyncChrome() {
  const hasEffort = EFFORT_EVENTS !== null;
  if (reportUi.slotDay !== SELECTED_DAY) { reportUi.slot = null; reportUi.slotDay = ""; }
  for (const pane of ["usage", "effort", "history"]) {
    const button = reportEl(`sheet-tab-${pane}`);
    const active = reportUi.sheet === pane;
    button.hidden = pane === "effort" && !hasEffort || pane === "history" && !historyKinds.length;
    button.setAttribute("aria-selected", String(active));
    button.tabIndex = active ? 0 : -1;
    reportEl(`${pane}-panel`).hidden = !active;
    const detail = document.querySelector(`[data-detail="${pane}"]`);
    if (detail) {
      detail.hidden = pane === "effort" && !hasEffort;
      detail.setAttribute("aria-pressed", String(reportUi.detail === pane));
    }
  }
  for (const button of document.querySelectorAll("[data-breakdown]")) {
    const active = reportUi.breakdown === button.dataset.breakdown;
    button.setAttribute("aria-pressed", String(active));
    reportEl(`breakdown-${button.dataset.breakdown}`).hidden = !active;
  }
  reportEl("date-from").value = DATE_FROM;
  reportEl("date-to").value = DATE_TO;
  reportEl("date-range").textContent = reportDateRange(...reportRange());
  for (const id of ["pricing-heading", "pricing-panel"]) reportEl(id).hidden = reportUi.sheet !== "usage";
  const [min, max] = reportUi.sheet === "history" ? reportDataBounds() : ["", ""];
  for (const id of ["date-from", "date-to"]) { reportEl(id).min = min; reportEl(id).max = max; }
  reportEl("related-drawer").hidden = reportUi.sheet === "history";
  reportEl("detail-usage").hidden = reportUi.detail !== "usage";
  reportEl("effort-detail").hidden = reportUi.detail !== "effort";
  document.querySelector('[data-jump-sheet="effort"]').hidden = !hasEffort || !SELECTED_DAY;
  reportEl("related-context").textContent = SELECTED_DAY
    ? `${reportDate(SELECTED_DAY)}${reportUi.detail === "effort" && reportUi.slot !== null ? ` · ${effortSpan(reportUi.slot, reportUi.slot + 1)}` : ""}`
    : reportDateRange(...reportRange());
}

function reportCalendarDomain() {
  const [min, max] = reportDataBounds();
  if (!min || !max) return null;
  if (reportUi.sheet === "history") return [dayNumber(min), dayNumber(max)];
  const low = [min, DATE_FROM, DATE_TO].filter(Boolean).sort()[0];
  const high = [max, DATE_FROM, DATE_TO].filter(Boolean).sort().at(-1);
  return [dayNumber(calendarPeriod(low, "year")[0]), dayNumber(calendarPeriod(high, "year")[1])];
}

function reportBrushBounds(domain) {
  return [DATE_FROM ? dayNumber(DATE_FROM) : domain[0], DATE_TO ? dayNumber(DATE_TO) : domain[1]];
}

function paintTimeWindow(domain, start, end) {
  const window = reportEl("time-window");
  const span = domain[1] - domain[0] + 1;
  window.style.left = `${100 * (start - domain[0]) / span}%`;
  window.style.width = `${100 * (end - start + 1) / span}%`;
  reportEl("time-track").dataset.start = dayFromNumber(start);
  reportEl("time-track").dataset.end = dayFromNumber(end);
  for (const [i, button] of [...window.querySelectorAll("[data-grip]")].entries()) {
    const date = reportDate(dayFromNumber(i ? end : start));
    const title = `${t(i ? "sheet.end" : "sheet.start")} · ${date}`;
    button.setAttribute("aria-label", title);
    button.title = title;
  }
}

function renderTimeNavigator() {
  const domain = reportCalendarDomain();
  reportEl("time-track").hidden = !domain;
  reportEl("time-reset").disabled = !domain || (!DATE_FROM && !DATE_TO && !SELECTED_DAY);
  reportEl("time-range-label").textContent = `${reportDateRange(...reportRange())} · ${t("sheet.dateZone")}`;
  if (!domain) return;
  const span = domain[1] - domain[0] + 1;
  // Bin long histories to bound SVG size; overview never uses the brush filter.
  const n = Math.min(span, 480);
  const values = new Float64Array(n);
  const bin = d => Math.floor((dayNumber(d) - domain[0]) / span * n);
  const effort = reportUi.sheet === "effort";
  const history = reportUi.sheet === "history";
  if (effort && SELECTED_HOSTS.has(EFFORT_HOST)) {
    const counted = new Set();
    for (const e of EFFORT_EVENTS || []) {
      if (!SELECTED_PROJECTS.has(projectOf(e))) continue;
      if (effortUi.source && e.source !== effortUi.source) continue;
      const slot = Math.floor(e.timestamp_ms / 1800000);
      if (counted.has(slot)) continue;
      counted.add(slot);
      values[bin(effortDay(e.timestamp_ms))]++;
    }
  } else if (history && historyUi.source !== "audit") {
    for (const r of historyRows(false)) if (dayKey(r.timestamp_ms)) values[bin(dayKey(r.timestamp_ms))] += r.request_count;
  } else if (!effort && !history) {
    for (const r of RECORDS) {
      const d = dayKey(r.timestamp_ms);
      if (d && SELECTED_HOSTS.has(r.hostname || EFFORT_HOST) && SELECTED_PROJECTS.has(projectOf(r))) {
        values[bin(d)] += billingNumber(r);
      }
    }
  }
  const points = () => {
    const max = Math.max(1, ...values);
    return Array.from(values, (v, i) => `${(1000 * (i + .5) / n).toFixed(1)},${(41 - 35 * v / max).toFixed(1)}`).join(" ");
  };
  const mode = effort ? "effort" : history ? (historyUi.source === "audit" ? "unclassified" : "requests") : "credits";
  reportEl("time-nav-title").textContent = `${t("sheet.history")} · ${t(effort ? "effort.slots" : history ? `history.${historyUi.source}` : "ledger.credits")}`;
  reportEl("time-spark").innerHTML = `<svg viewBox="0 0 1000 44" preserveAspectRatio="none" aria-hidden="true">
    ${mode === "unclassified" ? "" : `<polyline class="nav-${mode}" points="${points()}"/>`}</svg>`;
  const years = Number(dayFromNumber(domain[1]).slice(0, 4)) - Number(dayFromNumber(domain[0]).slice(0, 4)) + 1;
  const monthsPerTick = years === 1 ? 1 : years === 2 ? 3 : 12 * Math.ceil(years / 8);
  let ticks = "";
  for (let day = dayFromNumber(domain[0]); dayNumber(day) <= domain[1]; day = offsetMonth(day, monthsPerTick)) {
    const x = 100 * (dayNumber(day) - domain[0]) / span;
    const label = reportDate(day, years === 1 ? { month: "short" } : years === 2 ? { year: "2-digit", month: "short" } : { year: "numeric" });
    ticks += `<span style="left:${x}%">${escapeHtml(label)}</span>`;
  }
  reportEl("time-ticks").innerHTML = ticks;
  paintTimeWindow(domain, ...reportBrushBounds(domain));
}

function bindReportSheets() {
  reportEl("sheet-tabs").addEventListener("click", e => {
    const tab = e.target.closest("[data-sheet]");
    if (tab) reportSwitchSheet(tab.dataset.sheet);
  });
  reportEl("sheet-tabs").addEventListener("keydown", e => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
    const tabs = [...reportEl("sheet-tabs").querySelectorAll("[data-sheet]:not([hidden])")];
    if (tabs.length < 2) return;
    e.preventDefault();
    const index = tabs.findIndex(tab => tab.dataset.sheet === reportUi.sheet);
    const sheet = tabs[e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : (index + (e.key === "ArrowLeft" ? -1 : 1) + tabs.length) % tabs.length].dataset.sheet;
    reportSwitchSheet(sheet);
    reportEl(`sheet-tab-${sheet}`).focus();
  });
  document.addEventListener("click", e => {
    const source = e.target.closest("[data-history-source]");
    if (source && source.dataset.historySource !== historyUi.source) {
      historyUi.source = source.dataset.historySource;
      reportSetRange("", "", { preset: "all", focus: "" });
      document.querySelector(`[data-history-source="${historyUi.source}"]`).focus({ preventScroll: true });
    }
    const month = e.target.closest("[data-history-month]");
    if (month) reportSetRange(...calendarPeriod(month.dataset.historyMonth, "month"));
    const step = e.target.closest("[data-history-page]");
    if (step) {
      historyUi.page += Number(step.dataset.historyPage);
      renderAll();
      document.querySelector(`[data-history-page="${step.dataset.historyPage}"]:not([disabled])`)?.focus();
    }
    const jump = e.target.closest("[data-jump-sheet]");
    if (jump) { reportSwitchSheet(jump.dataset.jumpSheet); reportEl(`sheet-tab-${jump.dataset.jumpSheet}`).focus(); }
    const detail = e.target.closest("[data-detail]");
    if (detail) { reportUi.detail = detail.dataset.detail; renderAll(); }
    const filter = e.target.closest("[data-breakdown]");
    if (filter) { reportUi.breakdown = filter.dataset.breakdown; reportSyncChrome(); }
  });
  reportEl("time-reset").addEventListener("click", () => reportSetRange("", "", { preset: "all", focus: "" }));
  const track = reportEl("time-track");
  const cursorDay = (x, domain) => {
    const r = track.getBoundingClientRect();
    return Math.min(domain[1], Math.max(domain[0], domain[0] + Math.floor((x - r.left) / r.width * (domain[1] - domain[0] + 1))));
  };
  let drag = null;
  const commit = ([a, b]) => reportSetRange(dayFromNumber(a), dayFromNumber(b));
  track.addEventListener("pointerdown", e => {
    if (!e.isPrimary || e.button !== 0 || drag) return;
    const domain = reportCalendarDomain();
    if (!domain) return;
    const [a,b] = reportBrushBounds(domain);
    const mode = e.target.closest("[data-grip]")?.dataset.grip
      || (e.target.closest("#time-window") && (a !== domain[0] || b !== domain[1]) ? "move" : "new");
    drag = { domain, a, b, x: e.clientX, anchor: cursorDay(e.clientX, domain), mode, id: e.pointerId };
    track.setPointerCapture(e.pointerId);
  });
  const rangeAt = e => {
    const cursor = cursorDay(e.clientX, drag.domain);
    const edge = drag.mode === "start" ? drag.a : drag.mode === "end" ? drag.b : null;
    return reportBrush(drag.mode, drag.a, drag.b, drag.anchor,
      edge === null ? cursor : edge + cursor - drag.anchor, ...drag.domain);
  };
  track.addEventListener("pointermove", e => {
    if (drag?.id === e.pointerId && Math.abs(e.clientX - drag.x) >= 2) paintTimeWindow(drag.domain, ...rangeAt(e));
  });
  track.addEventListener("pointerup", e => {
    if (drag?.id !== e.pointerId) return;
    const range = rangeAt(e);
    const changed = drag.mode === "new" || Math.abs(e.clientX - drag.x) >= 2;
    const previous = drag;
    drag = null;
    if (changed) commit(range); else paintTimeWindow(previous.domain, previous.a, previous.b);
  });
  const cancel = e => {
    if (drag?.id !== e.pointerId) return;
    paintTimeWindow(drag.domain, drag.a, drag.b);
    drag = null;
  };
  track.addEventListener("pointercancel", cancel);
  track.addEventListener("lostpointercapture", cancel);
  track.addEventListener("keydown", e => {
    const mode = e.target.dataset.grip;
    const domain = reportCalendarDomain();
    if (!mode || !domain || !["ArrowLeft", "ArrowRight"].includes(e.key)) return;
    const [a,b] = reportBrushBounds(domain);
    const delta = (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 7 : 1);
    e.preventDefault();
    commit(reportBrush(mode, a, b, 0, (mode === "start" ? a : b) + delta, ...domain));
  });
}