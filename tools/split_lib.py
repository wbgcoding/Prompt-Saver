#!/usr/bin/env python3
"""Mechanical helpers for splitting src-tauri/src/lib.rs into flat modules.

Every subcommand edits text only, keeps the file's line terminator (CRLF in the
working copy), checks its own result by rebuilding the old text from the new one
and refuses to write anything when that check fails.

  scan [FILE]                 list the top-level items (start, header, end, kind, name)
  extract-mod NAME            move the inline `mod NAME { ... }` of lib.rs into NAME.rs
  hoist-use NEEDLE            move the one top-level `use` line containing NEEDLE up
                              into the header use block (after the last header `use`)
  vis                         add pub(crate) to every lib.rs item, field and inherent
                              method that has no visibility yet; #[tauri::command] fns
                              are skipped (a pub command at the crate root is E0255)
  carve MOD FROM..LAST [...]  move whole top-level items FROM through LAST (item names,
                              several ranges allowed) into MOD.rs; the moved commands get
                              pub(crate) and their generate_handler! entries become MOD::name
  blame-check FILE FN BASE    git blame (histogram, -w -C -C) on the five lines after the
                              header of `fn FN` in FILE; fails when one of them is blamed
                              on a commit after BASE, i.e. the move lost the line history
"""
import os
import re
import subprocess
import sys

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "src")
LIB = os.path.join(SRC, "lib.rs")
VIS = "pub(crate) "


def die(msg):
    sys.stderr.write("split_lib: " + msg + "\n")
    sys.exit(1)


def read(path):
    with open(path, "r", encoding="utf-8", newline="") as f:
        text = f.read()
    eol = "\r\n" if "\r\n" in text else "\n"
    lines = text.split(eol)
    if lines and lines[-1] == "":
        lines.pop()
    else:
        die(path + " does not end with a line terminator")
    if any(("\r" in x) or ("\n" in x) for x in lines):
        die(path + " has mixed line endings")
    return lines, eol


def write(path, lines, eol):
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(eol.join(lines) + eol)


# ---------- lexer ----------

CHAR_LIT = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'")
RAW_START = re.compile(r'b?r(#*)"')


class Lex:
    """Character-level Rust lexer, good enough for item structure.

    code: list of (line, col, ch, brace, paren, bracket) for every code character
    (outside comments, strings and char literals), depths taken BEFORE the char.
    For a string or char literal one placeholder char '"' is recorded at its start.
    line_state[i]: True when line i (0-based) starts inside a string literal.
    """

    def __init__(self, lines):
        self.lines = lines
        src = "\n".join(lines) + "\n"
        n = len(src)
        self.code = []
        self.line_in_str = [False] * (len(lines) + 1)
        self.line_in_comment = [False] * (len(lines) + 1)
        i, line, col = 0, 0, 0
        brace = paren = bracket = 0

        def adv(k):
            nonlocal i, line, col
            for _ in range(k):
                if src[i] == "\n":
                    line += 1
                    col = 0
                else:
                    col += 1
                i += 1

        def ident_before(pos):
            return pos > 0 and (src[pos - 1].isalnum() or src[pos - 1] == "_")

        while i < n:
            c = src[i]
            if c == "\n" or c == " " or c == "\t" or c == "\r":
                adv(1)
                continue
            if src.startswith("//", i):
                j = src.find("\n", i)
                adv(j - i)
                continue
            if src.startswith("/*", i):
                d = 0
                while True:
                    if src.startswith("/*", i):
                        d += 1
                        adv(2)
                    elif src.startswith("*/", i):
                        d -= 1
                        adv(2)
                        if d == 0:
                            break
                    else:
                        if src[i] == "\n":
                            self.line_in_comment[line + 1] = True
                        adv(1)
                continue
            m = RAW_START.match(src, i)
            if m and not ident_before(i):
                self.code.append((line, col, '"', brace, paren, bracket))
                end = '"' + m.group(1)
                j = src.find(end, m.end())
                if j < 0:
                    die("unterminated raw string at line %d" % (line + 1))
                adv(m.end() - i)
                while i < j:
                    if src[i] == "\n":
                        self.line_in_str[line + 1] = True
                    adv(1)
                adv(len(end))
                continue
            if c == '"' or (c == "b" and src.startswith('b"', i) and not ident_before(i)):
                self.code.append((line, col, '"', brace, paren, bracket))
                adv(2 if c == "b" else 1)
                while src[i] != '"':
                    if src[i] == "\\":
                        adv(1)
                    if src[i] == "\n":
                        self.line_in_str[line + 1] = True
                    adv(1)
                adv(1)
                continue
            if c == "'":
                m = CHAR_LIT.match(src, i)
                if m:
                    self.code.append((line, col, '"', brace, paren, bracket))
                    adv(m.end() - i)
                    continue
                self.code.append((line, col, c, brace, paren, bracket))
                adv(1)
                continue
            self.code.append((line, col, c, brace, paren, bracket))
            if c == "{":
                brace += 1
            elif c == "}":
                brace -= 1
            elif c == "(":
                paren += 1
            elif c == ")":
                paren -= 1
            elif c == "[":
                bracket += 1
            elif c == "]":
                bracket -= 1
            adv(1)
        if brace or paren or bracket:
            die("unbalanced delimiters at end of file")


HEAD_RE = re.compile(
    r"(?P<vis>pub(?:\([^)]*\))?\s+)?"
    r"(?P<quals>(?:(?:const|async|unsafe|default)\s+|extern\s+\"[^\"]*\"\s+)*)"
    r"(?P<kind>(?:fn|struct|enum|union|trait|type|const|static|mod|impl|use|extern)\b|thread_local!|macro_rules!)"
    r"(?:\s+mut)?\s*(?P<name>r#[A-Za-z_]\w*|[A-Za-z_]\w*)?"
)


class Item:
    def __init__(self, start, end, hline, hcol, attrs_text):
        self.start, self.end, self.hline, self.hcol = start, end, hline, hcol
        self.attrs = attrs_text
        self.kind = self.name = None
        self.vis = False
        self.command = "tauri::command" in attrs_text
        self.inherent = False

    def __repr__(self):
        return "%d-%d %s %s" % (self.start + 1, self.end + 1, self.kind, self.name)


def skip_attr(code, k):
    """code[k] is '#'; return the index after the attribute's closing ']' or k when it is no attribute."""
    if k + 1 < len(code) and code[k + 1][2] == "[" or (k + 2 < len(code) and code[k + 1][2] == "!" and code[k + 2][2] == "["):
        j = k + 1 if code[k + 1][2] == "[" else k + 2
        depth0 = code[j][5]
        j += 1
        while code[j][2] != "]" or code[j][5] != depth0 + 1:
            j += 1
        return j + 1
    return k


SEMI_ONLY = re.compile(r"(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|static)\b(?!\s+(?:unsafe\s+|async\s+|extern\s+\"[^\"]*\"\s+)*fn\b)|type\b|use\b)")


def items_at(lex, lo, hi, level):
    """Members whose first code char lies in code index range [lo, hi) at brace depth `level`
    (paren and bracket 0). A member ends at ';' or at the '}' that returns to `level`.
    Returns (first_code_index_after_attrs, end_code_index, attrs_text) triples."""
    code = lex.code
    out = []
    k = lo
    while k < hi:
        start_k = k
        attrs = []
        while k < hi and code[k][2] == "#":
            j = skip_attr(code, k)
            if j == k:
                break
            attrs.append("".join(c[2] for c in code[k:j]))
            k = j
        if k >= hi:
            break
        head = k
        ht = lex.lines[code[head][0]][code[head][1]:]
        semi_only = bool(SEMI_ONLY.match(ht))
        while k < hi:
            ch, b, p, br = code[k][2], code[k][3], code[k][4], code[k][5]
            if ch == ";" and b == level and p == 0 and br == 0:
                break
            if ch == "}" and b == level + 1 and p == 0 and br == 0 and not semi_only:
                break
            k += 1
        out.append((start_k, head, k, " ".join(attrs)))
        k += 1
    return out


def scan(lines):
    lex = Lex(lines)
    code = lex.code
    items = []
    prev_end = -1
    for (sk, head, endk, attrs) in items_at(lex, 0, len(code), 0):
        hl, hc = code[head][0], code[head][1]
        el = code[min(endk, len(code) - 1)][0]
        if el <= prev_end:
            die("two items share line %d; split them by hand first" % (el + 1))
        it = Item(prev_end + 1, el, hl, hc, attrs)
        it.code_head, it.code_end = head, endk
        m = HEAD_RE.match(lines[hl], hc)
        if m:
            it.vis = bool(m.group("vis"))
            it.kind = m.group("kind")
            it.name = m.group("name")
            it.name_end = (hl, m.end())
            if it.kind == "impl":
                # inherent impl = no top-level ' for ' before the opening brace
                txt = []
                j = head
                while code[j][2] != "{" or code[j][3] != 0:
                    txt.append(code[j][2])
                    j += 1
                hdr = re.sub(r"for\s*<", "", " ".join(lines[hl:code[j][0] + 1]))
                it.inherent = not re.search(r"\bfor\b", hdr.split("{")[0])
                it.name = None
        else:
            it.kind = "other"
        items.append(it)
        prev_end = el
    if prev_end != len(lines) - 1:
        trailing = [x for x in lines[prev_end + 1:] if x.strip() and not x.strip().startswith("//")]
        if trailing:
            die("text after the last item")
    return lex, items


def cmd_scan(path):
    lines, _ = read(path)
    _, items = scan(lines)
    for it in items:
        flag = " ".join(f for f, on in (("cmd", it.command), ("pub", it.vis), ("inherent", it.inherent)) if on)
        print("%d\t%d\t%d\t%s\t%s\t%s" % (it.start + 1, it.hline + 1, it.end + 1, it.kind, it.name or "-", flag))


# ---------- extract-mod / hoist-use ----------

def deindent(lines, lex, lo, hi):
    out = []
    for i in range(lo, hi):
        x = lines[i]
        if lex.line_in_str[i]:
            out.append(x)
            continue
        k = len(x) - len(x.lstrip(" "))
        out.append(x[min(k, 4):])
    return out


def cmd_extract_mod(name):
    lines, eol = read(LIB)
    lex, items = scan(lines)
    hits = [it for it in items if it.kind == "mod" and it.name == name]
    if len(hits) != 1:
        die("expected one top-level mod %s, found %d" % (name, len(hits)))
    it = hits[0]
    hdr = lines[it.hline]
    if hdr.rstrip() != "mod %s {" % name or lines[it.end] != "}":
        die("mod %s: header must be exactly 'mod %s {' and its last line '}'" % (name, name))
    target = os.path.join(SRC, name + ".rs")
    if os.path.exists(target):
        die(target + " exists already")
    body = deindent(lines, lex, it.hline + 1, it.end)
    kept = sum(1 for i in range(it.hline + 1, it.end) if lex.line_in_str[i])
    new_lib = lines[:it.hline] + ["mod %s;" % name] + lines[it.end + 1:]
    # self-check: re-indenting the file gives back the old body
    for a, b in zip(lines[it.hline + 1:it.end], body):
        if a.lstrip(" ") != b.lstrip(" ") or len(a) - len(b) not in (0, 1, 2, 3, 4):
            die("self-check failed on %r" % a)
    while body and body[-1] == "":
        die("mod %s ends with a blank line; trim it by hand first" % name)
    write(target, body, eol)
    write(LIB, new_lib, eol)
    print("mod %s: %d lines -> %s.rs (%d lines kept unindented inside string literals); lib.rs %d -> %d lines"
          % (name, len(body), name, kept, len(lines), len(new_lib)))


def cmd_hoist_use(needle):
    lines, eol = read(LIB)
    _, items = scan(lines)
    uses = [it for it in items if it.kind == "use"]
    hits = [it for it in uses if needle in lines[it.hline]]
    if len(hits) != 1 or hits[0].hline != hits[0].end:
        die("expected one single-line top-level use containing %r" % needle)
    first_non_use = next(k for k, it in enumerate(items) if it.kind not in ("use", "mod"))
    header_uses = [it for it in items[:first_non_use] if it.kind == "use"]
    if hits[0] in header_uses:
        die("that use is already in the header block")
    at = header_uses[-1].end + 1
    ln = hits[0].hline
    moved = lines[ln]
    new = lines[:ln] + lines[ln + 1:]
    new = new[:at] + [moved] + new[at:]
    write(LIB, new, eol)
    print("hoisted line %d to line %d: %s" % (ln + 1, at + 1, moved.strip()))


# ---------- vis ----------

FN_HEAD = re.compile(r"(?:(?:const|async|unsafe)\s+|extern\s+\"[^\"]*\"\s+)*fn\b")


def vis_points(lines, lex, items):
    """(line, col) positions where pub(crate) is inserted."""
    code = lex.code
    pts = []
    for it in items:
        if it.kind in ("fn", "struct", "enum", "union", "trait", "type", "const", "static"):
            if not it.vis and not it.command:
                pts.append((it.hline, it.hcol))
        if it.kind == "struct":
            pts += field_points(lines, lex, it)
        elif it.kind == "impl" and it.inherent:
            ob = next(j for j in range(it.code_head, it.code_end) if code[j][2] == "{" and code[j][3] == 0)
            for (_, head, _, _) in items_at(lex, ob + 1, it.code_end, 1):
                l, c = code[head][0], code[head][1]
                t = lines[l][c:]
                if t.startswith("pub"):
                    continue
                if FN_HEAD.match(t) or re.match(r"const\s+[A-Z_]", t):
                    pts.append((l, c))
                else:
                    die("unexpected member in inherent impl at line %d: %s" % (l + 1, t[:40]))
        elif it.kind == "thread_local!":
            ob = next(j for j in range(it.code_head, it.code_end) if code[j][2] in "{(" and code[j][3] + code[j][4] == 0)
            if code[ob][2] != "{":
                die("thread_local!(...) form at line %d; only the braced form is handled" % (it.hline + 1))
            for (_, head, _, _) in items_at(lex, ob + 1, it.code_end, 1):
                l, c = code[head][0], code[head][1]
                t = lines[l][c:]
                if t.startswith("static"):
                    pts.append((l, c))
                elif not t.startswith("pub"):
                    die("unexpected thread_local member at line %d" % (l + 1))
        elif it.kind not in ("fn", "enum", "union", "trait", "type", "const", "static", "mod", "use", "impl"):
            die("vis: unhandled item kind %s at line %d" % (it.kind, it.hline + 1))
    return pts


def field_points(lines, lex, it):
    code = lex.code
    j = it.code_head
    while (code[j][0], code[j][1]) < it.name_end:  # past `pub(crate) struct Name`
        j += 1
    while code[j][2] not in "{(;" or code[j][3] + code[j][4] + code[j][5] != 0:
        j += 1
    if code[j][2] == ";":
        return []  # unit struct
    open_ch = code[j][2]
    close_ch = "}" if open_ch == "{" else ")"
    # a '<' before the body (generics) is skipped because we start after the opener
    pts = []
    angle = 0
    expect_field = True
    k = j + 1
    base = (code[j][3], code[j][4], code[j][5])
    while True:
        ch, b, p, br = code[k][2], code[k][3], code[k][4], code[k][5]
        inner = (b, p, br) == (base[0] + (open_ch == "{"), base[1] + (open_ch == "("), base[2])
        if inner and ch == close_ch and angle == 0:
            break
        if inner and angle == 0 and expect_field:
            if ch == "#":
                k = skip_attr(code, k)
                continue
            l, c = code[k][0], code[k][1]
            if not lines[l][c:].startswith("pub"):
                pts.append((l, c))
            expect_field = False
        if ch == "<":
            angle += 1
        elif ch == ">" and code[k - 1][2] not in "-=":
            angle -= 1
        elif ch == "," and inner and angle == 0:
            expect_field = True
        k += 1
    return pts


def apply_points(lines, pts):
    new = list(lines)
    for (l, c) in sorted(pts, reverse=True):
        new[l] = new[l][:c] + VIS + new[l][c:]
    return new


def cmd_vis():
    lines, eol = read(LIB)
    lex, items = scan(lines)
    stop = next((k for k, it in enumerate(items) if it.kind == "mod" and it.name == "tests"), len(items))
    pts = vis_points(lines, lex, items[:stop])
    if len(set(pts)) != len(pts):
        die("vis: duplicate insertion point")
    new = apply_points(lines, pts)
    back = list(new)
    for (l, c) in sorted(pts, reverse=True):
        c += len(VIS) * sum(1 for (l2, c2) in pts if l2 == l and c2 < c)
        if back[l][c:c + len(VIS)] != VIS:
            die("self-check failed at line %d" % (l + 1))
        back[l] = back[l][:c] + back[l][c + len(VIS):]
    if back != lines:
        die("self-check failed")
    write(LIB, new, eol)
    print("vis: %d insertions on %d lines" % (len(pts), len({p[0] for p in pts})))


# ---------- carve ----------

def resolve(items, spec):
    if ".." not in spec:
        die("range must be FROM..LAST")
    a, b = spec.split("..")
    named = [k for k, it in enumerate(items) if it.name == a]
    if not named:
        die("no top-level item named %s" % a)
    ka = named[0]
    kb = next((k for k in range(ka, len(items)) if items[k].name == b), None)
    if kb is None:
        die("no top-level item named %s at or after %s" % (b, a))
    while kb + 1 < len(items) and items[kb + 1].name == b:  # cfg pair
        kb += 1
    return ka, kb


def handler_block(lines):
    s = [i for i, x in enumerate(lines) if "tauri::generate_handler![" in x]
    if len(s) != 1:
        die("expected one generate_handler! list")
    e = next(i for i in range(s[0], len(lines)) if lines[i].strip() == "])")
    return s[0] + 1, e


def cmd_carve(mod, specs):
    lines, eol = read(LIB)
    lex, items = scan(lines)
    target = os.path.join(SRC, mod + ".rs")
    if os.path.exists(target):
        die(target + " exists already")
    if any(it.kind == "mod" and it.name == mod for it in items):
        die("lib.rs already declares mod " + mod)
    ranges = sorted(resolve(items, s) for s in specs)
    for (a1, b1), (a2, b2) in zip(ranges, ranges[1:]):
        if a2 <= b1:
            die("ranges overlap")
    moved_idx = []
    for ka, kb in ranges:
        for k in range(ka, kb + 1):
            it = items[k]
            if it.kind == "mod":
                continue  # module declarations stay in lib.rs
            if it.kind == "use" or (it.kind == "other"):
                die("range holds a %s item at line %d; deal with it first" % (it.kind, it.hline + 1))
            if it.kind in ("fn", "struct", "enum", "union", "trait", "type", "const", "static") and not it.vis and not it.command:
                die("%s %s at line %d has no visibility; run vis first" % (it.kind, it.name, it.hline + 1))
            moved_idx.append(k)
    pts = [(items[k].hline, items[k].hcol) for k in moved_idx if items[k].command and not items[k].vis]
    cmds = sorted({items[k].name for k in moved_idx if items[k].command})
    work = apply_points(lines, pts)
    moved_lines = set()
    for k in moved_idx:
        moved_lines.update(range(items[k].start, items[k].end + 1))
    body = [work[i] for i in sorted(moved_lines)]
    first = min(moved_lines)
    new_lib = []
    for i, x in enumerate(work):
        if i == first:
            new_lib += ["", "mod %s;" % mod, "use %s::*;" % mod]
        if i not in moved_lines:
            new_lib.append(x)
    hs, he = handler_block(new_lib)
    for name in cmds:
        hit = [i for i in range(hs, he) if re.fullmatch(r"\s*%s,?\s*" % re.escape(name), new_lib[i])]
        if len(hit) != 1:
            die("generate_handler! entry for %s: %d hits" % (name, len(hit)))
        i = hit[0]
        new_lib[i] = new_lib[i].replace(name, "%s::%s" % (mod, name), 1)
    # self-check: undo everything and compare
    back = [x for x in new_lib]
    for i in range(hs, he):
        back[i] = back[i].replace("%s::" % mod, "", 1) if back[i].strip().startswith(mod + "::") else back[i]
    at = back.index("mod %s;" % mod) - 1
    back = back[:at] + back[at + 3:]
    rebuilt, bi, mi = [], 0, 0
    order = sorted(moved_lines)
    for i in range(len(lines)):
        if i in moved_lines:
            rebuilt.append(body[mi]); mi += 1
        else:
            rebuilt.append(back[bi]); bi += 1
    for p in pts:
        rebuilt[p[0]] = rebuilt[p[0]][:p[1]] + rebuilt[p[0]][p[1] + len(VIS):]
    if rebuilt != lines:
        die("self-check failed")
    write(target, ["use super::*;"] + body, eol)
    write(LIB, new_lib, eol)
    print("carve %s: %d items, %d lines moved, %d commands; %s.rs %d lines; lib.rs %d -> %d lines (-%d)"
          % (mod, len(moved_idx), len(moved_lines), len(cmds), mod, len(body) + 1, len(lines), len(new_lib),
             len(lines) - len(new_lib)))
    for name in cmds:
        print("  cmd " + name)


def cmd_blame_check(path, fn, base):
    root = os.path.normpath(os.path.join(SRC, "..", ".."))
    rel = os.path.relpath(os.path.abspath(path), root).replace(os.sep, "/")
    lines, _ = read(os.path.join(root, rel))
    hits = [i for i, x in enumerate(lines) if re.search(r"\bfn %s\s*[(<]" % re.escape(fn), x)]
    if len(hits) != 1:
        die("expected one `fn %s` in %s, found %d" % (fn, rel, len(hits)))
    git = lambda *a: subprocess.run(["git"] + list(a), cwd=root, capture_output=True, text=True, check=True).stdout
    newer = set(git("rev-list", "%s..HEAD" % base).split())
    out = git("blame", "--diff-algorithm=histogram", "-w", "-C", "-C", "-l", "-s",
              "-L", "%d,+5" % (hits[0] + 2), "--", rel).splitlines()
    hashes = [o.split()[0].lstrip("^") for o in out]
    if len(hashes) != 5:
        die("blame returned %d lines" % len(hashes))
    lost = [h for h in hashes if h in newer]
    if lost:
        die("fn %s: %d of 5 body lines blamed on this round's commits %s" % (fn, len(lost), sorted({h[:7] for h in lost})))
    print("blame ok: fn %s in %s -> %s" % (fn, rel, " ".join(sorted({h[:7] for h in hashes}))))


def main(argv):
    if not argv:
        print(__doc__)
        return
    c = argv[0]
    if c == "scan":
        cmd_scan(argv[1] if len(argv) > 1 else LIB)
    elif c == "extract-mod" and len(argv) == 2:
        cmd_extract_mod(argv[1])
    elif c == "hoist-use" and len(argv) == 2:
        cmd_hoist_use(argv[1])
    elif c == "vis" and len(argv) == 1:
        cmd_vis()
    elif c == "carve" and len(argv) >= 3:
        cmd_carve(argv[1], argv[2:])
    elif c == "blame-check" and len(argv) == 4:
        cmd_blame_check(argv[1], argv[2], argv[3])
    else:
        die("bad arguments; run without arguments for help")


if __name__ == "__main__":
    main(sys.argv[1:])
