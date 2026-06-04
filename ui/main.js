// Main-window controller. Uses Tauri's global API (withGlobalTauri).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const gridEl = $("grid");
const inputEl = $("input");
const saveBtn = $("save");
const toastEl = $("toast");
const ctxEl = $("ctx");
const settingsEl = $("settings");
const trayBtn = $("tray");
const trayMenu = $("tray-menu");
const viewsEl = $("views");
const modal = {
  root: $("modal"),
  title: $("modal-title"),
  name: $("modal-name"),
  text: $("modal-text"),
  cancel: $("modal-cancel"),
  confirm: $("modal-confirm"),
};

const DRAG_THRESHOLD = 5;
const INPUT_MAX = 160; // keep in sync with .input max-height
const MAX_VIEWS = 20;

// Cached state (refreshed by renderGrid).
let prompts = [];
let settings = { theme: "system", views: [], active_view: "" };
let overflow = []; // prompts without a cell in the active view's current grid

let modalState = null;
let ctxId = null;
let drag = null;
let toastTimer = null;
let deleteAllTimer = null;

// ---- Global error surfacing ----
window.addEventListener("error", (e) => toast(`Error: ${e.message}`));
window.addEventListener("unhandledrejection", (e) => toast(`Error: ${e.reason}`));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

const DOTS =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z"/></svg>';
const CROSS =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';

// ---- i18n (EN fallback, DE auto-detected) ----
const I18N = {
  en: {
    settings: "Settings",
    theme: "Theme",
    themeSystem: "System",
    themeLight: "Light",
    themeDark: "Dark",
    language: "Language",
    langAuto: "Automatic",
    tileText: "Prompt text (font · size)",
    fontSystem: "System",
    fontSerif: "Serif",
    fontMono: "Monospace",
    fontScript: "Handwriting",
    sizeSmall: "Small",
    sizeMedium: "Medium",
    sizeLarge: "Large",
    sizeXL: "Extra large",
    hideInView: "Hide in this view",
    viewsLabel: "Views (max 20)",
    addView: "+ Add view",
    viewNamePh: "View name",
    minimizeOnClose: "Minimize to background on close",
    autostart: "Start automatically at login",
    startMinimized: "Start minimized on autostart",
    importExport: "Import / export prompts",
    import: "Import",
    imported: "prompts imported",
    importFailed: "Import failed",
    deleteAll: "Delete ALL data & settings!",
    deleteAllConfirm: "Really delete ALL data & settings?",
    cancel: "Cancel",
    save: "Save",
    edit: "Edit",
    delete: "Delete",
    toggleFloating: "Toggle floating button",
    nameModalTitle: "Name this prompt",
    editModalTitle: "Edit prompt",
    namePh: "Prompt name",
    promptPh: "Type a prompt…",
    unplaced: "Unplaced prompts",
    trayHint: "Click: place in free cell · Drag onto grid: place/swap",
    gridFull: "Grid is full – drag onto a tile to swap",
    tileTooltip: "Click: copy  |  Drag: move  |  Right-click: menu",
    actions: "Actions",
    exportFailed: "Export failed",
    copied: "Copied!",
  },
  de: {
    settings: "Einstellungen",
    theme: "Design",
    themeSystem: "System",
    themeLight: "Hell",
    themeDark: "Dunkel",
    language: "Sprache",
    langAuto: "Automatisch",
    tileText: "Prompt-Text (Schrift · Größe)",
    fontSystem: "System",
    fontSerif: "Serife",
    fontMono: "Monospace",
    fontScript: "Handschrift",
    sizeSmall: "Klein",
    sizeMedium: "Mittel",
    sizeLarge: "Groß",
    sizeXL: "Sehr groß",
    hideInView: "In Ansicht verstecken",
    viewsLabel: "Ansichten (max. 20)",
    addView: "+ Ansicht hinzufügen",
    viewNamePh: "Name der Ansicht",
    minimizeOnClose: "Beim Schließen im Hintergrund minimieren",
    autostart: "Automatisch bei Anmeldung starten",
    startMinimized: "Bei Autostart minimiert starten",
    importExport: "Prompts importieren / exportieren",
    import: "Importieren",
    imported: "Prompts importiert",
    importFailed: "Import fehlgeschlagen",
    deleteAll: "Alle Daten und Einstellungen löschen!",
    deleteAllConfirm: "Wirklich ALLE Daten und Einstellungen löschen?",
    cancel: "Abbrechen",
    save: "Speichern",
    edit: "Bearbeiten",
    delete: "Löschen",
    toggleFloating: "Schwebenden Button umschalten",
    nameModalTitle: "Prompt benennen",
    editModalTitle: "Prompt bearbeiten",
    namePh: "Prompt-Name",
    promptPh: "Prompt eingeben…",
    unplaced: "Nicht platzierte Prompts",
    trayHint: "Klick: in freie Zelle · Auf Raster ziehen: platzieren/tauschen",
    gridFull: "Raster ist voll – zum Tauschen auf ein Feld ziehen",
    tileTooltip: "Klick: kopieren  |  Ziehen: verschieben  |  Rechtsklick: Menü",
    actions: "Aktionen",
    exportFailed: "Export fehlgeschlagen",
    copied: "Kopiert!",
  },
  es: {
    settings: "Ajustes",
    theme: "Tema",
    themeSystem: "Sistema",
    themeLight: "Claro",
    themeDark: "Oscuro",
    language: "Idioma",
    langAuto: "Automático",
    tileText: "Texto del prompt (fuente · tamaño)",
    fontSystem: "Sistema",
    fontSerif: "Serif",
    fontMono: "Monoespaciada",
    fontScript: "Manuscrita",
    sizeSmall: "Pequeño",
    sizeMedium: "Mediano",
    sizeLarge: "Grande",
    sizeXL: "Muy grande",
    hideInView: "Ocultar en esta vista",
    viewsLabel: "Vistas (máx. 20)",
    addView: "+ Añadir vista",
    viewNamePh: "Nombre de la vista",
    minimizeOnClose: "Minimizar al cerrar",
    autostart: "Iniciar automáticamente al iniciar sesión",
    startMinimized: "Iniciar minimizado con el autoarranque",
    importExport: "Importar / exportar prompts",
    import: "Importar",
    imported: "prompts importados",
    importFailed: "Error al importar",
    deleteAll: "¡Borrar TODOS los datos y ajustes!",
    deleteAllConfirm: "¿Borrar realmente TODOS los datos y ajustes?",
    cancel: "Cancelar",
    save: "Guardar",
    edit: "Editar",
    delete: "Eliminar",
    toggleFloating: "Botón flotante sí/no",
    nameModalTitle: "Nombra este prompt",
    editModalTitle: "Editar prompt",
    namePh: "Nombre del prompt",
    promptPh: "Escribe un prompt…",
    unplaced: "Prompts sin colocar",
    trayHint: "Clic: celda libre · Arrastrar a la cuadrícula: colocar/intercambiar",
    gridFull: "Cuadrícula llena: arrastra sobre una celda para intercambiar",
    tileTooltip: "Clic: copiar  |  Arrastrar: mover  |  Clic derecho: menú",
    actions: "Acciones",
    exportFailed: "Error al exportar",
    copied: "¡Copiado!",
  },
  fr: {
    settings: "Paramètres",
    theme: "Thème",
    themeSystem: "Système",
    themeLight: "Clair",
    themeDark: "Sombre",
    language: "Langue",
    langAuto: "Automatique",
    tileText: "Texte du prompt (police · taille)",
    fontSystem: "Système",
    fontSerif: "Serif",
    fontMono: "Monospace",
    fontScript: "Manuscrite",
    sizeSmall: "Petit",
    sizeMedium: "Moyen",
    sizeLarge: "Grand",
    sizeXL: "Très grand",
    hideInView: "Masquer dans cette vue",
    viewsLabel: "Vues (max. 20)",
    addView: "+ Ajouter une vue",
    viewNamePh: "Nom de la vue",
    minimizeOnClose: "Réduire en arrière-plan à la fermeture",
    autostart: "Démarrer automatiquement à la connexion",
    startMinimized: "Démarrer réduit au démarrage automatique",
    importExport: "Importer / exporter les prompts",
    import: "Importer",
    imported: "prompts importés",
    importFailed: "Échec de l'import",
    deleteAll: "Supprimer TOUTES les données et paramètres !",
    deleteAllConfirm: "Vraiment supprimer TOUTES les données et paramètres ?",
    cancel: "Annuler",
    save: "Enregistrer",
    edit: "Modifier",
    delete: "Supprimer",
    toggleFloating: "Bouton flottant oui/non",
    nameModalTitle: "Nommer ce prompt",
    editModalTitle: "Modifier le prompt",
    namePh: "Nom du prompt",
    promptPh: "Saisir un prompt…",
    unplaced: "Prompts non placés",
    trayHint: "Clic : cellule libre · Glisser sur la grille : placer/échanger",
    gridFull: "Grille pleine – glissez sur une case pour échanger",
    tileTooltip: "Clic : copier  |  Glisser : déplacer  |  Clic droit : menu",
    actions: "Actions",
    exportFailed: "Échec de l'export",
    copied: "Copié !",
  },
  it: {
    settings: "Impostazioni",
    theme: "Tema",
    themeSystem: "Sistema",
    themeLight: "Chiaro",
    themeDark: "Scuro",
    language: "Lingua",
    langAuto: "Automatico",
    tileText: "Testo del prompt (font · dimensione)",
    fontSystem: "Sistema",
    fontSerif: "Serif",
    fontMono: "Monospace",
    fontScript: "Corsivo",
    sizeSmall: "Piccolo",
    sizeMedium: "Medio",
    sizeLarge: "Grande",
    sizeXL: "Molto grande",
    hideInView: "Nascondi in questa vista",
    viewsLabel: "Viste (max 20)",
    addView: "+ Aggiungi vista",
    viewNamePh: "Nome della vista",
    minimizeOnClose: "Riduci in background alla chiusura",
    autostart: "Avvia automaticamente all'accesso",
    startMinimized: "Avvia ridotto con l'avvio automatico",
    importExport: "Importa / esporta prompt",
    import: "Importa",
    imported: "prompt importati",
    importFailed: "Importazione non riuscita",
    deleteAll: "Elimina TUTTI i dati e le impostazioni!",
    deleteAllConfirm: "Eliminare davvero TUTTI i dati e le impostazioni?",
    cancel: "Annulla",
    save: "Salva",
    edit: "Modifica",
    delete: "Elimina",
    toggleFloating: "Pulsante flottante sì/no",
    nameModalTitle: "Assegna un nome al prompt",
    editModalTitle: "Modifica prompt",
    namePh: "Nome del prompt",
    promptPh: "Scrivi un prompt…",
    unplaced: "Prompt non posizionati",
    trayHint: "Clic: cella libera · Trascina sulla griglia: posiziona/scambia",
    gridFull: "Griglia piena: trascina su una cella per scambiare",
    tileTooltip: "Clic: copia  |  Trascina: sposta  |  Clic destro: menu",
    actions: "Azioni",
    exportFailed: "Esportazione non riuscita",
    copied: "Copiato!",
  },
  pt: {
    settings: "Configurações",
    theme: "Tema",
    themeSystem: "Sistema",
    themeLight: "Claro",
    themeDark: "Escuro",
    language: "Idioma",
    langAuto: "Automático",
    tileText: "Texto do prompt (fonte · tamanho)",
    fontSystem: "Sistema",
    fontSerif: "Serif",
    fontMono: "Monoespaçada",
    fontScript: "Manuscrita",
    sizeSmall: "Pequeno",
    sizeMedium: "Médio",
    sizeLarge: "Grande",
    sizeXL: "Muito grande",
    hideInView: "Ocultar nesta visão",
    viewsLabel: "Visões (máx. 20)",
    addView: "+ Adicionar visão",
    viewNamePh: "Nome da visão",
    minimizeOnClose: "Minimizar ao fechar",
    autostart: "Iniciar automaticamente ao entrar",
    startMinimized: "Iniciar minimizado no arranque automático",
    importExport: "Importar / exportar prompts",
    import: "Importar",
    imported: "prompts importados",
    importFailed: "Falha na importação",
    deleteAll: "Apagar TODOS os dados e configurações!",
    deleteAllConfirm: "Apagar mesmo TODOS os dados e configurações?",
    cancel: "Cancelar",
    save: "Salvar",
    edit: "Editar",
    delete: "Excluir",
    toggleFloating: "Botão flutuante sim/não",
    nameModalTitle: "Nomeie este prompt",
    editModalTitle: "Editar prompt",
    namePh: "Nome do prompt",
    promptPh: "Digite um prompt…",
    unplaced: "Prompts não posicionados",
    trayHint: "Clique: célula livre · Arraste para a grade: posicionar/trocar",
    gridFull: "Grade cheia – arraste sobre uma célula para trocar",
    tileTooltip: "Clique: copiar  |  Arrastar: mover  |  Clique direito: menu",
    actions: "Ações",
    exportFailed: "Falha na exportação",
    copied: "Copiado!",
  },
  pl: {
    settings: "Ustawienia",
    theme: "Motyw",
    themeSystem: "Systemowy",
    themeLight: "Jasny",
    themeDark: "Ciemny",
    language: "Język",
    langAuto: "Automatycznie",
    tileText: "Tekst promptu (czcionka · rozmiar)",
    fontSystem: "Systemowa",
    fontSerif: "Szeryfowa",
    fontMono: "Monospace",
    fontScript: "Odręczna",
    sizeSmall: "Mały",
    sizeMedium: "Średni",
    sizeLarge: "Duży",
    sizeXL: "Bardzo duży",
    hideInView: "Ukryj w tym widoku",
    viewsLabel: "Widoki (maks. 20)",
    addView: "+ Dodaj widok",
    viewNamePh: "Nazwa widoku",
    minimizeOnClose: "Minimalizuj do tła przy zamknięciu",
    autostart: "Uruchamiaj automatycznie po zalogowaniu",
    startMinimized: "Uruchamiaj zminimalizowany przy autostarcie",
    importExport: "Import / eksport promptów",
    import: "Importuj",
    imported: "zaimportowane prompty",
    importFailed: "Import nie powiódł się",
    deleteAll: "Usuń WSZYSTKIE dane i ustawienia!",
    deleteAllConfirm: "Na pewno usunąć WSZYSTKIE dane i ustawienia?",
    cancel: "Anuluj",
    save: "Zapisz",
    edit: "Edytuj",
    delete: "Usuń",
    toggleFloating: "Przycisk pływający wł./wył.",
    nameModalTitle: "Nazwij ten prompt",
    editModalTitle: "Edytuj prompt",
    namePh: "Nazwa promptu",
    promptPh: "Wpisz prompt…",
    unplaced: "Nieumieszczone prompty",
    trayHint: "Klik: wolna komórka · Przeciągnij na siatkę: umieść/zamień",
    gridFull: "Siatka pełna – przeciągnij na kafelek, aby zamienić",
    tileTooltip: "Klik: kopiuj  |  Przeciągnij: przenieś  |  PPM: menu",
    actions: "Akcje",
    exportFailed: "Eksport nie powiódł się",
    copied: "Skopiowano!",
  },
  ru: {
    settings: "Настройки",
    theme: "Тема",
    themeSystem: "Системная",
    themeLight: "Светлая",
    themeDark: "Тёмная",
    language: "Язык",
    langAuto: "Автоматически",
    tileText: "Текст промпта (шрифт · размер)",
    fontSystem: "Системный",
    fontSerif: "С засечками",
    fontMono: "Моноширинный",
    fontScript: "Рукописный",
    sizeSmall: "Мелкий",
    sizeMedium: "Средний",
    sizeLarge: "Крупный",
    sizeXL: "Очень крупный",
    hideInView: "Скрыть в этом виде",
    viewsLabel: "Виды (макс. 20)",
    addView: "+ Добавить вид",
    viewNamePh: "Название вида",
    minimizeOnClose: "Сворачивать в фон при закрытии",
    autostart: "Запускать автоматически при входе",
    startMinimized: "Запускать свёрнутым при автозапуске",
    importExport: "Импорт / экспорт промптов",
    import: "Импорт",
    imported: "промптов импортировано",
    importFailed: "Ошибка импорта",
    deleteAll: "Удалить ВСЕ данные и настройки!",
    deleteAllConfirm: "Действительно удалить ВСЕ данные и настройки?",
    cancel: "Отмена",
    save: "Сохранить",
    edit: "Изменить",
    delete: "Удалить",
    toggleFloating: "Плавающая кнопка вкл/выкл",
    nameModalTitle: "Назовите этот промпт",
    editModalTitle: "Изменить промпт",
    namePh: "Название промпта",
    promptPh: "Введите промпт…",
    unplaced: "Неразмещённые промпты",
    trayHint: "Клик: свободная ячейка · Перетащите на сетку: разместить/поменять",
    gridFull: "Сетка заполнена – перетащите на плитку для обмена",
    tileTooltip: "Клик: копировать  |  Перетащить: переместить  |  ПКМ: меню",
    actions: "Действия",
    exportFailed: "Ошибка экспорта",
    copied: "Скопировано!",
  },
  zh: {
    settings: "设置",
    theme: "主题",
    themeSystem: "跟随系统",
    themeLight: "浅色",
    themeDark: "深色",
    language: "语言",
    langAuto: "自动",
    tileText: "提示词文本（字体 · 大小）",
    fontSystem: "系统",
    fontSerif: "衬线",
    fontMono: "等宽",
    fontScript: "手写",
    sizeSmall: "小",
    sizeMedium: "中",
    sizeLarge: "大",
    sizeXL: "特大",
    hideInView: "在此视图中隐藏",
    viewsLabel: "视图（最多 20 个）",
    addView: "+ 添加视图",
    viewNamePh: "视图名称",
    minimizeOnClose: "关闭时最小化到后台",
    autostart: "登录时自动启动",
    startMinimized: "自动启动时最小化",
    importExport: "导入 / 导出提示词",
    import: "导入",
    imported: "条提示词已导入",
    importFailed: "导入失败",
    deleteAll: "删除所有数据和设置！",
    deleteAllConfirm: "确定删除所有数据和设置？",
    cancel: "取消",
    save: "保存",
    edit: "编辑",
    delete: "删除",
    toggleFloating: "悬浮按钮开/关",
    nameModalTitle: "为提示词命名",
    editModalTitle: "编辑提示词",
    namePh: "提示词名称",
    promptPh: "输入提示词…",
    unplaced: "未放置的提示词",
    trayHint: "点击：放入空格 · 拖到网格：放置/交换",
    gridFull: "网格已满 – 拖到方块上交换",
    tileTooltip: "点击：复制  |  拖动：移动  |  右键：菜单",
    actions: "操作",
    exportFailed: "导出失败",
    copied: "已复制！",
  },
  ja: {
    settings: "設定",
    theme: "テーマ",
    themeSystem: "システム",
    themeLight: "ライト",
    themeDark: "ダーク",
    language: "言語",
    langAuto: "自動",
    tileText: "プロンプト文字（フォント · サイズ）",
    fontSystem: "システム",
    fontSerif: "明朝",
    fontMono: "等幅",
    fontScript: "手書き",
    sizeSmall: "小",
    sizeMedium: "中",
    sizeLarge: "大",
    sizeXL: "特大",
    hideInView: "このビューで隠す",
    viewsLabel: "ビュー（最大20）",
    addView: "+ ビューを追加",
    viewNamePh: "ビュー名",
    minimizeOnClose: "閉じるときバックグラウンドへ最小化",
    autostart: "ログイン時に自動起動",
    startMinimized: "自動起動時に最小化で開始",
    importExport: "プロンプトのインポート / エクスポート",
    import: "インポート",
    imported: "件のプロンプトをインポート",
    importFailed: "インポートに失敗しました",
    deleteAll: "すべてのデータと設定を削除！",
    deleteAllConfirm: "本当にすべてのデータと設定を削除しますか？",
    cancel: "キャンセル",
    save: "保存",
    edit: "編集",
    delete: "削除",
    toggleFloating: "フローティングボタン切替",
    nameModalTitle: "プロンプトに名前を付ける",
    editModalTitle: "プロンプトを編集",
    namePh: "プロンプト名",
    promptPh: "プロンプトを入力…",
    unplaced: "未配置のプロンプト",
    trayHint: "クリック：空きセルへ · グリッドへドラッグ：配置/入替",
    gridFull: "グリッドが満杯 – タイルにドラッグして入替",
    tileTooltip: "クリック：コピー  |  ドラッグ：移動  |  右クリック：メニュー",
    actions: "操作",
    exportFailed: "エクスポートに失敗しました",
    copied: "コピーしました！",
  },
};
// Resolved at init from settings.language ("auto" -> OS language). EN fallback.
const LANGS = ["en", "de", "es", "fr", "it", "pt", "pl", "ru", "zh", "ja"];
let LANG = "en";
function resolveLang(pref) {
  const p = (pref && pref !== "auto" ? pref : (navigator.language || "en")).toLowerCase();
  return LANGS.find((code) => p.startsWith(code)) || "en";
}
const t = (key) => I18N[LANG][key] ?? I18N.en[key] ?? key;

function applyI18n() {
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => { el.title = t(el.dataset.i18nTitle); });
}

// ---- Helpers ----
function applyTheme(theme) {
  document.documentElement.setAttribute("data-theme", theme);
}

// Tile color palette ("" = default surface).
const COLORS = ["", "#3b82f6", "#22c55e", "#ef4444", "#f59e0b", "#8b5cf6", "#ec4899", "#14b8a6", "#64748b"];

// Font options for saved prompt tiles (all Windows system fonts).
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

function applyTileStyle() {
  const root = document.documentElement.style;
  root.setProperty("--tile-font", FONTS[settings.tile_font] || FONTS.system);
  // tile_size 0 = auto-fit (per-tile, handled after every grid render).
  root.setProperty("--tile-size", `${settings.tile_size || 15}px`);
  fitCache.clear(); // font metrics changed -> cached fit sizes are stale
}

// Auto-fit: grow/shrink each tile's text so the WHOLE text is visible at the
// largest possible size.
// Cache fit results per (text, cell size); same-size cells with the same text
// skip the measuring loop entirely. Cleared on font change / grown too large.
const fitCache = new Map();

function fitTileText(tile) {
  const name = tile.querySelector(".tile-name");
  if (!name) return;
  name.classList.add("fit");
  const maxH = tile.clientHeight - 14;
  const maxW = name.clientWidth; // the real wrapping width of the text block
  const key = `${name.textContent}|${maxW}x${maxH}`;
  const cached = fitCache.get(key);
  if (cached) {
    name.style.fontSize = `${cached}px`;
    return;
  }
  // Binary search for the LARGEST size where the fully wrapped text fits:
  // height = all lines visible (text wraps freely); width check ONLY catches
  // a single unbreakable word overflowing the tile. NOTE: the block is
  // width:100%, so scrollWidth always equals maxW unless a word overflows —
  // any "maxW - x" margin here would make the check always fail and collapse
  // the search to the 8px minimum.
  const fits = (s) => {
    name.style.fontSize = `${s}px`;
    return name.scrollHeight <= maxH && name.scrollWidth <= maxW;
  };
  let lo = 8;
  let hi = Math.max(8, Math.min(96, maxH));
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (fits(mid)) lo = mid;
    else hi = mid - 1;
  }
  name.style.fontSize = `${lo}px`;
  if (fitCache.size > 1000) fitCache.clear();
  fitCache.set(key, lo);
}

function fitAllTiles() {
  if (Number(settings.tile_size) !== 0) return;
  document.querySelectorAll(".tile").forEach(fitTileText);
}

// Re-fit on window resize (cells change size with the window).
let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(fitAllTiles);
});

function toast(msg) {
  toastEl.textContent = msg;
  toastEl.classList.remove("hidden");
  void toastEl.offsetWidth;
  toastEl.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.classList.remove("show");
    setTimeout(() => toastEl.classList.add("hidden"), 200);
  }, 1400);
}

function autoGrow(el) {
  el.style.height = "auto";
  const target = el.scrollHeight + 2;
  el.style.height = `${Math.min(target, INPUT_MAX)}px`;
  el.style.overflowY = target > INPUT_MAX ? "auto" : "hidden";
}

const cellKey = (c, r) => `${c},${r}`;

// ---- View helpers ----
function activeView() {
  return settings.views.find((v) => v.id === settings.active_view) || settings.views[0];
}
const gridKeyOf = (v) => `${v.cols}x${v.rows}`;
const layoutOf = (v) => v.layouts?.[gridKeyOf(v)] || {};

// ---- Layout normalization (active view, current grid size) ----
function firstFree(occupied, cols, rows) {
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      if (!occupied.has(cellKey(c, r))) return [c, r];
    }
  }
  return null;
}

function normalizeLayout(view) {
  const { cols, rows } = view;
  const layout = { ...layoutOf(view) };
  const occupied = new Map();
  const ids = new Set(prompts.map((p) => p.id));
  let changed = false;

  for (const id of Object.keys(layout)) {
    if (!ids.has(id)) { delete layout[id]; changed = true; }
  }
  for (const p of prompts) {
    const cell = layout[p.id];
    if (cell && cell[0] < cols && cell[1] < rows && !occupied.has(cellKey(...cell))) {
      occupied.set(cellKey(...cell), p.id);
    } else if (cell) {
      delete layout[p.id];
      changed = true;
    }
  }
  // Unplaced prompts stay in the overflow tray. No auto-fill: per-grid-size
  // arrangements and freshly created (empty) views must stay untouched.
  view.layouts[gridKeyOf(view)] = layout;
  return changed;
}

// ---- Render ----
// skipFetch: caller already updated the local state (drag/hide hot path) —
// render instantly without an IPC roundtrip.
async function renderGrid(skipFetch = false) {
  if (!skipFetch) {
    const s = await invoke("get_state"); // one roundtrip for prompts + settings
    prompts = s.prompts;
    settings = s.settings;
  }
  const view = activeView();
  if (normalizeLayout(view)) {
    invoke("set_layout", { layout: layoutOf(view) }).catch(() => {});
  }

  const { cols, rows } = view;
  const layout = layoutOf(view);
  gridEl.style.gridTemplateColumns = `repeat(${cols}, 1fr)`;
  gridEl.style.gridTemplateRows = `repeat(${rows}, 1fr)`;

  const byCell = new Map();
  overflow = [];
  for (const p of prompts) {
    const cell = layout[p.id];
    if (cell) byCell.set(cellKey(...cell), p);
    else overflow.push(p);
  }

  // Build off-DOM, attach once (1 reflow instead of cols*rows).
  const frag = document.createDocumentFragment();
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cellEl = document.createElement("div");
      cellEl.className = "cell";
      cellEl.dataset.col = c;
      cellEl.dataset.row = r;
      const p = byCell.get(cellKey(c, r));
      if (p) cellEl.appendChild(buildTile(p));
      frag.appendChild(cellEl);
    }
  }
  gridEl.innerHTML = "";
  gridEl.appendChild(frag);

  renderViews();
  renderTray();
  fitAllTiles();
}

// Header view-switcher buttons (hidden while only one view exists).
function renderViews() {
  viewsEl.classList.toggle("hidden", settings.views.length <= 1);
  viewsEl.innerHTML = "";
  for (const v of settings.views) {
    const btn = document.createElement("button");
    btn.className = "view-btn" + (v.id === settings.active_view ? " active" : "");
    btn.textContent = v.name;
    btn.title = v.name;
    btn.addEventListener("click", async () => {
      settings = await invoke("set_active_view", { id: v.id });
      await renderGrid(true); // prompts unchanged, settings already fresh
    });
    viewsEl.appendChild(btn);
  }
}

function buildTile(p) {
  const tile = document.createElement("div");
  tile.className = "tile";
  tile.dataset.id = p.id;
  tile.title = `${p.name}\n${t("tileTooltip")}`;

  if (p.color) {
    tile.classList.add("tinted");
    tile.style.background = p.color;
    tile.style.borderColor = p.color;
  }

  const name = document.createElement("span");
  name.className = "tile-name";
  name.textContent = p.name;
  tile.appendChild(name);

  const menuBtn = document.createElement("button");
  menuBtn.className = "tile-menu";
  menuBtn.innerHTML = DOTS;
  menuBtn.title = t("actions");
  menuBtn.addEventListener("pointerdown", (e) => e.stopPropagation());
  menuBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    const r = menuBtn.getBoundingClientRect();
    openCtx(p.id, r.left, r.bottom + 4);
  });
  tile.appendChild(menuBtn);

  tile.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.target.closest(".tile-menu")) return;
    drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile };
  });
  tile.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openCtx(p.id, e.clientX, e.clientY);
  });
  return tile;
}

// ---- Overflow tray ----
function renderTray() {
  trayBtn.classList.toggle("hidden", overflow.length === 0);
  $("tray-count").textContent = overflow.length;
  if (overflow.length === 0) trayMenu.classList.add("hidden");

  trayMenu.innerHTML = "";
  const hint = document.createElement("div");
  hint.className = "hint";
  hint.textContent = t("trayHint");
  trayMenu.appendChild(hint);

  for (const p of overflow) {
    const item = document.createElement("button");
    item.className = "tray-item";
    item.title = p.text;

    const dot = document.createElement("span");
    dot.className = "dot";
    const label = document.createElement("span");
    label.textContent = p.name;
    item.append(dot, label);

    item.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: item };
    });
    item.addEventListener("click", async () => {
      const view = activeView();
      const occupied = new Map(
        Object.entries(layoutOf(view)).map(([id, c]) => [cellKey(...c), id])
      );
      const free = firstFree(occupied, view.cols, view.rows);
      if (!free) { toast(t("gridFull")); return; }
      await placeTile(p.id, free[0], free[1]);
    });
    trayMenu.appendChild(item);
  }
}

function toggleTrayMenu() {
  if (trayMenu.classList.contains("hidden")) {
    trayMenu.classList.remove("hidden");
    const r = trayBtn.getBoundingClientRect();
    trayMenu.style.top = `${r.bottom + 6}px`;
    trayMenu.style.left = `${Math.max(8, r.right - trayMenu.offsetWidth)}px`;
  } else {
    trayMenu.classList.add("hidden");
  }
}

// ---- Pointer-based drag to any cell ----
function cellAt(x, y) {
  return document.elementFromPoint(x, y)?.closest(".cell") || null;
}

// Track the hovered cell instead of querying all cells on every mousemove.
let hoverCell = null;

function setHoverCell(cell) {
  if (cell === hoverCell) return;
  hoverCell?.classList.remove("drag-over");
  hoverCell = cell;
  hoverCell?.classList.add("drag-over");
}

function endDragVisuals() {
  document.body.classList.remove("drag-active");
  setHoverCell(null);
}

window.addEventListener("pointermove", (e) => {
  if (!drag) return;
  if (!drag.moved) {
    if (Math.abs(e.clientX - drag.startX) < DRAG_THRESHOLD &&
        Math.abs(e.clientY - drag.startY) < DRAG_THRESHOLD) return;
    drag.moved = true;
    drag.el.classList.add("dragging");
    document.body.classList.add("drag-active");
    trayMenu.classList.add("hidden");
    // Live ghost: a clone of the tile follows the cursor.
    const r = drag.el.getBoundingClientRect();
    drag.offX = drag.startX - r.left;
    drag.offY = drag.startY - r.top;
    drag.ghost = drag.el.cloneNode(true);
    drag.ghost.classList.add("drag-ghost");
    drag.ghost.classList.remove("dragging");
    drag.ghost.style.width = `${r.width}px`;
    drag.ghost.style.height = `${r.height}px`;
    document.body.appendChild(drag.ghost);
  }
  drag.ghost.style.transform =
    `translate(${e.clientX - drag.offX}px, ${e.clientY - drag.offY}px)`;
  setHoverCell(cellAt(e.clientX, e.clientY));
});

window.addEventListener("pointerup", async (e) => {
  if (!drag) return;
  const { id, moved, el, ghost } = drag;
  drag = null;
  ghost?.remove();
  el.classList.remove("dragging");
  endDragVisuals();

  if (!moved) {
    if (el.classList.contains("tile") && (await invoke("copy_prompt", { id }))) {
      showCopied(el);
    }
    return;
  }
  const cell = cellAt(e.clientX, e.clientY);
  if (!cell) return;
  await placeTile(id, Number(cell.dataset.col), Number(cell.dataset.row));
});

window.addEventListener("pointercancel", () => {
  if (drag) {
    drag.el.classList.remove("dragging");
    drag.ghost?.remove();
  }
  drag = null;
  endDragVisuals();
});

// Flash + small "Copied!" bubble at the bottom of the tile.
function showCopied(tile) {
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), 350);
  const pop = document.createElement("div");
  pop.className = "copy-pop";
  pop.textContent = t("copied");
  tile.appendChild(pop);
  setTimeout(() => pop.remove(), 950);
}

// Place a tile at [col,row] in the active view. Swaps with the occupant if the
// source had a cell; otherwise (from tray) the occupant moves to the tray.
// Renders instantly from local state; persistence runs in the background.
async function placeTile(id, col, row) {
  const view = activeView();
  const layout = { ...layoutOf(view) };
  const old = layout[id];
  const occupant = Object.entries(layout).find(
    ([oid, c]) => oid !== id && c[0] === col && c[1] === row
  );
  if (occupant) {
    if (old) layout[occupant[0]] = old;
    else delete layout[occupant[0]];
  }
  layout[id] = [col, row];
  view.layouts[gridKeyOf(view)] = layout;
  invoke("set_layout", { layout }).catch((e) => toast(String(e)));
  await renderGrid(true);
}

// ---- Context menu ----
function openCtx(id, x, y) {
  ctxId = id;
  ctxEl.classList.remove("hidden");
  const w = ctxEl.offsetWidth, h = ctxEl.offsetHeight;
  ctxEl.style.left = `${Math.min(x, window.innerWidth - w - 4)}px`;
  ctxEl.style.top = `${Math.min(y, window.innerHeight - h - 4)}px`;
}
function closeCtx() {
  ctxEl.classList.add("hidden");
  ctxId = null;
}

// ---- Modal ----
function renderSwatches(selected) {
  const row = $("color-row");
  row.innerHTML = "";
  for (const c of COLORS) {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = "swatch" + (c === "" ? " none" : "") + (c === selected ? " sel" : "");
    if (c) sw.style.background = c;
    sw.addEventListener("click", () => {
      modalState.color = c;
      renderSwatches(c);
    });
    row.appendChild(sw);
  }
}

function openModal({ mode, id, name = "", text = "", color = "", title }) {
  modalState = { mode, id, color };
  modal.title.textContent = title;
  modal.name.value = name;
  modal.text.value = text;
  modal.text.classList.toggle("hidden", mode !== "edit");
  renderSwatches(color);
  modal.root.classList.remove("hidden");
  modal.name.focus();
  modal.name.select();
}
function closeModal() {
  modal.root.classList.add("hidden");
  modalState = null;
}

function startCreate() {
  if (!inputEl.value.trim()) return;
  openModal({ mode: "create", title: t("nameModalTitle") });
}

async function confirmModal() {
  if (!modalState) return;
  const name = modal.name.value.trim();
  if (!name) { modal.name.focus(); return; }

  const color = modalState.color || "";
  if (modalState.mode === "create") {
    await invoke("add_prompt", { name, text: inputEl.value.trim(), color });
    inputEl.value = "";
    autoGrow(inputEl);
    saveBtn.disabled = true;
  } else {
    await invoke("update_prompt", { id: modalState.id, name, text: modal.text.value, color });
  }
  closeModal();
  await renderGrid();
}

async function editPrompt(id) {
  const p = await invoke("get_prompt", { id });
  if (p) {
    openModal({
      mode: "edit",
      id,
      name: p.name,
      text: p.text,
      color: p.color || "",
      title: t("editModalTitle"),
    });
  }
}

// ---- Settings: views editor ----
function renderViewsEditor() {
  const editor = $("views-editor");
  editor.innerHTML = "";
  for (const v of settings.views) {
    const row = document.createElement("div");
    row.className = "view-row";

    const input = document.createElement("input");
    input.className = "modal-input";
    input.type = "text";
    input.maxLength = 30;
    input.value = v.name;
    input.placeholder = t("viewNamePh");
    input.addEventListener("change", async () => {
      settings = await invoke("rename_view", { id: v.id, name: input.value });
      renderViews();
      renderViewsEditor();
    });
    row.appendChild(input);

    // Per-view grid size (columns × rows), applied on change.
    const grid = document.createElement("span");
    grid.className = "view-grid";
    const mkNum = (value) => {
      const n = document.createElement("input");
      n.className = "grid-mini";
      n.type = "number";
      n.min = 1;
      n.max = 20;
      n.value = value;
      return n;
    };
    const colsIn = mkNum(v.cols);
    const rowsIn = mkNum(v.rows);
    const applyGrid = async () => {
      const cols = Math.min(20, Math.max(1, Math.round(Number(colsIn.value) || v.cols)));
      const rows = Math.min(20, Math.max(1, Math.round(Number(rowsIn.value) || v.rows)));
      settings = await invoke("set_view_grid", { id: v.id, cols, rows });
      renderViewsEditor();
      if (v.id === settings.active_view) await renderGrid(true);
    };
    colsIn.addEventListener("change", applyGrid);
    rowsIn.addEventListener("change", applyGrid);
    const times = document.createElement("span");
    times.className = "times";
    times.textContent = "×";
    grid.append(colsIn, times, rowsIn);
    row.appendChild(grid);

    if (settings.views.length > 1) {
      const del = document.createElement("button");
      del.className = "icon-btn";
      del.innerHTML = CROSS;
      del.title = t("delete");
      // Two-step confirm: first click arms (red), second click deletes.
      del.addEventListener("click", async () => {
        if (!del.classList.contains("confirm")) {
          del.classList.add("confirm");
          del.title = `${t("delete")}?`;
          setTimeout(() => {
            del.classList.remove("confirm");
            del.title = t("delete");
          }, 3000);
          return;
        }
        try {
          settings = await invoke("delete_view", { id: v.id });
          renderViewsEditor();
          await renderGrid(true);
        } catch (err) {
          toast(String(err));
        }
      });
      row.appendChild(del);
    }
    editor.appendChild(row);
  }
  // At the limit the add button disappears entirely.
  $("view-add").classList.toggle("hidden", settings.views.length >= MAX_VIEWS);
}

// ---- Settings actions ----
async function runExport(format) {
  try {
    await invoke("export_prompts", { format });
  } catch (err) {
    if (String(err) !== "canceled") toast(`${t("exportFailed")}: ${err}`);
  }
}

async function runImport() {
  try {
    const count = await invoke("import_prompts");
    toast(`${count} ${t("imported")}`);
    await renderGrid();
    renderViewsEditor();
  } catch (err) {
    if (String(err) !== "canceled") toast(`${t("importFailed")}: ${err}`);
  }
}

async function deleteAll() {
  const btn = $("delete-all");
  if (!btn.classList.contains("confirm")) {
    btn.classList.add("confirm");
    btn.textContent = t("deleteAllConfirm");
    clearTimeout(deleteAllTimer);
    deleteAllTimer = setTimeout(() => {
      btn.classList.remove("confirm");
      btn.textContent = t("deleteAll");
    }, 3000);
    return;
  }
  clearTimeout(deleteAllTimer);
  btn.classList.remove("confirm");
  btn.textContent = t("deleteAll");
  await invoke("delete_all_data");
  location.reload(); // full re-init: default theme, views, toggles, grid
}

// ---- Wire events ----
function bind() {
  inputEl.addEventListener("input", () => {
    autoGrow(inputEl);
    saveBtn.disabled = !inputEl.value.trim();
  });
  inputEl.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.key === "Enter") { e.preventDefault(); startCreate(); }
  });
  saveBtn.addEventListener("click", startCreate);

  modal.confirm.addEventListener("click", confirmModal);
  modal.cancel.addEventListener("click", closeModal);
  modal.name.addEventListener("keydown", (e) => { if (e.key === "Enter") confirmModal(); });
  // pointerdown (not click): selecting text that ends outside an input must not close.
  modal.root.addEventListener("pointerdown", (e) => { if (e.target === modal.root) closeModal(); });

  $("gear").addEventListener("click", () => {
    renderViewsEditor();
    settingsEl.classList.remove("hidden");
  });
  $("settings-close").addEventListener("click", () => settingsEl.classList.add("hidden"));
  // pointerdown (not click): selecting text that ends outside an input must not close.
  settingsEl.addEventListener("pointerdown", (e) => { if (e.target === settingsEl) settingsEl.classList.add("hidden"); });

  $("view-add").addEventListener("click", async () => {
    try {
      settings = await invoke("add_view", { name: "" });
      renderViewsEditor();
      await renderGrid(true);
    } catch (err) {
      toast(String(err));
    }
  });

  $("opt-minimize").addEventListener("change", (e) => {
    invoke("set_minimize_on_close", { enabled: e.target.checked });
  });
  $("opt-autostart").addEventListener("change", async (e) => {
    try {
      await invoke("set_autostart", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      toast(String(err));
    }
  });
  $("opt-startmin").addEventListener("change", async (e) => {
    try {
      await invoke("set_start_minimized", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      toast(String(err));
    }
  });

  $("import-btn").addEventListener("click", runImport);
  $("export-csv").addEventListener("click", () => runExport("csv"));
  $("export-txt").addEventListener("click", () => runExport("txt"));
  $("delete-all").addEventListener("click", deleteAll);
  trayBtn.addEventListener("click", toggleTrayMenu);

  const themeSelect = $("theme-select");
  themeSelect.addEventListener("change", async () => {
    applyTheme(await invoke("set_theme", { theme: themeSelect.value }));
  });

  // Language: persist, then reload so every string re-renders translated.
  $("lang-select").addEventListener("change", async (e) => {
    await invoke("set_language", { lang: e.target.value });
    location.reload();
  });

  // Prompt-tile font + size.
  const tileStyleChanged = async () => {
    settings.tile_font = $("tile-font").value;
    settings.tile_size = Number($("tile-size").value); // 0 = auto-fit
    applyTileStyle();
    invoke("set_tile_style", { font: settings.tile_font, size: settings.tile_size }).catch(() => {});
    await renderGrid(true); // re-render so auto-fit (or fixed size) applies cleanly
  };
  $("tile-font").addEventListener("change", tileStyleChanged);
  $("tile-size").addEventListener("change", tileStyleChanged);

  ctxEl.addEventListener("click", async (e) => {
    const act = e.target.closest("button")?.dataset.act;
    if (!act || !ctxId) return;
    const id = ctxId;
    closeCtx();
    if (act === "edit") {
      await editPrompt(id);
    } else if (act === "hide") {
      // Remove from the active view's current grid -> behaves like unplaced (tray).
      const view = activeView();
      const layout = { ...layoutOf(view) };
      delete layout[id];
      view.layouts[gridKeyOf(view)] = layout;
      invoke("set_layout", { layout }).catch((e) => toast(String(e)));
      await renderGrid(true);
    } else if (act === "delete") {
      await invoke("delete_prompt", { id });
      await renderGrid();
    } else if (act === "pin") {
      await invoke("toggle_floating", { id });
    }
  });

  document.addEventListener("pointerdown", (e) => {
    if (!ctxEl.classList.contains("hidden") && !ctxEl.contains(e.target)) closeCtx();
    if (!trayMenu.classList.contains("hidden") &&
        !trayMenu.contains(e.target) && !trayBtn.contains(e.target)) {
      trayMenu.classList.add("hidden");
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!trayMenu.classList.contains("hidden")) trayMenu.classList.add("hidden");
    else if (!modal.root.classList.contains("hidden")) closeModal();
    else if (!settingsEl.classList.contains("hidden")) settingsEl.classList.add("hidden");
  });

  listen("theme-changed", (e) => applyTheme(e.payload));
  // "Edit prompt" chosen in a floating pill's right-click menu.
  listen("edit-prompt", (e) => editPrompt(String(e.payload)));
}

// ---- Init ----
async function init() {
  settings = await invoke("get_settings");
  LANG = resolveLang(settings.language);
  applyI18n();
  applyTheme(await invoke("current_theme"));
  bind();
  await renderGrid();
  $("theme-select").value = settings.theme;
  $("lang-select").value = settings.language || "auto";
  $("opt-minimize").checked = settings.minimize_to_tray === true;
  $("opt-autostart").checked = settings.autostart === true;
  $("opt-startmin").checked = settings.start_minimized === true;
  $("tile-font").value = settings.tile_font || "system";
  $("tile-size").value = String(settings.tile_size ?? 0);
  applyTileStyle();
  autoGrow(inputEl);
  inputEl.focus();
}

init();
