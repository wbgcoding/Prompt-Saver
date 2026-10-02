// The camera window carries its own copies of the shared theme tokens and control
// rules (ui/styles/camera.css) instead of loading the whole main stylesheet.
// This check keeps those copies honest:
//   * every rule block in camera.css whose selector also exists in main.css has
//     the same declarations as that block in main.css,
//   * every custom property camera.css defines in a theme block has main.css's value,
//     and a theme that overrides a token camera.css defines overrides it here too,
//   * every var(--x) camera.css uses is defined in its :root block (a var() with a
//     fallback is exempt: the page sets those at run time).
//
// Run:  node tests/camera_css_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");

const STYLES = path.join(__dirname, "..", "ui", "styles");

const stripComments = (css) => css.replace(/\/\*[\s\S]*?\*\//g, "");
const squash = (s) => s.replace(/\s+/g, " ").trim();
const normSelector = (s) => squash(s).replace(/\s*,\s*/g, ", ");

// Index of the character that closes the group opened at `open`, skipping quoted
// strings and url(...) payloads.
function closing(text, open, openCh, closeCh) {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (c === '"' || c === "'") {
      const end = text.indexOf(c, i + 1);
      if (end < 0) return -1;
      i = end;
      continue;
    }
    if (c === openCh) depth++;
    else if (c === closeCh && --depth === 0) return i;
  }
  return -1;
}

// Splits a declaration list at the semicolons that are not inside quotes or parens.
function splitDecls(body) {
  const out = [];
  let start = 0;
  let paren = 0;
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (c === '"' || c === "'") {
      const end = body.indexOf(c, i + 1);
      i = end < 0 ? body.length : end;
    } else if (c === "(") paren++;
    else if (c === ")") paren--;
    else if (c === ";" && paren === 0) {
      out.push(body.slice(start, i));
      start = i + 1;
    }
  }
  out.push(body.slice(start));
  return out.map((d) => d.trim()).filter(Boolean);
}

const normDecl = (d) => {
  const at = d.indexOf(":");
  return at < 0 ? squash(d) : `${d.slice(0, at).trim().toLowerCase()}: ${squash(d.slice(at + 1))}`;
};

// Top-level rules as { media, selector, raw, decls[] }. A rule inside @media carries
// that media text; @keyframes blocks are kept whole.
function parseRules(css, media = null) {
  const text = stripComments(css);
  const rules = [];
  let i = 0;
  while (i < text.length) {
    const open = text.indexOf("{", i);
    if (open < 0) break;
    const close = closing(text, open, "{", "}");
    if (close < 0) break;
    const head = squash(text.slice(i, open));
    const body = text.slice(open + 1, close);
    if (/^@(media|supports)\b/.test(head)) {
      rules.push(...parseRules(body, head));
    } else if (/^@keyframes\b/.test(head)) {
      rules.push({ media, selector: head, decls: [squash(body)] });
    } else {
      rules.push({ media, selector: normSelector(head), decls: splitDecls(body).map(normDecl) });
    }
    i = close + 1;
  }
  return rules;
}

const keyOf = (r) => `${r.media || ""}|${r.selector}`;
const isTokenBlock = (r) => !r.media && /^:root(\[data-theme="[a-z]+"\])?$/.test(r.selector);

// Custom properties of the token blocks as { selector -> Map(name -> value) }.
function tokenMap(rules) {
  const map = new Map();
  for (const r of rules.filter(isTokenBlock)) {
    const props = map.get(r.selector) || new Map();
    for (const d of r.decls) {
      const at = d.indexOf(":");
      if (d.startsWith("--")) props.set(d.slice(0, at), d.slice(at + 2));
    }
    map.set(r.selector, props);
  }
  return map;
}

function check() {
  const main = fs.readFileSync(path.join(STYLES, "main.css"), "utf8");
  const cam = fs.readFileSync(path.join(STYLES, "camera.css"), "utf8");
  const problems = [];
  const mainRules = parseRules(main);
  const camRules = parseRules(cam);

  // Control and base rules: same selector -> same declarations as one of main's blocks.
  const mainByKey = new Map();
  for (const r of mainRules.filter((x) => !isTokenBlock(x))) {
    const list = mainByKey.get(keyOf(r)) || [];
    list.push(r.decls.join("; "));
    mainByKey.set(keyOf(r), list);
  }
  for (const r of camRules.filter((x) => !isTokenBlock(x))) {
    const want = mainByKey.get(keyOf(r));
    if (!want) continue;
    if (!want.includes(r.decls.join("; "))) {
      problems.push(`${r.media ? r.media + " " : ""}${r.selector}: declarations differ from main.css`);
    }
  }
  // A camera rule copied from main must also be complete: every main block with that
  // selector that camera.css copies at least once is copied in full.
  for (const [key, blocks] of mainByKey) {
    const camBlocks = camRules.filter((x) => !isTokenBlock(x) && keyOf(x) === key).map((x) => x.decls.join("; "));
    if (!camBlocks.length) continue;
    for (const b of blocks) {
      if (!camBlocks.includes(b)) problems.push(`${key.replace(/^\|/, "")}: a block of main.css (${b.slice(0, 60)}...) is missing from camera.css`);
    }
  }

  // Tokens.
  const mainTok = tokenMap(mainRules);
  const camTok = tokenMap(camRules);
  const camRoot = camTok.get(":root") || new Map();
  if (!camTok.has(":root")) problems.push("camera.css has no :root block");
  for (const [sel, props] of camTok) {
    const base = mainTok.get(sel);
    if (!base) { problems.push(`${sel}: not a theme block of main.css`); continue; }
    for (const [name, value] of props) {
      if (!base.has(name)) problems.push(`${sel}: main.css does not define ${name}`);
      else if (base.get(name) !== value) problems.push(`${sel}: ${name} differs from main.css`);
    }
  }
  for (const [sel, props] of mainTok) {
    if (sel === ":root") continue;
    for (const name of camRoot.keys()) {
      if (props.has(name) && !(camTok.get(sel) || new Map()).has(name)) {
        problems.push(`${sel}: main.css overrides ${name} but camera.css does not`);
      }
    }
  }

  // Every var(--x) in camera.css resolves to its :root block.
  const cleaned = stripComments(cam);
  const used = new Set();
  for (const m of cleaned.matchAll(/var\(\s*(--[\w-]+)\s*([,)])/g)) if (m[2] === ")") used.add(m[1]);
  for (const name of used) if (!camRoot.has(name)) problems.push(`var(${name}) is used but not defined in :root`);

  return problems;
}

module.exports = { parseRules, splitDecls, stripComments, normSelector, isTokenBlock, tokenMap };

if (require.main === module) {
  const problems = check();
  if (problems.length) {
    for (const p of problems) console.log("FAIL", p);
    console.log(`camera_css_check: ${problems.length} problem(s)`);
    process.exit(1);
  }
  console.log("camera_css_check: ok");
}
