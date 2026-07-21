const base = "/api/mirmir/v1";
const dashboardSchemaVersion = 3;
let csrf = null;
let updatesSocket = null;
let reconnectTimer = null;
let connectedOnce = false;
let runtimeReady = false;
let localModels = [];
let currentConfiguration = null;
let loadSelector = null;
let loadGeneration = false;
let removeRepoId = null;
let chatRunning = false;
let chatOperationId = null;
let chatMessages = [];
let chatImage = null;
let searching = false;
let catalogResults = null;
const activities = new Map();
const pendingModelOperations = new Map();
const initiatedModelOperations = new Set();
const liveOperationStates = new Set(["queued", "running", "cancelling"]);
const maxImageBytes = 20 * 1024 * 1024;
const imageTypes = new Set(["image/png", "image/jpeg", "image/webp", "image/gif"]);
const imageTypeByExtension = new Map([
  ["png", "image/png"], ["jpg", "image/jpeg"], ["jpeg", "image/jpeg"],
  ["webp", "image/webp"], ["gif", "image/gif"],
]);

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/ui/sw.js?v=7", { scope: "/ui/" }).catch(() => {});
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
  const notice = byId("notice");
  notice.textContent = message;
  notice.className = error ? "notice error" : "notice";
  notice.hidden = false;
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

const renderConnection = (state, detail = null, phase = "reconnecting") => {
  const indicator = byId("connection-state");
  indicator.dataset.state = state;
  indicator.textContent = state === "connected" ? "CONNECTED"
    : state === "connecting" ? "CONNECTING" : "DISCONNECTED";
  if (state !== "disconnected") return;
  runtimeReady = false;
  const banner = byId("startup-banner");
  banner.hidden = false;
  banner.classList.add("connection-lost");
  text("startup-title", "Connection lost");
  text("startup-detail", detail || "The dashboard is no longer receiving updates from the MiRMiR server.");
  text("startup-phase", phase);
  byId("startup-progress").hidden = true;
};

const renderWaitingForServer = () => {
  renderConnection("connecting");
  runtimeReady = false;
  const banner = byId("startup-banner");
  banner.hidden = false;
  banner.classList.remove("connection-lost");
  text("startup-title", "Starting runtime");
  text("startup-detail", "Waiting for the MiRMiR server to become available…");
  text("startup-phase", "connecting");
  byId("startup-progress").hidden = true;
};

const renderStartup = (startup) => {
  runtimeReady = startup.ready;
  const banner = byId("startup-banner");
  banner.classList.remove("connection-lost");
  banner.hidden = startup.ready;
  text("startup-title", startup.ready ? "Runtime ready" : `Waiting for ${startup.target}`);
  text("startup-detail", startup.detail);
  text("startup-phase", startup.phase);
  const progress = byId("startup-progress");
  progress.hidden = startup.total == null || startup.total === 0;
  progress.max = startup.total || 1;
  progress.value = startup.current || 0;
  if (startup.phase === "failed") showNotice(startup.detail, true);
  if (!searching && localModels.length > 0) renderLocalModels({ models: localModels });
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
  text("updated", `sample ${new Date(data.sampled_at_unix_ms).toLocaleTimeString()}`);
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
      byId("load-max-tokens").value = settings.max_tokens;
      byId("load-temperature").value = settings.temperature;
      byId("load-top-p").value = settings.top_p;
      byId("load-top-k").value = settings.top_k;
      byId("load-repetition").value = settings.repetition_penalty;
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
    option.textContent = `${model.id || model.repo_id} · ${model.image_input ? "image ready" : "no image"}`;
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

const operationControl = (operation) => {
  if (!operation.cancellable || !liveOperationStates.has(operation.state)) {
    return action(`${operation.kind}…`, () => {}, true);
  }
  return action("Cancel", async () => {
    const response = await mutate("/activity/cancel", { operation_id: operation.operation_id });
    showNotice(response.accepted
      ? "Cancellation requested; partial download will be removed"
      : `Cancellation not accepted (${response.state})`);
  }, operation.state === "cancelling");
};

const operationProgress = (container, operation) => {
  if (!operation) return;
  const progress = document.createElement("progress");
  progress.className = "model-progress";
  if (operation.total) {
    progress.max = operation.total;
    progress.value = operation.current || 0;
  }
  container.appendChild(progress);
};

const renderOperationTarget = (target) => {
  if (searching && catalogResults?.models.some((model) => model.id === target)) {
    renderCatalog(catalogResults);
  } else if (!searching) {
    renderLocalModels({ models: localModels });
  }
};

const appendPullRow = (rows, operation) => {
  const row = document.createElement("tr");
  row.className = "busy";
  cell(row, operation.target);
  const state = cell(row, "");
  const stateBadge = badge(operation.state === "queued" ? "queued" : "loading");
  stateBadge.textContent = operation.stage || operation.state;
  state.appendChild(stateBadge);
  operationProgress(state, operation);
  cell(row, "pending");
  cell(row, operation.detail || "Downloading from Hugging Face", "path");
  const controls = cell(row, "");
  controls.appendChild(operationControl(operation));
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
    cell(row, model.id || model.repo_id);
    const state = cell(row, "");
    const operation = operationFor(model.selector, model.id, model.repo_id, model.path);
    if (operation) {
      row.classList.add("busy");
      const stateBadge = badge(operation.state === "queued" ? "queued" : "loading");
      stateBadge.textContent = operation.stage || operation.state;
      state.appendChild(stateBadge);
      operationProgress(state, operation);
    } else {
      state.appendChild(badge(model.state === "available" && !model.loadable ? "unavailable" : model.state));
    }
    cell(row, model.model_class || "unknown").title = model.revision || "local";
    const unavailable = model.state === "available" && !model.loadable;
    const detail = unavailable ? model.load_unavailable_reason : (model.repo_id || model.path);
    cell(row, detail, "path").title = unavailable
      ? `${model.load_unavailable_reason}\n${model.path}`
      : model.path;
    const controls = cell(row, "");
    if (operation) {
      controls.appendChild(operationControl(operation));
    } else if (model.state === "ready") {
      controls.appendChild(action("Unload", () => runModelAction("/models/unload", { selector: model.selector }, "Unload requested")));
    } else if (model.state === "missing") {
      if (model.repo_id) controls.appendChild(action("Remove", () => openRemoveDialog(model.repo_id)));
    } else {
      if (unavailable) {
        controls.appendChild(action("Why?", () => showNotice(model.load_unavailable_reason, true)));
      }
      const load = action("Load", () => openLoadDialog(model.selector), unavailable || model.state !== "available" || !runtimeReady);
      if (unavailable) load.title = model.load_unavailable_reason;
      controls.appendChild(load);
      if (model.repo_id) {
        controls.appendChild(action("Remove", () => openRemoveDialog(model.repo_id), model.state !== "available"));
      }
    }
    rows.appendChild(row);
  });
  const missing = data.models.filter((model) => model.state === "missing").length;
  const summary = [`${data.models.length - missing} local`];
  const queued = downloads.filter((operation) => operation.state === "queued").length;
  const downloading = downloads.length - queued;
  if (downloading > 0) summary.push(`${downloading} downloading`);
  if (queued > 0) summary.push(`${queued} queued`);
  if (missing > 0) summary.push(`${missing} missing`);
  text("model-count", summary.join(" · "));
  byId("models-empty").hidden = rows.children.length > 0;
};

const renderCatalog = (data) => {
  catalogResults = data;
  const rows = byId("model-rows");
  rows.replaceChildren();
  const showIncompatible = byId("show-incompatible").checked;
  const models = data.models.filter((model) => showIncompatible ||
    model.downloaded || model.local_source !== "remote" ||
    (model.compatibility !== "unsupported" && model.memory_fit !== "does_not_fit"));
  models.forEach((model) => {
    const row = document.createElement("tr");
    const localModel = localModels.find((local) => local.repo_id === model.id);
    cell(row, model.id);
    const fit = cell(row, "");
    const operation = operationFor(model.id);
    if (operation) {
      row.classList.add("busy");
      const stateBadge = badge(operation.state === "queued" ? "queued" : "loading");
      stateBadge.textContent = operation.stage || operation.state;
      fit.appendChild(stateBadge);
      operationProgress(fit, operation);
    } else {
      fit.appendChild(badge(localModel && !localModel.loadable ? "unavailable" : (model.downloaded ? "downloaded" : model.memory_fit)));
    }
    if (model.gated) fit.appendChild(badge("gated"));
    cell(row, model.model_class || "unknown");
    const popularity = `${model.downloads.toLocaleString()} downloads · ${model.likes.toLocaleString()} likes`;
    const reason = localModel && !localModel.loadable ? localModel.load_unavailable_reason : model.reason;
    cell(row, reason || model.local_source, "path").title = `${reason}\n${popularity}`;
    const controls = cell(row, "");
    const local = model.downloaded || model.local_source === "hf_cache";
    const blocked = model.compatibility === "unsupported" || model.memory_fit === "does_not_fit";
    if (operation) {
      controls.appendChild(operationControl(operation));
    } else if (local) {
      if (localModel && !localModel.loadable) {
        controls.appendChild(action("Why?", () => showNotice(localModel.load_unavailable_reason, true)));
      }
      controls.appendChild(action("Remove", () => openRemoveDialog(model.id)));
    } else {
      controls.appendChild(action("Download", () =>
        runModelAction("/models/pull", { repo_id: model.id }, `Download accepted for ${model.id}`), blocked));
    }
    rows.appendChild(row);
  });
  const count = models.length === data.models.length ? `${models.length} results` : `${models.length} of ${data.models.length} results`;
  text("model-count", `${count} · ${data.memory_source}`);
  byId("models-empty").hidden = models.length > 0;
  byId("more-models").hidden = !data.next_cursor;
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
  input.value = setting.value;
  input.setAttribute("aria-label", `New value for ${setting.key}`);
  valueCell.replaceChildren(input);
  actionCell.replaceChildren();
  const save = action("Save", async () => {
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
  input.select();
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
    controls.appendChild(action("Edit", () => editConfigurationValue(row, setting), !setting.editable));
    rows.appendChild(row);
  });
  const setSecret = (prefix, secret) => {
    text(`${prefix}-state`, secret.configured ? "CONFIGURED" : "NOT SET");
    text(`${prefix}-source`, secret.configured ? `Effective source: ${secret.source}` : "No effective secret.");
  };
  setSecret("hf-token", configuration.hugging_face_token);
  setSecret("http-key", configuration.http_api_key);
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
  text("activity-count", sorted.length);
};

const applyActivity = (event) => {
  activities.set(event.operation_id, event);
  pendingModelOperations.delete(event.target);
  if (initiatedModelOperations.has(event.target) && !liveOperationStates.has(event.state)) {
    initiatedModelOperations.delete(event.target);
    if (event.state === "failed") {
      showNotice(`${event.kind} failed · ${event.detail}`, true);
    }
  }
  renderActivity();
  renderOperationTarget(event.target);
};

const chatNode = (role, content = "") => {
  byId("chat-empty")?.remove();
  const message = document.createElement("article");
  message.className = `chat-message ${role}`;
  const label = document.createElement("span");
  label.className = "role";
  label.textContent = role;
  const reasoning = document.createElement("div");
  reasoning.className = "reasoning";
  const body = document.createElement("div");
  body.className = "content";
  body.textContent = content;
  message.append(label, reasoning, body);
  byId("chat-log").appendChild(message);
  byId("chat-log").scrollTop = byId("chat-log").scrollHeight;
  return { message, reasoning, body };
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
    byId("chat-image-preview").removeAttribute("src");
    byId("chat-image-input").value = "";
    return;
  }
  byId("chat-image-preview").src = chatImage.dataUrl;
  text("chat-image-name", chatImage.name);
  text("chat-image-meta", `${bytes(chatImage.size)} · attached to each prompt`);
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
  const requestMessages = [...chatMessages, { role: "user", content: prompt }];
  chatMessages.push({ role: "user", content: prompt });
  chatNode("user", prompt);
  const assistant = chatNode("assistant");
  chatRunning = true;
  chatOperationId = null;
  text("chat-state", "starting");
  byId("chat-input").disabled = true;
  byId("attach-chat-image").disabled = true;
  byId("remove-chat-image").disabled = true;
  byId("send-chat").disabled = true;
  byId("cancel-chat").disabled = false;
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
      image: chatImage?.dataUrl ?? null,
    }),
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(body.error?.message || `HTTP ${response.status}`);
  }
  let completion = null;
  let receivedToken = false;
  await consumeSse(response, async (event, data) => {
    if (event === "started") chatOperationId = data.operation_id;
    if (event === "token") {
      if (!receivedToken) {
        assistant.message.classList.add("streaming");
        receivedToken = true;
      }
      const target = data.reasoning ? assistant.reasoning : assistant.body;
      target.textContent += data.text;
      byId("chat-log").scrollTop = byId("chat-log").scrollHeight;
    }
    if (event === "completion") {
      completion = data;
      assistant.body.textContent = data.text;
      assistant.reasoning.textContent = data.reasoning;
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
  renderChatModels(localModels);
  byId("chat-input").focus();
};

const applyModels = (data) => {
  localModels = data.models;
  renderChatModels(data.models);
  if (!searching) {
    renderLocalModels(data);
    return;
  }
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
    renderConnection("disconnected", error.message, "incompatible");
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

byId("hf-token-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const input = byId("hf-token");
  if (!input.value.trim()) return;
  try { await updateConfiguration({ operation: "set_hf_token", token: input.value }); input.value = ""; }
  catch (error) { showNotice(error.message, true); }
});

byId("test-hf-token").addEventListener("click", () =>
  updateConfiguration({ operation: "test_hf_token" }).catch((error) => showNotice(error.message, true)));
byId("remove-hf-token").addEventListener("click", () =>
  updateConfiguration({ operation: "remove_hf_token" }).catch((error) => showNotice(error.message, true)));

byId("http-key-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const input = byId("http-key");
  if (!input.value.trim()) return;
  try { await updateConfiguration({ operation: "set_http_api_key", key: input.value }); input.value = ""; }
  catch (error) { showNotice(error.message, true); }
});
byId("remove-http-key").addEventListener("click", () =>
  updateConfiguration({ operation: "remove_http_api_key" }).catch((error) => showNotice(error.message, true)));

byId("model-search").addEventListener("submit", async (event) => {
  event.preventDefault();
  const query = byId("model-query").value.trim();
  if (!query) return;
  searching = true;
  const button = byId("search-models");
  button.disabled = true;
  button.textContent = "Searching…";
  text("model-count", "searching Hugging Face");
  try { renderCatalog(await request(`/catalog/search?query=${encodeURIComponent(query)}&limit=20`)); }
  catch (error) { showNotice(error.message, true); }
  finally { button.disabled = false; button.textContent = "Search"; }
});

byId("clear-search").addEventListener("click", () => {
  searching = false;
  catalogResults = null;
  byId("model-query").value = "";
  renderLocalModels({ models: localModels });
  byId("more-models").hidden = true;
});
byId("more-models").addEventListener("click", async () => {
  if (!catalogResults?.next_cursor) return;
  const button = byId("more-models");
  button.disabled = true;
  try {
    const query = byId("model-query").value.trim();
    const page = await request(`/catalog/search?query=${encodeURIComponent(query)}&limit=20&cursor=${encodeURIComponent(catalogResults.next_cursor)}`);
    const models = [...catalogResults.models, ...page.models].sort((left, right) =>
      catalogRank(left) - catalogRank(right) || right.downloads - left.downloads || left.id.localeCompare(right.id));
    renderCatalog({ ...page, models });
  } catch (error) { showNotice(error.message, true); }
  finally { button.disabled = false; }
});
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
  try {
    const response = await mutate("/models/remove", { repo_id: removeRepoId });
    showNotice(response.removed ? `Removed ${removeRepoId}; freed ${bytes(response.freed_bytes)}` : `${removeRepoId} was not found`);
    byId("remove-dialog").close();
    searching = false;
  } catch (error) { showNotice(error.message, true); }
});

byId("chat-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const input = byId("chat-input");
  const prompt = input.value.trim();
  if (!prompt || chatRunning) return;
  input.value = "";
  try { await runChat(prompt); }
  catch (error) {
    if (chatMessages.at(-1)?.role === "user") chatMessages.pop();
    const assistant = byId("chat-log").querySelector(".chat-message.assistant:last-child");
    if (assistant) {
      assistant.classList.add("streaming");
      assistant.querySelector(".content").textContent = `Error: ${error.message}`;
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

byId("cancel-chat").addEventListener("click", async () => {
  if (!chatOperationId) return;
  try {
    const response = await mutate("/activity/cancel", { operation_id: chatOperationId });
    text("chat-state", response.accepted ? "cancelling" : response.state);
  } catch (error) { showNotice(error.message, true); }
});

connect().catch(waitForServer);
