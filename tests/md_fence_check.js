// Markdown preview (ui/markdown.js: renderMarkdown): a code fence of four or more
// marks is closed only by a fence at least as long, so a three-mark fence inside it
// stays code. Runs the real file in a node vm with its core.js import stubbed.
// Run:  node tests/md_fence_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "markdown.js"), "utf8")
  .replace(/\r\n/g, "\n")
  .replace(/^import .*$/m, "const flag = () => false, t = (k) => k;")
  .replace(/^export /gm, "");
const sb = {};
vm.createContext(sb);
vm.runInContext(src + "\nthis.renderMarkdown = renderMarkdown; this.hlCode = hlCode;", sb);

const problems = [];
const check = (label, actual, expected) => {
  if (actual !== expected) problems.push(`${label}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
};
const blocks = (html) => (html.match(/<pre class="md-code">/g) || []).length;

const nested = sb.renderMarkdown("````md\n```js\nx\n```\n````\nafter");
check("four-mark fence keeps an inner three-mark fence as code", blocks(nested), 1);
check("text after the long fence is a paragraph", /<p>after<\/p>/.test(nested), true);
check("plain three-mark fence still closes", blocks(sb.renderMarkdown("```\na\n```\n```\nb\n```")), 2);
check("longer closing fence closes", blocks(sb.renderMarkdown("```\na\n`````\nb")), 1);

// An escaped apostrophe (a lifetime, an unterminated quote) keeps its entity whole.
for (const lang of ["rust", "js", "py", "sql", "bash", "css"]) {
  check(`${lang}: apostrophe entity is not cut by the number rule`, sb.hlCode("x &#39;a 12", lang).includes("&#39;"), true);
  check(`${lang}: a real number is still wrapped`, /<span class="hl-num">12<\/span>/.test(sb.hlCode("x &#39;a 12", lang)), true);
}

if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
console.log("md_fence_check: ok");
