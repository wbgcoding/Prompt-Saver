// 97: in-app documentation. EN + DE only (explicit scope decision, see WORK.md) —
// every other language falls back to English. Kept out of i18n.js on purpose: that
// file's parity check (tests/i18n_check.js) requires every key in all 11 languages,
// which this content deliberately does not follow.
//
// Mirrored (structure, not word-for-word) in docs/help.md — keep both in sync when
// a category changes.

export const HELP_CONTENT = {
  en: [
    {
      section: "basics",
      title: "Getting started",
      overview:
        "A tile is one saved prompt. Double-click an empty grid cell (or the + button) to create one, click any tile to copy it. Three example tiles (an email with placeholders, an image, and a Markdown template) ship on first launch — a button above the grid removes them with one click once you don't need them anymore.",
      settings: [],
    },
    {
      section: "basics",
      title: "Creating prompts",
      overview:
        "Beyond plain text, a prompt can carry an attached file, a screenshot, or a picked screen colour.",
      settings: [
        "File attach — clicking the tile copies the file itself, not text",
        "Screenshot tool — snip a region or a whole window straight into a new prompt",
        "Colour picker — pick any on-screen pixel, copies as hex/RGB/HSL",
        "Drag & drop — drop a file or image straight onto the grid",
      ],
      tip: "The colour picker (pipette) is off by default.",
    },
    {
      section: "basics",
      title: "Variables & placeholders",
      overview:
        "Write {#{name}#} anywhere in a prompt's text — copying it opens a small popup asking for each placeholder's value before the filled-in text reaches the clipboard.",
      settings: [
        "Dynamic placeholders — {#{date}#}, {#{time}#}, {#{clipboard}#} fill themselves in, no popup",
        "Choice placeholders — a dropdown of preset values instead of free text",
        "Remember choices — the popup pre-fills your last answer per prompt",
        "Popup in its own window — useful with the floating buttons, where the main window may be hidden",
      ],
    },
    {
      section: "basics",
      title: "The editor",
      overview:
        "The prompt editor is a plain text box by default; a few opt-in features add structure and history to it.",
      settings: [
        "Markdown preview — renders the text as formatted Markdown next to (or instead of) the box",
        "Syntax highlighting — code blocks inside the Markdown preview get colour",
        "Version history — every content change is kept, restore any earlier version",
        "Private notes — a note field only you see, never copied or exported with the text",
      ],
      tip: "Markdown preview and private notes both ship off.",
    },
    {
      section: "copying",
      title: "Copying",
      overview: "What happens between clicking a tile and the text landing on the clipboard.",
      settings: [
        "Prompt chaining — click several tiles in a row, they combine into one clipboard copy",
        "Per-prompt hotkeys — copy a specific prompt from anywhere, without opening the app",
        "Drag-out — drag a tile onto another window to paste directly, no clipboard round-trip",
        "Auto-paste — after copying, the app switches to your last window and presses paste for you",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Floating buttons & folders",
      overview:
        "Floating buttons are small always-on-top copy buttons that sit outside the main window, over whatever app you're using.",
      settings: [
        "Pill folders — merge several floating buttons into one expandable group",
        "Grid folders — drop one tile onto another in the main grid to group them the same way",
      ],
      tip: "Both kinds of folder ship off.",
    },
    {
      section: "alwaysOnTop",
      title: "Tray",
      overview: "The system tray icon's right-click menu offers quick access without opening the main window.",
      settings: [
        "Recently copied section — your last few copies, ready to copy again",
        "Favourites section — starred prompts, always in the same place in the menu",
      ],
    },
    {
      section: "copying",
      title: "Clipboard watcher",
      overview:
        "When switched on, everything you copy anywhere on the system (not just from this app) is logged into an inbox you can review and turn into prompts later.",
      settings: [
        "Minimum length — copies shorter than this are ignored (skips stray single-character copies)",
        "Inbox cap — oldest entries drop once the inbox grows past this size",
      ],
      tip: "Off by default — nothing is watched until you turn it on.",
    },
    {
      section: "organising",
      title: "Tile behaviour & appearance",
      overview: "How a tile reacts to the mouse, and what it looks like.",
      settings: [
        "Tile menu / drag-to-reorder — right-click actions and free rearranging",
        "Hover lift — a tile rises slightly under the pointer",
        "Type badges, shadows, gradients, rounded corners, bold/uppercase/monospace text — pure looks",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      overview: "Hovering a tile shows its full text, length, and (if present) its private note.",
      settings: ["Delay before showing", "Auto-hide timeout", "Tooltip width"],
    },
    {
      section: "organising",
      title: "Favourites",
      overview: "Star a prompt to mark it as a favourite.",
      settings: ["Favourites view — a dedicated view listing only starred prompts, reorderable on its own"],
    },
    {
      section: "look",
      title: "Scaling & zoom",
      overview: "Independent size sliders for different parts of the window, in case the defaults don't fit your screen or preference.",
      settings: ["Overall UI scale", "Modal (dialog) scale", "Composer scale", "Icon scale"],
    },
    {
      section: "look",
      title: "Appearance & themes",
      overview: "Eight built-in themes: system, light, dark, Programmer, Neon Violet, Ocean, Cyberpunk and Black & White.",
      settings: ["Theme editor — tweak any theme's colours and save it as your own custom theme"],
      tip: "Custom themes are an opt-in feature — the editor button appears once it's turned on.",
    },
    {
      section: "organising",
      title: "Library & search",
      overview: "The library lists every prompt as a searchable, filterable list — the grid only shows what fits on screen, the library shows everything.",
      settings: [
        "Fuzzy search — finds close matches even with a typo",
        "Smart sort — most-used or most-recently-used ordering",
        "Type / colour filters — narrow the list before searching",
      ],
    },
    {
      section: "organising",
      title: "Tags",
      overview: "Free-form labels you assign to prompts; filter the library or the grid down to a set of tags.",
      settings: ["Tag filter mode — match ANY selected tag, or ALL of them"],
    },
    {
      section: "copying",
      title: "Copy history",
      overview: "A log of what you've copied and when — separate from the clipboard watcher, this only records copies made through this app.",
      settings: [
        "Grouped view — one row per prompt (its most recent copy) instead of every event",
        "Retention — automatically drop history older than N days",
        "Per-entry delete — remove a single history row without clearing everything",
        "Usage statistics — totals, a 14-day activity chart, most-used prompts",
      ],
    },
    {
      section: "organising",
      title: "Media",
      overview: "Settings for prompts that carry a video or image.",
      settings: ["Video controls, autoplay, mute, loop", "Grayscale / dim until hovered — privacy-leaning looks for media tiles"],
    },
    {
      section: "look",
      title: "Toolbar",
      overview: "Which buttons the header and composer bar show.",
      settings: ["Quick grid-size control", "Library / history / pin buttons", "Multiple views + view groups (tabs for the grid)"],
    },
    {
      section: "system",
      title: "Privacy",
      overview: "Options that keep this window's contents out of screenshots, screen shares, or a passer-by's glance.",
      settings: [
        "Exclude from screen capture — the window is invisible to screenshot/recording tools",
        "Blur tiles / media until hovered",
        "Hide tile names",
        "Dim unhovered tiles",
      ],
      tip: "Screen-capture exclusion ships OFF — turn it on if you want it.",
    },
    {
      section: "system",
      title: "System",
      overview: "App-level behaviour that isn't about any one prompt.",
      settings: [
        "Update checks — on by default, never installs anything without asking",
        "Keyboard navigation — arrow keys move a focus ring across the grid",
        "Allow multiple instances — normally a second launch just refocuses the first window",
        "Data folder — where prompts and settings are stored; changeable, portable-friendly",
      ],
    },
    {
      section: "system",
      title: "Data maintenance",
      overview: "Housekeeping tools reached from the expert menu's Data group.",
      settings: [
        "Duplicate finder — flags near-identical prompts by text similarity",
        "Trash — a deleted prompt sits here before it's gone for good, with its own retention setting",
        "Import / export — CSV/TXT export, full backup export/import",
      ],
    },
    {
      section: "system",
      title: "Standalone: Backup manager",
      overview: "Encrypted, compressed snapshots of your entire prompt library and settings, on your own schedule.",
      settings: [
        "Automatic backups on an interval",
        "Smart retention tiers (daily/weekly/monthly/yearly, GFS-style)",
        "Restore scope — data only, settings only, or both",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Standalone: Quick launcher",
      overview: "A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across every prompt's name and text — pick one and it copies immediately, no need to bring the main window forward.",
      settings: [],
    },
    {
      section: "organising",
      title: "Standalone: Multiple views",
      overview: "A view is one grid page with its own size and arrangement; a prompt can live in several views. View groups turn views into tabs instead of a dropdown.",
      settings: [],
    },
    {
      section: "look",
      title: "Standalone: Theme editor",
      overview: "Start from any built-in theme, change individual colours, and save the result as your own theme — the original stays untouched.",
      settings: [],
    },
  ],
  de: [
    {
      section: "basics",
      title: "Erste Schritte",
      overview:
        "Eine Kachel ist ein gespeicherter Prompt. Doppelklick auf eine leere Rasterzelle (oder der +-Knopf) legt eine an, ein Klick auf eine Kachel kopiert sie. Beim ersten Start sind drei Beispielkacheln vorhanden (eine E-Mail mit Platzhaltern, ein Bild, eine Markdown-Vorlage) — ein Knopf über dem Raster entfernt sie mit einem Klick, sobald sie nicht mehr gebraucht werden.",
      settings: [],
    },
    {
      section: "basics",
      title: "Prompts anlegen",
      overview: "Neben reinem Text kann ein Prompt eine angehängte Datei, einen Screenshot oder eine per Pipette abgelesene Bildschirmfarbe tragen.",
      settings: [
        "Datei-Anhang — ein Klick auf die Kachel kopiert die Datei selbst, keinen Text",
        "Screenshot-Werkzeug — ein Bereich oder ein ganzes Fenster wird direkt zu einem neuen Prompt",
        "Farbpipette — beliebigen Bildschirmpunkt ablesen, Kopie als Hex/RGB/HSL",
        "Ziehen & Ablegen — eine Datei oder ein Bild direkt aufs Raster ziehen",
      ],
      tip: "Die Farbpipette ist standardmäßig aus.",
    },
    {
      section: "basics",
      title: "Variablen & Platzhalter",
      overview:
        "Schreib {#{Name}#} irgendwo in den Prompt-Text — beim Kopieren öffnet sich ein kleines Popup, das nach jedem Platzhalterwert fragt, bevor der ausgefüllte Text in der Zwischenablage landet.",
      settings: [
        "Dynamische Platzhalter — {#{date}#}, {#{time}#}, {#{clipboard}#} füllen sich selbst, kein Popup",
        "Auswahl-Platzhalter — ein Dropdown mit vorgegebenen Werten statt Freitext",
        "Auswahl merken — das Popup füllt die letzte Antwort je Prompt vor",
        "Popup im eigenen Fenster — nützlich bei schwebenden Buttons, wenn das Hauptfenster verborgen ist",
      ],
    },
    {
      section: "basics",
      title: "Der Editor",
      overview: "Der Prompt-Editor ist standardmäßig ein reines Textfeld; ein paar optionale Funktionen geben ihm Struktur und Verlauf.",
      settings: [
        "Markdown-Vorschau — zeigt den Text als formatiertes Markdown neben (oder statt) dem Feld",
        "Syntax-Hervorhebung — Codeblöcke in der Markdown-Vorschau bekommen Farbe",
        "Versionsverlauf — jede inhaltliche Änderung bleibt erhalten, jede frühere Version wiederherstellbar",
        "Private Notizen — ein nur für dich sichtbares Notizfeld, nie mitkopiert oder exportiert",
      ],
      tip: "Markdown-Vorschau und private Notizen sind beide standardmäßig aus.",
    },
    {
      section: "copying",
      title: "Kopieren",
      overview: "Was zwischen dem Klick auf eine Kachel und der Zwischenablage passiert.",
      settings: [
        "Verketten — mehrere Kacheln nacheinander anklicken, sie werden zu einer Kopie zusammengefasst",
        "Prompt-Hotkeys — einen bestimmten Prompt von überall kopieren, ohne das Programm zu öffnen",
        "Herausziehen — eine Kachel auf ein anderes Fenster ziehen fügt direkt ein, ohne Umweg über die Zwischenablage",
        "Automatisches Einfügen — nach dem Kopieren wechselt das Programm zum letzten Fenster und fügt selbst ein",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Schwebende Buttons & Ordner",
      overview: "Schwebende Buttons sind kleine, immer-oben-Kopierknöpfe außerhalb des Hauptfensters, über jedem beliebigen Programm.",
      settings: [
        "Pillen-Ordner — mehrere schwebende Buttons zu einer aufklappbaren Gruppe zusammenfassen",
        "Raster-Ordner — eine Kachel im Hauptraster auf eine andere ziehen, gruppiert sie genauso",
      ],
      tip: "Beide Ordnerarten sind standardmäßig aus.",
    },
    {
      section: "alwaysOnTop",
      title: "Tray",
      overview: "Das Rechtsklick-Menü des Symbols in der Taskleiste bietet schnellen Zugriff, ohne das Hauptfenster zu öffnen.",
      settings: [
        "Zuletzt kopiert — die letzten Kopien, direkt erneut kopierbar",
        "Favoriten — markierte Prompts, immer an derselben Stelle im Menü",
      ],
    },
    {
      section: "copying",
      title: "Zwischenablage-Wächter",
      overview: "Eingeschaltet wird alles, was irgendwo im System kopiert wird (nicht nur aus diesem Programm), in einem Eingang protokolliert, den man später durchsehen und zu Prompts machen kann.",
      settings: [
        "Mindestlänge — kürzere Kopien werden ignoriert (blendet einzelne Zeichen aus)",
        "Eingang-Obergrenze — älteste Einträge fallen raus, sobald der Eingang darüber wächst",
      ],
      tip: "Standardmäßig aus — nichts wird beobachtet, bis es eingeschaltet wird.",
    },
    {
      section: "organising",
      title: "Kachelverhalten & Aussehen",
      overview: "Wie eine Kachel auf die Maus reagiert und wie sie aussieht.",
      settings: [
        "Kachelmenü / Ziehen zum Umsortieren — Rechtsklick-Aktionen und freies Umordnen",
        "Hover-Anhebung — eine Kachel hebt sich leicht unter dem Zeiger",
        "Typ-Abzeichen, Schatten, Verläufe, abgerundete Ecken, fett/Großbuchstaben/Monospace — reine Optik",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      overview: "Hovern über eine Kachel zeigt ihren vollen Text, die Länge und (falls vorhanden) ihre private Notiz.",
      settings: ["Verzögerung bis zum Anzeigen", "Automatisches Ausblenden", "Tooltip-Breite"],
    },
    {
      section: "organising",
      title: "Favoriten",
      overview: "Ein Prompt lässt sich als Favorit markieren (Stern).",
      settings: ["Favoriten-Ansicht — eine eigene Ansicht nur mit markierten Prompts, eigenständig sortierbar"],
    },
    {
      section: "look",
      title: "Skalierung & Zoom",
      overview: "Unabhängige Größenregler für verschiedene Teile des Fensters, falls die Standardgrößen nicht zu Bildschirm oder Geschmack passen.",
      settings: ["Gesamte UI-Größe", "Dialog-Größe", "Composer-Größe", "Icon-Größe"],
    },
    {
      section: "look",
      title: "Aussehen & Themes",
      overview: "Acht mitgelieferte Themes: System, hell, dunkel, Programmierer, Neon-Violett, Ozean, Cyberpunk und Schwarzweiß.",
      settings: ["Theme-Editor — Farben jedes Themes anpassen und als eigenes Theme speichern"],
      tip: "Eigene Themes sind eine optionale Funktion — der Editor-Knopf erscheint erst, wenn sie eingeschaltet ist.",
    },
    {
      section: "organising",
      title: "Bibliothek & Suche",
      overview: "Die Bibliothek listet jeden Prompt als durchsuch- und filterbare Liste — das Raster zeigt nur, was auf den Bildschirm passt, die Bibliothek zeigt alles.",
      settings: [
        "Unscharfe Suche — findet Treffer auch bei einem Tippfehler",
        "Intelligente Sortierung — nach Nutzungshäufigkeit oder letzter Nutzung",
        "Typ-/Farbfilter — die Liste vor der Suche eingrenzen",
      ],
    },
    {
      section: "organising",
      title: "Tags",
      overview: "Frei vergebene Schlagwörter für Prompts; Bibliothek oder Raster lassen sich auf bestimmte Tags eingrenzen.",
      settings: ["Tag-Filtermodus — trifft EIN ausgewählter Tag, oder müssen ALLE zutreffen"],
    },
    {
      section: "copying",
      title: "Kopierverlauf",
      overview: "Ein Protokoll, was wann kopiert wurde — getrennt vom Zwischenablage-Wächter, erfasst nur Kopien aus diesem Programm.",
      settings: [
        "Gruppierte Ansicht — eine Zeile je Prompt (die letzte Kopie) statt jedes Ereignisses",
        "Aufbewahrung — Verlauf älter als N Tage automatisch löschen",
        "Einzeleintrag löschen — eine einzelne Verlaufszeile entfernen, ohne alles zu leeren",
        "Nutzungsstatistik — Summen, ein 14-Tage-Diagramm, meistgenutzte Prompts",
      ],
    },
    {
      section: "organising",
      title: "Medien",
      overview: "Einstellungen für Prompts mit Video oder Bild.",
      settings: ["Video-Steuerung, Autoplay, Stumm, Wiederholung", "Graustufen / Abdunkeln bis Hover — datenschutzfreundliche Optik für Medienkacheln"],
    },
    {
      section: "look",
      title: "Symbolleiste",
      overview: "Welche Knöpfe Kopfzeile und Composer-Leiste zeigen.",
      settings: ["Schnellraster-Größenregler", "Bibliothek-/Verlauf-/Anheften-Knöpfe", "Mehrfachansichten + Ansichtsgruppen (Reiter fürs Raster)"],
    },
    {
      section: "system",
      title: "Datenschutz",
      overview: "Einstellungen, die den Fensterinhalt aus Screenshots, Bildschirmfreigaben oder fremden Blicken heraushalten.",
      settings: [
        "Von Bildschirmaufnahme ausschließen — das Fenster ist für Screenshot-/Aufnahme-Werkzeuge unsichtbar",
        "Kacheln/Medien bis Hover weichzeichnen",
        "Kachelnamen verbergen",
        "Nicht gehoverte Kacheln abdunkeln",
      ],
      tip: "Der Bildschirmaufnahme-Ausschluss ist standardmäßig AUS — bei Bedarf einschalten.",
    },
    {
      section: "system",
      title: "System",
      overview: "Programmweites Verhalten, das keinen einzelnen Prompt betrifft.",
      settings: [
        "Update-Prüfung — standardmäßig an, installiert nie ungefragt etwas",
        "Tastatur-Navigation — Pfeiltasten bewegen einen Fokusring über das Raster",
        "Mehrere Instanzen erlauben — normalerweise holt ein zweiter Start nur das erste Fenster nach vorn",
        "Datenordner — wo Prompts und Einstellungen liegen; änderbar, portabel-freundlich",
      ],
    },
    {
      section: "system",
      title: "Datenpflege",
      overview: "Aufräum-Werkzeuge, erreichbar über die Datengruppe im Expertenmenü.",
      settings: [
        "Duplikatfinder — markiert nahezu identische Prompts anhand von Textähnlichkeit",
        "Papierkorb — ein gelöschter Prompt liegt hier, bevor er endgültig verschwindet, eigene Aufbewahrungsfrist",
        "Import/Export — CSV/TXT-Export, vollständiger Backup-Export/-Import",
      ],
    },
    {
      section: "system",
      title: "Eigenständig: Backup-Manager",
      overview: "Verschlüsselte, komprimierte Momentaufnahmen der gesamten Prompt-Bibliothek und Einstellungen, nach eigenem Zeitplan.",
      settings: [
        "Automatische Backups in festem Intervall",
        "Intelligente Aufbewahrungsstufen (täglich/wöchentlich/monatlich/jährlich, GFS-artig)",
        "Wiederherstellungs-Umfang — nur Daten, nur Einstellungen, oder beides",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Eigenständig: Schnellstarter",
      overview: "Ein kleines, immer-oben-Popup (Standard-Hotkey Strg+Alt+P) mit unscharfer Suche über Name und Text jedes Prompts — Auswahl kopiert sofort, ohne das Hauptfenster hervorzuholen.",
      settings: [],
    },
    {
      section: "organising",
      title: "Eigenständig: Mehrfachansichten",
      overview: "Eine Ansicht ist eine Rasterseite mit eigener Größe und Anordnung; ein Prompt kann in mehreren Ansichten liegen. Ansichtsgruppen machen aus Ansichten Reiter statt eines Dropdowns.",
      settings: [],
    },
    {
      section: "look",
      title: "Eigenständig: Theme-Editor",
      overview: "Von einem mitgelieferten Theme ausgehen, einzelne Farben ändern und das Ergebnis als eigenes Theme speichern — das Original bleibt unangetastet.",
      settings: [],
    },
  ],
};
