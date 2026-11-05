#!/usr/bin/env python3
"""StateSolver study runner: one LLM debugging agent run on one recorded crash.

    python3 study/runner.py --trace TRACE_DIR --src SRC_DIR --arm A4 \
        --model nova-fast --seed 1 --out results/runs/001/A4/nova-fast/1.json

Arms (docs/DIRECTION.md §2), identical prompt, budget, and common tools:

  A1  raw replay debugger: one `gdb` tool on a gdb session attached to
      `rr replay` (reverse execution and watchpoints included)
  A4  StateSolver causal queries: tools from `statesolver mcp TRACE`
  A5  StateSolver address-only: `statesolver mcp TRACE --address-only`

Common tools in every arm: read_file, list_dir, addr2line.

Every tool result is stamped [T<n>]; the final answer must cite these ids.
Standard library only. The API key is read from POLLINATIONS_KEY (or
.env.local) and never printed or stored in outputs.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import select
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

API = "https://gen.pollinations.ai/v1/chat/completions"
REPO = Path(__file__).resolve().parent.parent
MAX_TOOL_OUTPUT = 6000

SYSTEM_PROMPT = """You are a debugging agent investigating a crash in a native C/C++ program.
The crashing execution was recorded; you can query it with the tools provided.

Your goal: find the ROOT CAUSE, the place in the source code where the program
first did the wrong thing, which may be far from where it crashed.

Rules:
- Every factual claim must cite the id of the tool result that supports it,
  written like [T3]. Do not state facts you did not observe in a tool result.
- Be efficient: you have a limited number of tool calls.
- When done, reply with ONLY a JSON object (no prose, no code fence):
{
  "bug_class": "use-after-free | heap-buffer-overflow | double-free | null-dereference | other",
  "root_cause": {"file": "path relative to the source root", "line": 123, "function": "name"},
  "crash_site": {"file": "...", "line": 0, "function": "..."},
  "explanation": "short causal story from root cause to crash",
  "claims": [{"claim": "...", "cites": ["T3", "T5"]}]
}"""

TASK_PROMPT = """The program `{program}` crashed: {outcome}.
Its source code is available under the source root (use list_dir/read_file with
paths relative to it). Find the root cause."""


# ----------------------------------------------------------------------------
# Model client
# ----------------------------------------------------------------------------

def api_key() -> str:
    key = os.environ.get("POLLINATIONS_KEY")
    if not key:
        env = REPO / ".env.local"
        if env.exists():
            for line in env.read_text().splitlines():
                if line.startswith("POLLINATIONS_KEY="):
                    key = line.split("=", 1)[1].strip().strip("'\"")
    if not key:
        sys.exit("POLLINATIONS_KEY is not set (environment or .env.local)")
    return key


def chat(key: str, body: dict, retries: int = 6) -> dict:
    data = json.dumps(body).encode()
    for attempt in range(retries):
        req = urllib.request.Request(API, data=data, headers={
            "Authorization": f"Bearer {key}", "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=180) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            msg = e.read().decode(errors="replace")[:300]
            if e.code in (429, 500, 502, 503, 504) and attempt + 1 < retries:
                time.sleep(float(e.headers.get("Retry-After") or 2 ** attempt))
                continue
            raise RuntimeError(f"HTTP {e.code}: {msg}") from None
        except (urllib.error.URLError, TimeoutError):
            if attempt + 1 < retries:
                time.sleep(2 ** attempt)
                continue
            raise
    raise RuntimeError("unreachable")


# ----------------------------------------------------------------------------
# Tool backends
# ----------------------------------------------------------------------------

def fn(name: str, desc: str, props: dict, required: list[str]) -> dict:
    return {"type": "function", "function": {"name": name, "description": desc,
            "parameters": {"type": "object", "properties": props, "required": required}}}


class Common:
    """Source and symbol tools shared by every arm."""

    def __init__(self, src: Path, exe: Path, load_base: int = 0):
        self.src = src.resolve()
        self.exe = exe
        self.load_base = load_base

    def tools(self) -> list[dict]:
        return [
            fn("list_dir", "List a directory under the source root.", {"path": {"type": "string"}}, []),
            fn("read_file", "Read lines of a source file (path relative to the source root).",
               {"path": {"type": "string"}, "start": {"type": "integer"}, "end": {"type": "integer"}}, ["path"]),
            fn("addr2line", "Map a code address (hex) in the program to function and source line.",
               {"addr": {"type": "string"}}, ["addr"]),
        ]

    def _safe(self, rel: str) -> Path:
        p = (self.src / rel.lstrip("/")).resolve()
        if p != self.src and self.src not in p.parents:
            raise ValueError("path outside the source root")
        return p

    def call(self, name: str, a: dict) -> str | None:
        if name == "list_dir":
            p = self._safe(a.get("path", "."))
            return "\n".join(sorted(x.name + ("/" if x.is_dir() else "") for x in p.iterdir()))
        if name == "read_file":
            p = self._safe(a["path"])
            lines = p.read_text(errors="replace").splitlines()
            start = max(1, int(a.get("start") or 1))
            end = min(len(lines), int(a.get("end") or start + 199))
            return "\n".join(f"{i:5}  {lines[i - 1]}" for i in range(start, end + 1))
        if name == "addr2line":
            addr = int(str(a["addr"]), 0)
            # Position-independent executables: runtime address minus load base.
            if self.load_base and addr >= self.load_base:
                addr -= self.load_base
            sym = REPO / "study/data/llvm-symbolizer"
            if sym.exists():  # handles clang's DWARF 5, unlike binutils addr2line
                cmd = [str(sym), "--inlines", "--demangle", f"--obj={self.exe}", hex(addr)]
            else:
                cmd = ["addr2line", "-f", "-C", "-i", "-e", str(self.exe), hex(addr)]
            out = subprocess.run(cmd, capture_output=True, text=True, timeout=20).stdout.strip()
            out = re.sub(r":(\d+):\d+$", r":\1", out, flags=re.M)
            # Report paths relative to the source root, whatever the build path was.
            out = re.sub(r"(?m)^/\S*?/src/", "", out.replace(str(self.src) + "/", ""))
            if not out or out.startswith("??") or out.endswith(":0"):
                return f"{a['addr']}: not in the program's own code (likely a shared library such as libc)"
            return out
        return None


class Mcp:
    """StateSolver tools via `statesolver mcp` (arms A4 and A5)."""

    def __init__(self, trace: Path, address_only: bool):
        cmd = [str(REPO / "target/release/statesolver"), "mcp", str(trace)]
        if address_only:
            cmd.append("--address-only")
        self.p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, text=True, bufsize=1)
        self.n = 0
        self._rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                 "clientInfo": {"name": "statesolver-study", "version": "0"}})
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        self._tools = self._rpc("tools/list", {})["tools"]

    def _rpc(self, method: str, params: dict) -> dict:
        self.n += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        line = self.p.stdout.readline()
        if not line:
            raise RuntimeError("statesolver mcp exited")
        msg = json.loads(line)
        if "error" in msg:
            raise RuntimeError(msg["error"])
        return msg["result"]

    def tools(self) -> list[dict]:
        return [fn(t["name"], t["description"], t["inputSchema"].get("properties", {}),
                   t["inputSchema"].get("required", [])) for t in self._tools]

    def call(self, name: str, a: dict) -> str | None:
        if name not in {t["name"] for t in self._tools}:
            return None
        r = self._rpc("tools/call", {"name": name, "arguments": a})
        return "".join(c.get("text", "") for c in r.get("content", []))

    def close(self) -> None:
        self.p.stdin.close()
        self.p.wait(timeout=30)


class RawGdb:
    """Raw replay debugger (arm A1): gdb attached to `rr replay`."""

    PROMPT = "<<<gdb>>>"

    def __init__(self, trace: Path, exe: Path):
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
        s.close()
        self.rr = subprocess.Popen(["rr", "replay", "-s", str(port), str(trace)],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.5)
        self.g = subprocess.Popen(
            ["gdb", "-nx", "-q", "-ex", f"set prompt {self.PROMPT}\\n", "-ex", "set pagination off",
             "-ex", "set confirm off", "-ex", "set width 0", "-ex", f"file {exe}",
             "-ex", f"target extended-remote 127.0.0.1:{port}"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
        self._read()

    def _read(self, timeout: float = 120) -> str:
        out, deadline = "", time.time() + timeout
        fd = self.g.stdout
        while time.time() < deadline:
            r, _, _ = select.select([fd], [], [], 1)
            if not r:
                continue
            line = fd.readline()
            if not line:
                break
            if line.strip() == self.PROMPT:
                return out
            out += line
        return out + "\n[gdb command timed out]"

    def tools(self) -> list[dict]:
        return [fn("gdb", "Run one gdb command in a gdb session attached to an rr replay of the crash "
                   "(supports continue, reverse-continue, reverse-step, watch, break, bt, info registers, x, print, frame, ...).",
                   {"command": {"type": "string"}}, ["command"])]

    def call(self, name: str, a: dict) -> str | None:
        if name != "gdb":
            return None
        cmd = a.get("command", "").strip()
        if re.match(r"^(q|quit|kill|shell|!|python|pipe|source|run|start)\b", cmd):
            return "command not allowed in this study"
        self.g.stdin.write(cmd + "\n")
        self.g.stdin.flush()
        return self._read()

    def close(self) -> None:
        try:
            self.g.stdin.write("kill\nquit\n")
            self.g.stdin.flush()
            self.g.wait(timeout=10)
        except Exception:
            self.g.kill()
        self.rr.kill()
        self.rr.wait()


# ----------------------------------------------------------------------------
# The agent loop
# ----------------------------------------------------------------------------

def trace_facts(trace: Path) -> tuple[str, Path, str, int]:
    """Program path and outcome from the trace's StateSolver index."""
    binary = REPO / "target/release/statesolver"
    subprocess.run([str(binary), "index", str(trace)], capture_output=True, check=True)
    import gzip  # noqa: F401  (zstd is not in the stdlib; ask the CLI instead)
    out = subprocess.run([str(binary), "trace", "stats", str(trace)], capture_output=True, text=True).stdout
    program = re.search(r"^program\s+(\S+)", out, re.M).group(1)
    term = re.search(r"^terminated\s+(.*)$", out, re.M).group(1)
    base = re.search(r"^load base\s+(0x[0-9a-f]+)", out, re.M)
    return program, Path(program), term, int(base.group(1), 16) if base else 0


def parse_final(text: str) -> dict | None:
    m = re.search(r"\{.*\}", text, re.S)
    if not m:
        return None
    try:
        return json.loads(m.group(0))
    except json.JSONDecodeError:
        return None


def run(args: argparse.Namespace) -> dict:
    trace = Path(args.trace).resolve()
    program, exe, outcome, load_base = trace_facts(trace)
    common = Common(Path(args.src), exe, load_base)
    if args.arm == "A1":
        backend = RawGdb(trace, exe)
    elif args.arm in ("A4", "A5"):
        backend = Mcp(trace, address_only=args.arm == "A5")
    else:
        sys.exit(f"arm {args.arm} not implemented yet")
    tools = common.tools() + backend.tools()
    key = api_key()
    messages = [{"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": TASK_PROMPT.format(program=Path(program).name, outcome=outcome)}]
    log, usage, t_id, final = [], {"prompt_tokens": 0, "completion_tokens": 0}, 0, None
    started = time.time()
    try:
        step = 0
        while True:
            force_answer = t_id >= args.max_calls
            # Tools stay defined on every request (some providers require it
            # once tool messages exist); the final turn only disables them.
            body = {"model": args.model, "messages": messages, "temperature": args.temperature,
                    "seed": args.seed, "max_tokens": 2000, "tools": tools}
            if force_answer:
                body["tool_choice"] = "none"
                messages.append({"role": "user", "content": "Tool budget exhausted. Give your final JSON answer now."})
            step += 1
            r = chat(key, body)
            for k in usage:
                usage[k] += (r.get("usage") or {}).get(k, 0) or 0
            msg = r["choices"][0]["message"]
            calls = msg.get("tool_calls") or []
            messages.append({k: v for k, v in msg.items() if k in ("role", "content", "tool_calls")})
            log.append({"step": step, "assistant": msg.get("content"), "tool_calls": calls})
            if not calls or force_answer:
                final = parse_final(msg.get("content") or "")
                break
            # Calls beyond the budget are not executed.
            calls = calls[: max(0, args.max_calls - t_id)]
            for c in calls:
                t_id += 1
                name = c["function"]["name"]
                try:
                    a = json.loads(c["function"].get("arguments") or "{}")
                    out = common.call(name, a)
                    if out is None:
                        out = backend.call(name, a)
                    if out is None:
                        out = f"unknown tool {name}"
                except Exception as e:  # tool errors are shown to the agent
                    out = f"error: {e}"
                out = f"[T{t_id}] " + (out if len(out) <= MAX_TOOL_OUTPUT else out[:MAX_TOOL_OUTPUT] + "\n[truncated]")
                messages.append({"role": "tool", "tool_call_id": c["id"], "content": out})
                log.append({"tool": name, "args": a if isinstance(a, dict) else {}, "id": f"T{t_id}", "result": out})
    finally:
        backend.close()
    return {
        "trace": str(trace), "src": str(Path(args.src).resolve()), "arm": args.arm, "model": args.model, "seed": args.seed,
        "max_calls": args.max_calls, "tool_calls": t_id, "usage": usage,
        "seconds": round(time.time() - started, 1), "final": final, "log": log,
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--trace", required=True)
    p.add_argument("--src", required=True, help="source root shown to the agent")
    p.add_argument("--arm", required=True, choices=["A1", "A4", "A5"])
    p.add_argument("--model", default="nova-fast")
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--temperature", type=float, default=0.2)
    p.add_argument("--max-calls", type=int, default=30, help="total tool-call budget (same in every arm)")
    p.add_argument("--out", required=True)
    args = p.parse_args()
    random.seed(args.seed)
    result = run(args)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2))
    f = result["final"] or {}
    print(f"{args.arm} {args.model} seed {args.seed}: {result['tool_calls']} tool calls, "
          f"{result['usage']['prompt_tokens'] + result['usage']['completion_tokens']} tokens, {result['seconds']}s -> "
          f"class={f.get('bug_class')} root_cause={f.get('root_cause')}")


if __name__ == "__main__":
    main()
