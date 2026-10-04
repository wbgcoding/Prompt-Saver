// ---- Markdown preview + code highlighting (editor) ----
// The prompt body is untrusted text. It is HTML-escaped ONCE, up front, and every
// transform below only ever slices that escaped string, so nothing a user types can
// come out as markup. A link keeps its label as plain styled text and never becomes
// an <a>, which is why no href/src exists for a click to follow.
export const MD_MAX_CHARS = 100000; // past this the preview is cut, so split-mode typing stays smooth
// A single huge fence is rendered plain: an unterminated block comment makes the
// lazy scan quadratic, and nobody reads 20k characters of colour anyway.
export const HL_MAX_CHARS = 20000; // past this length code is not highlighted
export const MD_ESC_RE = /[&<>"']/g;
export const MD_ESC_MAP = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
export const mdEsc = (s) => String(s).replace(MD_ESC_RE, (c) => MD_ESC_MAP[c]);

// Token patterns. They run against the ESCAPED code, which is why quotes and
// angle brackets appear as &quot; / &#39; / &lt; here. Deliberately best-effort:
// anything the scanner does not recognise stays plain, so malformed code still renders.
export const HL_DQ = String.raw`&quot;(?:\\.|&(?!quot;)|[^&\\\n])*&quot;`;
export const HL_SQ = String.raw`&#39;(?:\\.|&(?!#39;)|[^&\\\n])*&#39;`;
export const HL_BT = "`(?:\\\\.|[^`\\\\])*`";
export const HL_BLOCK_C = String.raw`/\*[\s\S]*?(?:\*/|$)`;
export const HL_NUM = String.raw`\b(?:0[xXbBoO][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)\w*`;
export const hlLine = (mark) => `${mark}[^\\n]*`;
export const hlWords = (w) => String.raw`\b(?:${w})\b`;
export const HL_CLASSES = ["cmt", "str", "num", "kw", "fn"];
export const HL_RULES = {
  js: { rules: [
    ["cmt", `${hlLine("//")}|${HL_BLOCK_C}`],
    ["str", `${HL_BT}|${HL_DQ}|${HL_SQ}`],
    ["num", HL_NUM],
    ["kw", hlWords("const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|default|class|extends|super|new|delete|typeof|instanceof|in|of|this|null|undefined|true|false|async|await|import|export|from|as|try|catch|finally|throw|yield|void|static|interface|type|enum|implements|public|private|protected|readonly|declare|namespace|number|string|boolean|any|unknown|never")],
    ["fn", String.raw`\b[A-Za-z_$][\w$]*(?=\s*\()`],
  ] },
  py: { rules: [
    ["cmt", hlLine("#")],
    ["str", `(?:&quot;){3}[\\s\\S]*?(?:(?:&quot;){3}|$)|(?:&#39;){3}[\\s\\S]*?(?:(?:&#39;){3}|$)|${HL_DQ}|${HL_SQ}`],
    ["num", HL_NUM],
    ["kw", hlWords("def|class|return|if|elif|else|for|while|break|continue|pass|import|from|as|with|try|except|finally|raise|lambda|yield|global|nonlocal|assert|del|in|is|not|and|or|None|True|False|self|async|await|match|case")],
    ["fn", String.raw`\b[A-Za-z_]\w*(?=\s*\()`],
  ] },
  html: { rules: [
    ["cmt", String.raw`&lt;!--[\s\S]*?(?:--&gt;|$)`],
    ["str", `${HL_DQ}|${HL_SQ}`],
    ["kw", String.raw`&lt;/?[A-Za-z][\w:.-]*|/?&gt;`],
    ["fn", String.raw`[A-Za-z_:][\w:.-]*(?=\s*=)`],
  ] },
  css: { rules: [
    ["cmt", HL_BLOCK_C],
    ["str", `${HL_DQ}|${HL_SQ}`],
    ["num", String.raw`#[0-9a-fA-F]{3,8}\b|-?\b\d*\.?\d+(?:px|em|rem|%|vh|vw|vmin|vmax|ms|s|deg|fr|ch|pt)?\b`],
    ["kw", String.raw`@[a-zA-Z-]+|[-a-zA-Z]+(?=\s*:)`],
    ["fn", String.raw`[.#]?[-\w]+(?=\s*[{,])|::?[a-z-]+`],
  ] },
  json: { rules: [
    // A string followed by a colon is a key; plain strings keep the string colour.
    ["kw", `${HL_DQ}(?=\\s*:)|${hlWords("true|false|null")}`],
    ["str", HL_DQ],
    ["num", String.raw`-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b`],
  ] },
  sql: { flags: "i", rules: [
    ["cmt", `${hlLine("--")}|${HL_BLOCK_C}`],
    ["str", `${HL_SQ}|${HL_DQ}`],
    ["num", String.raw`\b\d+(?:\.\d+)?\b`],
    ["kw", hlWords("select|from|where|insert|into|values|update|set|delete|create|table|alter|drop|index|view|join|inner|left|right|full|outer|cross|on|group|by|order|having|limit|offset|distinct|as|and|or|not|null|is|in|between|like|exists|union|all|case|when|then|else|end|primary|key|foreign|references|default|unique|constraint|asc|desc|with|returning")],
    ["fn", String.raw`\b[A-Za-z_]\w*(?=\s*\()`],
  ] },
  bash: { rules: [
    ["cmt", hlLine("#")],
    ["str", `${HL_DQ}|${HL_SQ}`],
    ["num", String.raw`\b\d+\b`],
    ["kw", hlWords("if|then|elif|else|fi|for|in|do|done|while|until|case|esac|function|return|local|export|readonly|set|unset|shift|source|echo|cd|exit|trap|declare|sudo")],
    ["fn", String.raw`\$\{[^}\n]*\}|\$[A-Za-z_]\w*|\$[0-9@*#?]`],
  ] },
  rust: { rules: [
    ["cmt", `${hlLine("//")}|${HL_BLOCK_C}`],
    // A lifetime ('a) has no closing quote, so it simply falls through to plain.
    ["str", `${HL_DQ}|` + String.raw`&#39;(?:\\.|[^&\\\n])&#39;`],
    ["num", String.raw`\b(?:0[xXbBoO][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?)(?:[iuf](?:8|16|32|64|128|size))?\b`],
    ["kw", hlWords("fn|let|mut|const|static|struct|enum|impl|trait|pub|use|mod|match|if|else|for|while|loop|return|break|continue|as|where|in|ref|move|dyn|unsafe|async|await|type|crate|self|Self|super|true|false|Some|None|Ok|Err")],
    ["fn", String.raw`#!?\[[^\]\n]*\]|\b[a-z_]\w*(?=\s*\()|\b[A-Z]\w*\b`],
  ] },
};
// Fence tag -> ruleset. An unknown or missing tag renders as plain code.
export const HL_ALIAS = {
  js: "js", javascript: "js", jsx: "js", mjs: "js", node: "js",
  ts: "js", typescript: "js", tsx: "js",
  py: "py", python: "py",
  html: "html", htm: "html", xml: "html", svg: "html", vue: "html",
  css: "css", scss: "css", sass: "css", less: "css",
  json: "json", jsonc: "json",
  sql: "sql", mysql: "sql", psql: "sql", postgres: "sql", sqlite: "sql",
  sh: "bash", bash: "bash", shell: "bash", zsh: "bash", console: "bash",
  rs: "rust", rust: "rust",
};
// One regex per language, compiled once: never per keystroke.
export const HL_RE = Object.fromEntries(Object.entries(HL_RULES).map(([id, def]) => [
  id,
  new RegExp(def.rules.map(([cls, src]) => `(?<${cls}>${src})`).join("|"), `g${def.flags || ""}`),
]));

// Wrap recognised tokens of already-escaped code in themed spans.
export function hlCode(escCode, lang) {
  const re = HL_RE[lang];
  if (!re) return escCode;
  re.lastIndex = 0;
  let out = "", last = 0, m;
  while ((m = re.exec(escCode))) {
    if (!m[0]) { re.lastIndex++; continue; } // never spin on a zero-length match
    out += escCode.slice(last, m.index);
    const cls = HL_CLASSES.find((c) => m.groups[c] !== undefined);
    out += `<span class="hl-${cls}">${m[0]}</span>`;
    last = re.lastIndex;
  }
  return out + escCode.slice(last);
}

export const MD_FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([A-Za-z0-9+#._-]*)\s*$/;
export const MD_HEAD = /^\s{0,3}(#{1,6})\s+(.*)$/;
export const MD_HR = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
export const MD_QUOTE = /^\s{0,3}&gt;\s?(.*)$/; // ">" is already escaped at this point
export const MD_UL = /^\s{0,3}[-*+]\s+(.*)$/;
export const MD_OL = /^\s{0,3}\d{1,9}[.)]\s+(.*)$/;
export const MD_INLINE = /`([^`\n]+)`|\[([^\]\n]*)\]\([^)\n]*\)|\*\*([^*\n]+)\*\*|__([^_\n]+)__|\*([^*\n]+)\*|_([^_\n]+)_/g;

export function mdInline(s) {
  MD_INLINE.lastIndex = 0;
  let out = "", last = 0, m;
  while ((m = MD_INLINE.exec(s))) {
    out += s.slice(last, m.index);
    if (m[1] !== undefined) out += `<code class="md-inline-code">${m[1]}</code>`;
    else if (m[2] !== undefined) out += `<span class="md-link">${m[2]}</span>`; // label only, no target
    else if (m[3] !== undefined || m[4] !== undefined) out += `<strong>${m[3] ?? m[4]}</strong>`;
    else out += `<em>${m[5] ?? m[6]}</em>`;
    last = MD_INLINE.lastIndex;
  }
  return out + s.slice(last);
}

// ---- Code detection: a prompt that is plainly source code should look like code ----
// Markdown rules would eat its punctuation (a leading "-", "#" or "*" turns into a
// list, a heading or italics), so such a prompt is rendered as one highlighted block.
// The heuristic keys on three things, all cheap:
//   1. a language cue: a construct only that language writes (fn/def/SELECT/<tag>),
//   2. shape: most non-empty lines end in code punctuation or are indented,
//   3. no prose: not a single line reads like a sentence in running text.
// A cue marked strong stands on its own (a shebang, a whole JSON document, an SQL
// statement in the usual upper case) and skips the shape test, which those write too
// few punctuated line ends to pass; every other cue needs the shape as well.
// Markdown structure (a list, a quote, a fence) vetoes the whole thing, and so does a
// single line: when in doubt this returns "" and the text stays ordinary Markdown.
export const CODE_CUES = [
  ["json", /^\s*\{[\s\S]*"[^"\n]*"\s*:[\s\S]*\}\s*$|^\s*\[\s*[{["\d-][\s\S]*\]\s*$/, true],
  ["bash", /^\s*#!\/\S/, true],
  ["sql", /\bSELECT\b[\s\S]*\bFROM\b|\bINSERT\s+INTO\b|\bCREATE\s+TABLE\b|\bUPDATE\b[\s\S]*\bSET\b/, true],
  ["html", /<\/[a-z][\w-]*>|<[a-z][\w-]*(?:\s[^<>]*)?\/>/i, false],
  ["css", /(?:^|\n)[^\n{}]{1,80}\{[^}]*[\w-]+\s*:[^;}]+;/, false],
  ["py", /(?:^|\n)[ \t]*(?:def\s+\w+\s*\(|class\s+\w+\s*[:(]|from\s+[\w.]+\s+import\b|import\s+\w+\s*$)/m, false],
  ["rust", /\b(?:fn\s+\w+\s*[(<]|let\s+mut\s+\w+|impl\s+\w+|pub\s+(?:fn|struct|enum)\b|use\s+[\w:]+;)/, false],
  ["js", /\b(?:function\s*\w*\s*\(|=>\s*[{(]|(?:const|let|var)\s+\w+\s*=|console\.\w+\(|export\s+(?:default|const|function)\b)/, false],
  ["bash", /(?:^|\n)\s*(?:sudo|apt|npm|npx|git|curl|docker|chmod)\s+\S/, false],
];
// Markdown a code block would never contain. "#" is missing on purpose: it is a
// comment in half the languages here and would veto every commented snippet.
export const MD_STRUCTURE = /^\s{0,3}(?:[-*+]\s|\d{1,9}[.)]\s|>\s|```|~~~|\|)/m;
// A line that carries code punctuation where prose never would.
export const CODE_LINE = /[;{}([,:=]\s*$|^\s*(?:[}\])]|\/\/|\/\*|#|--|<\/|["'`])/;
// Five or more words ending in a full stop, with no code punctuation: running prose.
export const PROSE_LINE = /^[^;{}=<>|]*\s\S+\s\S+\s\S+\s\S+[^;{}=<>|]*[.!?:]$/;

export function detectCodeLang(text) {
  const s = text.trim();
  // The highlighter's own ceiling: past it a block renders plain anyway.
  if (!s || s.length > HL_MAX_CHARS || MD_STRUCTURE.test(s)) return "";
  const lines = s.split("\n").filter((l) => l.trim());
  if (lines.length < 2) return "";
  const cue = CODE_CUES.find(([, re]) => re.test(s));
  if (!cue) return "";
  if (!cue[2]) {
    const shaped = lines.filter((l) => CODE_LINE.test(l.trim()) || /^(?: {2,}|\t)/.test(l)).length;
    if (shaped * 2 < lines.length) return "";
  }
  if (lines.some((l) => PROSE_LINE.test(l.trim()))) return "";
  return cue[0];
}

// Whether `text` actually contains Markdown syntax, reusing the exact same
// patterns renderMarkdown() itself reacts to (against escaped text, same as
// there) -- so "does this look like Markdown" and "how it renders" can never
// disagree. Gates the formatted-copy entry: no point offering it on a prompt
// that would render as one plain paragraph anyway.
export function hasMarkdownSyntax(text) {
  if (!text) return false;
  const esc = mdEsc(text.replace(/\r\n?/g, "\n"));
  const lines = esc.split("\n");
  if (lines.some((l) => MD_FENCE.test(l) || MD_HEAD.test(l) || MD_HR.test(l) || MD_QUOTE.test(l) || MD_UL.test(l) || MD_OL.test(l))) return true;
  MD_INLINE.lastIndex = 0;
  const found = MD_INLINE.test(esc);
  MD_INLINE.lastIndex = 0; // MD_INLINE is a shared /g regex -- mdInline()'s own exec loop assumes it starts at 0
  return found;
}
