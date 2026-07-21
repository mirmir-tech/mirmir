const base = "/api/mirmir/v1";
const dashboardSchemaVersion = 4;
let csrf = null;
let updatesSocket = null;
let reconnectTimer = null;
let connectedOnce = false;
let waitingForServerNotified = false;
let startupToastState = null;
let localModels = [];
let currentConfiguration = null;
let loadSelector = null;
let loadGeneration = false;
let removeRepoId = null;
const removingModels = new Set();
let chatRunning = false;
let chatOperationId = null;
let chatMessages = [];
let chatImage = null;
let chatFollowing = true;
let catalogResults = null;
let catalogResultsQuery = "";
let catalogSearchTimer = null;
let catalogSearchController = null;
let catalogPageController = null;
let catalogPageLoading = false;
const activities = new Map();
const pendingModelOperations = new Map();
const initiatedModelOperations = new Set();
const optimisticModelStates = new Map();
const liveOperationStates = new Set(["queued", "running", "cancelling"]);
const maxImageBytes = 20 * 1024 * 1024;
const imageTypes = new Set(["image/png", "image/jpeg", "image/webp", "image/gif"]);
const imageTypeByExtension = new Map([
  ["png", "image/png"], ["jpg", "image/jpeg"], ["jpeg", "image/jpeg"],
  ["webp", "image/webp"], ["gif", "image/gif"],
]);

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/ui/sw.js?v=25", { scope: "/ui/" }).catch(() => {});
}

const byId = (id) => document.getElementById(id);
const text = (id, value) => { byId(id).textContent = value; };
const number = (value, digits = 1) => value == null ? "—" : Number(value).toFixed(digits);

const request = async (path, options = {}) => {
  const response = await fetch(`${base}${path}`, { credentials: "same-origin", ...options });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    const missingEndpoint = response.status === 404 && !body.error;
    const message = missingEndpoint
      ? `Dashboard API endpoint ${path.split("?")[0]} is unavailable. Restart MiRMiR from the current build and reload the dashboard.`
      : body.error?.message || `HTTP ${response.status}`;
    throw new Error(message);
  }
  return body;
};

const mutate = (path, body) => request(path, {
  method: "POST",
  headers: { "content-type": "application/json", "x-mirmir-csrf": csrf },
  body: JSON.stringify(body),
});

const showNotice = (message, error = false) => {
  const region = byId("toast-region");
  const toast = document.createElement("div");
  toast.className = error ? "toast error" : "toast";
  toast.setAttribute("role", error ? "alert" : "status");
  const content = document.createElement("span");
  content.textContent = message;
  const close = document.createElement("button");
  close.type = "button";
  close.setAttribute("aria-label", "Dismiss notification");
  close.textContent = "×";
  toast.append(content, close);
  region.appendChild(toast);
  while (region.children.length > 4) region.firstElementChild.remove();
  const dismiss = () => {
    if (!toast.isConnected || toast.classList.contains("dismissing")) return;
    toast.classList.add("dismissing");
    window.setTimeout(() => toast.remove(), 180);
  };
  const timeout = window.setTimeout(dismiss, error ? 8000 : 5000);
  close.addEventListener("click", () => {
    window.clearTimeout(timeout);
    dismiss();
  });
};

const bytes = (value) => {
  if (value == null) return "—";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = Number(value);
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; }
  return `${size.toFixed(unit > 1 ? 1 : 0)} ${units[unit]}`;
};

const duration = (milliseconds) => {
  const seconds = Math.floor(Number(milliseconds) / 1000);
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
};

const percent = (used, total) => total ? Math.min(100, Math.max(0, used / total * 100)) : 0;

const renderConnection = (state, detail = null) => {
  const indicator = byId("connection-state");
  const previous = indicator.dataset.state;
  indicator.dataset.state = state;
  indicator.textContent = state === "connected" ? "CONNECTED"
    : state === "connecting" ? "CONNECTING" : "DISCONNECTED";
  if (state === "connected") {
    waitingForServerNotified = false;
    if (previous === "disconnected") showNotice("Connection restored");
  } else if (state === "disconnected" && previous !== "disconnected") {
    showNotice(detail || "Connection lost. Reconnecting…", true);
  }
};

const renderWaitingForServer = () => {
  renderConnection("connecting");
  if (waitingForServerNotified) return;
  waitingForServerNotified = true;
  showNotice("Waiting for the MiRMiR server to become available…");
};

const renderStartup = (startup) => {
  const state = startup.phase === "failed" ? `failed:${startup.detail}`
    : startup.ready ? "ready" : "starting";
  if (state !== startupToastState) {
    startupToastState = state;
    if (startup.phase === "failed") showNotice(startup.detail, true);
    else showNotice(startup.ready ? "Runtime ready" : "Runtime is starting…");
  }
  if (localModels.length > 0) renderLocalModels({ models: localModels });
};

const renderOverview = (data) => {
  text("throughput", number(data.current_tokens_per_second ?? data.last_tokens_per_second));
  text("prefill", number(data.current_prefill_tokens_per_second));
  text("decode", number(data.current_decode_tokens_per_second));
  text("ttft", number(data.current_ttft_ms ?? data.last_ttft_ms));
  text("loaded-models", data.loaded_models);
  text("active-requests", data.active_requests);
  text("total-requests", data.total_requests);
  text("failed-requests", data.failed_requests);
  text("tokens", `${data.prompt_tokens} prompt / ${data.completion_tokens} completion`);
  text("uptime", duration(data.uptime_ms));
  text("stage", data.active_stage || "idle");
  const memoryUsed = data.host_total_memory_bytes == null || data.host_available_memory_bytes == null
    ? 0 : data.host_total_memory_bytes - data.host_available_memory_bytes;
  byId("memory").value = percent(memoryUsed, data.host_total_memory_bytes);
  text("memory-label", `${bytes(memoryUsed)} / ${bytes(data.host_total_memory_bytes)}`);
  byId("kv").value = percent(data.kv_used_blocks, data.kv_total_blocks);
  text("kv-label", `${data.kv_used_blocks} / ${data.kv_total_blocks}`);
  text("memory-source", data.memory_source || "Memory source unavailable");
  if (chatRunning) {
    text("chat-ttft", number(data.current_ttft_ms));
    text("chat-prefill", number(data.current_prefill_tokens_per_second));
    text("chat-decode", number(data.current_decode_tokens_per_second));
    text("chat-throughput", number(data.current_tokens_per_second));
    text("chat-state", data.active_stage || "working");
  }
};

const cell = (row, value, className = "") => {
  const item = document.createElement("td");
  item.textContent = value ?? "—";
  item.className = className;
  row.appendChild(item);
  return item;
};

const badge = (value) => {
  const item = document.createElement("span");
  item.className = `badge ${value}`;
  item.textContent = value;
  return item;
};

const action = (label, handler, disabled = false) => {
  const button = document.createElement("button");
  button.className = "action";
  button.type = "button";
  button.textContent = label;
  button.disabled = disabled;
  button.addEventListener("click", async () => {
    button.disabled = true;
    button.setAttribute("aria-busy", "true");
    try { await handler(); }
    catch (error) { showNotice(error.message, true); }
    finally {
      button.removeAttribute("aria-busy");
      if (button.isConnected) button.disabled = disabled;
    }
  });
  return button;
};

const icons = {
  load: '<circle cx="12" cy="12" r="9"/><path d="m10 8 6 4-6 4Z"/>',
  unload: '<circle cx="12" cy="12" r="9"/><rect x="9" y="9" width="6" height="6" rx="1"/>',
  trash: '<path d="M4 7h16M9 7V4h6v3m3 0-1 13H7L6 7m4 4v5m4-5v5"/>',
  download: '<path d="M12 3v12m0 0 4-4m-4 4-4-4M5 20h14"/>',
  stop: '<circle cx="12" cy="12" r="9"/><rect x="9" y="9" width="6" height="6" rx="1"/>',
  downloaded: '<circle cx="12" cy="12" r="9"/><path d="m8 12 2.5 2.5L16 9"/>',
  tools: '<path d="M14.7 6.3a4 4 0 0 0-5-5L12 3.6 9.6 6 7.3 3.7a4 4 0 0 0 5 5L4 17l3 3 8.3-8.3a4 4 0 0 0-.6-5.4Z"/>',
  thinking: '<path d="M9 18h6m-5 3h4"/><path d="M8.2 14.5A7 7 0 1 1 15.8 14.5C14.7 15.2 14 16 14 17h-4c0-1-.7-1.8-1.8-2.5Z"/>',
  vision: '<path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6S2 12 2 12Z"/><circle cx="12" cy="12" r="2.5"/>',
};

const icon = (name) => {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.innerHTML = icons[name];
  return svg;
};

const iconAction = (label, name, handler, disabled = false, className = "") => {
  const button = document.createElement("button");
  button.className = `icon-action ${className}`.trim();
  button.type = "button";
  button.disabled = disabled;
  button.dataset.action = name;
  button.setAttribute("aria-label", label);
  button.dataset.tooltip = label;
  button.appendChild(icon(name));
  button.addEventListener("click", async () => {
    button.disabled = true;
    try { await handler(); }
    catch (error) { showNotice(error.message, true); }
    finally { if (button.isConnected) button.disabled = disabled; }
  });
  return button;
};

const floatingTooltip = byId("floating-tooltip");
let floatingTooltipTarget = null;

const hideFloatingTooltip = () => {
  floatingTooltip.hidden = true;
  floatingTooltipTarget = null;
};

const showFloatingTooltip = (target) => {
  const message = target.dataset.tooltip;
  if (!message) return;
  floatingTooltipTarget = target;
  floatingTooltip.textContent = message;
  floatingTooltip.hidden = false;
  const anchor = target.getBoundingClientRect();
  const tooltip = floatingTooltip.getBoundingClientRect();
  const gap = 8;
  const below = anchor.bottom + gap + tooltip.height <= window.innerHeight - gap;
  const top = below ? anchor.bottom + gap : anchor.top - tooltip.height - gap;
  const centered = anchor.left + (anchor.width - tooltip.width) / 2;
  const left = Math.max(gap, Math.min(centered, window.innerWidth - tooltip.width - gap));
  floatingTooltip.style.left = `${left}px`;
  floatingTooltip.style.top = `${Math.max(gap, top)}px`;
};

document.addEventListener("pointerover", (event) => {
  const target = event.target.closest?.("[data-tooltip]");
  if (target) showFloatingTooltip(target);
});
document.addEventListener("pointerout", (event) => {
  if (floatingTooltipTarget && !floatingTooltipTarget.contains(event.relatedTarget)) hideFloatingTooltip();
});
document.addEventListener("focusin", (event) => {
  const target = event.target.closest?.("[data-tooltip]");
  if (target) showFloatingTooltip(target);
});
document.addEventListener("focusout", hideFloatingTooltip);

const runModelAction = async (path, payload, message) => {
  const target = payload.selector || payload.repo_id;
  const kind = path.split("/").at(-1);
  pendingModelOperations.set(target, {
    operation_id: `pending-${target}`,
    kind,
    target,
    state: "running",
    stage: "queued",
    detail: `${kind} request queued`,
    current: null,
    total: null,
    updated_at_unix_ms: Date.now(),
  });
  initiatedModelOperations.add(target);
  renderOperationTarget(target);
  try {
    await mutate(path, payload);
    showNotice(message);
  } catch (error) {
    pendingModelOperations.delete(target);
    initiatedModelOperations.delete(target);
    renderOperationTarget(target);
    throw error;
  }
};

const capability = (list, label, value) => {
  const term = document.createElement("dt");
  term.textContent = label;
  const detail = document.createElement("dd");
  detail.textContent = value;
  list.append(term, detail);
};

const renderLoadCapabilities = (inspection) => {
  const panel = byId("load-task-contract");
  const list = byId("load-task-capabilities");
  list.replaceChildren();
  panel.hidden = loadGeneration;
  if (loadGeneration) return;
  const task = inspection.task || "unknown";
  const capabilities = inspection.capabilities;
  capability(list, "Task", task);
  if (!capabilities) {
    capability(list, "Load settings", "none");
    return;
  }
  capability(list, task === "rerank" ? "Maximum pair" : "Maximum input",
    `${capabilities.max_input_tokens.toLocaleString()} tokens`);
  if (capabilities.embedding) {
    const embedding = capabilities.embedding;
    capability(list, "Output", `${embedding.native_dimensions.toLocaleString()} dimensions`);
    capability(list, "Pooling", `${embedding.pooling.replace("_", " ")} · ${embedding.normalized ? "normalized" : "not normalized"} · prompt ${embedding.includes_prompt ? "included" : "excluded"}`);
    const prompts = embedding.prompt_names.length === 0 ? "none"
      : `${embedding.prompt_names.join(", ")}${embedding.default_prompt ? ` · default ${embedding.default_prompt}` : ""}`;
    capability(list, "Prompt presets", prompts);
  }
  if (capabilities.rerank) {
    const rerank = capabilities.rerank;
    capability(list, "Classifier", `${rerank.labels} label${rerank.labels === 1 ? "" : "s"} · ${rerank.pooling} pooling`);
    capability(list, "Scores", rerank.raw_scores ? "relevance score · raw logits available" : "relevance score");
  }
};

const enableGenerationFields = (enabled) => {
  byId("load-generation-fields").querySelectorAll("input")
    .forEach((input) => { input.disabled = !enabled; });
};

const generationControls = [
  ["load-max-tokens", "load-max-tokens-range"],
  ["load-temperature", "load-temperature-range"],
  ["load-top-p", "load-top-p-range"],
  ["load-top-k", "load-top-k-range"],
  ["load-repetition", "load-repetition-range"],
];

const setGenerationControl = (numberId, rangeId, value) => {
  byId(numberId).value = value;
  byId(rangeId).value = value;
};

generationControls.forEach(([numberId, rangeId]) => {
  const numberInput = byId(numberId);
  const rangeInput = byId(rangeId);
  numberInput.addEventListener("input", () => {
    if (numberInput.value !== "" && numberInput.validity.valid) rangeInput.value = numberInput.value;
  });
  rangeInput.addEventListener("input", () => { numberInput.value = rangeInput.value; });
});

const setLoadButtonState = (state) => {
  const button = byId("confirm-load");
  const busy = state === "inspecting" || state === "submitting";
  button.disabled = state !== "ready";
  if (busy) button.setAttribute("aria-busy", "true");
  else button.removeAttribute("aria-busy");
  button.textContent = state === "inspecting" ? "Inspecting…"
    : state === "submitting" ? "Starting…" : "Load model";
};

const openLoadDialog = async (selector) => {
  loadSelector = selector;
  loadGeneration = false;
  text("load-target", selector);
  text("load-settings-kind", "Inspecting model");
  text("load-memory", "Inspecting model and memory…");
  byId("load-generation-fields").hidden = true;
  enableGenerationFields(false);
  byId("load-generation-note").hidden = true;
  byId("load-task-contract").hidden = true;
  setLoadButtonState("inspecting");
  byId("load-force").checked = false;
  byId("load-dialog").showModal();
  try {
    const inspection = await request(`/models/inspect?selector=${encodeURIComponent(selector)}`);
    const settings = inspection.settings;
    loadGeneration = inspection.task === "generation" || (!inspection.task && settings != null);
    if (loadGeneration && !settings) throw new Error("generation settings missing");
    if (loadGeneration) {
      const memory = inspection.memory;
      const tokenLimit = Math.max(settings.max_tokens,
        memory.max_safe_context_tokens || memory.configured_cache_tokens || 32768);
      byId("load-max-tokens").max = tokenLimit;
      byId("load-max-tokens-range").max = tokenLimit;
      setGenerationControl("load-max-tokens", "load-max-tokens-range", settings.max_tokens);
      setGenerationControl("load-temperature", "load-temperature-range", settings.temperature);
      setGenerationControl("load-top-p", "load-top-p-range", settings.top_p);
      setGenerationControl("load-top-k", "load-top-k-range", settings.top_k);
      setGenerationControl("load-repetition", "load-repetition-range", settings.repetition_penalty);
    }
    byId("load-generation-fields").hidden = !loadGeneration;
    enableGenerationFields(loadGeneration);
    byId("load-generation-note").hidden = !loadGeneration;
    renderLoadCapabilities(inspection);
    const task = inspection.task || (loadGeneration ? "generation" : "unknown");
    text("load-settings-kind", loadGeneration ? "Generation defaults" : `${task} model`);
    const memory = inspection.memory;
    const saved = loadGeneration
      ? ` · ${inspection.has_mirmir_overrides ? "saved MiRMiR defaults" : "model defaults"}` : "";
    text("load-memory", `${memory.fit} · ${bytes(memory.required_bytes)} required${saved} · ${memory.memory_source}`);
    setLoadButtonState("ready");
  } catch (error) {
    text("load-memory", error.message);
    setLoadButtonState("unavailable");
  }
};

const openRemoveDialog = (repoId) => {
  removeRepoId = repoId;
  text("remove-target", repoId);
  byId("remove-dialog").showModal();
};

const renderChatModels = (models) => {
  const select = byId("chat-model");
  const selected = select.value;
  const ready = models.filter((model) => model.state === "ready");
  select.replaceChildren();
  ready.forEach((model) => {
    const option = document.createElement("option");
    option.value = model.selector;
    option.textContent = model.id || model.repo_id;
    select.appendChild(option);
  });
  if (ready.some((model) => model.selector === selected)) select.value = selected;
  if (ready.length === 0) {
    const option = document.createElement("option");
    option.textContent = "No loaded models";
    option.disabled = true;
    option.selected = true;
    select.appendChild(option);
  }
  renderImageCapability();
};

const operationFor = (...targets) => {
  const targetSet = new Set(targets.filter(Boolean));
  const live = [...activities.values()]
    .filter((event) => targetSet.has(event.target) && liveOperationStates.has(event.state))
    .sort((left, right) => right.updated_at_unix_ms - left.updated_at_unix_ms)[0];
  if (live) return live;
  return targets.map((target) => pendingModelOperations.get(target)).find(Boolean) || null;
};

const pullOperations = () => {
  const operations = new Map();
  [...pendingModelOperations.values()]
    .filter((operation) => operation.kind === "pull")
    .forEach((operation) => operations.set(operation.target, operation));
  [...activities.values()]
    .filter((event) => event.kind === "pull" && liveOperationStates.has(event.state))
    .sort((left, right) => left.updated_at_unix_ms - right.updated_at_unix_ms)
    .forEach((event) => operations.set(event.target, event));
  return [...operations.values()];
};

const latestOperationFor = (...targets) => {
  const targetSet = new Set(targets.filter(Boolean));
  return [...activities.values()]
    .filter((event) => targetSet.has(event.target) && ["load", "restore", "unload", "pull"].includes(event.kind))
    .sort((left, right) => right.updated_at_unix_ms - left.updated_at_unix_ms)[0] || null;
};

const operationState = (operation) => {
  if (operation.kind === "pull") return "downloading";
  if (operation.kind === "unload") return "unloading";
  return "loading";
};

const progressDescription = (operation) => {
  const parts = [operation.detail || operation.stage || operation.state];
  if (operation.total) {
    const current = operation.current || 0;
    const remaining = Math.max(0, operation.total - current);
    const formatted = operation.kind === "pull" || operation.total > 1024 * 1024;
    parts.push(`${percent(current, operation.total).toFixed(1)}%`);
    parts.push(`${formatted ? bytes(remaining) : remaining.toLocaleString()} remaining`);
  }
  return parts.filter(Boolean).join(" · ");
};

const statePill = (state, detail = "", operation = null) => {
  const pill = document.createElement("span");
  pill.className = `state-pill ${state}`;
  const label = state.replaceAll("-", " ");
  pill.textContent = label;
  const tooltip = operation ? progressDescription(operation) : detail;
  if (tooltip) {
    pill.dataset.tooltip = tooltip;
    pill.setAttribute("aria-label", `${label}: ${tooltip}`);
    pill.tabIndex = 0;
  }
  if (operation && ["loading", "unloading", "downloading"].includes(state)) {
    const ring = document.createElement("span");
    ring.className = "progress-ring";
    if (operation.total) {
      ring.style.setProperty("--progress", `${percent(operation.current || 0, operation.total)}%`);
    } else {
      pill.classList.add("indeterminate");
    }
    pill.prepend(ring);
  }
  return pill;
};

const appendName = (row, name, detail) => {
  const item = cell(row, "");
  const content = document.createElement("span");
  content.className = "model-name";
  const title = document.createElement("strong");
  title.textContent = name;
  const source = document.createElement("small");
  source.textContent = detail || "local";
  content.append(title, source);
  item.appendChild(content);
};

const appendType = (row, value) => {
  const item = cell(row, "", "model-type");
  const type = document.createElement("span");
  const name = value || "Unknown";
  const kind = name.toLowerCase().replaceAll("_", "-").replaceAll(" ", "-");
  type.className = `type-pill ${kind}`;
  type.textContent = name;
  item.appendChild(type);
};

const appendFeatures = (row, model) => {
  const item = cell(row, "");
  const list = document.createElement("span");
  list.className = "feature-list";
  [["tool_use", "tools", "Tool use"], ["thinking", "thinking", "Thinking"], ["vision", "vision", "Vision"]]
    .filter(([property]) => model[property])
    .forEach(([, name, label]) => {
      const feature = document.createElement("span");
      feature.className = `feature-pill ${name}`;
      feature.dataset.tooltip = label;
      feature.setAttribute("aria-label", label);
      feature.tabIndex = 0;
      feature.appendChild(icon(name));
      list.appendChild(feature);
    });
  if (!list.children.length) {
    const empty = document.createElement("span");
    empty.className = "feature-empty";
    empty.textContent = "—";
    list.appendChild(empty);
  }
  item.appendChild(list);
};

const operationButton = (operation) => {
  const state = operationState(operation);
  if (operation.kind === "pull" && operation.operation_id && operation.cancellable) {
    const stopping = operation.state === "cancelling";
    return iconAction(stopping ? "Stopping download…" : "Stop download", "stop", async () => {
      const response = await mutate("/activity/cancel", { operation_id: operation.operation_id });
      if (!response.accepted) throw new Error(`Download cannot be stopped (${response.state})`);
    }, stopping, stopping ? "busy" : "stop");
  }
  const name = state === "unloading" ? "unload" : state === "downloading" ? "download" : "load";
  return iconAction(`${state}…`, name, () => {}, true, "busy");
};

const optimisticStateFor = (...identifiers) => identifiers
  .map((identifier) => optimisticModelStates.get(identifier))
  .find(Boolean);

const renderOperationTarget = (target) => {
  renderLocalModels({ models: localModels });
  if (catalogResults?.models.some((model) => model.id === target)) renderCatalog(catalogResults);
};

const appendPullRow = (rows, operation) => {
  const row = document.createElement("tr");
  row.className = "busy";
  appendName(row, operation.target, "Hugging Face");
  appendType(row, "Unknown");
  cell(row, bytes(operation.total), "model-size");
  appendFeatures(row, {});
  cell(row, "").appendChild(statePill("downloading", "", operation));
  const controls = cell(row, "", "model-actions");
  controls.appendChild(operationButton(operation));
  rows.appendChild(row);
};

const renderLocalModels = (data) => {
  localModels = data.models;
  renderChatModels(data.models);
  const rows = byId("model-rows");
  rows.replaceChildren();
  const downloads = pullOperations();
  const knownTargets = new Set(data.models.flatMap((model) =>
    [model.selector, model.id, model.repo_id, model.path].filter(Boolean)));
  downloads.filter((operation) => !knownTargets.has(operation.target))
    .forEach((operation) => appendPullRow(rows, operation));
  data.models.forEach((model) => {
    const row = document.createElement("tr");
    const removing = Boolean(model.repo_id && removingModels.has(model.repo_id));
    if (removing) {
      row.classList.add("removing");
      row.setAttribute("aria-busy", "true");
    }
    appendName(row, model.id || model.repo_id, model.repo_id || model.path);
    appendType(row, model.library);
    cell(row, bytes(model.size_bytes), "model-size");
    appendFeatures(row, model);
    const operation = operationFor(model.selector, model.id, model.repo_id, model.path);
    const failed = latestOperationFor(model.selector, model.id, model.repo_id, model.path);
    const optimistic = optimisticStateFor(model.selector, model.id, model.repo_id, model.path);
    const active = optimistic ? optimistic === "active" : model.state === "ready";
    const state = cell(row, "");
    if (removing) {
      state.appendChild(statePill("removing", "Removing local model…"));
    } else if (operation) {
      row.classList.add("busy");
      state.appendChild(statePill(operationState(operation), "", operation));
    } else if (model.state === "paused") {
      const reason = failed && ["failed", "rejected"].includes(failed.state)
        ? `${failed.detail} · ` : "";
      state.appendChild(statePill("paused", `${reason}${bytes(model.size_bytes)} kept on disk · resume or remove`));
    } else if (failed && ["failed", "rejected"].includes(failed.state)) {
      state.appendChild(statePill("error", failed.detail || "Model operation failed"));
    } else if (!model.loadable || model.state === "missing") {
      state.appendChild(statePill("error", model.load_unavailable_reason || "Model files are missing"));
    } else {
      state.appendChild(statePill(active ? "active" : "ready"));
    }
    const controls = cell(row, "", "model-actions");
    if (removing) {
      controls.appendChild(iconAction("Removing model…", "trash", () => {}, true, "busy removing"));
    } else if (operation) {
      controls.appendChild(operationButton(operation));
    } else if (model.state === "paused") {
      controls.appendChild(iconAction("Resume download", "download", () =>
        runModelAction("/models/pull", { repo_id: model.repo_id, revision: model.revision || null },
          `Resuming download for ${model.repo_id}`)));
      controls.appendChild(iconAction("Remove partial download", "trash", () =>
        openRemoveDialog(model.repo_id), false, "danger"));
    } else if (active) {
      controls.appendChild(iconAction("Unload model", "unload", () =>
        runModelAction("/models/unload", { selector: model.selector }, "Unload requested")));
    } else {
      controls.appendChild(iconAction("Load model", "load", () => openLoadDialog(model.selector),
        !model.loadable || (!optimistic && model.state !== "available")));
      if (model.repo_id) {
        controls.appendChild(iconAction("Remove model", "trash", () => openRemoveDialog(model.repo_id), false, "danger"));
      }
    }
    rows.appendChild(row);
  });
  const empty = byId("models-empty");
  empty.classList.remove("loading");
  empty.removeAttribute("aria-busy");
  empty.textContent = "No local models found.";
  empty.hidden = rows.children.length > 0;
};

const renderCatalog = (data) => {
  catalogResults = data;
  const rows = byId("catalog-rows");
  rows.replaceChildren();
  const showIncompatible = byId("show-incompatible").checked;
  const models = data.models.filter((model) => showIncompatible ||
    model.downloaded || model.local_source !== "remote" ||
    (model.compatibility !== "unsupported" && model.memory_fit !== "does_not_fit"));
  models.forEach((model) => {
    const row = document.createElement("tr");
    const localModel = localModels.find((local) => local.repo_id === model.id);
    const presented = localModel || model;
    appendName(row, model.id, `${model.downloads.toLocaleString()} downloads · ${model.likes.toLocaleString()} likes`);
    appendType(row, presented.library);
    cell(row, bytes(localModel?.size_bytes ?? model.estimated_weight_bytes), "model-size");
    appendFeatures(row, presented);
    const fit = cell(row, "");
    const operation = operationFor(model.id);
    const partial = localModel?.state === "paused" || model.local_source === "partial";
    const local = model.downloaded || model.local_source === "hf_cache";
    const blocked = model.compatibility === "unsupported" || model.memory_fit === "does_not_fit";
    if (operation) {
      row.classList.add("busy");
      fit.appendChild(statePill("downloading", "", operation));
    } else if (local) {
      fit.appendChild(statePill("downloaded", model.reason));
    } else if (partial) {
      fit.appendChild(statePill("paused", model.reason));
    } else {
      const fits = ["fits", "tight"].includes(model.memory_fit);
      fit.appendChild(statePill(blocked ? "does-not-fit" : fits ? "fits" : "fit-unknown", model.reason));
    }
    const controls = cell(row, "", "model-actions");
    if (operation) {
      controls.appendChild(operationButton(operation));
    } else if (local) {
      controls.appendChild(iconAction("Downloaded", "downloaded", () => {}, true, "downloaded"));
    } else {
      const label = partial ? "Resume download" : "Download model";
      controls.appendChild(iconAction(!partial && blocked ? model.reason : label, "download", () =>
        runModelAction("/models/pull", { repo_id: model.id },
          partial ? `Resuming download for ${model.id}` : `Download accepted for ${model.id}`),
        !partial && blocked));
    }
    rows.appendChild(row);
  });
  const count = models.length === data.models.length ? `${models.length} results` : `${models.length} of ${data.models.length} results`;
  byId("catalog-status").classList.remove("loading");
  byId("catalog-status").removeAttribute("aria-busy");
  text("catalog-status", `${count} · ${data.memory_source}`);
  byId("catalog-empty").hidden = models.length > 0;
  window.requestAnimationFrame(maybeLoadMoreCatalog);
};

const catalogRank = (model) => {
  if (model.compatibility === "supported") return { fits: 0, tight: 1, unknown: 2, does_not_fit: 3 }[model.memory_fit] ?? 4;
  return model.compatibility === "unknown" ? 4 : 5;
};

const editConfigurationValue = (row, setting) => {
  const valueCell = row.children[1];
  const actionCell = row.children[4];
  const input = document.createElement("input");
  input.className = "inline-input";
  const secret = setting.kind === "secret";
  const configured = setting.actions.includes("remove");
  input.type = secret ? "password" : "text";
  input.autocomplete = secret ? "new-password" : "off";
  input.value = secret ? "" : setting.value;
  if (secret) input.placeholder = configured ? "Replace stored value" : "Set value";
  input.setAttribute("aria-label", `New value for ${setting.key}`);
  valueCell.replaceChildren(input);
  actionCell.replaceChildren();
  const save = action("Save", async () => {
    if (secret && !input.value.trim()) throw new Error("Secret value cannot be empty");
    const response = await mutate("/configuration", {
      operation: "set_value", key: setting.key, value: input.value,
    });
    renderConfiguration(response.configuration);
    showNotice(response.restart_required ? `${response.message}; restart required` : response.message);
  });
  const cancel = document.createElement("button");
  cancel.className = "action secondary";
  cancel.type = "button";
  cancel.textContent = "Cancel";
  cancel.addEventListener("click", () => renderConfiguration(currentConfiguration));
  actionCell.append(save, cancel);
  input.focus();
  if (!secret) input.select();
};

const renderConfiguration = (configuration) => {
  currentConfiguration = configuration;
  const rows = byId("configuration-rows");
  rows.replaceChildren();
  configuration.values.forEach((setting) => {
    const row = document.createElement("tr");
    cell(row, setting.key, "mono");
    cell(row, setting.value, "mono config-value");
    cell(row, setting.source);
    cell(row, setting.restart_required ? "required" : "live");
    const controls = cell(row, "");
    if (setting.actions.includes("edit")) {
      controls.appendChild(action("Edit", () => editConfigurationValue(row, setting)));
    }
    if (setting.actions.includes("test")) {
      controls.appendChild(action("Test", async () => {
        const response = await updateConfiguration({ operation: "test_value", key: setting.key });
        return response;
      }));
    }
    if (setting.actions.includes("remove")) {
      const remove = action("Remove", () =>
        updateConfiguration({ operation: "remove_value", key: setting.key }));
      remove.classList.add("danger");
      controls.appendChild(remove);
    }
    rows.appendChild(row);
  });
  text("config-path", `config: ${configuration.config_path}`);
  text("secrets-path", `secrets: ${configuration.secrets_path}`);
  text("raw-config", configuration.raw_toml);
};

const updateConfiguration = async (payload) => {
  const response = await mutate("/configuration", payload);
  renderConfiguration(response.configuration);
  showNotice(response.restart_required ? `${response.message}; restart required` : response.message);
  return response;
};

const renderActivity = () => {
  const list = byId("activity-list");
  const empty = byId("activity-empty");
  list.replaceChildren();
  const sorted = [...activities.values()].sort((a, b) => b.updated_at_unix_ms - a.updated_at_unix_ms);
  sorted.forEach((event) => {
    const item = document.createElement("article");
    item.className = "activity-item";
    const head = document.createElement("div");
    head.className = "activity-head";
    head.appendChild(badge(event.state));
    const title = document.createElement("strong");
    title.textContent = `${event.kind} · ${event.target}`;
    head.appendChild(title);
    const time = document.createElement("time");
    time.textContent = new Date(event.updated_at_unix_ms).toLocaleTimeString();
    head.appendChild(time);
    item.appendChild(head);
    const meta = document.createElement("div");
    meta.className = "activity-meta";
    const detail = document.createElement("span");
    detail.textContent = `${event.stage} · ${event.detail}`;
    meta.appendChild(detail);
    if (event.cancellable && liveOperationStates.has(event.state)) {
      meta.appendChild(action("Cancel", async () => {
        const response = await mutate("/activity/cancel", { operation_id: event.operation_id });
        showNotice(response.accepted ? "Cancellation requested" : `Cancellation not accepted (${response.state})`);
      }, event.state === "cancelling"));
    }
    item.appendChild(meta);
    if (event.total) {
      const progress = document.createElement("progress");
      progress.className = "activity-progress";
      progress.max = event.total;
      progress.value = event.current || 0;
      item.appendChild(progress);
    }
    list.appendChild(item);
  });
  if (sorted.length === 0) list.appendChild(empty);
};

const applyActivity = (event) => {
  activities.set(event.operation_id, event);
  pendingModelOperations.delete(event.target);
  if (event.state === "completed") {
    if (["load", "restore"].includes(event.kind)) optimisticModelStates.set(event.target, "active");
    if (event.kind === "unload") optimisticModelStates.set(event.target, "ready");
  }
  if (initiatedModelOperations.has(event.target) && !liveOperationStates.has(event.state)) {
    initiatedModelOperations.delete(event.target);
    if (event.state === "failed") {
      showNotice(`${event.kind} failed · ${event.detail}`, true);
    }
  }
  renderActivity();
  renderOperationTarget(event.target);
};

const appendInlineMarkdown = (parent, source) => {
  const pattern = /(`[^`\n]+`|\*\*[^*\n]+\*\*|\*[^*\n]+\*|\[[^\]\n]+\]\([^\s)]+\)|\n)/g;
  let offset = 0;
  for (const match of source.matchAll(pattern)) {
    parent.appendChild(document.createTextNode(source.slice(offset, match.index)));
    const token = match[0];
    let node;
    if (token === "\n") {
      node = document.createElement("br");
    } else if (token.startsWith("`")) {
      node = document.createElement("code");
      node.textContent = token.slice(1, -1);
    } else if (token.startsWith("**")) {
      node = document.createElement("strong");
      appendInlineMarkdown(node, token.slice(2, -2));
    } else if (token.startsWith("*")) {
      node = document.createElement("em");
      appendInlineMarkdown(node, token.slice(1, -1));
    } else {
      const parts = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      const href = parts?.[2] || "";
      if (/^https?:\/\//i.test(href)) {
        node = document.createElement("a");
        node.href = href;
        node.target = "_blank";
        node.rel = "noreferrer noopener";
        appendInlineMarkdown(node, parts[1]);
      } else {
        node = document.createTextNode(token);
      }
    }
    parent.appendChild(node);
    offset = match.index + token.length;
  }
  parent.appendChild(document.createTextNode(source.slice(offset)));
};

const tableCells = (line) => line.trim().replace(/^\||\|$/g, "").split("|").map((part) => part.trim());
const tableDivider = (line) => tableCells(line).every((part) => /^:?-{3,}:?$/.test(part));
const markdownBlock = (line) => /^(#{1,6}\s|```|~~~|>\s?|[-*+]\s|\d+\.\s|---+$)/.test(line.trim());

const renderMarkdown = (container, markdown = "") => {
  const lines = markdown.replaceAll("\r\n", "\n").split("\n");
  const fragment = document.createDocumentFragment();
  let index = 0;
  const inlineBlock = (tag, value, className = "") => {
    const block = document.createElement(tag);
    block.className = className;
    appendInlineMarkdown(block, value);
    fragment.appendChild(block);
  };
  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }
    const fence = line.trim().match(/^(```|~~~)([^\s]*)/);
    if (fence) {
      const code = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith(fence[1])) {
        code.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      const pre = document.createElement("pre");
      const body = document.createElement("code");
      const language = fence[2].replace(/[^a-z0-9_-]/gi, "");
      if (language) body.className = `language-${language}`;
      body.textContent = code.join("\n");
      pre.appendChild(body);
      fragment.appendChild(pre);
      continue;
    }
    if (line.includes("|") && lines[index + 1]?.includes("|") && tableDivider(lines[index + 1])) {
      const table = document.createElement("table");
      const head = document.createElement("thead");
      const headRow = document.createElement("tr");
      tableCells(line).forEach((value) => {
        const cell = document.createElement("th");
        appendInlineMarkdown(cell, value);
        headRow.appendChild(cell);
      });
      head.appendChild(headRow);
      table.appendChild(head);
      index += 2;
      const body = document.createElement("tbody");
      while (index < lines.length && lines[index].includes("|") && lines[index].trim()) {
        const row = document.createElement("tr");
        tableCells(lines[index]).forEach((value) => {
          const cell = document.createElement("td");
          appendInlineMarkdown(cell, value);
          row.appendChild(cell);
        });
        body.appendChild(row);
        index += 1;
      }
      table.appendChild(body);
      fragment.appendChild(table);
      continue;
    }
    const heading = line.trim().match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      inlineBlock(`h${Math.min(4, heading[1].length)}`, heading[2]);
      index += 1;
      continue;
    }
    if (/^---+$/.test(line.trim())) {
      fragment.appendChild(document.createElement("hr"));
      index += 1;
      continue;
    }
    if (/^>\s?/.test(line.trim())) {
      const quote = [];
      while (index < lines.length && /^>\s?/.test(lines[index].trim())) {
        quote.push(lines[index].trim().replace(/^>\s?/, ""));
        index += 1;
      }
      inlineBlock("blockquote", quote.join("\n"));
      continue;
    }
    const list = line.trim().match(/^([-*+]|\d+\.)\s+(.+)$/);
    if (list) {
      const ordered = /\d+\./.test(list[1]);
      const wrapper = document.createElement(ordered ? "ol" : "ul");
      const itemPattern = ordered ? /^\d+\.\s+(.+)$/ : /^[-*+]\s+(.+)$/;
      while (index < lines.length) {
        const item = lines[index].trim().match(itemPattern);
        if (!item) break;
        const child = document.createElement("li");
        appendInlineMarkdown(child, item[1]);
        wrapper.appendChild(child);
        index += 1;
      }
      fragment.appendChild(wrapper);
      continue;
    }
    const paragraph = [line.trim()];
    index += 1;
    while (index < lines.length && lines[index].trim() && !markdownBlock(lines[index])
      && !(lines[index].includes("|") && tableDivider(lines[index + 1] || ""))) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    inlineBlock("p", paragraph.join("\n"));
  }
  container.replaceChildren(fragment);
};

const attachmentChip = (attachment) => {
  const chip = document.createElement("span");
  chip.className = "message-attachment";
  const icon = document.createElement("span");
  icon.className = "file-icon";
  icon.setAttribute("aria-hidden", "true");
  const name = document.createElement("strong");
  name.textContent = attachment.name;
  chip.append(icon, name);
  return chip;
};

const updateReasoningState = (assistant, active, available = true) => {
  assistant.reasoning.hidden = !available;
  assistant.message.classList.toggle("is-thinking", active);
  assistant.reasoningLabel.textContent = active ? "Thinking" : "Thought process";
  assistant.reasoningDots.hidden = !active;
};

const scrollChatToBottom = (force = false) => window.requestAnimationFrame(() => {
  if (!force && !chatFollowing) return;
  const log = byId("chat-log");
  log.scrollTop = log.scrollHeight;
});

const chatNode = (role, content = "", options = {}) => {
  byId("chat-empty")?.remove();
  const message = document.createElement("article");
  message.className = `chat-message ${role}`;
  if (role === "assistant") message.classList.add("pending");
  const label = document.createElement("span");
  label.className = "role";
  label.textContent = role === "assistant" ? "MiRMiR" : "You";
  const bubble = document.createElement("div");
  bubble.className = "chat-bubble";
  const reasoning = document.createElement("details");
  reasoning.className = "reasoning";
  reasoning.hidden = true;
  const reasoningSummary = document.createElement("summary");
  const reasoningLabel = document.createElement("span");
  reasoningLabel.className = "thinking-label";
  const reasoningDots = document.createElement("span");
  reasoningDots.className = "thinking-dots";
  reasoningSummary.append(reasoningLabel, reasoningDots);
  const reasoningBody = document.createElement("div");
  reasoningBody.className = "reasoning-bubble markdown-content";
  reasoning.append(reasoningSummary, reasoningBody);
  const body = document.createElement("div");
  body.className = "message-content markdown-content";
  renderMarkdown(body, content);
  bubble.append(reasoning, body);
  if (options.attachment) bubble.appendChild(attachmentChip(options.attachment));
  message.append(label, bubble);
  byId("chat-log").appendChild(message);
  const result = { message, reasoning, reasoningBody, reasoningLabel, reasoningDots, body };
  if (role === "assistant" && options.thinking) updateReasoningState(result, true);
  reasoning.addEventListener("toggle", () => scrollChatToBottom());
  scrollChatToBottom(true);
  return result;
};

const consumeSse = async (response, handler) => {
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    buffer += decoder.decode(value || new Uint8Array(), { stream: !done }).replaceAll("\r\n", "\n");
    let boundary = buffer.indexOf("\n\n");
    while (boundary >= 0) {
      const block = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const lines = block.split("\n");
      const name = lines.find((line) => line.startsWith("event:"))?.slice(6).trim();
      const data = lines.filter((line) => line.startsWith("data:")).map((line) => line.slice(5).trimStart()).join("\n");
      if (name && data) await handler(name, JSON.parse(data));
      boundary = buffer.indexOf("\n\n");
    }
    if (done) break;
  }
};

const optionalNumber = (id) => byId(id).value === "" ? null : Number(byId(id).value);

const selectedChatModel = () => localModels.find((model) =>
  model.state === "ready" && model.selector === byId("chat-model").value);

const renderImageCapability = () => {
  const model = selectedChatModel();
  const imageReady = Boolean(model?.image_input);
  const blockedAttachment = Boolean(chatImage && model && !imageReady);
  byId("attach-chat-image").disabled = chatRunning || !imageReady;
  byId("send-chat").disabled = chatRunning || !model || blockedAttachment;
  if (!model) {
    text("chat-hint", "Load a model to start chatting");
  } else if (!imageReady) {
    const reason = model.image_unavailable_reason || "image input is unavailable";
    text("chat-hint", chatImage ? `Cannot send image · ${reason}` : `Text only · ${reason}`);
  } else {
    text("chat-hint", "Drop an image here · Enter sends · Shift+Enter adds a line");
  }
};

const renderChatImage = () => {
  const attachment = byId("chat-attachment");
  attachment.hidden = !chatImage;
  if (!chatImage) {
    byId("chat-image-input").value = "";
    return;
  }
  text("chat-image-name", chatImage.name);
  text("chat-image-meta", `image · ${bytes(chatImage.size)}`);
};

const readImage = (file) => new Promise((resolve, reject) => {
  const reader = new FileReader();
  reader.addEventListener("load", () => resolve(reader.result));
  reader.addEventListener("error", () => reject(new Error(`Could not read ${file.name}`)));
  reader.readAsDataURL(file);
});

const attachChatImage = async (file) => {
  if (!file || chatRunning) return;
  const model = selectedChatModel();
  if (!model?.image_input) {
    throw new Error(model?.image_unavailable_reason || "Selected model cannot accept images");
  }
  const extension = file.name.split(".").pop().toLowerCase();
  const mime = imageTypes.has(file.type) ? file.type : imageTypeByExtension.get(extension);
  if (!mime) throw new Error("Choose a PNG, JPEG, WebP, or GIF image");
  if (file.size > maxImageBytes) throw new Error("Image exceeds the 20 MiB limit");
  const dataUrl = String(await readImage(file)).replace(/^data:[^;]*;/, `data:${mime};`);
  chatImage = { name: file.name, size: file.size, dataUrl };
  renderChatImage();
  renderImageCapability();
};

const runChat = async (prompt) => {
  const model = byId("chat-model").value;
  if (!model || chatRunning) return;
  const selected = selectedChatModel();
  if (chatImage && !selected?.image_input) {
    throw new Error(selected?.image_unavailable_reason || "Selected model cannot accept images");
  }
  const submittedImage = chatImage;
  const requestMessages = [...chatMessages, { role: "user", content: prompt }];
  chatMessages.push({ role: "user", content: prompt });
  chatNode("user", prompt, { attachment: submittedImage });
  const assistant = chatNode("assistant", "", { thinking: selected?.thinking });
  chatImage = null;
  renderChatImage();
  chatRunning = true;
  chatOperationId = null;
  text("chat-state", "starting");
  byId("chat-input").disabled = true;
  byId("attach-chat-image").disabled = true;
  byId("remove-chat-image").disabled = true;
  byId("send-chat").disabled = true;
  byId("cancel-chat").disabled = false;
  byId("cancel-chat").hidden = false;
  const response = await fetch(`${base}/chat`, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json", "x-mirmir-csrf": csrf },
    body: JSON.stringify({
      model,
      messages: requestMessages,
      max_tokens: optionalNumber("chat-max-tokens"),
      temperature: optionalNumber("chat-temperature"),
      top_p: optionalNumber("chat-top-p"),
      top_k: optionalNumber("chat-top-k"),
      repetition_penalty: optionalNumber("chat-repetition"),
      seed: optionalNumber("chat-seed"),
      image: submittedImage?.dataUrl ?? null,
    }),
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(body.error?.message || `HTTP ${response.status}`);
  }
  let completion = null;
  let streamedText = "";
  let streamedReasoning = "";
  await consumeSse(response, async (event, data) => {
    if (event === "started") chatOperationId = data.operation_id;
    if (event === "token") {
      if (data.reasoning) {
        streamedReasoning += data.text;
        updateReasoningState(assistant, true);
        renderMarkdown(assistant.reasoningBody, streamedReasoning);
      } else {
        streamedText += data.text;
        assistant.message.classList.remove("pending");
        updateReasoningState(assistant, false, Boolean(streamedReasoning));
        renderMarkdown(assistant.body, streamedText);
      }
      scrollChatToBottom();
    }
    if (event === "completion") {
      completion = data;
      assistant.message.classList.remove("pending");
      renderMarkdown(assistant.body, data.text);
      renderMarkdown(assistant.reasoningBody, data.reasoning);
      updateReasoningState(assistant, false, Boolean(data.reasoning));
      text("chat-ttft", number(data.ttft_ms));
      text("chat-prefill", number(data.prefill_tokens_per_second));
      text("chat-decode", number(data.decode_tokens_per_second));
      text("chat-throughput", number(data.tokens_per_second));
      text("chat-state", data.finish_reason);
    }
    if (event === "error") throw new Error(data.message);
  });
  if (completion) {
    chatMessages.push({
      role: "assistant",
      content: completion.text,
      reasoning_content: completion.reasoning || null,
    });
  }
};

const finishChat = () => {
  chatRunning = false;
  chatOperationId = null;
  byId("chat-input").disabled = false;
  byId("remove-chat-image").disabled = false;
  byId("cancel-chat").disabled = true;
  byId("cancel-chat").hidden = true;
  renderChatModels(localModels);
  byId("chat-input").focus();
};

const applyModels = (data) => {
  optimisticModelStates.clear();
  localModels = data.models;
  renderChatModels(data.models);
  renderLocalModels(data);
  if (catalogResults) {
    const downloaded = new Set(data.models.map((model) => model.repo_id));
    catalogResults = {
      ...catalogResults,
      models: catalogResults.models.map((model) => ({
        ...model,
        downloaded: model.downloaded || downloaded.has(model.id),
      })),
    };
    renderCatalog(catalogResults);
  }
};

const applyUpdate = (message) => {
  if (message.type === "startup") renderStartup(message.startup);
  if (message.type === "overview") renderOverview(message.overview);
  if (message.type === "models") applyModels(message.models);
  if (message.type === "configuration") renderConfiguration(message.configuration);
  if (message.type === "activity") applyActivity(message.activity);
  if (message.type === "error") showNotice(message.message, true);
};

const connectUpdates = () => {
  const scheme = window.location.protocol === "https:" ? "wss" : "ws";
  updatesSocket = new WebSocket(`${scheme}://${window.location.host}${base}/ws`);
  updatesSocket.addEventListener("open", () => {
    if (reconnectTimer) window.clearTimeout(reconnectTimer);
    reconnectTimer = null;
    connectedOnce = true;
    renderConnection("connected");
  });
  updatesSocket.addEventListener("message", (event) => {
    try { applyUpdate(JSON.parse(event.data)); }
    catch (error) { showNotice(`Invalid runtime update: ${error.message}`, true); }
  });
  const connectionLost = () => {
    if (connectedOnce) renderConnection("disconnected");
    else renderWaitingForServer();
    scheduleReconnect();
  };
  updatesSocket.addEventListener("error", connectionLost);
  updatesSocket.addEventListener("close", connectionLost);
};

const sessionRequest = async (path, options = {}) => {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), 1500);
  try {
    return await request(path, { ...options, signal: controller.signal });
  } finally {
    window.clearTimeout(timeout);
  }
};

const establishSession = async () => {
  const bootstrap = await sessionRequest("/bootstrap");
  text("server-version", bootstrap.server_version);
  text("protocol-version", bootstrap.protocol_version);
  if (bootstrap.schema_version !== dashboardSchemaVersion) {
    const error = new Error(
      `Dashboard/server mismatch: UI schema ${dashboardSchemaVersion}, server schema ${bootstrap.schema_version ?? "unknown"}. Restart MiRMiR from the current build and reload the dashboard.`,
    );
    error.name = "DashboardCompatibilityError";
    throw error;
  }
  const session = await sessionRequest("/session", { method: "POST" });
  csrf = session.csrf_token;
};

const renderConnectionFailure = (error) => {
  if (error?.name === "DashboardCompatibilityError") {
    renderConnection("disconnected", error.message);
  } else if (connectedOnce) {
    renderConnection("disconnected");
  } else {
    renderWaitingForServer();
  }
};

const scheduleReconnect = () => {
  if (reconnectTimer) return;
  reconnectTimer = window.setTimeout(async () => {
    reconnectTimer = null;
    try {
      await establishSession();
      connectUpdates();
    } catch (error) {
      renderConnectionFailure(error);
      scheduleReconnect();
    }
  }, 1000);
};

const connect = async () => {
  await establishSession();
  connectUpdates();
};

const waitForServer = (error) => {
  renderConnectionFailure(error);
  scheduleReconnect();
};

document.querySelectorAll(".tab").forEach((tab) => tab.addEventListener("click", () => {
  document.querySelectorAll(".tab, .view").forEach((item) => item.classList.remove("active"));
  document.querySelectorAll(".tab").forEach((item) => item.removeAttribute("aria-current"));
  tab.classList.add("active");
  tab.setAttribute("aria-current", "page");
  byId(tab.dataset.view).classList.add("active");
  text("page-title", tab.dataset.label);
  document.title = `MiRMiR · ${tab.dataset.label}`;
}));

const searchCatalog = async () => {
  const query = byId("model-query").value.trim();
  const status = byId("catalog-status");
  if (!query) {
    catalogResults = null;
    catalogResultsQuery = "";
    byId("catalog-rows").replaceChildren();
    byId("catalog-empty").hidden = true;
    byId("catalog-more").hidden = true;
    status.classList.remove("loading");
    status.removeAttribute("aria-busy");
    text("catalog-status", "Start typing to search Hugging Face.");
    return;
  }
  status.classList.add("loading");
  status.setAttribute("aria-busy", "true");
  text("catalog-status", "Searching Hugging Face…");
  if (catalogSearchController) catalogSearchController.abort();
  if (catalogPageController) catalogPageController.abort();
  const controller = new AbortController();
  catalogSearchController = controller;
  try {
    const results = await request(`/catalog/search?query=${encodeURIComponent(query)}&limit=20`, {
      signal: controller.signal,
    });
    if (query === byId("model-query").value.trim()) {
      catalogResultsQuery = query;
      renderCatalog(results);
    }
  } catch (error) {
    if (error.name === "AbortError") return;
    if (query === byId("model-query").value.trim()) {
      status.classList.remove("loading");
      status.removeAttribute("aria-busy");
      text("catalog-status", error.message);
      showNotice(error.message, true);
    }
  } finally {
    if (catalogSearchController === controller) catalogSearchController = null;
  }
};

const scheduleCatalogSearch = () => {
  if (catalogSearchTimer) window.clearTimeout(catalogSearchTimer);
  if (catalogSearchController) catalogSearchController.abort();
  if (catalogPageController) catalogPageController.abort();
  if (!byId("model-query").value.trim()) {
    searchCatalog();
    return;
  }
  byId("catalog-status").classList.add("loading");
  byId("catalog-status").setAttribute("aria-busy", "true");
  text("catalog-status", "Searching Hugging Face…");
  catalogSearchTimer = window.setTimeout(() => {
    catalogSearchTimer = null;
    searchCatalog();
  }, 320);
};

const positionModelSearch = () => {
  const dialog = byId("model-search-dialog");
  if (!dialog.open) return;
  const field = byId("model-query").getBoundingClientRect();
  const left = Math.max(14, field.left);
  const top = field.bottom + 8;
  const width = Math.min(field.width, window.innerWidth - left - 14);
  dialog.style.setProperty("--catalog-left", `${left}px`);
  dialog.style.setProperty("--catalog-top", `${top}px`);
  dialog.style.setProperty("--catalog-width", `${width}px`);
  dialog.style.setProperty("--catalog-height", `${Math.max(240, window.innerHeight - top - 16)}px`);
};

const openModelSearch = () => {
  const dialog = byId("model-search-dialog");
  if (!dialog.open) dialog.show();
  byId("model-query").setAttribute("aria-expanded", "true");
  byId("close-model-search").hidden = false;
  positionModelSearch();
  window.requestAnimationFrame(() => byId("model-query").focus({ preventScroll: true }));
  const query = byId("model-query").value.trim();
  if (query && catalogResultsQuery !== query && !catalogSearchTimer && !catalogSearchController) {
    scheduleCatalogSearch();
  }
};

byId("model-query").addEventListener("click", openModelSearch);
byId("close-model-search").addEventListener("click", () => byId("model-search-dialog").close());
byId("model-search-dialog").addEventListener("close", () => {
  byId("model-query").setAttribute("aria-expanded", "false");
  byId("close-model-search").hidden = true;
  if (catalogSearchTimer) window.clearTimeout(catalogSearchTimer);
  catalogSearchTimer = null;
  if (catalogSearchController) catalogSearchController.abort();
  catalogSearchController = null;
  if (catalogPageController) catalogPageController.abort();
  catalogPageController = null;
  catalogPageLoading = false;
  byId("catalog-more").hidden = true;
});
document.addEventListener("keydown", (event) => {
  const dialog = byId("model-search-dialog");
  if (event.key === "Escape" && dialog.open) {
    event.preventDefault();
    dialog.close();
  }
});
document.addEventListener("pointerdown", (event) => {
  const dialog = byId("model-search-dialog");
  if (dialog.open && !dialog.contains(event.target) && !byId("model-search-control").contains(event.target)) {
    dialog.close();
  }
});
window.addEventListener("resize", positionModelSearch);
window.addEventListener("scroll", positionModelSearch, true);
window.addEventListener("resize", hideFloatingTooltip);
window.addEventListener("scroll", hideFloatingTooltip, true);
byId("model-query").addEventListener("input", () => {
  openModelSearch();
  scheduleCatalogSearch();
});
const loadMoreCatalog = async () => {
  if (catalogPageLoading || !catalogResults?.next_cursor) return;
  const query = byId("model-query").value.trim();
  if (!query || query !== catalogResultsQuery) return;
  catalogPageLoading = true;
  byId("catalog-more").hidden = false;
  const controller = new AbortController();
  catalogPageController = controller;
  try {
    const cursor = catalogResults.next_cursor;
    const page = await request(`/catalog/search?query=${encodeURIComponent(query)}&limit=20&cursor=${encodeURIComponent(cursor)}`, {
      signal: controller.signal,
    });
    if (query !== byId("model-query").value.trim()) return;
    const merged = new Map([...catalogResults.models, ...page.models].map((model) => [model.id, model]));
    const models = [...merged.values()].sort((left, right) =>
      catalogRank(left) - catalogRank(right) || right.downloads - left.downloads || left.id.localeCompare(right.id));
    renderCatalog({ ...page, models });
  } catch (error) {
    if (error.name === "AbortError") return;
    showNotice(error.message, true);
  } finally {
    if (catalogPageController === controller) {
      catalogPageController = null;
      catalogPageLoading = false;
      byId("catalog-more").hidden = true;
      window.requestAnimationFrame(maybeLoadMoreCatalog);
    }
  }
};
const maybeLoadMoreCatalog = () => {
  const list = byId("catalog-table");
  if (list.scrollHeight - list.scrollTop - list.clientHeight <= 160) loadMoreCatalog();
};
byId("catalog-table").addEventListener("scroll", maybeLoadMoreCatalog);
byId("show-incompatible").addEventListener("change", () => {
  if (catalogResults) renderCatalog(catalogResults);
});

byId("cancel-load").addEventListener("click", () => byId("load-dialog").close());
byId("load-model-form").addEventListener("invalid", (event) => {
  showNotice(event.target.validationMessage || "Invalid model setting", true);
}, true);
byId("load-model-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!loadSelector) return;
  const settings = loadGeneration ? {
    max_tokens: Number(byId("load-max-tokens").value),
    temperature: Number(byId("load-temperature").value),
    top_p: Number(byId("load-top-p").value),
    top_k: Number(byId("load-top-k").value),
    repetition_penalty: Number(byId("load-repetition").value),
  } : null;
  setLoadButtonState("submitting");
  try {
    const model = localModels.find((candidate) => candidate.selector === loadSelector);
    await runModelAction("/models/load", {
      selector: loadSelector,
      settings,
      config_id: model?.id || "",
      repo_id: model?.repo_id || "",
      revision: model?.revision || "",
      commit: model?.commit || "",
      force: byId("load-force").checked,
    }, "Load accepted; saved settings will be reused");
    byId("load-dialog").close();
  } catch (error) {
    setLoadButtonState("ready");
    showNotice(error.message, true);
  }
});

byId("cancel-remove").addEventListener("click", () => byId("remove-dialog").close());
byId("remove-model-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!removeRepoId) return;
  const repoId = removeRepoId;
  removeRepoId = null;
  removingModels.add(repoId);
  byId("remove-dialog").close();
  renderLocalModels({ models: localModels });
  try {
    const response = await mutate("/models/remove", { repo_id: repoId });
    if (response.removed) localModels = localModels.filter((model) => model.repo_id !== repoId);
    showNotice(response.removed ? `Removed ${repoId}; freed ${bytes(response.freed_bytes)}` : `${repoId} was not found`);
  } catch (error) {
    showNotice(error.message, true);
  } finally {
    removingModels.delete(repoId);
    renderLocalModels({ models: localModels });
  }
});

byId("chat-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const input = byId("chat-input");
  const prompt = input.value.trim();
  if (!prompt || chatRunning) return;
  input.value = "";
  input.style.height = "auto";
  try { await runChat(prompt); }
  catch (error) {
    if (chatMessages.at(-1)?.role === "user") chatMessages.pop();
    const assistant = byId("chat-log").querySelector(".chat-message.assistant:last-child");
    if (assistant) {
      assistant.classList.remove("pending", "is-thinking");
      assistant.querySelector(".reasoning").hidden = true;
      renderMarkdown(assistant.querySelector(".message-content"), `**Error:** ${error.message}`);
    }
    showNotice(error.message, true);
    text("chat-state", "failed");
  }
  finally { finishChat(); }
});

byId("attach-chat-image").addEventListener("click", () => byId("chat-image-input").click());
byId("chat-image-input").addEventListener("change", async (event) => {
  try { await attachChatImage(event.target.files[0]); }
  catch (error) { showNotice(error.message, true); }
  finally { event.target.value = ""; }
});
byId("remove-chat-image").addEventListener("click", () => {
  chatImage = null;
  renderChatImage();
  renderImageCapability();
});
byId("chat-model").addEventListener("change", renderImageCapability);

const chatForm = byId("chat-form");
byId("chat-log").addEventListener("scroll", (event) => {
  const log = event.currentTarget;
  chatFollowing = log.scrollHeight - log.scrollTop - log.clientHeight < 72;
});
["dragenter", "dragover"].forEach((name) => chatForm.addEventListener(name, (event) => {
  event.preventDefault();
  if (!chatRunning && selectedChatModel()?.image_input) chatForm.classList.add("drag-active");
}));
["dragleave", "drop"].forEach((name) => chatForm.addEventListener(name, (event) => {
  event.preventDefault();
  chatForm.classList.remove("drag-active");
}));
chatForm.addEventListener("drop", async (event) => {
  try {
    if (event.dataTransfer.files.length !== 1) throw new Error("Drop exactly one image");
    await attachChatImage(event.dataTransfer.files[0]);
  } catch (error) { showNotice(error.message, true); }
});

byId("chat-input").addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    byId("chat-form").requestSubmit();
  }
});
byId("chat-input").addEventListener("input", (event) => {
  event.target.style.height = "auto";
  event.target.style.height = `${Math.min(event.target.scrollHeight, 180)}px`;
});

byId("cancel-chat").addEventListener("click", async () => {
  if (!chatOperationId) return;
  try {
    const response = await mutate("/activity/cancel", { operation_id: chatOperationId });
    text("chat-state", response.accepted ? "cancelling" : response.state);
  } catch (error) { showNotice(error.message, true); }
});

connect().catch(waitForServer);
