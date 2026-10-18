"""Drive the app's WebView2 windows over the DevTools protocol (VM runs).

Needs an SSH tunnel to the guest's port 9222. Usage:
  python tests/vm/cdp.py list
  python tests/vm/cdp.py eval <url-substring> "<js expression>"   (awaits promises, prints JSON)
  python tests/vm/cdp.py errors <url-substring> <seconds>          (collects uncaught exceptions)
  python tests/vm/cdp.py reload <url-substring> <seconds>          (reloads, prints script errors and failed IPC calls)
  python tests/vm/cdp.py watch <url-substring> <seconds>           (same, without the reload)
  python tests/vm/cdp.py act <url-substring> <seconds> <js>...     (runs each step 1.5 s apart, same report)
"""
import asyncio
import json
import sys
import urllib.request

import websockets

PORT = 9222


def targets():
    with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json") as r:
        return [t for t in json.load(r) if t.get("type") == "page"]


def pick(sub):
    # "<sub>@<n>" picks the n-th match (several windows share one page, like the pills).
    idx = 0
    if "@" in sub and sub.rsplit("@", 1)[1].isdigit():
        sub, n = sub.rsplit("@", 1)
        idx = int(n)
    # A trailing "$" anchors the match at the end of the URL ("localhost/$" = the main window).
    if sub.endswith("$"):
        hits = [t for t in targets() if t.get("url", "").endswith(sub[:-1])]
    else:
        hits = [t for t in targets() if sub in t.get("url", "")]
    if not hits:
        sys.exit(f"no target matches {sub!r}")
    if idx >= len(hits):
        sys.exit(f"only {len(hits)} targets match {sub!r}")
    return hits[idx]


async def call(ws, mid, method, params=None):
    await ws.send(json.dumps({"id": mid, "method": method, "params": params or {}}))
    while True:
        msg = json.loads(await ws.recv())
        if msg.get("id") == mid:
            return msg


async def evaluate(sub, expr):
    t = pick(sub)
    async with websockets.connect(t["webSocketDebuggerUrl"], max_size=None) as ws:
        res = await call(ws, 1, "Runtime.evaluate", {
            "expression": f"(async () => {{ return await ({expr}); }})()",
            "awaitPromise": True,
            "returnByValue": True,
        })
    r = res.get("result", {})
    if "exceptionDetails" in r:
        d = r["exceptionDetails"]
        return {"error": d.get("exception", {}).get("description") or d.get("text")}
    return r.get("result", {}).get("value")


async def errors(sub, secs):
    t = pick(sub)
    out = []
    async with websockets.connect(t["webSocketDebuggerUrl"], max_size=None) as ws:
        await call(ws, 1, "Runtime.enable")
        try:
            async with asyncio.timeout(secs):
                while True:
                    msg = json.loads(await ws.recv())
                    if msg.get("method") == "Runtime.exceptionThrown":
                        d = msg["params"]["exceptionDetails"]
                        out.append(d.get("exception", {}).get("description") or d.get("text"))
                    elif msg.get("method") == "Runtime.consoleAPICalled" and msg["params"]["type"] == "error":
                        out.append(" ".join(str(a.get("value", a.get("description", ""))) for a in msg["params"]["args"]))
        except TimeoutError:
            pass
    return out


async def reload(sub, secs, do_reload=True, steps=()):
    """Reloads a window and reports script errors plus every IPC call that failed.

    Tauri answers IPC over http://ipc.localhost/<command>; a refused or failed call comes
    back with a Tauri-Response: error header, so a command missing from the window's capability shows up
    here even when the page catches the rejection and hides it.
    """
    t = pick(sub)
    out = []
    failed = {}
    async with websockets.connect(t["webSocketDebuggerUrl"], max_size=None) as ws:
        for i, m in enumerate(["Runtime.enable", "Network.enable", "Page.enable"], 1):
            await call(ws, i, m)
        if do_reload:
            await ws.send(json.dumps({"id": 10, "method": "Page.reload", "params": {"ignoreCache": True}}))

        async def run_steps():
            for n, js in enumerate(steps, 20):
                await asyncio.sleep(1.5)
                await ws.send(json.dumps({"id": n, "method": "Runtime.evaluate", "params": {
                    "expression": f"(async () => {{ return await ({js}); }})()",
                    "awaitPromise": True, "returnByValue": True}}))
        stepper = asyncio.ensure_future(run_steps())
        try:
            async with asyncio.timeout(max(secs, 1.5 * len(steps) + 3)):
                while True:
                    msg = json.loads(await ws.recv())
                    m = msg.get("method")
                    if m == "Runtime.exceptionThrown":
                        d = msg["params"]["exceptionDetails"]
                        out.append(d.get("exception", {}).get("description") or d.get("text"))
                    elif m == "Runtime.consoleAPICalled" and msg["params"]["type"] == "error":
                        out.append(" ".join(str(a.get("value", a.get("description", ""))) for a in msg["params"]["args"]))
                    elif msg.get("id", 0) >= 20 and msg.get("id", 0) < 100:
                        r = msg.get("result", {})
                        val = r.get("exceptionDetails", {}).get("exception", {}).get("description") if "exceptionDetails" in r else r.get("result", {}).get("value")
                        out.append(f"step {msg['id'] - 19}: {val}")
                    elif m == "Network.responseReceived":
                        r = msg["params"]["response"]
                        hdr = {k.lower(): v for k, v in r.get("headers", {}).items()}
                        if "ipc.localhost" in r["url"] and hdr.get("tauri-response") == "error":
                            failed[msg["params"]["requestId"]] = r["url"].rsplit("/", 1)[-1]
        except TimeoutError:
            pass
        except websockets.ConnectionClosed:
            stepper.cancel()
            # The window closed itself (a dialog that ends on submit): bodies are gone.
            out.extend(f"IPC {cmd}: (failed, window closed)" for cmd in failed.values())
            return out + ["(window closed)"]
        for n, (rid, cmd) in enumerate(failed.items(), 100):
            res = await call(ws, n, "Network.getResponseBody", {"requestId": rid})
            out.append(f"IPC {cmd}: {res.get('result', {}).get('body', '')[:200]}")
    return out


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "list"
    if cmd == "list":
        for t in targets():
            print(t.get("url"), "|", t.get("title"))
    elif cmd == "eval":
        print(json.dumps(asyncio.run(evaluate(sys.argv[2], sys.argv[3])), ensure_ascii=False))
    elif cmd == "errors":
        print(json.dumps(asyncio.run(errors(sys.argv[2], float(sys.argv[3]))), ensure_ascii=False))
    elif cmd == "watch":
        print(json.dumps(asyncio.run(reload(sys.argv[2], float(sys.argv[3]), False)), ensure_ascii=False))
    elif cmd == "act":
        print(json.dumps(asyncio.run(reload(sys.argv[2], float(sys.argv[3]), False, sys.argv[4:])), ensure_ascii=False, indent=0))
    elif cmd == "reload":
        print(json.dumps(asyncio.run(reload(sys.argv[2], float(sys.argv[3]))), ensure_ascii=False))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
