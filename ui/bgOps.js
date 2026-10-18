// Background jobs popup.
import { $, EMPTY_VALUE, errToast, invoke, listen, t, toast } from "./core.js";
import { placePopover } from "./popover.js";
import { confirmDialog } from "./dialog.js";

// ---- Background jobs popover (header button) --------------------------
// The registry lives in Rust (see list_bg_ops/push_bg_op) and is shared with the taskbar
// icon; this only displays it. Export runs in the video-editor window, convert and the
// ffmpeg download can run from here, but every progress event is broadcast app-wide, so
// this window learns about all three the same way.
export const BG_OP_CANCEL_CMD = {
  export: "cancel_export",
  convert: "cancel_recording_convert",
  "ffmpeg-download": "cancel_ffmpeg_download",
  "ocr-index": "cancel_ocr_index_backfill",
};
export function bgKindNoun(kind) {
  if (kind === "export") return t("bgKindExport");
  if (kind === "convert") return t("bgKindConvert");
  if (kind === "ocr-index") return t("bgKindOcrIndex");
  return t("bgKindDownload");
}
// Short present-tense label of a running job; its own strings, because the
// progress sentences carry a placeholder in a language-dependent place.
export function bgOpLabel(kind) {
  if (kind === "export") return t("bgOpLabelExport");
  if (kind === "convert") return t("bgOpLabelConvert");
  if (kind === "ocr-index") return t("bgOpLabelOcrIndex");
  return t("bgOpLabelDownload");
}
export const bgOpsPop = document.createElement("div");

export function closeBgOpsPop() { bgOpsPop.classList.add("hidden"); }

export function renderBgOpsPop(ops) {
  bgOpsPop.innerHTML = "";
  if (!ops.length) {
    const empty = document.createElement("div");
    empty.className = "pick-history-empty";
    empty.textContent = t("bgNone");
    bgOpsPop.appendChild(empty);
    return;
  }
  for (const op of ops) {
    const row = document.createElement("div");
    row.className = "bg-ops-row";
    const label = document.createElement("span");
    label.className = "bg-ops-label";
    label.textContent = bgOpLabel(op.kind);
    const pct = document.createElement("span");
    pct.className = "bg-ops-pct";
    const pctNum = op.state === "done" ? 100 : op.pct != null ? op.pct : null;
    pct.textContent = op.state === "error" ? EMPTY_VALUE : op.state === "done" ? "100%" : (op.pct != null ? `${op.pct}%` : "…");
    if (pctNum != null) {
      pct.setAttribute("role", "progressbar");
      pct.setAttribute("aria-valuemin", "0");
      pct.setAttribute("aria-valuemax", "100");
      pct.setAttribute("aria-valuenow", String(pctNum));
      pct.setAttribute("aria-valuetext", t("a11yPctOnly").replace("{pct}", String(pctNum)));
    }
    row.append(label, pct);
    if (op.target) {
      const openBtn = document.createElement("button");
      openBtn.type = "button";
      openBtn.className = "ghost-btn bg-ops-action";
      openBtn.textContent = t("bgOpenFolder");
      openBtn.addEventListener("click", () => invoke("open_bg_op_folder", { path: op.target }).catch((err) => errToast(err)));
      row.appendChild(openBtn);
    }
    if (op.state === "running" || op.state === "indeterminate") {
      const cancelCmd = BG_OP_CANCEL_CMD[op.kind];
      if (cancelCmd) {
        const cancelBtn = document.createElement("button");
        cancelBtn.type = "button";
        cancelBtn.className = "ghost-btn bg-ops-action";
        cancelBtn.textContent = t("cancel");
        cancelBtn.addEventListener("click", () => {
          // A batch export can show several "export" rows, each with its own generation
          // number in op.id ("export-<n>"); a plain cancel_export() would hit the video
          // editor's last export instead of the clicked row.
          if (op.kind === "export") {
            const generation = Number(op.id.split("-").pop());
            invoke("cancel_export_job", { generation }).catch(() => {});
          } else {
            invoke(cancelCmd).catch(() => {});
          }
        });
        row.appendChild(cancelBtn);
      }
    }
    bgOpsPop.appendChild(row);
  }
}

// Keyed by "id:started_at_ms": the id is unique per job (a batch of exports runs
// several jobs of one kind, possibly started in the same millisecond), and the start
// time tells this toast-once check a NEW run of the same job id from one it showed.
export const bgOpsToasted = new Set();
export let bgOpsFirstRun = true;

export async function refreshBgOps() {
  let ops = [];
  try { ops = (await invoke("list_bg_ops")) || []; } catch { return; }
  const running = ops.filter((o) => o.state === "running" || o.state === "indeterminate");
  $("bg-ops-btn").classList.toggle("hidden", running.length === 0);
  $("bg-ops-badge").classList.toggle("hidden", running.length === 0);
  $("bg-ops-badge").textContent = String(running.length);
  if (!bgOpsPop.classList.contains("hidden")) renderBgOpsPop(ops);
  for (const op of ops) {
    if (op.state !== "done" && op.state !== "error") continue;
    const key = `${op.id || op.kind}:${op.started_at_ms}`;
    if (bgOpsToasted.has(key)) continue;
    bgOpsToasted.add(key);
    // First call after load just learns the current state: a job finished
    // before this window ever asked about it does not need a toast for it.
    if (bgOpsFirstRun || op.state !== "done") continue;
    const action = op.target
      ? { label: t("bgOpenFolder"), onClick: () => invoke("open_bg_op_folder", { path: op.target }).catch((err) => errToast(err)) }
      : null;
    toast(t("bgDoneToast").replace("{what}", () => bgKindNoun(op.kind)), action);
  }
  bgOpsFirstRun = false;
}

export function wireBgOps() {
  bgOpsPop.className = "pick-history-pop hidden";
  document.body.appendChild(bgOpsPop);
  document.addEventListener("pointerdown", (e) => {
    if (bgOpsPop.classList.contains("hidden")) return;
    if (!bgOpsPop.contains(e.target) && !e.target.closest("#bg-ops-btn")) closeBgOpsPop();
  });
  $("bg-ops-btn").addEventListener("click", async () => {
    if (!bgOpsPop.classList.contains("hidden")) { closeBgOpsPop(); return; }
    let ops = [];
    try { ops = (await invoke("list_bg_ops")) || []; } catch { /* show empty rather than nothing */ }
    renderBgOpsPop(ops);
    bgOpsPop.classList.remove("hidden");
    placePopover(bgOpsPop, $("bg-ops-btn").getBoundingClientRect());
  });
  // push_bg_op (Rust) broadcasts this on every state change: running tick,
  // done, error: regardless of which window (if any) started the job, so this
  // popover stays live without a separate poll timer.
  listen("bg-ops-changed", refreshBgOps);
  // Some Windows builds keep a window out of screenshots after the switch is turned
  // on; the backend says so once when its readback still shows it.
  listen("capture-restart-notice", () => toast(t("allowScreenshotRestart")));
  refreshBgOps();
  listen("bg-quit-requested", async (e) => {
    const n = Number(e.payload) || 0;
    const ok = await confirmDialog({
      title: t("bgTitle"),
      message: t("bgQuitRunning").replace("{n}", String(n)),
      confirmLabel: t("bgQuitAnyway"),
      danger: true,
    });
    if (ok) invoke("confirm_quit_with_running_ops").catch(() => {});
  });
}
