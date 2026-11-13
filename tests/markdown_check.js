// Markdown preview and code highlighting (ui/markdown.js): mdEsc, hlCode per language and with
// entities at the boundaries, renderMarkdown block and inline output, the guarantee that a link
// target never becomes a live href or src, the MD_MAX_CHARS cut, detectCodeLang and
// hasMarkdownSyntax. Runs the real file in a node vm with its core.js import stubbed
// (flag() reads the FLAGS object, t() returns "T:<key>").
// Run:  node tests/markdown_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "markdown.js"), "utf8")
  .replace(/\r\n/g, "\n")
  .replace(/^import .*$/m, "const flag = (k) => !!FLAGS[k], t = (k) => \"T:\" + k;")
  .replace(/^export /gm, "");
const sb = { FLAGS: { codeHighlight: true, codeAutoDetect: false } };
vm.createContext(sb);
vm.runInContext(
  src + "\nthis.api = { mdEsc, hlCode, renderMarkdown, detectCodeLang, hasMarkdownSyntax, mdInline, MD_MAX_CHARS, HL_MAX_CHARS };",
  sb
);
const api = sb.api;

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  if (actual !== expected) problems.push(`${label}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
};
const md = (text, flags) => {
  sb.FLAGS = Object.assign({ codeHighlight: true, codeAutoDetect: false }, flags || {});
  return api.renderMarkdown(text);
};
const span = (cls, text) => `<span class="hl-${cls}">${text}</span>`;

// ---- limits
check("MD_MAX_CHARS", api.MD_MAX_CHARS, 100000);
check("HL_MAX_CHARS", api.HL_MAX_CHARS, 20000);

// ---- mdEsc
check("mdEsc all five characters", api.mdEsc(`<a href="x">&'`), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
check("mdEsc empty", api.mdEsc(""), "");
check("mdEsc number", api.mdEsc(5), "5");
check("mdEsc null", api.mdEsc(null), "null");
check("mdEsc escapes an existing entity", api.mdEsc("&amp;"), "&amp;amp;");
check("mdEsc unicode untouched", api.mdEsc("\u00fc\u65e5\ud83d\ude00"), "\u00fc\u65e5\ud83d\ude00");
check("mdEsc plain text untouched", api.mdEsc("just text 1+1=2"), "just text 1+1=2");

// ---- hlCode
const hl = api.hlCode;
check("hlCode unknown language is unchanged", hl("const x = 1", "cobol"), "const x = 1");
check("hlCode missing language is unchanged", hl("const x = 1", undefined), "const x = 1");
check("hlCode empty input", hl("", "js"), "");
check("hlCode plain text without tokens", hl("hello world", "js"), "hello world");
check("hlCode js keyword number comment", hl("const x = 1; // hi", "js"), `${span("kw", "const")} x = ${span("num", "1")}; ${span("cmt", "// hi")}`);
check("hlCode js function call", hl("foo(1)", "js"), `${span("fn", "foo")}(${span("num", "1")})`);
check("hlCode js keyword before paren wins over call", hl("if (a)", "js"), `${span("kw", "if")} (a)`);
check("hlCode js double-quoted string", hl("x = &quot;a b&quot;", "js"), `x = ${span("str", "&quot;a b&quot;")}`);
check("hlCode js single-quoted string with escape", hl("&#39;it\\&#39;s&#39;", "js"), span("str", "&#39;it\\&#39;s&#39;"));
check("hlCode js template string", hl("`a${b}c`", "js"), span("str", "`a${b}c`"));
check("hlCode js block comment", hl("/* a\nb */ x", "js"), `${span("cmt", "/* a\nb */")} x`);
check("hlCode js unterminated block comment runs to the end", hl("x /* open\nmore", "js"), `x ${span("cmt", "/* open\nmore")}`);
check("hlCode js hex number", hl("0xFF", "js"), span("num", "0xFF"));
check("hlCode js float with exponent", hl("1.5e-3", "js"), span("num", "1.5e-3"));
check("hlCode js word containing a keyword stays plain", hl("constant", "js"), "constant");
check("hlCode ts alias is not applied by hlCode itself", hl("const x", "ts"), "const x");
check("hlCode py def and comment", hl("def f(x): # c", "py"), `${span("kw", "def")} ${span("fn", "f")}(x): ${span("cmt", "# c")}`);
check("hlCode py triple quoted string", hl("&quot;&quot;&quot;a\nb&quot;&quot;&quot;", "py"), span("str", "&quot;&quot;&quot;a\nb&quot;&quot;&quot;"));
check("hlCode py unterminated triple quote runs to the end", hl("x = &#39;&#39;&#39;abc", "py"), `x = ${span("str", "&#39;&#39;&#39;abc")}`);
check("hlCode py True is a keyword", hl("True", "py"), span("kw", "True"));
check("hlCode json key, number, literal", hl("{&quot;a&quot;: 1, &quot;b&quot;: true}", "json"), `{${span("kw", "&quot;a&quot;")}: ${span("num", "1")}, ${span("kw", "&quot;b&quot;")}: ${span("kw", "true")}}`);
check("hlCode json string value keeps the string colour", hl("{&quot;k&quot;: &quot;v&quot;}", "json"), `{${span("kw", "&quot;k&quot;")}: ${span("str", "&quot;v&quot;")}}`);
check("hlCode json negative exponent number", hl("[-2.5e+3]", "json"), `[${span("num", "-2.5e+3")}]`);
check("hlCode sql keywords are case-insensitive", hl("select a FROM t", "sql"), `${span("kw", "select")} a ${span("kw", "FROM")} t`);
check("hlCode sql line comment", hl("-- note\nx", "sql"), `${span("cmt", "-- note")}\nx`);
check("hlCode sql string", hl("&#39;abc&#39;", "sql"), span("str", "&#39;abc&#39;"));
check("hlCode bash comment and variable", hl("echo $HOME # x", "bash"), `${span("kw", "echo")} ${span("fn", "$HOME")} ${span("cmt", "# x")}`);
check("hlCode bash braced variable", hl("${A_B}", "bash"), span("fn", "${A_B}"));
check("hlCode html tag, attribute, string", hl("&lt;b class=&quot;x&quot;&gt;", "html"), `${span("kw", "&lt;b")} ${span("fn", "class")}=${span("str", "&quot;x&quot;")}${span("kw", "&gt;")}`);
check("hlCode html comment", hl("&lt;!-- c --&gt;", "html"), span("cmt", "&lt;!-- c --&gt;"));
check("hlCode css block comment", hl("/* c */", "css"), span("cmt", "/* c */"));
check("hlCode css hex colour", hl("#fff", "css"), span("num", "#fff"));
check("hlCode rust fn and attribute", hl("#[derive(Debug)]\nfn main() {}", "rust"), `${span("fn", "#[derive(Debug)]")}\n${span("kw", "fn")} ${span("fn", "main")}() {}`);
check("hlCode rust char literal", hl("&#39;x&#39;", "rust"), span("str", "&#39;x&#39;"));
check("hlCode rust lifetime stays plain", hl("&amp;&#39;a str", "rust"), "&amp;&#39;a str");
check("hlCode rust typed number", hl("5u8", "rust"), span("num", "5u8"));

// Entities at the boundaries are never cut.
check("hlCode entity at the very start", hl("&amp;&amp; x", "js"), "&amp;&amp; x");
check("hlCode number then a trailing apostrophe entity", hl("1 &#39;", "js"), `${span("num", "1")} &#39;`);
check("hlCode a bare apostrophe entity digit is no number", hl("&#39;", "js"), "&#39;");
check("hlCode a bare apostrophe entity is no python comment", hl("&#39;", "py"), "&#39;");
check("hlCode a bare apostrophe entity is no bash comment", hl("&#39;", "bash"), "&#39;");
check("hlCode a bare apostrophe entity is no css hex colour", hl("&#39;", "css"), "&#39;");
check("hlCode lt and gt entities around code stay whole", hl("a &lt; b &gt; c", "js"), "a &lt; b &gt; c");
check("hlCode string directly after an entity", hl("&amp;&quot;x&quot;", "js"), `&amp;${span("str", "&quot;x&quot;")}`);
check("hlCode string directly before an entity", hl("&quot;x&quot;&amp;", "js"), `${span("str", "&quot;x&quot;")}&amp;`);
check("hlCode a quote entity inside a double-quoted string", hl("&quot;a &amp; b&quot;", "js"), span("str", "&quot;a &amp; b&quot;"));
check("hlCode repeated calls give the same result", hl("const x = 1", "js") === hl("const x = 1", "js"), true);

// ---- renderMarkdown: blocks
check("empty text", md(""), "");
check("one paragraph", md("hello"), "<p>hello</p>");
check("line breaks stay as br", md("a\nb"), "<p>a<br>b</p>");
check("blank line splits paragraphs", md("a\n\nb"), "<p>a</p><p>b</p>");
check("CRLF line breaks", md("a\r\nb"), "<p>a<br>b</p>");
check("lone CR line breaks", md("a\rb"), "<p>a<br>b</p>");
check("headings 1 and 3", md("# One\n### Three"), "<h1>One</h1><h3>Three</h3>");
check("heading level 6", md("###### Six"), "<h6>Six</h6>");
check("seven hashes is no heading", md("####### seven"), "<p>####### seven</p>");
check("hash without a blank is no heading", md("#nospace"), "<p>#nospace</p>");
check("heading with inline markup", md("# a **b**"), "<h1>a <strong>b</strong></h1>");
check("bullet list", md("- a\n- b"), "<ul><li>a</li><li>b</li></ul>");
check("star and plus bullets share one list", md("* a\n+ b"), "<ul><li>a</li><li>b</li></ul>");
check("numbered list", md("1. a\n2) b"), "<ol><li>a</li><li>b</li></ol>");
check("switching list kind closes the first", md("- a\n1. b"), "<ul><li>a</li></ul><ol><li>b</li></ol>");
check("list then paragraph", md("- a\n\ntext"), "<ul><li>a</li></ul><p>text</p>");
check("list directly followed by text closes the list", md("- a\ntext"), "<ul><li>a</li></ul><p>text</p>");
check("a dash with no space is no bullet", md("-a"), "<p>-a</p>");
check("a ten-digit number is no list marker", md("1234567890. x"), "<p>1234567890. x</p>");
check("blockquote", md("> q1\n> q2"), "<blockquote>q1<br>q2</blockquote>");
check("blockquote without a space", md(">q"), "<blockquote>q</blockquote>");
check("quote then paragraph", md("> q\ntext"), "<blockquote>q</blockquote><p>text</p>");
check("horizontal rule", md("---"), "<hr>");
check("horizontal rule between paragraphs", md("a\n\n***\n\nb"), "<p>a</p><hr><p>b</p>");
check("spaced rule", md("* * *"), "<hr>");
check("two dashes are no rule", md("--"), "<p>--</p>");
// A pipe table is not rendered as a table: its lines stay a plain paragraph.
check("table lines stay a paragraph", md("| a | b |\n|---|---|\n| 1 | 2 |"), "<p>| a | b |<br>|---|---|<br>| 1 | 2 |</p>");
check("table output has no table element", /<table|<tr|<td/.test(md("| a | b |\n|---|---|\n| 1 | 2 |")), false);

// ---- renderMarkdown: fences
check("fenced code with highlight", md("```js\nconst a = 1;\n```"), `<pre class="md-code"><code>${span("kw", "const")} a = ${span("num", "1")};</code></pre>`);
check("fenced code with highlight off", md("```js\nconst a = 1;\n```", { codeHighlight: false }), '<pre class="md-code"><code>const a = 1;</code></pre>');
check("fence tag is case-insensitive", md("```JS\nlet a\n```").includes("hl-kw"), true);
check("fence alias typescript", md("```typescript\nlet a\n```").includes("hl-kw"), true);
check("unknown fence tag is plain code", md("```foo\nlet a\n```"), '<pre class="md-code"><code>let a</code></pre>');
check("unterminated fence takes the rest", md("```\nx\ny"), '<pre class="md-code"><code>x\ny</code></pre>');
check("tilde fence", md("~~~\nx\n~~~"), '<pre class="md-code"><code>x</code></pre>');
check("markdown inside a fence stays literal", md("```\n# no\n- no\n```"), '<pre class="md-code"><code># no\n- no</code></pre>');
check("html inside a fence is escaped", md("```\n<b>x</b>\n```"), '<pre class="md-code"><code>&lt;b&gt;x&lt;/b&gt;</code></pre>');
check("an empty fence", md("```\n```"), '<pre class="md-code"><code></code></pre>');
check("text after a fence is a paragraph", md("```\nx\n```\nafter"), '<pre class="md-code"><code>x</code></pre><p>after</p>');
check("a fence ends the paragraph before it", md("before\n```\nx\n```"), '<p>before</p><pre class="md-code"><code>x</code></pre>');
check("a long fence is rendered plain", md("```js\n" + "const x = 1;\n".repeat(2000) + "```").includes("hl-"), false);
check("a short fence of the same shape is highlighted", md("```js\n" + "const x = 1;\n".repeat(100) + "```").includes("hl-kw"), true);

// ---- renderMarkdown: inline
check("bold stars", md("**b**"), "<p><strong>b</strong></p>");
check("bold underscores", md("__b__"), "<p><strong>b</strong></p>");
check("italic star", md("*i*"), "<p><em>i</em></p>");
check("italic underscore", md("_i_"), "<p><em>i</em></p>");
check("italic underscore between words", md("a _b_ c"), "<p>a <em>b</em> c</p>");
check("inline code", md("`c`"), '<p><code class="md-inline-code">c</code></p>');
check("inline code keeps marks literal", md("`**x**`"), '<p><code class="md-inline-code">**x**</code></p>');
check("all inline kinds in one line", md("**b** *i* `c`"), '<p><strong>b</strong> <em>i</em> <code class="md-inline-code">c</code></p>');
check("unclosed bold stays literal", md("**open"), "<p>**open</p>");
check("emphasis does not span lines", md("*a\nb*"), "<p>*a<br>b*</p>");
check("escaped content inside bold", md("**a & b**"), "<p><strong>a &amp; b</strong></p>");
check("a link keeps only its label", md("[site](https://x.y)"), '<p><span class="md-link">site</span></p>');
check("a link with an empty label", md("[](https://x.y)"), '<p><span class="md-link"></span></p>');
check("a link without a target stays literal", md("[site]"), "<p>[site]</p>");
check("an image keeps only its label", md("![pic](a.png)"), '<p>!<span class="md-link">pic</span></p>');

// ---- a link target is never a live href or src
const hostile = [
  "[x](javascript:alert(1))",
  "[x](JaVaScRiPt:alert(1))",
  "[x](data:text/html;base64,PHNjcmlwdD4=)",
  "[x](vbscript:msgbox(1))",
  "![x](data:image/svg+xml,<svg onload=alert(1)>)",
  "[x](  javascript:alert(1))",
  "[<img src=x onerror=alert(1)>](https://x.y)",
  "<a href=\"javascript:alert(1)\">click</a>",
  "<img src=\"data:text/html,x\">",
  "<script>alert(1)</script>",
  "[a](b)[c](d)",
  "[x](java\nscript:alert(1))",
];
for (const h of hostile) {
  const html = md(h);
  check(`no anchor element: ${JSON.stringify(h)}`, /<a[\s>]/i.test(html), false);
  check(`no href or src attribute inside a real tag: ${JSON.stringify(h)}`, /<[a-z][^>]*\s(href|src|onerror|onload)\s*=/i.test(html), false);
  check(`no image, script or frame element: ${JSON.stringify(h)}`, /<(img|script|iframe|svg|object|embed)[\s>\/]/i.test(html), false);
  check(`every angle bracket is a known tag: ${JSON.stringify(h)}`, html.replace(/<\/?(p|br|span|strong|em|code|ul|ol|li|h[1-6]|blockquote|pre|hr)(\s[^<>]*)?>/g, "").includes("<"), false);
}
check("javascript target leaves only the label", md("[x](javascript:alert(1))"), '<p><span class="md-link">x</span>)</p>');
check("raw html is shown as text", md("<a href=\"x\">y</a>"), "<p>&lt;a href=&quot;x&quot;&gt;y&lt;/a&gt;</p>");
check("script tag is shown as text", md("<script>alert(1)</script>"), "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>");

// ---- oversize input is cut at MD_MAX_CHARS
const exact = md("a".repeat(100000));
check("exactly MD_MAX_CHARS is not cut", exact, "<p>" + "a".repeat(100000) + "</p>");
const over = md("a".repeat(100001));
check("one past MD_MAX_CHARS is cut", over, "<p>" + "a".repeat(100000) + '</p><p class="md-trunc">T:mdTruncated</p>');
const huge = md("line\n".repeat(60000));
check("a huge multi-line text is cut and flagged", huge.endsWith('<p class="md-trunc">T:mdTruncated</p>'), true);
check("a huge text cut keeps the first 100000 characters only", (huge.match(/line/g) || []).length, 20000);
check("a cut that splits a bold run stays well formed", md("*" + "a".repeat(99999) + "*b*"), "<p>*" + "a".repeat(99999) + '</p><p class="md-trunc">T:mdTruncated</p>');

// ---- detectCodeLang
const lang = api.detectCodeLang;
check("detect empty", lang(""), "");
check("detect whitespace", lang("  \n "), "");
check("detect a single line", lang("const x = 1;"), "");
check("detect a json document", lang('{\n  "a": 1,\n  "b": [1, 2]\n}'), "json");
check("detect a json array", lang('[\n  {"a": 1},\n  {"a": 2}\n]'), "json");
check("detect a shebang", lang("#!/bin/bash\necho hi"), "bash");
check("detect upper-case sql", lang("SELECT a\nFROM t"), "sql");
check("detect python", lang("def f(x):\n    return x\n"), "py");
check("detect js", lang("function f(a) {\n  return a;\n}"), "js");
check("detect rust", lang("fn main() {\n    let mut x = 1;\n}"), "rust");
check("detect html", lang("<div>\n  <p>hi</p>\n</div>"), "html");
check("a markdown list vetoes code", lang("- const x = 1;\n- let y = 2;"), "");
check("a fence vetoes code", lang("```js\nconst x = 1;\n```"), "");
check("prose with a code word is no code", lang("I think the function should be called\nwhen we have something to say about it today."), "");
check("text over the highlight ceiling is no code", lang('{\n"a": 1,\n' + "x\n".repeat(20000) + "}"), "");
check("auto detect renders one code block", md("def f(x):\n    return x", { codeAutoDetect: true }), `<pre class="md-code"><code>${span("kw", "def")} ${span("fn", "f")}(x):\n    ${span("kw", "return")} x</code></pre>`);
check("auto detect off leaves markdown", md("def f(x):\n    return x").startsWith("<p>"), true);
check("auto detect on without highlight is escaped plain code", md("function f(a) {\n  return a < 1;\n}", { codeAutoDetect: true, codeHighlight: false }), '<pre class="md-code"><code>function f(a) {\n  return a &lt; 1;\n}</code></pre>');

// ---- hasMarkdownSyntax
const has = api.hasMarkdownSyntax;
check("syntax: empty", has(""), false);
check("syntax: null", has(null), false);
check("syntax: plain sentence", has("just a sentence."), false);
check("syntax: heading", has("# h"), true);
check("syntax: bullet", has("- a"), true);
check("syntax: numbered", has("1. a"), true);
check("syntax: quote", has("> a"), true);
check("syntax: rule", has("---"), true);
check("syntax: fence", has("```\nx\n```"), true);
check("syntax: bold", has("a **b**"), true);
check("syntax: inline code", has("a `b`"), true);
check("syntax: link", has("[a](b)"), true);
check("syntax: a lone star", has("2 * 3"), false);
check("syntax: a number with a dot is no list", has("3.14"), false);
check("syntax: repeated calls agree (shared regex state)", [has("**a**"), has("**a**"), has("plain")].join(), "true,true,false");

// ---- known defect: intraword underscores must not italicise
check("renderMarkdown keeps snake_case_name literal", md("use my_var_name here"), "<p>use my_var_name here</p>");
check("renderMarkdown keeps two placeholder names literal", md("{?first_name} {?last_name}"), "<p>{?first_name} {?last_name}</p>");

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
}
console.log(`markdown_check: ${count} assertions OK`);
