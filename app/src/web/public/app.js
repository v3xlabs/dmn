(function () {
  "use strict";

  const byId = (id) => document.getElementById(id);
  const tbody = byId("domain-rows");
  const rows = tbody ? Array.from(tbody.rows) : [];
  const drawer = byId("detail-drawer");
  const drawerContent = byId("drawer-content");
  const prevBtn = byId("drawer-prev-btn");
  const nextBtn = byId("drawer-next-btn");
  const calendarBtn = byId("calendar-btn");
  const calendarPopover = byId("calendar-popover");
  const providerTrigger = byId("provider-trigger");
  const providerMenu = byId("provider-menu");
  const providerOptions = providerMenu ? Array.from(providerMenu.querySelectorAll(".provider-option")) : [];
  const searchInput = byId("domain-search");
  const statusCells = Array.from(document.querySelectorAll(".totals-cell"));
  const sortHeaders = Array.from(document.querySelectorAll("th[aria-sort]"));
  const toast = byId("toast");

  const state = { provider: "", status: "all", search: "", sort: "expires", dir: 1, selected: null, highlight: 0 };

  const compareName = (a, b) => a.dataset.name.localeCompare(b.dataset.name);
  const comparators = {
    expires: (a, b) => a.dataset.index - b.dataset.index,
    name: compareName,
    provider: (a, b) => a.dataset.provider.localeCompare(b.dataset.provider) || compareName(a, b),
    changed: (a, b) => a.dataset.changed - b.dataset.changed || compareName(a, b),
  };

  function matches(row) {
    const query = state.search.trim().toLowerCase();
    if (state.provider && row.dataset.provider !== state.provider) return false;
    if (state.status !== "all" && !(state.status in row.dataset)) return false;
    return !query || row.dataset.search.includes(query);
  }

  function visibleRows() {
    return rows.filter(matches).sort((a, b) => state.dir * comparators[state.sort](a, b));
  }

  function render() {
    const visible = visibleRows();
    rows.forEach((row) => row.toggleAttribute("data-selected", row === state.selected));
    tbody.replaceChildren(...visible);
    byId("filtered-empty").hidden = visible.length > 0;
    sortHeaders.forEach((th) => {
      const active = th.querySelector(".th-sort").dataset.sort === state.sort;
      th.setAttribute("aria-sort", active ? (state.dir > 0 ? "ascending" : "descending") : "none");
    });
    return visible;
  }

  function showToast(message) {
    toast.textContent = message;
    toast.setAttribute("data-visible", "");
    clearTimeout(showToast.timer);
    showToast.timer = setTimeout(() => toast.removeAttribute("data-visible"), 2200);
  }

  function copy(text, message) {
    if (!navigator.clipboard) return showToast("Copy needs https");
    navigator.clipboard.writeText(text).then(() => showToast(message), () => showToast("Copy failed"));
  }

  function setOpen(trigger, panel, open) {
    trigger.setAttribute("aria-expanded", String(open));
    panel.setAttribute("data-open", String(open));
  }

  function setProviderMenu(open) {
    setOpen(providerTrigger, providerMenu, open);
    if (open) highlight(providerOptions.findIndex((option) => option.dataset.provider === state.provider));
  }

  function highlight(index) {
    state.highlight = (index + providerOptions.length) % providerOptions.length;
    providerOptions.forEach((option, i) => option.toggleAttribute("data-highlighted", i === state.highlight));
  }

  function selectProvider(option) {
    state.provider = option.dataset.provider;
    providerOptions.forEach((o) => o.setAttribute("aria-selected", String(o === option)));
    byId("provider-trigger-icon").replaceChildren(option.querySelector(".provider-icon svg").cloneNode(true));
    byId("provider-trigger-label").textContent = option.querySelector(".provider-option-label").textContent;
    byId("provider-trigger-count").textContent = option.querySelector(".provider-count").textContent;
    setProviderMenu(false);
    providerTrigger.focus();
    render();
  }

  function setStatus(status) {
    state.status = status;
    statusCells.forEach((cell) => cell.setAttribute("aria-pressed", String(cell.dataset.status === status)));
    render();
  }

  function showDetail(row) {
    state.selected = row;
    const template = byId("detail-" + row.dataset.index);
    drawerContent.replaceChildren(template.content.cloneNode(true));
    const visible = render();
    const position = visible.indexOf(row);
    prevBtn.disabled = position <= 0;
    nextBtn.disabled = position === -1 || position >= visible.length - 1;
  }

  function openDrawer(row) {
    showDetail(row);
    if (!drawer.open) {
      drawer.showModal();
      document.body.style.overflow = "hidden";
    }
    byId("drawer-close-btn").focus();
  }

  function stepDrawer(delta) {
    const visible = visibleRows();
    const next = visible[visible.indexOf(state.selected) + delta];
    if (next) showDetail(next);
  }

  const themeToggle = byId("theme-toggle");
  function applyTheme(dark) {
    document.documentElement.classList.toggle("dark", dark);
    themeToggle.setAttribute("aria-label", dark ? "Light mode" : "Dark mode");
  }
  applyTheme(document.documentElement.classList.contains("dark"));

  themeToggle.addEventListener("click", () => {
    const dark = !document.documentElement.classList.contains("dark");
    applyTheme(dark);
    try {
      localStorage.setItem("dmn-theme", dark ? "dark" : "light");
    } catch (e) {}
  });

  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (event) => {
    let saved = null;
    try {
      saved = localStorage.getItem("dmn-theme");
    } catch (e) {}
    if (!saved) applyTheme(event.matches);
  });

  const icsUrl = window.location.origin + "/api/domains.ics";
  byId("ics-url").textContent = icsUrl;
  calendarBtn.addEventListener("click", () => {
    if (providerTrigger) setProviderMenu(false);
    setOpen(calendarBtn, calendarPopover, calendarBtn.getAttribute("aria-expanded") !== "true");
  });
  byId("copy-ics-btn").addEventListener("click", () => {
    copy(icsUrl, "Calendar URL copied");
    setOpen(calendarBtn, calendarPopover, false);
  });

  document.addEventListener("click", (event) => {
    if (!calendarPopover.parentElement.contains(event.target)) setOpen(calendarBtn, calendarPopover, false);
    if (providerMenu && !providerMenu.parentElement.contains(event.target)) setProviderMenu(false);
  });

  drawer.addEventListener("click", (event) => {
    const copyBtn = event.target.closest(".copy-btn");
    if (copyBtn) copy(copyBtn.dataset.copy, "Copied");
    if (event.target === drawer) drawer.close();
  });
  drawer.addEventListener("close", () => {
    document.body.style.overflow = "";
    const row = state.selected;
    state.selected = null;
    render();
    row?.focus();
  });
  byId("drawer-close-btn").addEventListener("click", () => drawer.close());
  prevBtn.addEventListener("click", () => stepDrawer(-1));
  nextBtn.addEventListener("click", () => stepDrawer(1));

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && calendarBtn.getAttribute("aria-expanded") === "true") {
      setOpen(calendarBtn, calendarPopover, false);
      calendarBtn.focus();
    } else if (drawer.open && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
      event.preventDefault();
      stepDrawer(event.key === "ArrowUp" ? -1 : 1);
    } else if (event.key === "/" && searchInput && document.activeElement !== searchInput && !drawer.open) {
      event.preventDefault();
      searchInput.focus();
    }
  });

  if (!tbody) return;

  providerTrigger.addEventListener("click", () => {
    setOpen(calendarBtn, calendarPopover, false);
    setProviderMenu(providerTrigger.getAttribute("aria-expanded") !== "true");
  });
  providerOptions.forEach((option, index) => {
    option.addEventListener("click", () => selectProvider(option));
    option.addEventListener("mouseenter", () => highlight(index));
  });
  providerMenu.parentElement.addEventListener("keydown", (event) => {
    if (providerTrigger.getAttribute("aria-expanded") !== "true") return;
    if (event.key === "Escape") {
      setProviderMenu(false);
      providerTrigger.focus();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      highlight(state.highlight + (event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === "Enter" || event.key === " ") {
      selectProvider(providerOptions[state.highlight]);
    } else if (event.key === "Tab") {
      setProviderMenu(false);
      return;
    } else {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
  });

  statusCells.forEach((cell) => {
    cell.addEventListener("click", () => {
      const status = cell.dataset.status;
      setStatus(state.status === status ? "all" : status);
    });
  });

  sortHeaders.forEach((th) => {
    const button = th.querySelector(".th-sort");
    button.addEventListener("click", () => {
      const key = button.dataset.sort;
      state.dir = state.sort === key ? -state.dir : key === "changed" ? -1 : 1;
      state.sort = key;
      render();
    });
  });

  searchInput.addEventListener("input", () => {
    state.search = searchInput.value;
    render();
  });
  searchInput.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && searchInput.value) {
      searchInput.value = "";
      state.search = "";
      render();
    }
  });

  byId("reset-filters-btn").addEventListener("click", () => {
    searchInput.value = "";
    state.search = "";
    selectProvider(providerOptions[0]);
    setStatus("all");
  });

  tbody.addEventListener("click", (event) => {
    const row = event.target.closest(".domain-row");
    if (row) openDrawer(row);
  });
  tbody.addEventListener("keydown", (event) => {
    const row = event.target.closest(".domain-row");
    if (!row) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openDrawer(row);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const visible = visibleRows();
      visible[visible.indexOf(row) + (event.key === "ArrowDown" ? 1 : -1)]?.focus();
    }
  });
})();
