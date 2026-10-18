// Per-window command permissions (src-tauri/capabilities/*.json).
// Run:  node tests/capability_check.js      Exit: 0 = clean, 1 = problems.
//
// For every satellite page it follows the page script and every module it imports (static
// and literal dynamic imports), collects each backend command the reachable code invokes,
// and checks the capability file of that window allows exactly that set: a command missing
// is a broken feature, a command too many is a permission the window never uses.
// Also: the main window's capability allows every command of the handler list in
// src-tauri/src/lib.rs, no window label is covered twice, core permissions of a satellite
// stay within the main window's, and build.rs builds the app manifest from that same list.
//
// Command names are read from invoke(...), orderedInvoke(...) (first argument) and
// serialInvoke(key, ...) (second argument). A name that is not a plain string literal fails
// the run unless it is listed in NON_LITERAL_OK; a string equal to a command name that is
// not the argument of such a call fails as well (UNATTRIBUTED_OK holds the reasoned
// exceptions), so a new wrapper cannot hide a command.

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const UI = path.join(ROOT, "ui");
const CAPS = path.join(ROOT, "src-tauri", "capabilities");

// page html -> windows it serves -> capability file (without .json)
const SATELLITES = [
  { page: "floating.html", windows: ["float-*"], cap: "float" },
  { page: "folder.html", windows: ["folder-*"], cap: "folder" },
  { page: "snip.html", windows: ["snip-*"], cap: "snip" },
  { page: "vars.html", windows: ["vars-*"], cap: "vars" },
  { page: "launcher.html", windows: ["launcher"], cap: "launcher" },
  { page: "tray.html", windows: ["tray-menu"], cap: "tray-menu" },
  { page: "recbar.html", windows: ["rec-bar"], cap: "rec-bar" },
  { page: "video.html", windows: ["video-editor"], cap: "video-editor" },
  { page: "camera.html", windows: ["camera"], cap: "camera" },
];
const MAIN = { page: "index.html", windows: ["main"], cap: "default" };
// Pages that load no script at all: their window needs no capability.
const NO_SCRIPT_PAGES = ["recoutline.html"];

// Wrappers whose first (or second) argument is a command name.
const INVOKERS = { invoke: 0, orderedInvoke: 0, serialInvoke: 1 };

// "<file>|<argument text>" of calls whose command name is not a literal. These forward a
// name that every caller passes as a literal, and those callers are checked themselves.
const NON_LITERAL_OK = {
  "floatShared.js|cmd": "serialInvoke and orderedInvoke forward the name their callers passed",
  "recbar.js|command": "endTake(command) is called with literal names only, listed in WRAPPED_LITERAL",
};

// "<file>|<command>" of a command name passed as a literal to a local helper that invokes it
// (so the name is no invoke argument itself). The command counts for every page that
// reaches the file.
const WRAPPED_LITERAL = {
  "recbar.js|cancel_recording": "endTake(command) invokes the name it receives",
  "recbar.js|stop_recording": "endTake(command) invokes the name it receives",
};

// "<file>|<command>" ("*" = any) of a string equal to a command name that is no command call.
const NOT_A_COMMAND = {
  "floatShared.js|*": "ORDERED_KEYS maps a command to its ordering key, a string of the same text",
};
const has = (obj, key) => Object.prototype.hasOwnProperty.call(obj, key);

const problems = [];
const fail = (msg) => problems.push(msg);

// ---------------------------------------------------------------- command list

function stripRustComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
}

function handlerCommands() {
  const src = stripRustComments(fs.readFileSync(path.join(ROOT, "src-tauri", "src", "lib.rs"), "utf8"));
  const start = src.indexOf("generate_handler![");
  if (start < 0) { fail("lib.rs: no generate_handler! block found"); return []; }
  const open = src.indexOf("[", start);
  const close = src.indexOf("]", open);
  const body = src.slice(open + 1, close);
  const names = [];
  for (const raw of body.split(",")) {
    const entry = raw.trim();
    if (!entry) continue;
    if (!/^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$/.test(entry)) {
      fail(`lib.rs: handler entry "${entry}" is not a plain path`);
      continue;
    }
    names.push(entry.split("::").pop());
  }
  const seen = new Set();
  for (const n of names) {
    if (seen.has(n)) fail(`lib.rs: command ${n} is listed twice in generate_handler!`);
    seen.add(n);
  }
  return names;
}

const COMMANDS = handlerCommands();
const COMMAND_SET = new Set(COMMANDS);
const permOf = (cmd) => "allow-" + cmd.replace(/_/g, "-");
const cmdOf = (perm) => perm.slice("allow-".length).replace(/-/g, "_");

// ---------------------------------------------------------------- js tokenizer

const KEYWORDS_BEFORE_REGEX = new Set([
  "return", "typeof", "case", "do", "else", "in", "instanceof", "new", "void", "delete", "throw", "yield", "await", "of",
]);
const isIdStart = (c) => /[A-Za-z_$]/.test(c);
const isIdPart = (c) => /[A-Za-z0-9_$]/.test(c);

function tokenize(src) {
  const toks = [];
  let i = 0;
  let nl = false;
  const n = src.length;
  // Stack of open template expressions: each entry is the brace depth inside "${ ... }".
  const tplStack = [];
  let braceDepth = 0;
  const push = (t, v) => { toks.push({ t, v, nl }); nl = false; };
  const prevAllowsRegex = () => {
    const p = toks[toks.length - 1];
    if (!p) return true;
    if (p.t === "num" || p.t === "str" || p.t === "tpl" || p.t === "re") return false;
    if (p.t === "id") return KEYWORDS_BEFORE_REGEX.has(p.v);
    return !(p.v === ")" || p.v === "]" || p.v === "}");
  };
  // Scans template text from i (just after a backtick or a closing brace of an expression).
  const scanTemplate = (cont) => {
    let text = "";
    while (i < n) {
      const c = src[i];
      if (c === "\\") { text += src.slice(i, i + 2); i += 2; continue; }
      if (c === "`") { i++; push("tpl", text); toks[toks.length - 1].cont = !!cont; return; }
      if (c === "$" && src[i + 1] === "{") {
        i += 2;
        push("tplstart", text);
        toks[toks.length - 1].cont = !!cont;
        tplStack.push(braceDepth);
        braceDepth = 0;
        return;
      }
      text += c; i++;
    }
  };
  while (i < n) {
    const c = src[i];
    if (c === "\n") { nl = true; i++; continue; }
    if (/\s/.test(c)) { i++; continue; }
    if (c === "/" && src[i + 1] === "/") { while (i < n && src[i] !== "\n") i++; continue; }
    if (c === "/" && src[i + 1] === "*") {
      const e = src.indexOf("*/", i + 2);
      const end = e < 0 ? n : e + 2;
      if (src.slice(i, end).includes("\n")) nl = true;
      i = end; continue;
    }
    if (c === '"' || c === "'") {
      let j = i + 1, v = "";
      while (j < n && src[j] !== c) {
        if (src[j] === "\\") { v += src.slice(j, j + 2); j += 2; continue; }
        if (src[j] === "\n") break;
        v += src[j]; j++;
      }
      i = j + 1;
      push("str", v);
      continue;
    }
    if (c === "`") { i++; scanTemplate(false); continue; }
    if (c === "/" && prevAllowsRegex()) {
      let j = i + 1, inClass = false;
      while (j < n) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === "[") inClass = true;
        else if (src[j] === "]") inClass = false;
        else if (src[j] === "/" && !inClass) break;
        else if (src[j] === "\n") break;
        j++;
      }
      j++;
      while (j < n && isIdPart(src[j])) j++;
      push("re", src.slice(i, j));
      i = j;
      continue;
    }
    if (isIdStart(c)) {
      let j = i + 1;
      while (j < n && isIdPart(src[j])) j++;
      push("id", src.slice(i, j));
      i = j;
      continue;
    }
    if (/[0-9]/.test(c) || (c === "." && /[0-9]/.test(src[i + 1] || ""))) {
      let j = i + 1;
      while (j < n && /[0-9A-Za-z_.]/.test(src[j])) j++;
      push("num", src.slice(i, j));
      i = j;
      continue;
    }
    if (c === "{") { braceDepth++; push("p", "{"); i++; continue; }
    if (c === "}") {
      if (tplStack.length && braceDepth === 0) {
        braceDepth = tplStack.pop();
        i++;
        scanTemplate(true);
        continue;
      }
      braceDepth--; push("p", "}"); i++; continue;
    }
    if (src.startsWith("...", i)) { push("p", "..."); i += 3; continue; }
    if (src.startsWith("=>", i)) { push("p", "=>"); i += 2; continue; }
    if (src.startsWith("?.", i) && !/[0-9]/.test(src[i + 2] || "")) { push("p", "?."); i += 2; continue; }
    push("p", c);
    i++;
  }
  return toks;
}

// ---------------------------------------------------------------- module graph

const readUi = (rel) => fs.readFileSync(path.join(UI, rel), "utf8");
const normRel = (from, spec) => path.posix.normalize(path.posix.join(path.posix.dirname(from), spec));

function htmlScripts(page) {
  const html = readUi(page);
  return [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map((m) => m[1]);
}

function importsOf(rel, toks) {
  const out = [];
  for (let k = 0; k < toks.length; k++) {
    const t = toks[k];
    if (t.t === "id" && t.v === "import") {
      const next = toks[k + 1];
      if (next && next.t === "p" && next.v === "(") {
        const arg = toks[k + 2];
        const after = toks[k + 3];
        if (arg && arg.t === "str" && after && after.v === ")") out.push(arg.v);
        else if (arg && arg.t === "tplstart" && /^\.\/[A-Za-z0-9_-]+\/$/.test(arg.v)) {
          // import(`./dir/${name}.js`): every module of that folder can load
          const dir = path.posix.join(path.posix.dirname(rel), arg.v);
          for (const f of fs.readdirSync(path.join(UI, dir))) if (f.endsWith(".js")) out.push("./" + path.posix.join(arg.v, f).replace(/^\.\//, ""));
        } else fail(`${rel}: import() with a name that is not a plain literal`);
      } else if (next && next.t === "p" && next.v === ".") {
        // import.meta
      } else {
        // import ... from "x"  /  import "x"
        let j = k + 1;
        while (j < toks.length && toks[j].t !== "str") {
          if (toks[j].t === "p" && toks[j].v === ";") break;
          j++;
        }
        if (j < toks.length && toks[j].t === "str") out.push(toks[j].v);
      }
    } else if (t.t === "id" && t.v === "from" && toks[k + 1] && toks[k + 1].t === "str" &&
      toks[k - 1] && toks[k - 1].t === "p" && toks[k - 1].v === "}") {
      // export { a } from "x"
      if (!out.includes(toks[k + 1].v)) out.push(toks[k + 1].v);
    }
  }
  return out;
}

const moduleCache = new Map();
function loadModule(rel) {
  if (!moduleCache.has(rel)) {
    const toks = tokenize(readUi(rel));
    moduleCache.set(rel, { rel, toks, deps: importsOf(rel, toks).filter((s) => s.startsWith(".")).map((s) => normRel(rel, s)) });
  }
  return moduleCache.get(rel);
}

function reachable(entries) {
  const seen = new Map();
  const queue = entries.slice();
  while (queue.length) {
    const rel = queue.shift();
    if (seen.has(rel)) continue;
    if (!fs.existsSync(path.join(UI, rel))) { fail(`${rel}: imported but missing`); continue; }
    const mod = loadModule(rel);
    seen.set(rel, mod);
    for (const d of mod.deps) queue.push(d);
  }
  return [...seen.values()];
}

// ---------------------------------------------------------------- command inventory

// ---- statement units: a top-level declaration counts only when reachable code names it
const depthDelta = (a) => {
  if (a.t === "p") return a.v === "(" || a.v === "[" || a.v === "{" ? 1 : a.v === ")" || a.v === "]" || a.v === "}" ? -1 : 0;
  if (a.t === "tplstart") return a.cont ? 0 : 1;
  if (a.t === "tpl") return a.cont ? -1 : 0;
  return 0;
};
const NOT_A_START_P = new Set([".", "?.", "?", ":", ",", ")", "]", "}", "=>", "=", "&&", "||", "??", "*", "/", "%", "<", ">", "==", "===", "!=", "!==", "|", "&", "^"]);
const NOT_A_START_ID = new Set(["else", "catch", "finally", "in", "of", "instanceof"]);

// Splits a module into units at depth 0. { start, end, names: [..] | null, quiet: bool }.
// names null = root unit (always runs). quiet = its identifiers are not counted as references.
function unitsOf(mod) {
  if (mod.units) return mod.units;
  const { toks } = mod;
  const cuts = [0];
  let depth = 0;
  for (let k = 0; k < toks.length; k++) {
    const t = toks[k];
    if (k > 0 && depth === 0) {
      const p = toks[k - 1];
      const afterEnd = (p.t === "p" && p.v === ";") || (p.t === "p" && p.v === "}" && t.nl);
      const startOk = !(t.t === "p" && NOT_A_START_P.has(t.v)) && !(t.t === "id" && NOT_A_START_ID.has(t.v));
      if (afterEnd && startOk) cuts.push(k);
    }
    depth += depthDelta(t);
  }
  cuts.push(toks.length);
  const units = [];
  for (let c = 0; c < cuts.length - 1; c++) {
    const start = cuts[c], end = cuts[c + 1];
    let k = start;
    while (k < end && toks[k].t === "id" && (toks[k].v === "export" || toks[k].v === "default" || toks[k].v === "async")) k++;
    const head = toks[k];
    let names = null, quiet = false;
    if (toks[start].t === "id" && toks[start].v === "import") quiet = true;
    else if (toks[start].t === "id" && toks[start].v === "export" && toks[start + 1] && toks[start + 1].t === "p" && (toks[start + 1].v === "{" || toks[start + 1].v === "*")) quiet = true;
    else if (head && head.t === "id" && (head.v === "function" || head.v === "class")) {
      let n = toks[k + 1];
      if (n && n.t === "p" && n.v === "*") n = toks[k + 2];
      if (n && n.t === "id") names = [n.v];
    } else if (head && head.t === "id" && (head.v === "const" || head.v === "let" || head.v === "var")) {
      const first = toks[k + 1];
      if (first && first.t === "id") {
        names = [first.v];
        let d = 0;
        for (let j = k + 2; j < end; j++) {
          d += depthDelta(toks[j]);
          if (d === 0 && toks[j].t === "p" && toks[j].v === "," && toks[j + 1] && toks[j + 1].t === "id") names.push(toks[j + 1].v);
        }
      }
    }
    units.push({ start, end, names, quiet });
  }
  mod.units = units;
  return units;
}

// Which units of the reachable modules can run: roots, plus declarations named from included code.
function includedUnits(mods) {
  const byName = new Map(); // name -> [{mod, unit}]
  const queue = [];
  const taken = new Set();
  const take = (mod, u) => { if (!taken.has(u)) { taken.add(u); queue.push({ mod, u }); } };
  for (const mod of mods) for (const u of unitsOf(mod)) {
    if (u.names) for (const n of u.names) { if (!byName.has(n)) byName.set(n, []); byName.get(n).push({ mod, u }); }
    else take(mod, u);
  }
  const seenId = new Set();
  while (queue.length) {
    const { mod, u } = queue.shift();
    if (u.quiet) continue;
    for (let k = u.start; k < u.end; k++) {
      const t = mod.toks[k];
      if (t.t !== "id" || seenId.has(t.v)) continue;
      seenId.add(t.v);
      for (const hit of byName.get(t.v) || []) take(hit.mod, hit.u);
    }
  }
  return taken;
}

// Returns { literal: [cmd], nonLiteral: [{file, expr}], argTokIdx } for the token range [from, to).
function invokesOf(mod, from, to) {
  const { toks, rel } = mod;
  const literal = [];
  const nonLiteral = [];
  const argTokIdx = new Set();
  for (let k = from; k < to; k++) {
    const t = toks[k];
    if (t.t !== "id" || !has(INVOKERS, t.v)) continue;
    const next = toks[k + 1];
    if (!next || next.t !== "p" || next.v !== "(") continue;
    const prev = toks[k - 1];
    if (prev && prev.t === "id" && prev.v === "function") continue;
    // split the arguments at depth 0
    let depth = 0, j = k + 2;
    const args = [[]];
    for (; j < toks.length; j++) {
      const a = toks[j];
      const d = depthDelta(a);
      if (d < 0 && depth === 0) break;
      depth += d;
      if (depth === 0 && a.t === "p" && a.v === ",") { args.push([]); continue; }
      args[args.length - 1].push(j);
    }
    const arg = args[INVOKERS[t.v]] || [];
    if (arg.length === 1 && (toks[arg[0]].t === "str" || toks[arg[0]].t === "tpl")) {
      literal.push(toks[arg[0]].v);
      argTokIdx.add(arg[0]);
    } else if (arg.length >= 5 && toks[arg[arg.length - 4]].v === "?" && toks[arg[arg.length - 3]].t === "str" &&
      toks[arg[arg.length - 2]].v === ":" && toks[arg[arg.length - 1]].t === "str") {
      // cond ? "a" : "b"
      for (const at of [arg[arg.length - 3], arg[arg.length - 1]]) { literal.push(toks[at].v); argTokIdx.add(at); }
    } else {
      const expr = arg.map((x) => toks[x].v).join(" ") || "(none)";
      nonLiteral.push({ file: rel, expr });
    }
  }
  return { literal, nonLiteral, argTokIdx };
}

function mentionsOf(mod, from, to, argTokIdx) {
  const out = [];
  for (let k = from; k < to; k++) {
    const t = mod.toks[k];
    if ((t.t === "str" || t.t === "tpl") && COMMAND_SET.has(t.v) && !argTokIdx.has(k)) out.push(t.v);
  }
  return out;
}

// shake=false counts every unit of every reachable module (used to list what the shaking removed).
function inventory(entryScripts, pageLabel, shake = true) {
  const mods = reachable(entryScripts);
  const keep = shake ? includedUnits(mods) : null;
  const cmds = new Map(); // cmd -> Set(files)
  for (const mod of mods) {
    for (const u of unitsOf(mod)) {
      if (keep && !keep.has(u)) continue;
      const { literal, nonLiteral, argTokIdx } = invokesOf(mod, u.start, u.end);
      for (const c of literal) {
        if (!COMMAND_SET.has(c)) fail(`${pageLabel}: ${mod.rel} invokes "${c}", which is not in the handler list`);
        if (!cmds.has(c)) cmds.set(c, new Set());
        cmds.get(c).add(mod.rel);
      }
      for (const nl of nonLiteral) {
        const key = path.posix.basename(nl.file) + "|" + nl.expr;
        if (!has(NON_LITERAL_OK, key)) fail(`${pageLabel}: ${nl.file} invokes a command by the expression "${nl.expr}" that cannot be resolved (list it in NON_LITERAL_OK with a reason, or pass a literal)`);
      }
      for (const c of mentionsOf(mod, u.start, u.end, argTokIdx)) {
        const base = path.posix.basename(mod.rel);
        if (has(WRAPPED_LITERAL, base + "|" + c)) {
          if (!cmds.has(c)) cmds.set(c, new Set());
          cmds.get(c).add(mod.rel);
        } else if (!has(NOT_A_COMMAND, base + "|" + c) && !has(NOT_A_COMMAND, base + "|*")) fail(`${pageLabel}: ${mod.rel} mentions the command name "${c}" outside an invoke call (an alias or wrapper?)`);
      }
    }
  }
  return { cmds, files: mods.map((m) => m.rel) };
}

// ---------------------------------------------------------------- capability files

function readCap(name) {
  const file = path.join(CAPS, name + ".json");
  if (!fs.existsSync(file)) { fail(`capabilities/${name}.json is missing`); return null; }
  try { return JSON.parse(fs.readFileSync(file, "utf8")); } catch (e) { fail(`capabilities/${name}.json: ${e.message}`); return null; }
}

const setEq = (a, b) => a.size === b.size && [...a].every((x) => b.has(x));

function reportInventory(spec) {
  const entries = htmlScripts(spec.page).map((s) => normRel(spec.page, s));
  if (!entries.length) { fail(`${spec.page}: no script tag`); return null; }
  const inv = inventory(entries, spec.page);
  if (process.env.CAP_SHAKEN) {
    const all = inventory(entries, spec.page, false);
    const gone = [...all.cmds.keys()].filter((c) => !inv.cmds.has(c)).sort();
    console.log(`  shaken out of ${spec.page}: ${gone.join(", ") || "-"}`);
  }
  const list = process.env.CAP_LIST ? "\n  " + [...inv.cmds.keys()].sort().join("\n  ") : "";
  console.log(`${spec.page}: ${inv.cmds.size} commands from ${inv.files.length} script file(s)${list}`);
  return inv;
}

function checkCapabilityFiles() {
  const onDisk = fs.readdirSync(CAPS).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)).sort();
  const expected = [MAIN, ...SATELLITES].map((s) => s.cap).sort();
  if (JSON.stringify(onDisk) !== JSON.stringify(expected)) {
    fail(`capabilities/ holds [${onDisk.join(", ")}], expected [${expected.join(", ")}]`);
  }
  const mainCap = readCap(MAIN.cap);
  const mainCore = new Set();
  const labelOwner = new Map();

  for (const spec of [MAIN, ...SATELLITES]) {
    const cap = readCap(spec.cap);
    if (!cap) { if (spec !== MAIN) reportInventory(spec); continue; }
    const where = `capabilities/${spec.cap}.json`;
    if (cap.identifier !== spec.cap) fail(`${where}: identifier is "${cap.identifier}", expected "${spec.cap}"`);
    if (JSON.stringify(cap.windows) !== JSON.stringify(spec.windows)) {
      fail(`${where}: windows are ${JSON.stringify(cap.windows)}, expected ${JSON.stringify(spec.windows)}`);
    }
    for (const w of cap.windows || []) {
      if (labelOwner.has(w)) fail(`${where}: window "${w}" is also covered by ${labelOwner.get(w)}`);
      labelOwner.set(w, where);
    }
    const perms = cap.permissions || [];
    const dup = perms.filter((p, k) => perms.indexOf(p) !== k);
    if (dup.length) fail(`${where}: duplicate permissions ${dup.join(", ")}`);
    const core = perms.filter((p) => typeof p === "string" && p.startsWith("core:"));
    const app = perms.filter((p) => typeof p === "string" && !p.includes(":"));
    const odd = perms.filter((p) => typeof p !== "string" || (!p.startsWith("core:") && p.includes(":")));
    if (odd.length) fail(`${where}: unexpected permission entries ${JSON.stringify(odd)}`);
    const allowed = new Set();
    for (const p of app) {
      if (!/^allow-[a-z0-9-]+$/.test(p)) { fail(`${where}: permission "${p}" is not an allow-<command> entry`); continue; }
      const c = cmdOf(p);
      if (!COMMAND_SET.has(c)) { fail(`${where}: ${p} names no command of the handler list`); continue; }
      if (permOf(c) !== p) fail(`${where}: ${p} is not the permission name of ${c}`);
      allowed.add(c);
    }

    if (spec === MAIN) {
      core.forEach((p) => mainCore.add(p));
      const all = new Set(COMMANDS);
      const missing = COMMANDS.filter((c) => !allowed.has(c));
      if (missing.length) fail(`${where}: the main window lacks ${missing.length} commands, e.g. ${missing.slice(0, 5).join(", ")}`);
      if (!setEq(allowed, all) && !missing.length) fail(`${where}: allows commands that are not in the handler list`);
      continue;
    }

    for (const p of core) if (!mainCore.has(p)) fail(`${where}: core permission ${p} is not one of the main window's`);
    if (!core.includes("core:event:default")) fail(`${where}: lacks core:event:default (every page listens to events)`);

    const inv = reportInventory(spec);
    if (!inv) continue;
    const used = new Set(inv.cmds.keys());
    const missing = [...used].filter((c) => !allowed.has(c)).sort();
    const extra = [...allowed].filter((c) => !used.has(c)).sort();
    if (missing.length) fail(`${where}: ${spec.page} invokes ${missing.length} command(s) the window may not call (feature broken): ${missing.map((c) => `${c} [${[...inv.cmds.get(c)].join(", ")}]`).join("; ")}`);
    if (extra.length) fail(`${where}: allows ${extra.length} command(s) ${spec.page} never invokes: ${extra.join(", ")}`);
  }

  for (const page of NO_SCRIPT_PAGES) {
    if (htmlScripts(page).length) fail(`${page}: now loads a script; give its window a capability and list it in SATELLITES`);
  }
  // A window label the app builds must be covered by a capability (or be a no-script page).
  const covered = [...labelOwner.keys()];
  const match = (glob, label) => glob.endsWith("*") ? label.startsWith(glob.slice(0, -1)) : glob === label;
  const builders = [
    ["src/camera.rs", "camera"], ["src/rec_bar.rs", "rec-bar"], ["src/tray.rs", "tray-menu"],
    ["src/var_prompt.rs", "vars-0"], ["src/var_prompt.rs", "launcher"], ["src/video_editor.rs", "video-editor"],
    ["src/snip.rs", "snip-0"], ["src/main_window.rs", "main"],
  ];
  for (const [file, label] of builders) {
    const src = fs.readFileSync(path.join(ROOT, "src-tauri", file), "utf8");
    const mentions = src.includes(`"${label}"`) || src.includes(`: &str = "${label}"`);
    if (!mentions) fail(`${file}: the window label "${label}" is no longer in the source; update the checks`);
    if (!covered.some((g) => match(g, label))) fail(`window "${label}" is covered by no capability`);
  }
  for (const label of ["float-0", "folder-0"]) {
    if (!covered.some((g) => match(g, label))) fail(`window "${label}" is covered by no capability`);
  }
}

function checkBuildScript() {
  const src = fs.readFileSync(path.join(ROOT, "src-tauri", "build.rs"), "utf8");
  if (!src.includes("app_manifest")) fail("build.rs: no AppManifest; every window would keep every command");
  if (!src.includes("generate_handler!")) fail("build.rs: the command list is not read from the generate_handler! block of lib.rs");
  if (!src.includes("rerun-if-changed=src/lib.rs")) fail("build.rs: no rerun-if-changed for src/lib.rs");
  // After a build the generated manifest lists what the compiler saw; it must equal the handler list.
  const gen = path.join(ROOT, "src-tauri", "gen", "schemas", "acl-manifests.json");
  if (fs.existsSync(gen)) {
    try {
      const manifests = JSON.parse(fs.readFileSync(gen, "utf8"));
      const app = manifests["__app-acl__"];
      if (!app) { fail("gen/schemas/acl-manifests.json: no app manifest (stale build?)"); return; }
      const perms = Object.keys(app.permissions || {}).filter((p) => p.startsWith("allow-"));
      const got = new Set(perms.map(cmdOf));
      if (!setEq(got, COMMAND_SET)) {
        const miss = COMMANDS.filter((c) => !got.has(c));
        const more = [...got].filter((c) => !COMMAND_SET.has(c));
        fail(`gen/schemas/acl-manifests.json differs from the handler list (missing: ${miss.slice(0, 5).join(", ")}; extra: ${more.slice(0, 5).join(", ")})`);
      }
    } catch (e) { fail(`gen/schemas/acl-manifests.json: ${e.message}`); }
  }
}

checkCapabilityFiles();
checkBuildScript();

console.log(`${COMMANDS.length} commands in the handler list`);
if (problems.length) {
  console.error(problems.map((p) => "FAIL " + p).join("\n"));
  console.error(`${problems.length} problem(s)`);
  process.exit(1);
}
console.log("capability_check: ok");
