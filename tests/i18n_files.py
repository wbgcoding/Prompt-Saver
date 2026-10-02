"""Reads the UI dictionaries for the Python tests.

The texts live in one file per language (ui/i18n/<code>.js, plus float.js for the
compact strings). combined_source() rebuilds the old single-file text so the
regex-based checks keep working unchanged. Tests read the dictionaries through here,
never by opening a language file directly.
"""
from pathlib import Path

DIR = Path(__file__).resolve().parents[1] / "ui" / "i18n"
LANGS = ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"]


def _body(name):
    text = (DIR / f"{name}.js").read_text(encoding="utf-8").replace("\r\n", "\n")
    lines = text.split("\n")
    end = len(lines) - 1 - lines[::-1].index("};")
    return "\n".join(lines[2:end])


def combined_source():
    out = ("// Shared translations: main window (I18N), floating pills (FLOAT_TXT).\n"
           "// 11 languages; EN is the fallback for missing keys.\n")
    out += "export const LANGS = [\n  " + ", ".join(f'"{c}"' for c in LANGS) + ",\n];\n\nexport const I18N = {\n"
    for c in LANGS:
        out += f"  {c}: {{\n{_body(c)}\n  }},\n"
    out += "};\n\n// Compact strings for the floating pill windows.\nexport const FLOAT_TXT = {\n" + _body("float") + "\n};\n"
    return out
