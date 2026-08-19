"""Build a large, realistic data.db so the render path can be measured, not guessed.

500 prompts, 60 of them carrying a stored PNG image of the size a pasted
screenshot really has (~400 KB of base64), written to test-data/data.db in the
app's own schema. test-data/ is gitignored: keep this generator, never the corpus.

Run:  python tests/gen_perf_corpus.py [--out DIR] [--prompts N] [--images N]

The rows are plain JSON, the same shape the app writes when DPAPI sealing is
unavailable, so a build reads the file back without any further setup: point the
app's data folder at test-data/ (or copy the file next to the portable exe).
"""
import argparse
import json
import random
import struct
import sqlite3
import sys
import zlib
from base64 import b64encode
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# 320x320 of noise deflates to roughly 300 KB, which is ~400 KB once base64'd —
# the size range a real pasted screenshot lands in. A solid colour would compress
# to a few KB and measure nothing.
IMG_SIDE = 320


def make_png(seed: int, side: int = IMG_SIDE) -> bytes:
    """A valid RGB PNG of incompressible noise, built with the standard library."""
    rnd = random.Random(seed)
    raw = bytearray()
    for _ in range(side):
        raw.append(0)  # per-scanline filter: none
        raw.extend(rnd.randbytes(side * 3))

    def chunk(tag: bytes, body: bytes) -> bytes:
        return (struct.pack(">I", len(body)) + tag + body
                + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF))

    header = struct.pack(">IIBBBBB", side, side, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
            + chunk(b"IEND", b""))


def data_url(seed: int) -> str:
    return "data:image/png;base64," + b64encode(make_png(seed)).decode("ascii")


def build_prompts(count: int, with_images: int) -> list:
    body = ("Rewrite the passage below so it reads clearly and keeps every fact "
            "intact. Prefer short sentences.\n\n")
    prompts = []
    for i in range(count):
        image = data_url(i) if i < with_images else ""
        prompts.append({
            "id": f"perf{i:04d}",
            "name": f"Prompt {i + 1}",
            "text": body + f"Sample body number {i + 1}. " * 6,
            "color": ["", "#3b82f6", "#ef4444", "#10b981", "#f59e0b"][i % 5],
            "image": image,
            "show_image": bool(image),
            "copy_image": bool(image),
            "file_path": "",
            "pdf_page": 0,
            "icon_path": "",
            "caption": "",
            "caption_size": 0,
            "font": "",
            "font_size": 0,
            "favorite": i % 25 == 0,
            "tags": ["Work"] if i % 3 == 0 else [],
            "notes": "",
            "hotkey": "",
            "hotkey_paste": "",
            "trashed_at": 0,
            "variants": [],
            "rotate_mode": "",
            "rotate_auto": False,
        })
    return prompts


def build_settings(prompts: list, cols: int = 10, rows: int = 8) -> dict:
    """One view large enough that the image prompts really land on the grid."""
    layout = {}
    for i, p in enumerate(prompts[:cols * rows]):
        layout[p["id"]] = [i % cols, i // cols]
    return {
        "theme": "dark",
        "language": "en",
        "views": [{"id": "home", "name": "Home", "cols": cols, "rows": rows,
                   "layouts": {f"{cols}x{rows}": layout}, "color": ""}],
        "active_view": "home",
    }


def write_db(path: Path, prompts: list, settings: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    for stale in (path, path.with_suffix(".db-wal"), path.with_suffix(".db-shm")):
        stale.unlink(missing_ok=True)
    conn = sqlite3.connect(path)
    try:
        conn.executescript("""
            CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS idx_copy_log_ts ON copy_log(ts);
            CREATE TABLE IF NOT EXISTS prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
            CREATE INDEX IF NOT EXISTS idx_pv_prompt ON prompt_versions(prompt_id);
            CREATE TABLE IF NOT EXISTS clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
        """)
        conn.executemany(
            "INSERT INTO prompts(id, ord, data) VALUES(?, ?, ?)",
            [(p["id"], i, json.dumps(p, separators=(",", ":"))) for i, p in enumerate(prompts)])
        conn.execute("INSERT OR REPLACE INTO meta(key, value) VALUES('settings', ?)",
                     (json.dumps(settings, separators=(",", ":")),))
        conn.commit()
    finally:
        conn.close()


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default=str(REPO / "test-data"))
    ap.add_argument("--prompts", type=int, default=500)
    ap.add_argument("--images", type=int, default=60)
    args = ap.parse_args()

    prompts = build_prompts(args.prompts, args.images)
    settings = build_settings(prompts)
    db = Path(args.out) / "data.db"
    write_db(db, prompts, settings)

    image_bytes = sum(len(p["image"]) for p in prompts)
    print(f"{db}: {len(prompts)} prompts, {args.images} with an image")
    print(f"  stored image payload: {image_bytes / 1024 / 1024:.2f} MB of base64")
    print(f"  file on disk:         {db.stat().st_size / 1024 / 1024:.2f} MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
