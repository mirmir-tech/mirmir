const base = "/api/mirmir/v1";
let csrf = null;
let overviewTimer = null;
let modelsTimer = null;
let activitySource = null;
let localModels = [];
let currentConfiguration = null;
let loadSelector = null;
let removeRepoId = null;
let chatRunning = false;
let chatOperationId = null;
let chatMessages = [];
let chatImage = null;
let searching = false;
let catalogResults = null;
const activities = new Map();
const maxImageBytes = 20 * 1024 * 1024;
const imageTypes = new Set(["image/png", "image/jpeg", "image/webp", "image/gif"]);
const imageTypeByExtension = new Map([
  ["png", "image/png"], ["jpg", "image/jpeg"], ["jpeg", "image/jpeg"],
  ["webp", "image/webp"], ["gif", "image/gif"],
]);

const byId = (id) => document.getElementById(id);
const text = (id, value) => { byId(id).textContent = value; };
const number = (value, digits = 1) => value == null ? "—" : Number(value).toFixed(digits);

const request = async (path, options = {}) => {
  const response = await fetch(`${base}${path}`, { credentials: "same-origin", ...options });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error?.message || `HTTP ${response.status}`);
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
    try { await handler(); } catch (error) { showNotice(error.message, true); button.disabled = false; }
  });
  return button;
};

const runModelAction = async (path, payload, message) => {
  await mutate(path, payload);
  showNotice(message);
  window.setTimeout(() => refreshModels().catch((error) => showNotice(error.message, true)), 300);
};

const openLoadDialog = async (selector) => {
  loadSelector = selector;
  text("load-target", selector);
  text("load-memory", "Inspecting model and memory…");
  byId("confirm-load").disabled = true;
  byId("load-force").checked = false;
  byId("load-dialog").showModal();
  try {
    const inspection = await request(`/models/inspect?selector=${encodeURIComponent(selector)}`);
    const settings = inspection.settings;
    byId("load-max-tokens").value = settings.max_tokens;
    byId("load-temperature").value = settings.temperature;
    byId("load-top-p").value = settings.top_p;
    byId("load-top-k").value = settings.top_k;
    byId("load-repetition").value = settings.repetition_penalty;
    const memory = inspection.memory;
    const saved = inspection.has_mirmir_overrides ? "saved MiRMiR defaults" : "model defaults";
    text("load-memory", `${memory.fit} · ${bytes(memory.required_bytes)} required · ${saved} · ${memory.memory_source}`);
    byId("confirm-load").disabled = false;
  } catch (error) {
    text("load-memory", error.message);
    byId("confirm-load").disabled = true;
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

const renderLocalModels = (data) => {
  localModels = data.models;
  renderChatModels(data.models);
  const rows = byId("model-rows");
  rows.replaceChildren();
  data.models.forEach((model) => {
    const row = document.createElement("tr");
    cell(row, model.id || model.repo_id);
    const state = cell(row, "");
    state.appendChild(badge(model.state));
    cell(row, model.revision || "local");
    cell(row, model.repo_id || model.path, "path").title = model.path;
    const controls = cell(row, "");
    if (model.state === "ready") {
      controls.appendChild(action("Unload", () => runModelAction("/models/unload", { selector: model.selector }, "Unload requested")));
    } else {
      controls.appendChild(action("Load", () => openLoadDialog(model.selector), model.state !== "available"));
      controls.appendChild(action("Remove", () => openRemoveDialog(model.repo_id), model.state !== "available"));
    }
    rows.appendChild(row);
  });
  text("model-count", `${data.models.length} local`);
  byId("models-empty").hidden = data.models.length > 0;
};

const renderCatalog = (data) => {
  catalogResults = data;
  const rows = byId("model-rows");
  rows.replaceChildren();
  const showIncompatible = byId("show-incompatible").checked;
  const models = data.models.filter((model) => showIncompatible ||
    model.downloaded || model.local_source !== "remote" ||
    (model.compatibility === "supported" && model.memory_fit !== "does_not_fit"));
  models.forEach((model) => {
    const row = document.createElement("tr");
    cell(row, model.id);
    const fit = cell(row, "");
    fit.appendChild(badge(model.downloaded ? "downloaded" : model.memory_fit));
    if (model.gated) fit.appendChild(badge("gated"));
    cell(row, model.architecture || "unknown");
    const popularity = `${model.downloads.toLocaleString()} downloads · ${model.likes.toLocaleString()} likes`;
    cell(row, model.reason || model.local_source, "path").title = `${model.reason}\n${popularity}`;
    const controls = cell(row, "");
    const local = model.downloaded || model.local_source === "hf_cache";
    const blocked = model.compatibility !== "supported" || model.memory_fit === "does_not_fit";
    if (local) {
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
    if (event.cancellable && ["running", "cancelling"].includes(event.state)) {
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
  renderActivity();
  if (["completed", "failed", "cancelled"].includes(event.state)) {
    refreshModels().catch((error) => showNotice(error.message, true));
  }
};

const connectActivity = () => {
  activitySource = new EventSource(`${base}/activity`);
  activitySource.addEventListener("activity", (message) => applyActivity(JSON.parse(message.data)));
  activitySource.addEventListener("error", () => showNotice("Activity stream reconnecting", true));
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

const refreshOverview = () => request("/overview").then(renderOverview);
const refreshConfiguration = () => request("/configuration").then(renderConfiguration);
const refreshModels = () => request("/models").then((data) => {
  localModels = data.models;
  renderChatModels(data.models);
  if (!searching) renderLocalModels(data);
});

const connect = async () => {
  const bootstrap = await request("/bootstrap");
  text("server-version", bootstrap.server_version);
  text("protocol-version", bootstrap.protocol_version);
  const session = await request("/session", { method: "POST" });
  csrf = session.csrf_token;
  await Promise.all([refreshOverview(), refreshModels(), refreshConfiguration()]);
  connectActivity();
  const status = byId("status");
  status.textContent = "Local session active";
  status.className = "status ready";
  overviewTimer = window.setInterval(() => refreshOverview().catch(disconnect), 1000);
  modelsTimer = window.setInterval(() => refreshModels().catch(disconnect), 5000);
};

const disconnect = (error) => {
  if (overviewTimer) window.clearInterval(overviewTimer);
  if (modelsTimer) window.clearInterval(modelsTimer);
  if (activitySource) activitySource.close();
  const status = byId("status");
  status.textContent = error ? `Unavailable: ${error.message}` : "Session ended";
  status.className = error ? "status error" : "status";
};

document.querySelectorAll(".tab").forEach((tab) => tab.addEventListener("click", () => {
  document.querySelectorAll(".tab, .view").forEach((item) => item.classList.remove("active"));
  document.querySelectorAll(".tab").forEach((item) => item.removeAttribute("aria-current"));
  tab.classList.add("active");
  tab.setAttribute("aria-current", "page");
  byId(tab.dataset.view).classList.add("active");
  text("page-title", tab.dataset.label);
  document.title = `MiRMiR · ${tab.dataset.label}`;
  if (tab.dataset.view === "configuration") {
    refreshConfiguration().catch((error) => showNotice(error.message, true));
  }
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
byId("load-model-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!loadSelector) return;
  const settings = {
    max_tokens: Number(byId("load-max-tokens").value),
    temperature: Number(byId("load-temperature").value),
    top_p: Number(byId("load-top-p").value),
    top_k: Number(byId("load-top-k").value),
    repetition_penalty: Number(byId("load-repetition").value),
  };
  try {
    await runModelAction("/models/load", {
      selector: loadSelector, settings, force: byId("load-force").checked,
    }, "Load accepted; saved settings will be reused");
    byId("load-dialog").close();
  } catch (error) { showNotice(error.message, true); }
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
    await refreshModels();
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

byId("logout").addEventListener("click", async () => {
  try {
    await request("/session", { method: "DELETE", headers: { "x-mirmir-csrf": csrf } });
    disconnect();
  } catch (error) { disconnect(error); }
});

connect().catch(disconnect);
