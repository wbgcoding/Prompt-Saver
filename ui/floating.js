// Floating quick-copy pill. Click copies; press-and-move drags the window;
// right-click opens a small menu (size, edit prompt, remove).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
const promptId = appWin.label.replace(/^float-/, "");

const pill = document.getElementById("pill");
const label = document.getElementById("label");
const menu = document.getElementById("menu");

// i18n: follows the language setting ("auto" -> OS language). EN fallback.
const TXT = {
  en: { copied: "Copied!", size: "Size", edit: "Edit prompt", remove: "Remove" },
  de: { copied: "Kopiert!", size: "Größe", edit: "Prompt bearbeiten", remove: "Entfernen" },
  es: { copied: "¡Copiado!", size: "Tamaño", edit: "Editar prompt", remove: "Quitar" },
  fr: { copied: "Copié !", size: "Taille", edit: "Modifier le prompt", remove: "Retirer" },
  it: { copied: "Copiato!", size: "Dimensione", edit: "Modifica prompt", remove: "Rimuovi" },
  pt: { copied: "Copiado!", size: "Tamanho", edit: "Editar prompt", remove: "Remover" },
  pl: { copied: "Skopiowano!", size: "Rozmiar", edit: "Edytuj prompt", remove: "Usuń" },
  ru: { copied: "Скопировано!", size: "Размер", edit: "Изменить промпт", remove: "Убрать" },
  zh: { copied: "已复制！", size: "大小", edit: "编辑提示词", remove: "移除" },
  ja: { copied: "コピーしました！", size: "サイズ", edit: "プロンプトを編集", remove: "削除" },
};

// Same font catalog as the grid tiles (main.js) — pill text matches the tiles.
const FONTS = {
  system: '"Segoe UI", system-ui, sans-serif',
  arial: "Arial, Helvetica, sans-serif",
  verdana: "Verdana, Geneva, sans-serif",
  tahoma: "Tahoma, Geneva, sans-serif",
  georgia: "Georgia, serif",
  times: '"Times New Roman", Times, serif',
  mono: 'Consolas, "Courier New", monospace',
  courier: '"Courier New", Courier, monospace',
  script: '"Segoe Script", "Comic Sans MS", cursive',
  impact: 'Impact, "Arial Black", sans-serif',
};

let scale = 1;
let tileSize = 0; // 0 = auto-fit, like the grid tiles
let menuOpen = false;

// Same sizing rules as the grid: fixed size -> same px; auto-fit -> largest
// size whose single line still fits the pill.
function fitPill() {
  if (tileSize > 0) {
    label.style.fontSize = Math.round(tileSize * scale) + "px";
    return;
  }
  const maxW = pill.clientWidth - 24;
  const maxH = pill.clientHeight - 10;
  const fits = (s) => {
    label.style.fontSize = s + "px";
    return label.scrollWidth <= maxW && s <= maxH;
  };
  let lo = 8;
  let hi = Math.max(8, Math.min(48, maxH));
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (fits(mid)) lo = mid;
    else hi = mid - 1;
  }
  label.style.fontSize = lo + "px";
}

function markSelectedSize() {
  for (const btn of menu.querySelectorAll("[data-scale]")) {
    btn.classList.toggle("sel", Math.abs(Number(btn.dataset.scale) - scale) < 0.01);
  }
}

async function applySettings() {
  const s = await invoke("get_settings");
  const pref = (s.language && s.language !== "auto"
    ? s.language
    : navigator.language || "en"
  ).toLowerCase();
  const lang = Object.keys(TXT).find((c) => pref.startsWith(c)) || "en";
  const t = TXT[lang];
  document.getElementById("feedback").textContent = t.copied;
  document.getElementById("size-label").textContent = t.size;
  document.getElementById("menu-edit").textContent = t.edit;
  document.getElementById("menu-remove").textContent = t.remove;
  scale = (s.float_scale && s.float_scale[promptId]) || 1;
  tileSize = Number(s.tile_size) || 0;
  pill.style.fontFamily = FONTS[s.tile_font] || FONTS.system;
  markSelectedSize();
  fitPill();
}

// Re-fit when the window is resized (size preset changed).
let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(fitPill);
});

const DRAG_THRESHOLD = 4;
let drag = null; // {x, y, moved}
let copiedTimer = null;

async function loadName() {
  const p = await invoke("get_prompt", { id: promptId });
  if (p) {
    label.textContent = p.name;
    pill.title = p.name;
    if (p.color) pill.style.background = p.color;
    fitPill();
  }
}

function showCopied() {
  pill.classList.add("copied");
  clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => pill.classList.remove("copied"), 900);
}

// ---- Right-click menu (window grows while open, shrinks back after) ----
async function openMenu() {
  if (menuOpen) return;
  menuOpen = true;
  await invoke("resize_float_menu", { id: promptId, open: true });
  pill.classList.add("hidden");
  menu.classList.remove("hidden");
}

async function closeMenu() {
  if (!menuOpen) return;
  menuOpen = false;
  menu.classList.add("hidden");
  pill.classList.remove("hidden");
  await invoke("resize_float_menu", { id: promptId, open: false });
}

window.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  openMenu();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeMenu();
});
window.addEventListener("blur", closeMenu);

for (const btn of menu.querySelectorAll("[data-scale]")) {
  btn.addEventListener("click", async () => {
    scale = Number(btn.dataset.scale) || 1;
    markSelectedSize();
    menuOpen = false;
    menu.classList.add("hidden");
    pill.classList.remove("hidden");
    // Persists the factor and resizes the window to the new pill size.
    await invoke("set_float_scale", { id: promptId, scale });
  });
}

document.getElementById("menu-edit").addEventListener("click", async () => {
  await closeMenu();
  await invoke("edit_prompt_request", { id: promptId });
});

document.getElementById("menu-remove").addEventListener("click", () => {
  invoke("toggle_floating", { id: promptId }); // closes this window
});

// ---- Drag / copy ----
pill.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  drag = { x: e.screenX, y: e.screenY, moved: false };
});

window.addEventListener("mousemove", async (e) => {
  if (!drag || drag.moved) return;
  if (Math.abs(e.screenX - drag.x) < DRAG_THRESHOLD &&
      Math.abs(e.screenY - drag.y) < DRAG_THRESHOLD) return;
  drag.moved = true;
  pill.classList.add("dragging");
  await appWin.startDragging(); // OS takes over; position saved in backend on move
});

window.addEventListener("mouseup", async () => {
  if (!drag) return;
  const wasDrag = drag.moved;
  drag = null;
  pill.classList.remove("dragging");
  if (!wasDrag && (await invoke("copy_prompt", { id: promptId }))) showCopied();
});

// Refresh label when the prompt is edited in the main window.
listen("prompt-updated", (e) => {
  if (e.payload && e.payload.id === promptId) {
    label.textContent = e.payload.name;
    pill.title = e.payload.name;
    pill.style.background = e.payload.color || "";
    fitPill();
  }
});

listen("theme-changed", (e) => document.documentElement.setAttribute("data-theme", e.payload));

(async () => {
  document.documentElement.setAttribute("data-theme", await invoke("current_theme"));
  applySettings();
  loadName();
})();
