// The Whisper model manager (#whisper-models-modal): one card per model with its size, the
// recommendation for this PC, and the download, use and delete actions. The first model also
// turns the local engine on (the backend does that and reports it).
import { $, errText, errToast, invoke, listen, settings, t, toast } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { assignWhisperCatalogCache, persistUiText, renderExpert } from "./expertMenu.js";

// Codes the backend refuses with that already read well on their own (mapped in errors.js).
const OWN_MESSAGE = new Set(["whisper-damaged", "whisper-busy", "whisper-model-in-use"]);
const CANCELLED = "cancelled";

let catalog = null; // whisper_catalog() as last loaded
let download = null; // { id, done, total, cancelling } while a model is coming in
let loadFailed = false;

// Sizes read in decimal megabytes, like the catalog labels ("574 MB"), so the numbers agree.
const mb = (bytes) => Math.round(bytes / 1e6) + " MB";

// "Small · 190 MB · good accuracy": the first part names the model, the rest describes it.
function splitLabel(text) {
  const at = text.indexOf(" · ");
  return at < 0 ? { name: text, sub: "" } : { name: text.slice(0, at), sub: text.slice(at + 3) };
}

// The model a dictation session would load: the chosen one, else the recommended one, else
// any that is downloaded (the same order the backend uses).
export function activeWhisperModel(cat, chosen) {
  if (!cat) return "";
  const have = (id) => cat.models.some((m) => m.id === id && m.downloaded);
  return [chosen, cat.recommended].find((id) => id && have(id)) || cat.models.find((m) => m.downloaded)?.id || "";
}

async function loadCatalog() {
  try {
    catalog = await invoke("whisper_catalog");
    assignWhisperCatalogCache(catalog);
    loadFailed = false;
    if (catalog.downloading && !download) download = { id: "", done: 0, total: 0, cancelling: false };
  } catch (e) {
    loadFailed = true;
    errToast(e);
  }
}

function button(label, onClick, { disabled = false } = {}) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "ghost-btn";
  b.textContent = label;
  b.disabled = disabled;
  b.addEventListener("click", onClick);
  return b;
}

function progressEl(m) {
  const wrap = document.createElement("div");
  wrap.className = "whisper-progress";
  const bar = document.createElement("div");
  bar.className = "whisper-progress-track";
  bar.setAttribute("role", "progressbar");
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", "100");
  bar.setAttribute("aria-label", splitLabel(t(m.label_key)).name);
  const fill = document.createElement("div");
  fill.className = "whisper-progress-fill";
  bar.appendChild(fill);
  const text = document.createElement("span");
  text.className = "whisper-progress-text";
  const cancel = button(t("cancel"), cancelDownload);
  cancel.classList.add("whisper-progress-cancel");
  wrap.append(bar, text, cancel);
  return wrap;
}

function paintProgress() {
  const wrap = document.querySelector("#whisper-models-body .whisper-progress");
  if (!wrap || !download) return;
  const total = download.total || 0;
  const pct = total > 0 ? Math.min(100, Math.floor((download.done / total) * 100)) : 0;
  wrap.querySelector(".whisper-progress-track").setAttribute("aria-valuenow", String(pct));
  wrap.querySelector(".whisper-progress-fill").style.width = pct + "%";
  wrap.querySelector(".whisper-progress-text").textContent = total > 0
    ? `${mb(download.done)} / ${mb(total)}`
    : "";
  wrap.querySelector(".whisper-progress-cancel").disabled = download.cancelling;
}

function modelCard(m, active) {
  const row = document.createElement("div");
  row.className = "clip-row whisper-card" + (m.id === active ? " is-active" : "");
  row.dataset.model = m.id;
  const main = document.createElement("div");
  main.className = "whisper-card-main";
  const { name, sub } = splitLabel(t(m.label_key));
  const title = document.createElement("div");
  title.className = "lang-row-name whisper-card-name";
  title.textContent = name;
  main.appendChild(title);
  if (sub) {
    const subEl = document.createElement("div");
    subEl.className = "lang-row-sub";
    subEl.textContent = sub;
    main.appendChild(subEl);
  }
  if (catalog.recommended === m.id) {
    const badge = document.createElement("span");
    badge.className = "whisper-badge";
    badge.textContent = t("whisperRecommended");
    title.appendChild(badge);
  }
  const actions = document.createElement("div");
  actions.className = "whisper-card-actions";
  const mine = download && download.id === m.id;
  if (!m.downloaded) {
    const busy = !!download;
    actions.appendChild(button(t("whisperDownload"), () => downloadModel(m), { disabled: busy || !catalog.supported }));
  } else {
    if (m.id === active) actions.appendChild(button(t("whisperInUse"), () => {}, { disabled: true }));
    else actions.appendChild(button(t("whisperUse"), () => useModel(m)));
    actions.appendChild(button(t("whisperDelete"), () => deleteModel(m), { disabled: !!download }));
  }
  row.append(main, actions);
  if (mine) row.appendChild(progressEl(m));
  return row;
}

function render() {
  const body = $("whisper-models-body");
  body.replaceChildren();
  if (!catalog) {
    if (loadFailed) {
      const retry = button(t("ocrLangsCheckNow"), async () => { await loadCatalog(); render(); });
      body.appendChild(retry);
    }
    return;
  }
  const pc = $("whisper-models-pc");
  const p = catalog.profile || {};
  pc.textContent = t("whisperThisPc").replace("{cores}", () => String(p.physical_cores)).replace("{ram}", () => String(p.ram_gb));
  const warn = $("whisper-models-cpu");
  warn.classList.toggle("hidden", catalog.supported);
  warn.textContent = catalog.supported ? "" : t("voiceWhisperCpu");
  const active = activeWhisperModel(catalog, settings.ui_texts?.whisperModel);
  for (const m of catalog.models) body.appendChild(modelCard(m, active));
  paintProgress();
}

async function refreshModels() {
  await loadCatalog();
  render();
}

async function downloadModel(m) {
  const { name } = splitLabel(t(m.label_key));
  const ok = await confirmDialog({
    title: t("whisperConsentTitle"),
    message: t("whisperConsentBody").replace("{name}", () => name).replace("{size}", () => mb(m.bytes)),
    confirmLabel: t("whisperDownload"),
    danger: false,
  });
  if (!ok) return;
  try {
    await invoke("grant_whisper_download_consent", { model: m.id });
  } catch (e) {
    errToast(e);
    return;
  }
  download = { id: m.id, done: 0, total: m.bytes, cancelling: false };
  render();
  try {
    const res = await invoke("download_whisper_model", { model: m.id });
    // The first model switches dictation to the local engine on the backend side; mirror it here.
    if (res && res.engine_set) {
      settings.ui_texts = { ...(settings.ui_texts || {}), voiceEngine: "whisper" };
      renderExpert();
    }
  } catch (e) {
    if (e !== CANCELLED) {
      toast(OWN_MESSAGE.has(e) ? errText(e) : t("whisperDownloadFailed").replace("{reason}", () => errText(e)));
    }
  }
  download = null;
  await refreshModels();
}

function cancelDownload() {
  if (!download) return;
  download.cancelling = true;
  paintProgress();
  invoke("cancel_whisper_download").catch(() => {});
}

async function useModel(m) {
  if (await persistUiText("whisperModel", m.id)) render();
}

async function deleteModel(m) {
  const { name } = splitLabel(t(m.label_key));
  const ok = await confirmDialog({
    title: t("whisperDelete"),
    message: t("whisperDeleteConfirm").replace("{name}", () => name),
    confirmLabel: t("whisperDelete"),
  });
  if (!ok) return;
  try {
    await invoke("delete_whisper_model", { model: m.id });
  } catch (e) {
    errToast(e);
  }
  await refreshModels();
}

export async function openWhisperModels() {
  $("whisper-models-modal").classList.remove("hidden");
  render();
  await refreshModels();
  $("whisper-models-close-x").focus();
}

export function closeWhisperModels() {
  $("whisper-models-modal").classList.add("hidden");
}

export function wireWhisperModels() {
  $("whisper-models-close-x").addEventListener("click", closeWhisperModels);
  $("whisper-models-modal").addEventListener("pointerdown", (e) => { if (e.target === $("whisper-models-modal")) closeWhisperModels(); });
  listen("whisper-download-progress", (e) => {
    const p = e.payload || {};
    if (!download) return;
    // A download another window or an earlier session started names its model with the first event.
    if (!download.id) { download.id = p.model; render(); }
    download.done = p.done || 0;
    download.total = p.total || download.total;
    paintProgress();
  });
}
