// Embedded in report-template.html; no network or external browser dependencies.
function effortDay(ms) {
  return reportDay(ms);
}

function effortSlot(ms) {
  return ((Math.floor((ms + 8 * 3600000) / 1800000) % 48) + 48) % 48;
}

function effortMonthBar(slots) {
  const n = Math.max(0, Math.min(48, slots));
  return { band: n < 8 ? "green" : n < 16 ? "yellow" : n < 24 ? "orange" : "red", width: n / 48 * 100 };
}

function effortWeekBand(hours) {
  return hours < 30 ? "green" : hours < 50 ? "yellow" : "red";
}

function effortAggregate(events) {
  const days = new Map();
  let slots = 0, segments = 0;
  for (const e of events) {
    const index = effortSlot(e.timestamp_ms);
    const date = effortDay(e.timestamp_ms);
    if (!date) continue;
    if (!days.has(date)) days.set(date, { slots: new Map(), ranges: [], events: [] });
    const day = days.get(date);
    if (!day.slots.has(index)) day.slots.set(index, []);
    day.slots.get(index).push(e);
    day.events.push(e);
  }
  for (const day of days.values()) {
    for (const index of [...day.slots.keys()].sort((a, b) => a - b)) {
      const last = day.ranges.at(-1);
      if (last && last[1] === index) last[1]++;
      else day.ranges.push([index, index + 1]);
    }
    slots += day.slots.size;
    segments += day.ranges.length;
  }
  return { days, slots, segments };
}

Object.assign(I18N.en, {
  "effort.kicker": "Human activity / UTC+08:00",
  "effort.title": "Effort timeline",
  "effort.rule": "Each occupied half hour counts once across the selected machines and projects. Only human submissions count, not waiting or AI execution.",
  "effort.importScope": "Effort data imported for {covered} of {total} selected machine(s).",
  "effort.notImported": "No effort data imported for the selected machines.",
  "effort.missingHosts": "Not provided:",
  "effort.projectRank": "Projects by effort",
  "effort.overlap": "Select a project to drill down. Projects can share slots; their hours are not additive.",
  "effort.projectFilter": "Effort project",
  "effort.sourceFilter": "Effort submission source",
  "effort.allProjects": "All projects",
  "effort.multiProjects": "{n} selected projects",
  "effort.hours": "Effort hours",
  "effort.slots": "Occupied slots",
  "effort.events": "Human submissions",
  "effort.segments": "Activity blocks",
  "effort.legend": "Submissions per slot",
  "effort.empty": "No verified human submissions for these filters.",
  "effort.scale": "Calendar scale", "effort.year": "Year", "effort.month": "Month", "effort.week": "Week", "effort.day": "Day",
  "effort.prev": "Previous period", "effort.next": "Next period", "effort.wholeDay": "Clear slot selection",
  "effort.drill": "Human submissions", "effort.hoursValue": "{n} h", "effort.zoom": "Ctrl + scroll to zoom at the pointer",
  "effort.count": "{n} submissions", "effort.oneSubmission": "1 submission", "effort.noDay": "Select a day or a slot to inspect submissions.",
  "effort.coverage": "Occupied intervals", "effort.latest": "Latest activity in this range",
  "effort.monthLegend": "Bar length: share of 24 h", "effort.band1": "< 4 h", "effort.band2": "4–<8 h", "effort.band3": "8–<12 h", "effort.band4": "≥ 12 h",
  "effort.weekHours": "Weekly effort hours", "effort.weekBand1": "< 30 h", "effort.weekBand2": "30 to < 50 h", "effort.weekBand3": "≥ 50 h",
  "effort.time": "Time (UTC+08:00)",
  "effort.nextDay": "24:00",
  "effort.open": "Open source line",
});
Object.assign(I18N.zh, {
  "effort.kicker": "人工触发 / UTC+08:00",
  "effort.title": "精力时间带",
  "effort.rule": "只按人工提交落格，所选机器和项目在同一半小时只计一格；等待和 AI 运行不计。",
  "effort.importScope": "所选 {total} 台机器中，{covered} 台已导入精力数据。",
  "effort.notImported": "所选机器未导入精力数据。",
  "effort.missingHosts": "未提供：",
  "effort.projectRank": "按项目看精力",
  "effort.overlap": "点击查看该项目。不同项目可能占用同一格，小时数不能直接相加。",
  "effort.projectFilter": "精力项目",
  "effort.sourceFilter": "人工触发来源",
  "effort.allProjects": "全部项目",
  "effort.multiProjects": "已选 {n} 个项目",
  "effort.hours": "折算精力小时",
  "effort.slots": "已占半小时槽",
  "effort.events": "人工触发",
  "effort.segments": "连续管理段",
  "effort.legend": "每格触发次数",
  "effort.empty": "当前筛选下没有可核实的人工提交。",
  "effort.scale": "日历粒度", "effort.year": "年", "effort.month": "月", "effort.week": "周", "effort.day": "日",
  "effort.prev": "上一个时段", "effort.next": "下一个时段", "effort.wholeDay": "清除半小时选中状态",
  "effort.drill": "人工触发", "effort.hoursValue": "{n} 小时", "effort.zoom": "Ctrl + 滚轮可围绕指针缩放",
  "effort.count": "{n} 次触发", "effort.oneSubmission": "1 次触发", "effort.noDay": "选择日期或半小时格，查看具体人工触发。",
  "effort.coverage": "占用时段", "effort.latest": "当前范围最近一次活动",
  "effort.monthLegend": "条长：占全天 24 小时的比例", "effort.band1": "少于 4 小时", "effort.band2": "4 至不足 8 小时", "effort.band3": "8 至不足 12 小时", "effort.band4": "12 小时及以上",
  "effort.weekHours": "周投入小时", "effort.weekBand1": "少于 30 小时", "effort.weekBand2": "30 至不足 50 小时", "effort.weekBand3": "50 小时及以上",
  "effort.time": "时间（北京时间）",
  "effort.nextDay": "次日00:00",
  "effort.open": "打开原始日志行",
});

const effortUi = { source: "" };
const EFFORT_SCALES = ["year", "month", "week", "day"];
const effortSources = [...new Set((EFFORT_EVENTS || []).map(e => e.source))].sort();
const effortProjectSet = new Set((EFFORT_EVENTS || []).map(projectOf));
const effortProjects = PROJECTS.filter(p => effortProjectSet.has(p));
const effortEl = id => document.getElementById(`effort-${id}`);
const effortFormat = (n, digits = 0) => fmtNum(n, digits);
const effortHourText = n => t("effort.hoursValue", { n: effortFormat(n, 1) });
const effortCount = n => n === 1 ? t("effort.oneSubmission") : t("effort.count", { n: effortFormat(n) });
const effortHostOf = event => event.hostname || EFFORT_HOST;
const effortMissingAll = () => SELECTED_HOSTS.size > 0 && ![...SELECTED_HOSTS].some(host => EFFORT_HOSTS.has(host));

function effortClock(slot) {
  return slot === 48 ? t("effort.nextDay") : `${String(slot >> 1).padStart(2, "0")}:${slot % 2 ? "30" : "00"}`;
}

function effortSpan(a, b) {
  return `${effortClock(a)}–${effortClock(b)}`;
}

function effortFilters() {
  const project = effortEl("project-filter");
  const selected = [...SELECTED_PROJECTS];
  const index = selected.length === 1 ? effortProjects.indexOf(selected[0]) : -1;
  const all = selected.length === PROJECTS.length;
  project.innerHTML = `<option value="all">${t("effort.allProjects")}</option>` +
    (!all && index < 0 ? `<option value="multiple" disabled>${t("effort.multiProjects", { n: selected.length })}</option>` : "") +
    effortProjects.map((p, i) => `<option value="${i}">${escapeHtml(projectLabel(p))}</option>`).join("");
  project.value = all ? "all" : index >= 0 ? String(index) : "multiple";
  const source = effortEl("source-filter");
  source.innerHTML = `<option value="">${t("toolbar.allSources")}</option>` +
    effortSources.map(s => `<option value="${escapeHtml(s)}">${escapeHtml(s)}</option>`).join("");
  source.value = effortUi.source;
}

function effortIsolateProject(index) {
  SELECTED_PROJECTS.clear();
  if (index === "all") PROJECTS.forEach(p => SELECTED_PROJECTS.add(p));
  else SELECTED_PROJECTS.add(effortProjects[Number(index)]);
  CURRENT_PAGE = 1;
  renderAll();
}

function effortScopedEvents({ allProjects = false, useSource = false } = {}) {
  return (EFFORT_EVENTS || []).filter(e => SELECTED_HOSTS.has(effortHostOf(e)) &&
    (allProjects || SELECTED_PROJECTS.has(projectOf(e))) && (!useSource || !effortUi.source || effortUi.source === e.source));
}

function effortEventsInRange(filters) {
  return effortScopedEvents(filters).filter(e => reportInRange(effortDay(e.timestamp_ms)));
}

function effortScale(from, to) {
  const width = from && to ? dayNumber(to) - dayNumber(from) + 1 : 0;
  return reportUi.scale || (width === 1 ? "day" : width <= 7 && width > 0 ? "week" : width <= 31 && width > 0 ? "month" : "year");
}

function effortZoom(scale, date, step = 0) {
  const [from, to] = reportRange();
  if (step && !reportUi.scale && DATE_FROM && DATE_TO) {
    const width = dayNumber(to) - dayNumber(from) + 1;
    reportSetRange(offsetDay(from, step * width), offsetDay(to, step * width));
    return;
  }
  // Expanding to a whole year exposes future dates. Subsequent zooming
  // should follow observed activity, not jump to the empty December edge.
  const latest = !step && !date && !SELECTED_DAY
    ? effortEventsInRange({ useSource: true }).reduce((last, e) => Math.max(last, e.timestamp_ms), -Infinity) : null;
  const focus = date || SELECTED_DAY || effortDay(latest) || (step < 0 ? from : to);
  if (!focus) return;
  const [a, b] = calendarPeriod(focus, scale, step);
  reportSetRange(a, b, { scale, focus: step ? (scale === "day" ? a : "") : scale === "day" ? focus : SELECTED_DAY });
}

function effortSlotButton(date, i, count, labeled = false) {
  const clock = labeled ? `<span>${effortClock(i)}</span>` : "";
  if (!count) return `<span class="effort-slot" aria-hidden="true">${clock}</span>`;
  const label = `${reportDate(date)} · ${effortSpan(i, i + 1)} · ${effortCount(count)}`;
  const pressed = date === SELECTED_DAY && reportUi.slot === i;
  return `<button type="button" class="effort-slot" data-effort-day="${date}" data-effort-slot="${i}" data-count="${count}" data-level="${count <= 2 ? 1 : count <= 5 ? 2 : 3}" aria-pressed="${pressed}" aria-label="${escapeHtml(label)}" title="${escapeHtml(label)}">${clock}</button>`;
}

function effortCalendar(scale, from, to, days) {
  if (!from || !to || from > to) return `<p class="empty">${t("effort.empty")}</p>`;
  if (scale === "day") {
    return `<div class="day-board" data-effort-date="${from}">${Array.from({ length: 48 }, (_, i) =>
      effortSlotButton(from, i, days.get(from)?.slots.get(i)?.length, true)).join("")}</div>`;
  }
  if (scale === "week") {
    let rows = `<div class="effort-row effort-axis" aria-hidden="true"><span></span><div class="effort-hours">${[0,8,16,24,32,40].map(i => `<span>${effortClock(i)}</span>`).join("")}</div><span></span></div>`;
    for (let date = from; date <= to; date = offsetDay(date, 1)) {
      const day = days.get(date);
      rows += `<div class="effort-row" data-effort-date="${date}"><button class="effort-day" type="button" data-effort-day="${date}" aria-pressed="${date === SELECTED_DAY && reportUi.slot === null}">${reportDate(date, { month: "short", day: "numeric" })}</button>
        <div class="effort-hours">${Array.from({ length: 48 }, (_, i) => effortSlotButton(date, i, day?.slots.get(i)?.length)).join("")}</div>
        <span class="effort-total-day">${effortHourText((day?.slots.size || 0) / 2)}</span></div>`;
    }
    return rows;
  }
  const months = [];
  for (let month = calendarPeriod(from, "month")[0]; month <= to; month = offsetMonth(month, 1)) months.push(month);
  if (scale === "year") {
    const totals = new Map();
    for (const [date, day] of days) {
      const key = date.slice(0, 7);
      totals.set(key, (totals.get(key) || 0) + day.slots.size);
    }
    const max = Math.max(1, ...totals.values());
    return `<div class="year-board">${months.map(month => {
      const slots = totals.get(month.slice(0, 7)) || 0;
      return `<button type="button" class="month-tile" data-month="${month}" aria-pressed="${SELECTED_DAY.slice(0, 7) === month.slice(0, 7)}">
        <span>${reportDate(month, { month: "short", year: "numeric" })}</span><strong>${slots ? effortHourText(slots / 2) : "—"}</strong>
        <span aria-hidden="true" class="track"><i style="width:${100 * slots / max}%"></i></span></button>`;
    }).join("")}</div>`;
  }
  const weekdayLabels = Array.from({ length: 7 }, (_, i) => reportDate(offsetDay("2026-11-23", i), { weekday: "short" }));
  return months.map(month => {
    const end = calendarPeriod(month, "month")[1];
    const leading = (new Date(dayNumber(month) * REPORT_DAY_MS).getUTCDay() + 6) % 7;
    let cells = weekdayLabels.map(s => `<span class="calendar-weekday">${s}</span>`).join("") + '<span aria-hidden="true"></span>'.repeat(leading);
    for (let date = month; date <= end; date = offsetDay(date, 1)) {
      const selected = date >= from && date <= to;
      const slots = days.get(date)?.slots.size || 0;
      const { band, width } = effortMonthBar(slots);
      const label = `${reportDate(date)} · ${effortHourText(slots / 2)}`;
      cells += `<button type="button" data-effort-day="${date}" data-slots="${slots}" data-band="${band}" aria-label="${escapeHtml(label)}" title="${escapeHtml(label)}" aria-pressed="${SELECTED_DAY === date}" ${selected ? "" : "disabled"}>
        <span>${Number(date.slice(-2))}</span><strong>${slots ? effortHourText(slots / 2) : "—"}</strong>
        <span class="month-effort-track" aria-hidden="true"><i style="width:${width}%"></i></span></button>`;
    }
    return `<section class="effort-month"><h4>${reportDate(month, { month: "long", year: "numeric" })}</h4><div class="month-board">${cells}</div></section>`;
  }).join("");
}

function renderEffort() {
  effortFilters();
  const missing = [...SELECTED_HOSTS].filter(host => !EFFORT_HOSTS.has(host)).sort();
  const unavailable = effortMissingAll();
  const scope = effortEl("import-scope");
  scope.hidden = Boolean(RAW.localSourceHost) || !SELECTED_HOSTS.size;
  scope.innerHTML = t("effort.importScope", { covered: SELECTED_HOSTS.size - missing.length, total: SELECTED_HOSTS.size }) +
    (missing.length ? ` ${t("effort.missingHosts")} <span class="mask">${escapeHtml(missing.join(", "))}</span>` : "");
  const events = effortEventsInRange({ useSource: true });
  const { days, slots, segments } = effortAggregate(events);
  const [from, to] = reportRange(), scale = effortScale(from, to);
  const weekBand = !unavailable && scale === "week" ? effortWeekBand(slots / 2) : "";

  effortEl("metrics").innerHTML = [["hours", slots / 2, 1], ["slots", slots], ["events", events.length], ["segments", segments]]
    .map(([key, value, digits]) => {
      const band = key === "hours" ? weekBand : "";
      return `<div class="effort-metric"${band ? ` data-band="${band}"` : ""}><b id="effort-${key}">${effortFormat(unavailable ? null : value, digits)}</b><span>${t(key === "hours" && scale === "week" ? "effort.weekHours" : `effort.${key}`)}</span></div>`;
    }).join("");

  const grid = effortEl("grid");
  grid.dataset.scale = scale;
  grid.innerHTML = unavailable ? `<p class="empty">${t("effort.notImported")}</p>` : effortCalendar(scale, from, to, days);
  effortEl("period").textContent = reportDateRange(from, to);
  const [min, max] = reportDataBounds();
  for (const button of effortEl("scales").querySelectorAll("[data-scale]")) {
    button.setAttribute("aria-pressed", String(button.dataset.scale === scale));
    button.disabled = !from;
  }
  for (const button of effortEl("panel").querySelectorAll("[data-step]")) {
    const step = Number(button.dataset.step);
    button.disabled = !from || (step < 0 ? from <= min : to >= max);
  }
  effortEl("nav").innerHTML = `<span>${t("effort.zoom")}</span>` + (scale === "week" || scale === "day" ?
    `<span class="effort-legend">${t("effort.legend")} <i style="background:var(--slot-1)"></i> 1–2 <i style="background:var(--slot-2)"></i> 3–5 <i style="background:var(--slot-3)"></i> 6+</span>` : scale === "month" ?
    `<span class="effort-legend effort-band-legend">${t("effort.monthLegend")} ${["green", "yellow", "orange", "red"].map((band, i) => `<span data-band="${band}"><i></i>${t(`effort.band${i + 1}`)}</span>`).join("")}</span>` : "") + (weekBand ?
    `<span class="effort-legend effort-band-legend">${t("effort.weekHours")} ${["green", "yellow", "red"].map((band, i) => `<span data-band="${band}"><i></i>${t(`effort.weekBand${i + 1}`)}</span>`).join("")}</span>` : "");

  if (unavailable) {
    effortEl("nav").textContent = "";
    effortEl("project-rank").textContent = "";
    effortEl("inspector").textContent = "";
    return;
  }

  const counts = new Map();
  for (const e of effortEventsInRange({ allProjects: true, useSource: true })) {
    const p = projectOf(e);
    if (!counts.has(p)) counts.set(p, new Set());
    counts.get(p).add(Math.floor(e.timestamp_ms / 1800000));
  }
  const ranked = [...counts].sort((a, b) => b[1].size - a[1].size || projectLabel(a[0]).localeCompare(projectLabel(b[0]))).slice(0, 6);
  effortEl("project-rank").innerHTML = ranked.map(([p, values]) => {
    const ratio = 100 * values.size / ranked[0][1].size;
    const active = SELECTED_PROJECTS.size === 1 && SELECTED_PROJECTS.has(p);
    return `<button class="effort-rank" type="button" data-effort-project="${effortProjects.indexOf(p)}" aria-pressed="${active}"><span class="name${p === NO_PROJECT ? "" : " mask"}">${escapeHtml(projectLabel(p))}</span><span class="amount">${effortHourText(values.size / 2)}</span><span class="track" aria-hidden="true"><i style="width:${ratio}%"></i></span></button>`;
  }).join("") || `<p class="effort-note">${t("effort.empty")}</p>`;
  const date = SELECTED_DAY || (from === to ? from : [...days.keys()].sort().at(-1));
  const day = days.get(date);
  const info = date ? `<h4>${reportDate(date, { month: "long", day: "numeric", weekday: "short" })}</h4>
    ${SELECTED_DAY ? "" : `<p class="effort-note">${t("effort.latest")}</p>`}
    <div class="effort-inspect-hours">${effortHourText((day?.slots.size || 0) / 2)}</div>
    <p class="effort-note">${effortFormat(day?.slots.size || 0)} · ${t("effort.slots")} / ${effortCount(day?.events.length || 0)}</p>
    ${day ? `<p class="effort-note">${t("effort.coverage")}</p><p class="effort-ranges">${day.ranges.map(([a,b]) => effortSpan(a,b)).join(" · ")}</p>` : `<p class="effort-note">${t("effort.empty")}</p>`}
    <div class="effort-inspect-actions"><button type="button" class="pg-btn" data-effort-day="${date}">${t("effort.drill")}</button>
    <button type="button" class="pg-btn" data-scale="day" data-at="${date}">${t("effort.day")}</button></div>
    ${SELECTED_DAY ? `<button type="button" class="pg-btn cross-link" data-jump-sheet="usage">${t("sheet.toUsage")}</button>` : ""}` : `<p class="effort-note">${t("effort.empty")}</p>`;
  effortEl("inspector").innerHTML = info;
}

function renderEffortDetail() {
  const detail = effortEl("detail");
  if (effortMissingAll()) { detail.innerHTML = `<p class="empty">${t("effort.notImported")}</p>`; return; }
  if (!SELECTED_DAY) { detail.innerHTML = `<p class="empty">${t("effort.noDay")}</p>`; return; }
  const slot = reportUi.slot;
  const rows = effortEventsInRange({ useSource: true }).filter(e => effortDay(e.timestamp_ms) === SELECTED_DAY && (slot === null || effortSlot(e.timestamp_ms) === slot));
  detail.innerHTML = `<div class="effort-detail-head"><p class="effort-note">${effortCount(rows.length)}</p>
      ${slot === null ? "" : `<button type="button" class="pg-btn" id="effort-clear-slot">${t("effort.wholeDay")}</button>`}</div>
    <div class="effort-events" tabindex="0" role="region" aria-label="${t("effort.drill")}"><table>
    <thead><tr><th>${t("effort.time")}</th><th>${t("cols.machine")}</th><th>${t("cols.project")}</th><th>${t("cols.session")}</th><th>${t("cols.source")}</th></tr></thead>
    <tbody>${rows.map(e => `<tr><td>${new Date(e.timestamp_ms + REPORT_OFFSET_MS).toISOString().slice(11, 19)}</td>
      <td><span class="mask">${escapeHtml(effortHostOf(e))}</span></td>
      <td><span class="${projectOf(e) === NO_PROJECT ? "" : "mask"}">${escapeHtml(projectLabel(projectOf(e)))}</span></td>
      <td><span class="mask">${escapeHtml(sessionName(e) || t("session.untitled"))}</span></td>
      <td>${sourceInline(e, "effort.open")}</td></tr>`).join("") || `<tr><td colspan="5" class="empty">${t("effort.empty")}</td></tr>`}</tbody></table></div>`;
}

effortEl("project-filter").addEventListener("change", e => effortIsolateProject(e.target.value));
effortEl("source-filter").addEventListener("change", e => {
  effortUi.source = e.target.value;
  renderAll();
});
effortEl("panel").addEventListener("click", e => {
  const btn = e.target.closest("button");
  if (!btn) return;
  if (btn.dataset.effortProject != null) {
    const project = btn.dataset.effortProject;
    effortIsolateProject(project);
    effortEl("project-rank").querySelector(`[data-effort-project="${project}"]`)?.focus({ preventScroll: true });
  } else if (btn.dataset.scale) {
    effortZoom(btn.dataset.scale, btn.dataset.at);
  } else if (btn.dataset.step) {
    effortZoom(effortEl("grid").dataset.scale, "", Number(btn.dataset.step));
  } else if (btn.dataset.month) {
    effortZoom("month", btn.dataset.month);
    effortEl("scales").querySelector('[data-scale="month"]').focus({ preventScroll: true });
  } else if (btn.dataset.effortDay) {
    const date = btn.dataset.effortDay, slot = btn.dataset.effortSlot;
    reportPickDay(date, slot == null ? null : Number(slot), "effort");
    const suffix = slot == null ? ":not([data-effort-slot])" : `[data-effort-slot="${slot}"]`;
    (effortEl("grid").querySelector(`[data-effort-day="${date}"]${suffix}`) || effortEl("grid")).focus({ preventScroll: true });
  }
});
effortEl("detail").addEventListener("click", e => {
  if (e.target.id === "effort-clear-slot") reportPickDay(SELECTED_DAY, null, "effort");
});
let effortLastZoom = -Infinity;
effortEl("grid").addEventListener("wheel", e => {
  if (!e.ctrlKey || !e.deltaY) return;
  e.preventDefault();
  const now = performance.now();
  if (now - effortLastZoom < 180) return;
  const index = EFFORT_SCALES.indexOf(effortEl("grid").dataset.scale);
  const scale = EFFORT_SCALES[index + (e.deltaY < 0 ? 1 : -1)];
  if (!scale) return;
  effortLastZoom = now;
  const target = e.target.closest("[data-effort-day], [data-effort-date], [data-month]");
  effortZoom(scale, target?.dataset.effortDay || target?.dataset.effortDate || target?.dataset.month);
}, { passive: false });