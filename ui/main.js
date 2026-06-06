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
const libraryEl = $("library");
const viewsEl = $("views");
const modal = {
  root: $("modal"),
  title: $("modal-title"),
  name: $("modal-name"),
  text: $("modal-text"),
  cancel: $("modal-cancel"),
  confirm: $("modal-confirm"),
  delete: $("modal-delete"),
  imgWrap: $("modal-img-wrap"),
  img: $("modal-img"),
  showImage: $("modal-show-image"),
  showText: $("modal-show-text"),
  replaceImg: $("modal-replace-img"),
  removeImg: $("modal-remove-img"),
  addIcon: $("modal-add-icon"),
};

const DRAG_THRESHOLD = 5;
const INPUT_MAX = 160; // keep in sync with .input max-height
const MAX_VIEWS = 20;
const GRID_MAX = 20; // keep in sync with backend GRID_MAX
const PREVIEW_MAX = 220; // tooltip preview length of the prompt text

const clampGrid = (n, fallback) =>
  Math.min(GRID_MAX, Math.max(1, Math.round(Number(n) || fallback)));

// Cached state (refreshed by renderGrid).
let prompts = [];
let settings = { theme: "system", views: [], active_view: "" };

let modalState = null;
let ctxId = null;
let drag = null;
let toastTimer = null;
let deleteAllTimer = null;

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => toast(String(e.message)));
window.addEventListener("unhandledrejection", (e) => toast(String(e.reason)));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

const DOTS =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z"/></svg>';
const CROSS =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';
const GRID_PLUS =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M4 4h7v7H4V4Zm9 0h7v7h-7V4ZM4 13h7v7H4v-7Zm12 0h2v3h3v2h-3v3h-2v-3h-3v-2h3v-3Z"/></svg>';

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
    addToLayout: "Add to layout",
    gridFull: "Grid is full – drag onto a tile to swap",
    tileTooltip: "Click: copy  |  Drag: move  |  Right-click: menu",
    actions: "Actions",
    exportFailed: "Export failed",
    copied: "Copied!",
    library: "All prompts",
    libraryEmpty: "No prompts saved yet",
    addImage: "Add image",
    showImageBtn: "Image",
    showTextBtn: "Text",
    replaceImage: "Replace",
    imageModalTitle: "Name this image",
    imageEditTitle: "Edit image",
    imageNamePh: "Image name",
    addIcon: "Image as icon",
    removeImage: "Remove image",
    gridSize: "Grid size",
    deleteConfirm: "Really delete?",
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
    addToLayout: "Zum Layout hinzufügen",
    gridFull: "Raster ist voll – zum Tauschen auf ein Feld ziehen",
    tileTooltip: "Klick: kopieren  |  Ziehen: verschieben  |  Rechtsklick: Menü",
    actions: "Aktionen",
    exportFailed: "Export fehlgeschlagen",
    copied: "Kopiert!",
    library: "Alle Prompts",
    libraryEmpty: "Noch keine Prompts gespeichert",
    addImage: "Bild hinzufügen",
    showImageBtn: "Bild",
    showTextBtn: "Text",
    replaceImage: "Ersetzen",
    imageModalTitle: "Bild benennen",
    imageEditTitle: "Bild bearbeiten",
    imageNamePh: "Bild-Name",
    addIcon: "Bild als Icon",
    removeImage: "Bild entfernen",
    gridSize: "Rastergröße",
    deleteConfirm: "Wirklich löschen?",
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
    addToLayout: "Añadir al diseño",
    gridFull: "Cuadrícula llena: arrastra sobre una celda para intercambiar",
    tileTooltip: "Clic: copiar  |  Arrastrar: mover  |  Clic derecho: menú",
    actions: "Acciones",
    exportFailed: "Error al exportar",
    copied: "¡Copiado!",
    library: "Todos los prompts",
    libraryEmpty: "Aún no hay prompts guardados",
    addImage: "Añadir imagen",
    showImageBtn: "Imagen",
    showTextBtn: "Texto",
    replaceImage: "Reemplazar",
    imageModalTitle: "Nombra esta imagen",
    imageEditTitle: "Editar imagen",
    imageNamePh: "Nombre de la imagen",
    addIcon: "Imagen como icono",
    removeImage: "Quitar imagen",
    gridSize: "Tamaño de la cuadrícula",
    deleteConfirm: "¿Eliminar realmente?",
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
    addToLayout: "Ajouter à la grille",
    gridFull: "Grille pleine – glissez sur une case pour échanger",
    tileTooltip: "Clic : copier  |  Glisser : déplacer  |  Clic droit : menu",
    actions: "Actions",
    exportFailed: "Échec de l'export",
    copied: "Copié !",
    library: "Tous les prompts",
    libraryEmpty: "Aucun prompt enregistré",
    addImage: "Ajouter une image",
    showImageBtn: "Image",
    showTextBtn: "Texte",
    replaceImage: "Remplacer",
    imageModalTitle: "Nommer cette image",
    imageEditTitle: "Modifier l'image",
    imageNamePh: "Nom de l'image",
    addIcon: "Image comme icône",
    removeImage: "Retirer l'image",
    gridSize: "Taille de la grille",
    deleteConfirm: "Vraiment supprimer ?",
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
    addToLayout: "Aggiungi alla griglia",
    gridFull: "Griglia piena: trascina su una cella per scambiare",
    tileTooltip: "Clic: copia  |  Trascina: sposta  |  Clic destro: menu",
    actions: "Azioni",
    exportFailed: "Esportazione non riuscita",
    copied: "Copiato!",
    library: "Tutti i prompt",
    libraryEmpty: "Nessun prompt salvato",
    addImage: "Aggiungi immagine",
    showImageBtn: "Immagine",
    showTextBtn: "Testo",
    replaceImage: "Sostituisci",
    imageModalTitle: "Assegna nome all'immagine",
    imageEditTitle: "Modifica immagine",
    imageNamePh: "Nome dell'immagine",
    addIcon: "Immagine come icona",
    removeImage: "Rimuovi immagine",
    gridSize: "Dimensione griglia",
    deleteConfirm: "Eliminare davvero?",
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
    addToLayout: "Adicionar à grade",
    gridFull: "Grade cheia – arraste sobre uma célula para trocar",
    tileTooltip: "Clique: copiar  |  Arrastar: mover  |  Clique direito: menu",
    actions: "Ações",
    exportFailed: "Falha na exportação",
    copied: "Copiado!",
    library: "Todos os prompts",
    libraryEmpty: "Nenhum prompt salvo ainda",
    addImage: "Adicionar imagem",
    showImageBtn: "Imagem",
    showTextBtn: "Texto",
    replaceImage: "Substituir",
    imageModalTitle: "Nomear esta imagem",
    imageEditTitle: "Editar imagem",
    imageNamePh: "Nome da imagem",
    addIcon: "Imagem como ícone",
    removeImage: "Remover imagem",
    gridSize: "Tamanho da grade",
    deleteConfirm: "Excluir mesmo?",
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
    addToLayout: "Dodaj do siatki",
    gridFull: "Siatka pełna – przeciągnij na kafelek, aby zamienić",
    tileTooltip: "Klik: kopiuj  |  Przeciągnij: przenieś  |  PPM: menu",
    actions: "Akcje",
    exportFailed: "Eksport nie powiódł się",
    copied: "Skopiowano!",
    library: "Wszystkie prompty",
    libraryEmpty: "Brak zapisanych promptów",
    addImage: "Dodaj obraz",
    showImageBtn: "Obraz",
    showTextBtn: "Tekst",
    replaceImage: "Zamień",
    imageModalTitle: "Nazwij ten obraz",
    imageEditTitle: "Edytuj obraz",
    imageNamePh: "Nazwa obrazu",
    addIcon: "Obraz jako ikona",
    removeImage: "Usuń obraz",
    gridSize: "Rozmiar siatki",
    deleteConfirm: "Na pewno usunąć?",
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
    addToLayout: "Добавить в сетку",
    gridFull: "Сетка заполнена – перетащите на плитку для обмена",
    tileTooltip: "Клик: копировать  |  Перетащить: переместить  |  ПКМ: меню",
    actions: "Действия",
    exportFailed: "Ошибка экспорта",
    copied: "Скопировано!",
    library: "Все промпты",
    libraryEmpty: "Промптов пока нет",
    addImage: "Добавить изображение",
    showImageBtn: "Изображение",
    showTextBtn: "Текст",
    replaceImage: "Заменить",
    imageModalTitle: "Назовите изображение",
    imageEditTitle: "Изменить изображение",
    imageNamePh: "Название изображения",
    addIcon: "Изображение как значок",
    removeImage: "Убрать изображение",
    gridSize: "Размер сетки",
    deleteConfirm: "Точно удалить?",
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
    addToLayout: "添加到网格",
    gridFull: "网格已满 – 拖到方块上交换",
    tileTooltip: "点击：复制  |  拖动：移动  |  右键：菜单",
    actions: "操作",
    exportFailed: "导出失败",
    copied: "已复制！",
    library: "全部提示词",
    libraryEmpty: "还没有保存的提示词",
    addImage: "添加图片",
    showImageBtn: "图片",
    showTextBtn: "文本",
    replaceImage: "替换",
    imageModalTitle: "为图片命名",
    imageEditTitle: "编辑图片",
    imageNamePh: "图片名称",
    addIcon: "图片作为图标",
    removeImage: "移除图片",
    gridSize: "网格大小",
    deleteConfirm: "确定删除？",
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
    addToLayout: "グリッドに追加",
    gridFull: "グリッドが満杯 – タイルにドラッグして入替",
    tileTooltip: "クリック：コピー  |  ドラッグ：移動  |  右クリック：メニュー",
    actions: "操作",
    exportFailed: "エクスポートに失敗しました",
    copied: "コピーしました！",
    library: "すべてのプロンプト",
    libraryEmpty: "保存されたプロンプトはありません",
    addImage: "画像を追加",
    showImageBtn: "画像",
    showTextBtn: "テキスト",
    replaceImage: "置き換え",
    imageModalTitle: "画像に名前を付ける",
    imageEditTitle: "画像を編集",
    imageNamePh: "画像名",
    addIcon: "画像をアイコンに",
    removeImage: "画像を削除",
    gridSize: "グリッドサイズ",
    deleteConfirm: "本当に削除？",
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

// Tile color palette ("" = default surface), full spectrum, one modal row.
const COLORS = [
  "", "#ef4444", "#f97316", "#f59e0b", "#eab308", "#22c55e", "#14b8a6",
  "#06b6d4", "#3b82f6", "#6366f1", "#8b5cf6", "#ec4899", "#64748b",
];

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

// Auto-fit cache per (text, cell size); cleared on font changes.
const fitCache = new Map();

function fitTileText(tile) {
  if (tile.classList.contains("has-image")) return;
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
  // Largest size where the wrapped text fits. Width must compare against
  // maxW exactly: the block is width:100%, so scrollWidth only exceeds it
  // when a single unbreakable word overflows.
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

// Re-fit when the window moves to a monitor with a different scale factor —
// sizes measured under the old DPI are no longer valid.
function watchDpr() {
  matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
    "change",
    () => {
      fitCache.clear();
      fitAllTiles();
      watchDpr();
    },
    { once: true }
  );
}
watchDpr();

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

// Two-step confirmation: first call arms the button and returns false,
// the second call (while armed) returns true.
function armButton(btn, confirmLabel) {
  if (btn.classList.contains("confirm")) return true;
  btn.classList.add("confirm");
  btn.textContent = confirmLabel;
  return false;
}
function disarmButton(btn, label) {
  btn.classList.remove("confirm");
  btn.textContent = label;
}

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
  // Unplaced prompts stay reachable via the library. No auto-fill: saved
  // per-grid-size arrangements must stay untouched.
  view.layouts[gridKeyOf(view)] = layout;
  return changed;
}

// ---- Render ----
// skipFetch: caller already updated the local state (drag/hide hot path).
async function renderGrid(skipFetch = false) {
  if (!skipFetch) {
    const s = await invoke("get_state");
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
  $("qg-cols").value = cols;
  $("qg-rows").value = rows;

  const byCell = new Map();
  for (const p of prompts) {
    const cell = layout[p.id];
    if (cell) byCell.set(cellKey(...cell), p);
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
  const preview = p.text.length > PREVIEW_MAX ? `${p.text.slice(0, PREVIEW_MAX)}…` : p.text;
  tile.title = preview
    ? `${p.name}\n\n${preview}\n\n${t("tileTooltip")}`
    : `${p.name}\n${t("tileTooltip")}`;

  if (p.show_image && p.image) {
    tile.classList.add("has-image");
    // The chosen color tints the border area around the image.
    if (p.color) {
      tile.style.background = p.color;
      tile.style.borderColor = p.color;
    }
    const img = document.createElement("img");
    img.className = "tile-img";
    img.src = p.image;
    img.draggable = false;
    tile.appendChild(img);
  } else if (p.color) {
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
    // Dragging out of the library: hide the overlay so the grid is visible.
    if (drag.fromLibrary) libraryEl.classList.add("hidden");
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

// Place a tile at [col,row]; swaps with the occupant. Renders from local
// state immediately, persistence runs in the background.
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
  // Reset an armed delete confirmation from a previous open.
  disarmButton(ctxEl.querySelector('[data-act="delete"]'), t("delete"));
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
  const isCustom = !!selected && !COLORS.includes(selected);

  const mkSwatch = (cls, bg) => {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = `swatch ${cls}`.trim();
    if (bg) sw.style.background = bg;
    row.appendChild(sw);
    return sw;
  };

  // "No color" first, then the free color picker, then the palette.
  const none = mkSwatch("none" + (selected === "" ? " sel" : ""));
  none.addEventListener("click", () => {
    modalState.color = "";
    renderSwatches("");
  });

  const picker = $("color-pick");
  const custom = mkSwatch("custom" + (isCustom ? " sel" : ""), isCustom ? selected : "");
  custom.addEventListener("click", () => {
    picker.value = /^#[0-9a-f]{6}$/i.test(selected) ? selected : "#3b82f6";
    picker.click();
  });

  for (const c of COLORS.slice(1)) {
    const sw = mkSwatch(c === selected ? "sel" : "", c);
    sw.addEventListener("click", () => {
      modalState.color = c;
      renderSwatches(c);
    });
  }
}

function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, title }) {
  modalState = { mode, id, color, image, showImage, copyImage };
  modal.title.textContent = title;
  modal.name.value = name;
  modal.text.value = text;
  syncModalImageUi(mode);
  modal.delete.classList.toggle("hidden", mode !== "edit");
  disarmButton(modal.delete, t("delete"));
  renderSwatches(color);
  modal.root.classList.remove("hidden");
  modal.name.focus();
  modal.name.select();
}

// Keep all image-related modal controls consistent with modalState.
function syncModalImageUi(mode) {
  const { image, showImage, copyImage } = modalState;
  const hasImg = !!image;
  // Pure image prompts have no text field — the name doubles as the copy text.
  modal.text.classList.toggle("hidden", mode !== "edit" || copyImage);
  modal.name.placeholder = copyImage ? t("imageNamePh") : t("namePh");
  modal.imgWrap.classList.toggle("hidden", !hasImg);
  modal.addIcon.classList.toggle("hidden", hasImg);
  // The image of an image prompt cannot be removed, only replaced.
  modal.removeImg.classList.toggle("hidden", copyImage);
  if (hasImg) {
    modal.img.src = image;
    modal.showImage.classList.toggle("active", showImage);
    modal.showText.classList.toggle("active", !showImage);
  }
}

async function startImageCreate() {
  const img = await invoke("get_clipboard_image");
  const data = img || await invoke("pick_image_file");
  if (!data) return;
  openModal({ mode: "image-create", title: t("imageModalTitle"), image: data, showImage: true, copyImage: true });
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
  const image = modalState.image || "";
  // NOTE: Tauri expects camelCase keys for snake_case Rust args.
  const showImage = image ? modalState.showImage : false;
  const copyImage = image ? modalState.copyImage : false;
  if (modalState.mode === "create") {
    const text = inputEl.value.trim();
    if (!text) { closeModal(); return; }
    await invoke("add_prompt", { name, text, color, image, showImage, copyImage });
    inputEl.value = "";
    autoGrow(inputEl);
    saveBtn.disabled = true;
  } else if (modalState.mode === "image-create") {
    // The name doubles as the copy text when the tile is switched to "Text".
    await invoke("add_prompt", { name, text: name, color, image, showImage, copyImage });
  } else {
    const text = copyImage ? name : modal.text.value;
    await invoke("update_prompt", { id: modalState.id, name, text, color, image, showImage, copyImage });
  }
  closeModal();
  await renderGrid();
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
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
      image: p.image || "",
      showImage: p.show_image || false,
      copyImage: p.copy_image || false,
      title: p.copy_image ? t("imageEditTitle") : t("editModalTitle"),
    });
  }
}

// ---- Prompt library (all prompts, click to edit) ----
function renderLibrary() {
  const list = $("library-list");
  list.innerHTML = "";
  if (!prompts.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryEmpty");
    list.appendChild(empty);
    return;
  }
  const placed = new Set(Object.keys(layoutOf(activeView())));
  for (const p of prompts) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.title = t("edit");

    const body = document.createElement("span");
    body.className = "lib-body";
    const name = document.createElement("span");
    name.className = "lib-name";
    name.textContent = p.name;
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.text;
    body.append(name, text);

    // Image prompts get a thumbnail, text prompts the color dot.
    if (p.show_image && p.image) {
      const thumb = document.createElement("img");
      thumb.className = "lib-thumb";
      thumb.src = p.image;
      thumb.draggable = false;
      row.append(thumb, body);
    } else {
      const dot = document.createElement("span");
      dot.className = "dot";
      if (p.color) dot.style.background = p.color;
      row.append(dot, body);
    }

    // Place on the current layout: drag the row onto the grid, or one click.
    row.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || e.target.closest(".lib-add")) return;
      drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: row, fromLibrary: true };
    });
    if (!placed.has(p.id)) {
      const add = document.createElement("span");
      add.className = "icon-btn lib-add";
      add.title = t("addToLayout");
      add.innerHTML = GRID_PLUS;
      add.addEventListener("pointerdown", (e) => e.stopPropagation());
      add.addEventListener("click", async (e) => {
        e.stopPropagation();
        const view = activeView();
        const occupied = new Map(
          Object.entries(layoutOf(view)).map(([id, c]) => [cellKey(...c), id])
        );
        const free = firstFree(occupied, view.cols, view.rows);
        if (!free) { toast(t("gridFull")); return; }
        await placeTile(p.id, free[0], free[1]);
        renderLibrary();
      });
      row.appendChild(add);
    }

    row.addEventListener("click", () => editPrompt(p.id));
    list.appendChild(row);
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
      n.max = GRID_MAX;
      n.value = value;
      return n;
    };
    const colsIn = mkNum(v.cols);
    const rowsIn = mkNum(v.rows);
    const applyGrid = async () => {
      const cols = clampGrid(colsIn.value, v.cols);
      const rows = clampGrid(rowsIn.value, v.rows);
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
  if (!armButton(btn, t("deleteAllConfirm"))) {
    clearTimeout(deleteAllTimer);
    deleteAllTimer = setTimeout(() => disarmButton(btn, t("deleteAll")), 3000);
    return;
  }
  clearTimeout(deleteAllTimer);
  disarmButton(btn, t("deleteAll"));
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

  // Image / Text display toggle (text mode shows the name on the tile).
  modal.showImage.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = true;
    syncModalImageUi(modalState.mode);
  });
  modal.showText.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });
  modal.replaceImg.addEventListener("click", async () => {
    if (!modalState) return;
    const data = await invoke("pick_image_file");
    if (!data) return;
    modalState.image = data;
    modalState.showImage = true;
    syncModalImageUi(modalState.mode);
  });
  // Icon image for a text prompt: shown on the tile, never copied.
  modal.addIcon.addEventListener("click", async () => {
    if (!modalState) return;
    const data = await invoke("pick_image_file");
    if (!data) return;
    modalState.image = data;
    modalState.showImage = true;
    modalState.copyImage = false;
    syncModalImageUi(modalState.mode);
  });
  modal.removeImg.addEventListener("click", () => {
    if (!modalState) return;
    modalState.image = "";
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });

  // Image button in the composer bar.
  $("image-btn").addEventListener("click", startImageCreate);

  // Free color choice from the native color-wheel dialog.
  $("color-pick").addEventListener("input", (e) => {
    if (!modalState) return;
    modalState.color = e.target.value;
    renderSwatches(modalState.color);
  });
  // Delete from the edit dialog, with the same two-step confirmation.
  modal.delete.addEventListener("click", async () => {
    if (!modalState || modalState.mode !== "edit") return;
    if (!armButton(modal.delete, t("deleteConfirm"))) return;
    const id = modalState.id;
    closeModal();
    await invoke("delete_prompt", { id });
    await renderGrid();
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  modal.root.addEventListener("pointerdown", (e) => { if (e.target === modal.root) closeModal(); });

  $("gear").addEventListener("click", () => {
    renderViewsEditor();
    settingsEl.classList.remove("hidden");
  });

  // Quick grid-size control (top-right of the layout, active view).
  const applyQuickGrid = async () => {
    const view = activeView();
    const cols = clampGrid($("qg-cols").value, view.cols);
    const rows = clampGrid($("qg-rows").value, view.rows);
    if (cols === view.cols && rows === view.rows) return;
    settings = await invoke("set_view_grid", { id: view.id, cols, rows });
    await renderGrid(true);
  };
  $("qg-cols").addEventListener("change", applyQuickGrid);
  $("qg-rows").addEventListener("change", applyQuickGrid);

  $("library-btn").addEventListener("click", () => {
    renderLibrary();
    libraryEl.classList.remove("hidden");
  });
  $("library-close").addEventListener("click", () => libraryEl.classList.add("hidden"));
  libraryEl.addEventListener("pointerdown", (e) => {
    if (e.target === libraryEl) libraryEl.classList.add("hidden");
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
    const btn = e.target.closest("button");
    const act = btn?.dataset.act;
    if (!act || !ctxId) return;
    const id = ctxId;
    if (act === "delete") {
      if (!armButton(btn, t("deleteConfirm"))) return;
      closeCtx();
      await invoke("delete_prompt", { id });
      await renderGrid();
      return;
    }
    closeCtx();
    if (act === "edit") {
      await editPrompt(id);
    } else if (act === "hide") {
      // Remove from the active view's grid; stays available in the library.
      const view = activeView();
      const layout = { ...layoutOf(view) };
      delete layout[id];
      view.layouts[gridKeyOf(view)] = layout;
      invoke("set_layout", { layout }).catch((e) => toast(String(e)));
      await renderGrid(true);
    } else if (act === "pin") {
      await invoke("toggle_floating", { id });
    }
  });

  document.addEventListener("pointerdown", (e) => {
    if (!ctxEl.classList.contains("hidden") && !ctxEl.contains(e.target)) closeCtx();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!modal.root.classList.contains("hidden")) closeModal();
    else if (!libraryEl.classList.contains("hidden")) libraryEl.classList.add("hidden");
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
  // Reveal the window only after the first fully fitted paint — the user
  // never sees the text sizing itself.
  requestAnimationFrame(() => {
    fitCache.clear();
    fitAllTiles();
    requestAnimationFrame(() => invoke("show_main_window").catch(() => {}));
  });
}

init();
