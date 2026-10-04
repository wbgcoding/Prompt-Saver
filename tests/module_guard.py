"""Static guard for the main-window modules listed in tests/ui_sources.json.

The main window is split into ES modules. Two mistakes are not load errors, they
only fail when the code runs: assigning to an imported binding (a TypeError) and
using a name another module defines without importing it (a ReferenceError). This
guard finds both statically, plus the structural rules the split relies on.

Rules (one finding per line, exit 1 when there is any):
  (a) assignment to an imported name in a file that has no let/var of that name
      anywhere, and a top-level let/var carrying the name of an import
  (b) a free name: a name another listed module defines at its top level, or that
      a listed module imports from a shared module, used in a file that neither
      declares it at its own top level nor imports it (strict: a local shadow does
      not count; importing a name a file also shadows is harmless)
  (c) an import of ./main.js
  (d) a module imported by ui/core.js that imports ./core.js directly or transitively
  (e) start-up hand-over mismatch: the names of the "linked from main.js" let, the
      link<Name> destructure and the link<Name>({...}) call in main.js differ
  (f) a ui/*.js file that imports ./core.js but is not in the list
  (g) an imported name with no use outside its import line

Comments, strings and regex literals are blanked before any rule runs; code inside
template literal substitutions is kept. Run: python tests/module_guard.py
"""
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
UI = REPO / "ui"

IDENT = re.compile(r"(?<![\w$.])([A-Za-z_$][\w$]*)(?![\w$])")
IMPORT_RE = re.compile(r'^import\s+(?P<what>[^;]*?)\s+from\s+"(?P<src>[^"]+)";?\s*$', re.M)
DECL_FN = re.compile(r"(?:export\s+)?(?:async\s+)?(?:function\s*\*?|class)\s+([A-Za-z_$][\w$]*)")
DECL_VAR = re.compile(r"(?:export\s+)?(const|let|var)\s+")
REGEX_PREV = set("(,=:[!&|?{};+-*%<>~^")
REGEX_KEYWORDS = ("return", "typeof", "case", "do", "else", "in", "of", "void", "delete", "throw", "new", "yield", "await")


def blank(src):
    """Same-length text with comment, string and regex contents replaced by spaces
    (newlines kept). Template-literal substitution code is kept."""
    out = list(src)
    n = len(src)
    i = 0
    stack = []  # entries: ["tpl"] or ["sub", brace_depth]
    last = ""  # last significant char of code

    def wipe(a, b):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        mode = stack[-1][0] if stack else "code"
        if mode == "tpl":
            if c == "\\":
                wipe(i, min(i + 2, n))
                i += 2
                continue
            if c == "`":
                stack.pop()
                last = "`"
                i += 1
                continue
            if c == "$" and i + 1 < n and src[i + 1] == "{":
                wipe(i, i + 2)
                stack.append(["sub", 0])
                last = "{"
                i += 2
                continue
            wipe(i, i + 1)
            i += 1
            continue
        # code (top level or inside a substitution)
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            wipe(i, j)
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            wipe(i, j)
            i = j
            continue
        if c in "\"'":
            j = i + 1
            while j < n and src[j] != c and src[j] != "\n":
                j += 2 if src[j] == "\\" else 1
            wipe(i + 1, j)
            i = j + 1
            last = c
            continue
        if c == "`":
            stack.append(["tpl"])
            i += 1
            continue
        if c == "/":
            before = "".join(out[max(0, i - 12):i]).rstrip()
            is_regex = last == "" or last in REGEX_PREV or any(before.endswith(k) for k in REGEX_KEYWORDS)
            if is_regex:
                j = i + 1
                in_class = False
                while j < n and src[j] != "\n":
                    ch = src[j]
                    if ch == "\\":
                        j += 2
                        continue
                    if ch == "[":
                        in_class = True
                    elif ch == "]":
                        in_class = False
                    elif ch == "/" and not in_class:
                        break
                    j += 1
                wipe(i + 1, j)
                i = j + 1
                last = "/"
                continue
        if stack and stack[-1][0] == "sub":
            if c == "{":
                stack[-1][1] += 1
            elif c == "}":
                if stack[-1][1] == 0:
                    wipe(i, i + 1)
                    stack.pop()
                    i += 1
                    continue
                stack[-1][1] -= 1
        if not c.isspace():
            last = c
        i += 1
    return "".join(out)


def depth_map(text):
    d = 0
    res = []
    for ch in text:
        if ch in ")]}":
            d -= 1
        res.append(d)
        if ch in "([{":
            d += 1
    return res


def pattern_names(head):
    head = re.sub(r"=[^,}\]]*", "", head)
    names = []
    for m in re.finditer(r"([A-Za-z_$][\w$]*)", head):
        after = head[m.end():].lstrip()[:1]
        if after == ":":
            continue
        names.append(m.group(1))
    return names


def top_level(blanked):
    """Names declared at the top level, and the subset declared with let/var."""
    dm = depth_map(blanked)
    names, lets = set(), set()
    for m in DECL_FN.finditer(blanked):
        if dm[m.start()] == 0:
            names.add(m.group(1))
    for m in DECL_VAR.finditer(blanked):
        if dm[m.start()] != 0:
            continue
        prev = blanked[:m.start()].rstrip()[-1:]
        if prev not in ("", ";", "}", "{"):
            continue
        i = m.end()
        base = dm[i]
        j = i
        heads, cur = [], i
        n = len(blanked)
        while j < n:
            ch = blanked[j]
            if dm[j] == base and ch == ";":
                break
            if dm[j] == base and ch == ",":
                heads.append(blanked[cur:j])
                cur = j + 1
            j += 1
        heads.append(blanked[cur:j])
        for h in heads:
            # head = text before the first top-level "="
            depth, cut = 0, len(h)
            for k, ch in enumerate(h):
                if ch in "([{":
                    depth += 1
                elif ch in ")]}":
                    depth -= 1
                elif ch == "=" and depth == 0 and h[k + 1:k + 2] not in ("=", ">"):
                    cut = k
                    break
            head = h[:cut].strip()
            for nm in pattern_names(head):
                names.add(nm)
                if m.group(1) in ("let", "var"):
                    lets.add(nm)
    return names, lets


def parse_imports(raw):
    """[(local, source, is_default)] from the raw text."""
    out = []
    for m in IMPORT_RE.finditer(raw):
        what, src = m.group("what"), m.group("src")
        braces = re.search(r"\{([^}]*)\}", what)
        head = what[:braces.start()] if braces else what
        default = head.strip().rstrip(",").strip()
        if default and not default.startswith("*"):
            out.append((default, src, True))
        if braces:
            for part in braces.group(1).split(","):
                part = part.strip()
                if not part:
                    continue
                local = part.split(" as ")[-1].strip()
                out.append((local, src, False))
    return out


class Mod:
    def __init__(self, name, raw):
        self.name = name
        self.raw = raw
        self.blanked = blank(raw)
        self.imports = parse_imports(raw)
        # the code without import lines, for token scans
        self.body = IMPORT_RE.sub(lambda m: " " * len(m.group(0)), self.blanked)
        self.top, self.top_lets = top_level(self.body)
        self.any_let = set()
        for m in re.finditer(r"\b(?:let|var)\s+", self.body):
            i = m.end()
            j = self.body.find(";", i)
            j = len(self.body) if j < 0 else j
            for h in re.split(r",(?![^{\[(]*[}\])])", self.body[i:j]):
                head = h.split("=")[0]
                for nm in pattern_names(head):
                    self.any_let.add(nm)
        self.imported = {loc for loc, _src, _d in self.imports}


def tokens(mod):
    """(name, index, key_skipped) for every identifier token in the code."""
    body = mod.body
    for m in IDENT.finditer(body):
        after = body[m.end():].lstrip()[:1]
        before = body[:m.start()].rstrip()[-1:]
        if after == ":" and before in ("{", ","):
            continue
        yield m.group(1), m.start()


def module_key(src):
    if src.startswith("./"):
        return src[2:]
    return None


def check(files):
    """files: dict name -> raw source. Returns a list of finding strings."""
    mods = {n: Mod(n, raw) for n, raw in files.items()}
    findings = []
    listed = set(mods)
    shared = {}
    for m in mods.values():
        for loc, src, _d in m.imports:
            if module_key(src) not in listed:
                shared.setdefault(loc, src)
    defined = {}
    for m in mods.values():
        if m.name == "main.js":
            continue  # main.js cannot be imported; its names reach modules by hand-over
        for nm in m.top:
            defined.setdefault(nm, set()).add(m.name)
    for m in mods.values():
        # (c)
        for loc, src, _d in m.imports:
            if module_key(src) == "main.js":
                findings.append(f"module_guard: {m.name}: (c) imports ./main.js ({loc})")
        # (a)
        for nm in sorted(m.imported & m.top_lets):
            findings.append(f"module_guard: {m.name}: (a) top-level let/var with the name of an import: {nm}")
        assign_re = re.compile(r"(?<![\w$.])([A-Za-z_$][\w$]*)\s*(?:(?:\*\*|<<|>>>|>>|&&|\|\||\?\?|[-+*/%&|^])?=(?![=>])|\+\+|--)")
        pre_re = re.compile(r"(?:\+\+|--)\s*([A-Za-z_$][\w$]*)")
        hits = set()
        for am in assign_re.finditer(m.body):
            nm = am.group(1)
            if re.search(r"\b(?:const|let|var|function|class)\s*$", m.body[max(0, am.start() - 16):am.start()]):
                continue
            hits.add(nm)
        for pm in pre_re.finditer(m.body):
            hits.add(pm.group(1))
        for nm in sorted(hits):
            if nm in m.imported and nm not in m.any_let:
                findings.append(f"module_guard: {m.name}: (a) assignment to the imported name {nm}")
        # (g)
        used = {}
        for nm, _i in tokens(m):
            used[nm] = True
        for loc, src, _d in m.imports:
            if loc not in used:
                findings.append(f"module_guard: {m.name}: (g) unused import {loc}")
        # (b)
        seen = set()
        for nm, _i in tokens(m):
            if nm in seen:
                continue
            seen.add(nm)
            if nm in m.top or nm in m.imported:
                continue
            if nm in defined and defined[nm] - {m.name}:
                findings.append(f"module_guard: {m.name}: (b) free name {nm} (defined in {sorted(defined[nm] - {m.name})[0]})")
            elif nm in shared:
                findings.append(f"module_guard: {m.name}: (b) free name {nm} (import it from {shared[nm]})")
    # (d)
    if "core.js" in mods:
        graph = {n: {module_key(s) for _l, s, _d in m.imports if module_key(s) in listed} for n, m in mods.items()}

        def reaches_core(start, seen):
            for nxt in graph.get(start, ()):
                if nxt == "core.js":
                    return True
                if nxt not in seen:
                    seen.add(nxt)
                    if reaches_core(nxt, seen):
                        return True
            return False

        for nxt in sorted(graph["core.js"]):
            if reaches_core(nxt, {nxt}):
                findings.append(f"module_guard: core.js: (d) imports {nxt}, which imports core.js")
    # (e)
    link_let = {}
    link_fn = {}
    for m in mods.values():
        for lm in re.finditer(r"^let ([^;]*); // linked from main\.js", m.raw, re.M):
            link_let[m.name] = {x.strip() for x in lm.group(1).split(",")}
        for fm in re.finditer(r"^export function link([A-Z]\w*)\(fns\) \{ \(\{ ([^}]*) \} = fns\); \}", m.raw, re.M):
            link_fn[m.name] = {x.strip() for x in fm.group(2).split(",")}
            if m.name[:-3][:1].upper() + m.name[:-3][1:] != fm.group(1):
                findings.append(f"module_guard: {m.name}: (e) link function name link{fm.group(1)} does not match the file")
    calls = {}
    if "main.js" in mods:
        for cm in re.finditer(r"^link([A-Z]\w*)\(\{ (.*) \}\);", mods["main.js"].raw, re.M):
            keys = set()
            for part in re.split(r",\s*(?=[A-Za-z_$][\w$]*\s*(?::|,|$))", cm.group(2)):
                keys.add(part.split(":")[0].strip())
            calls[cm.group(1)] = keys
    for name in sorted(set(link_let) | set(link_fn)):
        base = name[:-3][:1].upper() + name[:-3][1:]
        a, b, c = link_let.get(name), link_fn.get(name), calls.get(base)
        if not (a == b == c):
            findings.append(f"module_guard: {name}: (e) hand-over names differ: let={sorted(a or [])} fn={sorted(b or [])} main={sorted(c or [])}")
    for base in sorted(calls):
        fname = base[:1].lower() + base[1:] + ".js"
        if fname not in link_let:
            findings.append(f"module_guard: main.js: (e) link{base}() has no matching module scaffold")
    return findings


def self_test():
    cases = [
        ("assign to import",
         {"core.js": "export let settings = {};\nexport function assignSettings(v) { settings = v; }\n",
          "m.js": 'import { settings } from "./core.js";\nexport function f() { settings = {}; }\n'}, "(a)"),
        ("free kernel name",
         {"core.js": "export const t = (k) => k;\n",
          "m.js": "export function f() { return t(\"x\"); }\n"}, "(b)"),
        ("clean pair",
         {"core.js": "export let settings = {};\nexport function assignSettings(v) { settings = v; }\nexport const t = (k) => k;\n",
          "m.js": 'import { assignSettings, t } from "./core.js";\nexport function f() { assignSettings({ a: t("k") }); }\n'}, None),
    ]
    for label, files, expect in cases:
        found = check(files)
        if expect is None:
            ok = not found
        else:
            ok = any(expect in x for x in found)
        if not ok:
            print(f"module_guard: self-test failed ({label}): expected {expect}, got {found}")
            return False
    return True


def main():
    if not self_test():
        return 1
    cfg = json.loads((Path(__file__).resolve().parent / "ui_sources.json").read_text(encoding="utf-8"))
    names = cfg["main_window_modules"]
    files = {n: (UI / n).read_text(encoding="utf-8") for n in names}
    findings = check(files)
    # (f) any ui/*.js importing ./core.js must be listed
    for p in sorted(UI.glob("*.js")):
        if p.name in names:
            continue
        if re.search(r'^import\s[^;]*from\s+"\./core\.js"', p.read_text(encoding="utf-8"), re.M):
            findings.append(f"module_guard: {p.name}: (f) imports ./core.js but is not in tests/ui_sources.json")
    if findings:
        print("\n".join(findings))
        return 1
    print("module_guard: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
