// Small UI fixes, each section runs the real extracted source of the function in a vm.
// UI_CHECK_DIR points the test at another copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/chain_editor_search_fixes_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const uiDir = process.env.UI_CHECK_DIR || path.join(__dirname, "..", "..", "ui");
const read = (name) => fs.readFileSync(path.join(uiDir, name), "utf8").replace(/\r\n/g, "\n");
function extractBlock(src, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find (text changed?): ${marker}`);
  const braceStart = src.indexOf("{", src.indexOf(")", start));
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const problems = [];
const check = (ok, what) => { if (!ok) problems.push(what); };
const pending = [];
const later = (fn) => { pending.push(fn().catch((e) => problems.push(String(e.message || e)))); };
const section = (fn) => { try { fn(); } catch (e) { problems.push(String(e.message || e)); } };
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };
const tick = () => new Promise((r) => setImmediate(r));
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

// chain.js pushChainClipboard: only the newest push counts.
later(async () => {
  const src = read("chain.js");
  const from = src.includes("let chainPushSeq") ? src.indexOf("let chainPushSeq") : src.indexOf("export function pushChainClipboard");
  const code = src.slice(from, src.indexOf("// Reflect one tile"));
  const viaCopy = (cmd, args) => {
    if (cmd !== "copy_text") { commits.push(args.id); return Promise.resolve(); }
    const d = deferred(); calls.push({ key: "-", cmd, args, d }); return d.promise;
  };
  const calls = [];
  const commits = [];
  const toasts = [];
  const usage = [];
  const sel = [];
  const sandbox = run(code.replace(/export /g, "") + "\n;globalThis.push = pushChainClipboard;", {
    chainSel: sel,
    chainText: () => sel.join("+"),
    serialInvoke: (key, cmd, args) => { const d = deferred(); calls.push({ key, cmd, args, d }); return d.promise; },
    invoke: viaCopy,
    errToast: (e) => toasts.push(String(e)),
    recordToolUsage: (k) => usage.push(k),
  });
  // a picked, then b picked while a's write is slow: the old result commits nothing,
  // the newest one commits both picks.
  sel.push("a"); sandbox.push("a");
  sel.push("b"); sandbox.push("b");
  check(calls.length === 2 && calls[0].key === calls[1].key, "chain writes share one ordered key");
  calls[1].d.resolve(true);
  await tick();
  check(commits.join() === "a,b", `newest push commits the picks it carries, got ${commits}`);
  calls[0].d.resolve(true);
  await tick();
  check(commits.length === 2 && usage.length === 1, "a stale result commits and records nothing");
  // a stale failure shows no toast, a current one does
  sel.push("c"); sandbox.push("c");
  sel.push("d"); sandbox.push("d");
  calls[2].d.reject(new Error("old"));
  calls[3].d.reject(new Error("new"));
  await tick();
  check(toasts.length === 1 && toasts[0].includes("new"), `only the current failure toasts, got ${toasts}`);
  // an empty chain cancels the in-flight push and drops its pending picks
  sel.length = 0; sel.push("e"); sandbox.push("e");
  sel.length = 0; sandbox.push();
  calls[4].d.resolve(true);
  await tick();
  check(commits.length === 2, `an empty push cancels an in-flight one, got ${commits}`);
  // a pick removed from the chain before the write lands is not committed
  sel.length = 0; sel.push("f", "g");
  sandbox.push("f"); sandbox.push("g");
  sel.splice(0, 1);
  calls[6].d.resolve(true);
  await tick();
  check(commits.slice(2).join() === "g", `a removed pick burns nothing, got ${commits.slice(2)}`);
});

// editor.js openFileCreate: a slower preview never opens over a newer file.
later(async () => {
  const src = read("editor.js");
  const from = src.includes("let fileCreateSeq") ? src.indexOf("let fileCreateSeq") : src.indexOf("export async function openFileCreate");
  const code = src.slice(from, src.indexOf("// Left click: open the file dialog directly."));
  const opened = [];
  const previews = {};
  const sandbox = run(code.replace(/export /g, "") + "\n;globalThis.open = openFileCreate;", {
    modalSession: 0,
    mediaKind: () => "",
    PDF_EXT: /\.pdf$/i,
    t: (k) => k,
    loadFilePreview: (p) => { previews[p] = deferred(); return previews[p].promise; },
    openModal: (o) => { opened.push(o.filePath); sandbox.modalSession++; },
  });
  const slow = sandbox.open("slow.png");
  const fast = sandbox.open("fast.png");
  previews["fast.png"].resolve("img-fast");
  await fast;
  previews["slow.png"].resolve("img-slow");
  await slow;
  check(opened.join() === "fast.png", `only the latest file opens the dialog, got ${opened}`);
  // a dialog opened while the preview loads is left alone
  opened.length = 0;
  const late = sandbox.open("late.png");
  sandbox.modalSession++;
  previews["late.png"].resolve("x");
  await late;
  check(opened.length === 0, "a preview does not open over a dialog opened meanwhile");
});

// expertMenu.js: the restart hint belongs to an accepted switch only (the handler is a closure
// over the row, so the one source line is read).
section(() => {
  const line = read("expertMenu.js").split("\n").find((l) => l.includes('key === "allowMultipleInstances"') && l.includes("toastRestartNeeded"));
  check(!!line, "restart toast line not found");
  check(!!line && /&&\s*accepted\b/.test(line), "the restart toast is not gated on an accepted switch");
});

// theme.js previewThemeDraft: native controls follow the draft's base, like applyTheme.
section(() => {
  const src = read("theme.js");
  const attrs = {};
  const style = {};
  const sandbox = run(extractBlock(src, "export function previewThemeDraft(").replace("export ", "") + "\n;globalThis.preview = previewThemeDraft;", {
    themeDraft: { base: "dark", vars: {} },
    document: { documentElement: { setAttribute: (k, v) => { attrs[k] = v; }, style } },
    themeColorScheme: (theme) => (theme === "dark" ? "dark" : "light"),
    applyThemeVars: () => {},
    broadcastThemeSoon: () => {},
  });
  sandbox.preview();
  check(attrs["data-theme"] === "dark" && style.colorScheme === "dark", `a dark draft sets color-scheme dark, got ${style.colorScheme}`);
  sandbox.themeDraft = { base: "light", vars: {} };
  sandbox.preview();
  check(style.colorScheme === "light", `switching the draft back follows, got ${style.colorScheme}`);
});

// video.js snapRect: every number of a snapped rect is even, also at the edge of an odd source.
section(() => {
  const src = require("../ui_sources.js").videoWindowSource().replace(/\r\n/g, "\n");
  const sandbox = run(
    "const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));\nconst even = (v) => Math.max(0, Math.round(v / 2) * 2);\n"
    + extractBlock(src, "function snapRect(") + "\n;globalThis.snap = snapRect;",
    { VE: { w: 641, h: 481 } },
  );
  for (const rect of [{ x: 640, y: 480, w: 100, h: 100 }, { x: 999, y: 999, w: 2, h: 2 }, { x: 541, y: 381, w: 100, h: 100 }, { x: -5, y: -5, w: 641, h: 481 }]) {
    const r = sandbox.snap(rect);
    check([r.x, r.y, r.w, r.h].every((n) => n % 2 === 0), `snapRect(${JSON.stringify(rect)}) gave an odd number: ${JSON.stringify(r)}`);
    check(r.x + r.w <= 641 && r.y + r.h <= 481, `snapRect(${JSON.stringify(rect)}) leaves the frame: ${JSON.stringify(r)}`);
  }
});

// globalSearch.js gsPromptHits: a prompt that matches only through its file path belongs to
// the attachments group, not the prompt group.
section(() => {
  const src = read("globalSearch.js");
  const score = (p) => {
    const q = sandbox.libQuery;
    const fields = [["name", 3, p.name], ["text", 1, p.text], ["file", 1, p.file_path]];
    let best = 0, field = "";
    for (const [kind, w, val] of fields) if ((val || "").toLowerCase().includes(q) && w * 100 > best) { best = w * 100; field = kind; }
    return best ? { score: best, field } : { score: 0 };
  };
  const sandbox = run(extractBlock(src, "export function gsPromptHits(").replace("export ", "") + "\n;globalThis.hits = gsPromptHits;", {
    libQuery: "",
    assignLibQuery: (v) => { sandbox.libQuery = v; },
    gsImageHits: { query: "", ids: new Set() },
    computeLibSearchScore: (p) => score(p),
    varPreview: (s) => s,
    gsCopyPrompt: () => {},
    prompts: [
      { id: 1, name: "Alpha", text: "x", file_path: "C:/docs/report.pdf" },
      { id: 2, name: "Beta", text: "report text", file_path: "" },
      { id: 3, name: "Gamma", text: "see report", file_path: "C:/docs/report.pdf" },
      { id: 4, name: "Delta report", text: "", file_path: "C:/docs/report.pdf" },
    ],
  });
  const labels = sandbox.hits("report").map((h) => h.label).sort().join();
  check(labels === "Beta,Delta report,Gamma", `a file-path-only prompt stays out of the prompt group, got ${labels}`);
});

// globalSearch.js gsHelpHits: the article tip is searchable, like the help dialog's own search.
section(() => {
  const src = read("globalSearch.js");
  const sandbox = run(extractBlock(src, "export function gsHelpHits(").replace("export ", "") + "\n;globalThis.hits = gsHelpHits;", {
    helpNorm: (s) => String(s).toLowerCase(),
    helpCategories: () => [{ title: "Copying", whatItIs: "a", howTo: ["b"], whatHappens: "c", settings: ["d"], tip: "Hold shift for the hidden trick", section: "s" }],
    helpSectionLabel: (s) => s,
    gsOpenHelp: () => {},
  });
  check(sandbox.hits("hidden trick").length === 1, "a word only in the tip finds the help article");
  check(sandbox.hits("nothing here").length === 0, "an unrelated word finds nothing");
});

// journal.js journalMatches: the history search reads the same fields as the library search.
section(() => {
  const src = read("journal.js");
  const flags = { tags: true, notes: true };
  const sandbox = run(extractBlock(src, "export function journalMatches(").replace("export ", "") + "\n;globalThis.match = journalMatches;", {
    typeAllowed: () => true,
    journalTypes: new Set(),
    journalQuery: "",
    tagsOn: () => flags.tags,
    notesOn: () => flags.notes,
  });
  const p = { name: "Mail", text: "body", file_path: "", variants: ["second entry zebra"], tags: ["Work Stuff"], notes: "remember oranges" };
  const hit = (q) => { sandbox.journalQuery = q; return sandbox.match(p); };
  check(hit("zebra"), "a rotation entry is found");
  check(hit("work stuff"), "a tag is found while tags are on");
  check(hit("oranges"), "a note is found while notes are on");
  check(hit("body") && hit("mail"), "name and body still match");
  check(!hit("absent"), "an unrelated word matches nothing");
  flags.tags = false; flags.notes = false;
  check(!hit("work stuff") && !hit("oranges"), "tags and notes stay out of the search while their features are off");
  check(hit("zebra"), "a rotation entry is found with the features off");
});

// library.js computeLibSearchScore: a caller can hand in its own recognized-text hits, and the
// library's own set stays untouched.
section(() => {
  const src = read("library.js");
  const sandbox = run(extractBlock(src, "export function computeLibSearchScore(").replace("export ", "") + "\n;globalThis.score = computeLibSearchScore;", {
    libQuery: "sunset",
    imageHitQuery: "",
    imageHitIds: new Set(),
    parseSearchQuery: (raw) => ({ rest: raw, conditions: [] }),
    searchConditionMet: () => true,
    tagsOn: () => false,
    notesOn: () => false,
    flag: () => true,
    fuzzyTokens: (s) => s.split(/[^\p{L}\p{N}]+/u).filter(Boolean),
    fuzzyMaxTyposFor: () => 0,
    fuzzyWordHit: (a, b) => a === b,
    optFlag: () => true,
  });
  const p = { id: "p1", name: "Beach", text: "waves", file_path: "", variants: [] };
  check(sandbox.score(p).score === 0, "no hit set, no image match");
  const own = { query: "sunset", ids: new Set(["p1"]) };
  const r = sandbox.score(p, own);
  check(r.score > 0 && r.fromImage === true, `a handed-in hit set matches, got ${JSON.stringify(r)}`);
  check(sandbox.score(p, { query: "other", ids: new Set(["p1"]) }).score === 0, "a hit set for another query is ignored");
  check(sandbox.imageHitIds.size === 0 && sandbox.imageHitQuery === "", "the library's own hit state is untouched");
});

// globalSearch.js gsRefreshImageHits: one lookup per query, stale answers dropped.
later(async () => {
  const src = read("globalSearch.js");
  const from = src.indexOf("let gsImageHits");
  const code = src.slice(from, src.indexOf("// Reuses the library's scoring"));
  const calls = [];
  const renders = [];
  const sandbox = run(code.replace(/export /g, "") + "\n;globalThis.refresh = gsRefreshImageHits; globalThis.hits = () => gsImageHits;", {
    gsQuery: "",
    gsOpen: true,
    optFlag: () => true,
    gsRender: () => renders.push(1),
    invoke: (cmd, args) => { const d = deferred(); calls.push({ cmd, args, d }); return d.promise; },
  });
  sandbox.gsQuery = "sun";
  const a = sandbox.refresh();
  sandbox.refresh(); // a second render for the same query asks nothing more
  check(calls.length === 1 && calls[0].cmd === "search_image_text" && calls[0].args.query === "sun", `one lookup for the query, got ${calls.length}`);
  sandbox.gsQuery = "sunset";
  const b = sandbox.refresh();
  calls[1].d.resolve(["p2"]);
  await b;
  calls[0].d.resolve(["p1"]); // the slower, older answer
  await a;
  check(sandbox.hits().query === "sunset" && [...sandbox.hits().ids].join() === "p2", `the stale answer is dropped, got ${JSON.stringify([sandbox.hits().query, [...sandbox.hits().ids]])}`);
  check(renders.length === 1, "only the current answer renders");
  // closed overlay or a too-short query asks nothing
  sandbox.gsOpen = false;
  sandbox.gsQuery = "x";
  await sandbox.refresh();
  check(calls.length === 2, "a one-letter query asks nothing");
  // the flag off asks nothing
  sandbox.optFlag = () => false;
  sandbox.gsQuery = "another";
  await sandbox.refresh();
  check(calls.length === 2, "with picture text search off nothing is asked");
});

Promise.all(pending).then(() => {
  if (problems.length) { console.error("chain_editor_search_fixes_check FAILED:\n- " + problems.join("\n- ")); process.exit(1); }
  console.log("chain_editor_search_fixes_check: ok");
});
