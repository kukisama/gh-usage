const rhythmLabels = {
  en: {
    theme: "Theme", classic: "Classic", rhythm: "Rhythm",
    title: "Usage by machine and date", scroll: "Drag or swipe to see more dates",
    time: "Beijing time", project: "Project", model: "Model", empty: "No dated usage in this selection",
    preview: "{n} records · latest {shown} shown", count: "{n} records", daily: "By day", monthly: "By month",
    zone: "Beijing time · UTC+08:00",
    workspace: "Rhythm workspace", view: "Usage views", atlas: "Device atlas", trend: "Trend analysis",
    usage: "Usage in range", rows: "Turns / summaries", days: "Active days",
    date: "Date", machines: "Machines", projects: "Projects", filters: "Scope",
    settings: "Billing options", breakdown: "Usage breakdown", trendTitle: "Usage by machine",
    periodDetail: "Period details", scale: "Common scale across machines",
    previous: "Previous period with records", next: "Next period with records",
    noDetail: "Change the report filters to see records here."
  },
  zh: {
    theme: "主题", classic: "经典", rhythm: "节奏", title: "各机器用量",
    scroll: "左右拖动或滑动，查看更多日期",
    time: "北京时间", project: "项目", model: "模型", empty: "当前筛选下没有带日期的用量",
    preview: "{n} 条记录 · 显示最近 {shown} 条", count: "{n} 条记录", daily: "按日", monthly: "按月",
    zone: "北京时间 · UTC+08:00",
    workspace: "节奏工作台", view: "用量视图", atlas: "设备线路", trend: "趋势分析",
    usage: "范围内用量", rows: "轮次 / 汇总", days: "活跃天数",
    date: "日期", machines: "机器", projects: "项目", filters: "筛选范围",
    settings: "计费选项", breakdown: "用量分布", trendTitle: "按机器看趋势",
    periodDetail: "时段明细", scale: "各机器使用同一刻度",
    previous: "上一个有记录的时段", next: "下一个有记录的时段",
    noDetail: "调整报表筛选后，可在这里查看明细。"
  }
};

function rhythmData([from, to], hosts, records, api) {
  const monthly = api.dayNumber(to) - api.dayNumber(from) >= 45;
  const key = day => monthly ? `${day.slice(0, 7)}-01` : day;
  const next = monthly ? api.offsetMonth : api.offsetDay;
  const dates = [];
  if (from && to && from <= to) {
    for (let date = key(from); date <= to; date = next(date, 1)) dates.push(date);
  }
  const totals = new Map(dates.map(date => [date, 0]));
  const periods = new Map();
  const lanes = new Map(hosts.map(host => [host, { stops: new Map() }]));
  let peak = 0;
  for (const record of records) {
    const day = api.day(record.timestamp_ms);
    const lane = lanes.get(api.host(record));
    if (!lane || !day || day < from || day > to) continue;
    const date = key(day);
    if (!totals.has(date)) continue;
    const stop = lane.stops.get(date) || { amount: 0, rows: [] };
    const amount = api.amount(record);
    stop.rows.push(record);
    if (!periods.has(date)) periods.set(date, []);
    periods.get(date).push(record);
    stop.amount += amount;
    lane.stops.set(date, stop);
    totals.set(date, totals.get(date) + amount);
    peak = Math.max(peak, stop.amount);
  }
  return { monthly, dates, lanes, totals, periods, peak, recordCount: records.length };
}

function rhythmFocus(data, previous) {
  const date = previous && data.periods.has(previous.date)
    ? previous.date : data.dates.findLast(date => data.periods.has(date));
  if (!date) return null;
  const host = previous?.date === date && data.lanes.get(previous.host)?.stops.has(date) ? previous.host : null;
  return { date, host };
}

function bindRhythmLayout(shell, classic) {
  const selectors = {
    tabs: "#sheet-tabs", filters: "#filter-bar", date: ".date-slicer",
    machines: "#machine-list", projects: "#project-list", pricing: "#pricing-panel",
    usage: "#usage-panel", effort: "#effort-panel", history: "#history-panel",
    details: "#related-drawer", distribution: "#usage-distribution", "effort-link": "[data-jump-sheet=effort]",
  };
  const parts = Object.fromEntries(Object.entries(selectors).map(([name, selector]) => {
    const node = classic.querySelector(selector);
    const home = document.createComment(`classic ${name}`);
    node.before(home);
    return [name, { node, home }];
  }));
  const slot = name => shell.querySelector(`[data-rhythm-slot="${name}"]`);
  const place = (part, position) => {
    const { node, home } = parts[part];
    if (position) {
      const target = slot(position);
      if (node.parentNode !== target) target.append(node);
    } else if (home.nextSibling !== node) home.after(node);
  };
  // Move existing controls so listeners and DOM-backed values survive a layout switch.
  return (active, trend) => {
    if (active) {
      for (const part of ["tabs", "filters", "date", "pricing", "details", "effort-link"]) place(part, part);
      for (const part of ["usage", "effort", "history"]) place(part, "sheets");
      for (const part of ["machines", "projects"]) place(part, trend ? `trend-${part}` : part);
      place("distribution", trend ? "trend-breakdown" : "atlas-breakdown");
    } else {
      for (const part of Object.keys(parts)) place(part);
    }
    classic.hidden = active;
    shell.hidden = !active;
  };
}

function mountRhythmTheme(api) {
  const controls = document.getElementById("rhythm-theme-controls").content.cloneNode(true);
  const select = controls.querySelector("[data-rhythm-control]");
  document.querySelector("header.top .right").prepend(controls);
  const shell = document.getElementById("rhythm-theme-layout").content.firstElementChild.cloneNode(true);
  const classic = document.getElementById("classic-report");
  classic.after(shell);
  const arrange = bindRhythmLayout(shell, classic);
  const panel = shell.querySelector(".rhythm-panel");
  const chartHome = shell.querySelector("[data-rhythm-chart-home]");
  const usagePanel = classic.querySelector("#usage-panel");
  const scroll = panel.querySelector(".rhythm-scroll");
  const board = panel.querySelector(".rhythm-board");
  const hint = panel.querySelector(".rhythm-hint");
  const body = panel.querySelector("tbody");
  const navigation = [...panel.querySelectorAll("[data-rhythm-step]")];
  const translations = [...shell.querySelectorAll("[data-rhythm-text]"), ...select.parentElement.querySelectorAll("[data-rhythm-text]")];
  const colors = ["#225abb", "#167058", "#7944a0", "#a45d0b", "#a63455", "#366d82"];
  const esc = api.escape;
  let focus = null;
  const focusByView = new Map();
  let view = "atlas";
  let data = null;

  const label = key => rhythmLabels[api.lang()][key];
  const hostColor = host => colors[api.hostIndex(host) % colors.length];
  const syncOverflow = () => {
    const overflow = !panel.hidden && scroll.scrollWidth > scroll.clientWidth;
    scroll.classList.toggle("is-scrollable", overflow);
    hint.hidden = !overflow;
  };
  const active = () => select.value === "rhythm";
  const applyLayout = (sheet) => {
    arrange(active(), sheet === "usage" && view === "trend");
    const parent = active() ? usagePanel : chartHome;
    if (panel.parentNode !== parent) parent.prepend(panel);
  };
  const useTheme = () => {
    document.body.classList.toggle("report-rhythm", active());
    applyLayout(api.sheet());
  };
  try {
    if (localStorage.getItem("gh-usage.theme") === "rhythm") select.value = "rhythm";
  } catch (_) {}
  useTheme();
  select.addEventListener("change", () => {
    useTheme();
    try { localStorage.setItem("gh-usage.theme", select.value); } catch (_) {}
    api.refresh();
  });
  api.bindPan(scroll, { surface: ".rhythm-board", onResize: syncOverflow });
  shell.querySelector(".rhythm-view-switch").addEventListener("click", event => {
    const button = event.target.closest("[data-rhythm-view]");
    if (!button || button.dataset.rhythmView === view) return;
    focusByView.set(view, focus);
    view = button.dataset.rhythmView;
    focus = focusByView.get(view) ?? null;
    api.refresh();
  });
  shell.addEventListener("keydown", event => {
    if (event.key !== "Escape") return;
    const popup = event.target.closest(".rhythm-popover[open]");
    if (popup) {
      popup.open = false;
      popup.querySelector("summary").focus();
    }
  });

  const dateLabel = (date, detailed = false) => api.date(date, data.monthly
    ? { month: "short", year: detailed ? "numeric" : "2-digit" }
    : detailed ? undefined : { month: "numeric", day: "numeric" });
  const amountLabel = amount => `${api.number(amount, 2)} ${api.unit()}`;
  const availableDates = () => data.dates.filter(date => data.periods.has(date));
  const revealFocus = () => {
    const column = board.querySelector(`.rhythm-date[data-column="${data.dates.indexOf(focus.date)}"]`);
    if (!column || !scroll.clientWidth) return;
    const sticky = board.querySelector(".rhythm-row-label").getBoundingClientRect().width;
    const cell = column.getBoundingClientRect(), viewport = scroll.getBoundingClientRect();
    if (cell.left < viewport.left + sticky || cell.right > viewport.left + scroll.clientWidth) {
      scroll.scrollLeft += cell.left - viewport.left - (scroll.clientWidth + sticky - cell.width) / 2;
    }
  };
  const inspector = () => {
    const stop = !focus ? null : focus.host === null
      ? { amount: data.totals.get(focus.date), rows: data.periods.get(focus.date) }
      : data.lanes.get(focus.host)?.stops.get(focus.date);
    const heading = panel.querySelector("[data-rhythm-focus]");
    const note = panel.querySelector("[data-rhythm-note]");
    const subtotal = panel.querySelector("[data-rhythm-subtotal]");
    const comparison = panel.querySelector("[data-rhythm-comparison]");
    const dates = availableDates();
    const index = focus ? dates.indexOf(focus.date) : -1;
    navigation[0].disabled = index <= 0;
    navigation[1].disabled = index < 0 || index === dates.length - 1;
    subtotal.hidden = !stop || stop.rows.length === data.recordCount;
    comparison.hidden = true;
    panel.querySelector(".rhythm-records").hidden = !stop;
    body.replaceChildren();
    if (!stop) {
      heading.textContent = label("empty");
      note.textContent = label("noDetail");
      return;
    }
    const machine = document.createElement("span");
    machine.className = "mask";
    if (focus.host === null) heading.textContent = dateLabel(focus.date, true);
    else {
      machine.textContent = focus.host;
      heading.replaceChildren(machine, ` · ${dateLabel(focus.date, true)}`);
    }
    if (!subtotal.hidden) subtotal.querySelector("strong").textContent = api.number(stop.amount, 2);
    const contributors = [...data.lanes].filter(([, lane]) => lane.stops.has(focus.date));
    if (focus.host === null && contributors.length > 1) {
      comparison.hidden = false;
      comparison.innerHTML = contributors.map(([host, lane]) =>
        `<div style="--route-color:${hostColor(host)}"><span class="mask">${esc(host)}</span><strong>${esc(api.number(lane.stops.get(focus.date).amount, 2))}</strong></div>`).join("");
    }
    const shown = stop.rows.slice().sort((a, b) => b.timestamp_ms - a.timestamp_ms).slice(0, 8);
    note.textContent = label(shown.length === stop.rows.length ? "count" : "preview")
      .replace("{n}", api.number(stop.rows.length, 0)).replace("{shown}", api.number(shown.length, 0));
    body.innerHTML = shown.map(row => {
      const time = api.time(row.timestamp_ms);
      return `<tr><td title="${esc(time)}">${esc(time.slice(5, 16))}</td><td><span class="mask">${esc(api.host(row))}</span></td>
        <td><span class="mask">${esc(api.project(row))}</span></td><td>${esc(row.model || "—")}</td>
        <td class="num">${esc(api.number(api.amount(row), 2))}</td></tr>`;
    }).join("");
  };
  const paintFocus = () => {
    const column = focus ? data.dates.indexOf(focus.date) : -1;
    for (const row of board.querySelectorAll(".rhythm-row")) {
      row.querySelector(".rhythm-cursor")?.remove();
      if (column >= 0) {
        const cursor = document.createElement("span");
        cursor.className = "rhythm-cursor";
        cursor.style.gridColumn = column + 2;
        row.append(cursor);
      }
    }
    for (const btn of board.querySelectorAll("[data-station]")) {
      const [lane, index] = btn.dataset.station.split(":").map(Number);
      btn.setAttribute("aria-pressed", String(column === index && focus?.host === (data.hosts[lane] ?? null)));
    }
    inspector();
  };
  for (const button of navigation) button.addEventListener("click", () => {
    if (!focus) return;
    const dates = availableDates();
    const date = dates[dates.indexOf(focus.date) + Number(button.dataset.rhythmStep)];
    if (!date) return;
    focus = { date, host: null };
    paintFocus();
    revealFocus();
    if (button.disabled) (navigation.find(next => !next.disabled) || scroll).focus({ preventScroll: true });
  });
  board.addEventListener("click", event => {
    const btn = event.target.closest("[data-station]");
    if (!btn) return;
    const [lane, column] = btn.dataset.station.split(":").map(Number);
    const date = data.dates[column], host = data.hosts[lane] ?? null;
    focus = { host: focus?.date === date && focus.host === host ? null : host, date };
    paintFocus();
  });

  return {
    active,
    render(sheet, records) {
      for (const node of translations) node.textContent = label(node.dataset.rhythmText);
      for (const node of shell.querySelectorAll("[data-rhythm-label]")) node.setAttribute("aria-label", label(node.dataset.rhythmLabel));
      select.setAttribute("aria-label", label("theme"));
      const trend = sheet === "usage" && view === "trend";
      applyLayout(sheet);
      if (!active()) return;
      shell.dataset.view = trend ? "trend" : "atlas";
      shell.dataset.sheet = sheet;
      shell.querySelector(".rhythm-view-switch").hidden = sheet !== "usage";
      shell.querySelector("[data-rhythm-overview]").hidden = sheet !== "usage";
      shell.querySelector('[data-rhythm-drop="pricing"]').hidden = sheet !== "usage";
      for (const button of shell.querySelectorAll("[data-rhythm-view]")) {
        button.setAttribute("aria-pressed", String(button.dataset.rhythmView === view));
      }
      const range = api.range();
      shell.querySelector("[data-rhythm-range]").textContent = api.dateRange(...range);
      const selection = api.selected();
      shell.querySelector('[data-rhythm-count="hosts"]').textContent = `${selection.hosts}/${selection.allHosts}`;
      shell.querySelector('[data-rhythm-count="projects"]').textContent = `${selection.projects}/${selection.allProjects}`;
      panel.hidden = sheet !== "usage";
      if (panel.hidden) return;
      const previousLeft = scroll.scrollLeft;
      const previousFocus = focus;
      data = rhythmData(range, api.hosts(), records, api);
      for (const [host, lane] of data.lanes) {
        if (!lane.stops.size) data.lanes.delete(host);
      }
      const { dates, lanes, totals, monthly, peak } = data;
      data.hosts = [...lanes.keys()];
      focus = rhythmFocus(data, focus);
      const summary = api.summary();
      shell.querySelectorAll("[data-rhythm-summary]").forEach((node, i) => { node.textContent = summary[i]; });
      panel.querySelector("#rhythm-title").textContent = label(trend ? "trendTitle" : "title");
      panel.querySelector("[data-rhythm-caption]").textContent =
        `${label(monthly ? "monthly" : "daily")} · ${label("zone")} · ${label("scale")}`;
      for (const node of shell.querySelectorAll("[data-rhythm-unit]")) node.textContent = api.unit();
      scroll.hidden = !data.periods.size;
      if (!data.periods.size) {
        board.replaceChildren();
      } else {
        board.style.setProperty("--rhythm-columns", dates.length);
        const axis = `<div class="rhythm-row">
          <div class="rhythm-row-label">${esc(label(trend ? "date" : "machines"))}</div>
          ${dates.map((date, i) => data.periods.has(date)
            ? `<button type="button" class="rhythm-date" data-column="${i}" data-station="-1:${i}" style="grid-column:${i + 2}">${esc(dateLabel(date))}</button>`
            : `<span class="rhythm-date" data-column="${i}" style="grid-column:${i + 2}">${esc(dateLabel(date))}</span>`).join("")}
          </div>`;
        if (trend) {
          const routes = [...lanes].map(([host, lane]) => {
            const color = hostColor(host);
            const y = date => 112 - 100 * ((lane.stops.get(date)?.amount || 0) / (peak || 1));
            const points = dates.length === 1 ? `0,${y(dates[0])} 1,${y(dates[0])}` :
              dates.map((date, i) => `${i + .5},${y(date)}`).join(" ");
            return `<polyline style="stroke:${color}" points="${points}"/>`;
          }).join("");
          const targets = dates.map((date, i) => data.periods.has(date) ?
            `<button type="button" class="rhythm-trend-point" data-station="-1:${i}" style="grid-column:${i + 2}"
              aria-label="${esc(`${dateLabel(date, true)} · ${amountLabel(totals.get(date))}`)}"></button>` : "").join("");
          board.innerHTML = `<div class="rhythm-row rhythm-trend-chart">
            <div class="rhythm-row-label"><strong>${esc(amountLabel(peak))}</strong><span>0</span></div>
            <svg class="rhythm-pulse" viewBox="0 0 ${dates.length} 124" preserveAspectRatio="none" aria-hidden="true">${routes}</svg>
            ${targets}</div>${axis}<div class="rhythm-trend-legend">${[...lanes.keys()].map(host =>
              `<span><i style="background:${hostColor(host)}"></i><span class="mask">${esc(host)}</span></span>`).join("")}</div>`;
        } else {
          const columns = new Map(dates.map((date, i) => [date, i]));
          const routes = [...lanes].map(([host, lane], index) => {
            const color = hostColor(host);
            const stops = [...lane.stops].sort(([a], [b]) => a.localeCompare(b)).map(([date, stop]) => {
              const i = columns.get(date);
              const height = peak ? 100 * stop.amount / peak : 0;
              const title = `${dateLabel(date, true)} · ${amountLabel(stop.amount)}`;
              return `<button type="button" class="rhythm-station" data-station="${index}:${i}" title="${esc(title)}"
                aria-label="${esc(title)}" aria-describedby="rhythm-lane-${index}" style="grid-column:${i + 2};--bar-height:${height}%"><span></span></button>`;
            }).join("");
            return `<div class="rhythm-row rhythm-lane" style="--route-color:${color}">
              <div class="rhythm-row-label" id="rhythm-lane-${index}"><strong><i aria-hidden="true"></i><span class="mask">${esc(host)}</span></strong></div>
              <span class="rhythm-rail" aria-hidden="true"></span>${stops}</div>`;
          }).join("");
          board.innerHTML = axis + routes;
        }
      }
      scroll.scrollLeft = previousLeft;
      paintFocus();
      if (focus && focus.date !== previousFocus?.date) revealFocus();
      syncOverflow();
    }
  };
}
