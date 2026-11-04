"""Capture a MorphStack trace (format v0, JSONL) from a running CPython program.

Requires Python 3.12+ (``sys.monitoring``). Usage::

    from py_trace import MorphTracer
    with MorphTracer("out.jsonl", source="aiohttp-server"):
        asyncio.run(main())

Model mapping
-------------
* Context 0 is the main thread's stack, which includes the event loop. Every
  ``asyncio.Task`` is its own context, because a task's coroutine frames
  outlive any one run of the loop.
* A frame belongs to its parent frame's context. The exception is a task's
  root coroutine: when ``Task.__step`` sends into it, it is placed in that
  task's context, matched by ``task.get_coro().cr_frame``.
* When a task's root coroutine yields, the task **suspends**; when it is
  resumed, the task **resumes**. Inner awaiting coroutines stay live on the
  logical stack while suspended.
* Plain generators, including ``Future.__await__``, detach from the stack
  when they yield: a yield is recorded as a return and a resume as a new call.
  This undercounts a suspended await chain's depth by about one frame.
* Frame size is estimated from the code object as
  ``8 × (co_stacksize + co_nlocals + cells + frees) + 80``, roughly the size
  of CPython's ``_PyInterpreterFrame``.

Timing
------
Tracing slows Python down. Timestamps are corrected by subtracting a measured
per-event overhead (scaled by ``overhead_fraction``) times the number of
events so far, then clamped to stay monotonic. Suspension durations (I/O,
sleeps) are mostly real; compute durations are approximate.
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import time

CO_COROUTINE = 0x0080
CO_ITERABLE_COROUTINE = 0x0100
CO_ASYNC_GENERATOR = 0x0200
_COROUTINE_FLAGS = CO_COROUTINE | CO_ITERABLE_COROUTINE | CO_ASYNC_GENERATOR

_THIS_FILE = os.path.abspath(__file__)


def _frame_bytes(code) -> int:
    slots = code.co_stacksize + code.co_nlocals + len(code.co_cellvars) + len(code.co_freevars)
    return 8 * slots + 80


class MorphTracer:
    def __init__(self, path: str, source: str, description: str = "", overhead_fraction: float = 0.8):
        self.path = path
        self.source = source
        self.description = description
        self.overhead_fraction = overhead_fraction
        self.tool = sys.monitoring.PROFILER_ID
        self.out = None
        self.n = 0
        self.t0 = 0
        self.last_ts = 0
        self.overhead_ns = 0.0
        self.next_ctx = 1
        self.next_act = 0
        self.frames: dict[int, tuple[int, int, object]] = {}  # id(frame) -> (ctx, act, frame)
        self.stacks: dict[int, list[int]] = {0: []}  # ctx -> [act]
        self.roots: dict[int, int] = {}  # ctx -> id(root frame)
        self.task_ctx: dict[int, int] = {}  # id(task) -> ctx
        self.tasks: dict[int, object] = {}  # keep tasks alive while tracked, so ids are unique
        self.suspended: set[int] = set()
        self.exited: set[int] = set()
        self.fn_ids: dict[object, int] = {}
        self.functions: list[dict] = []

    # ----- output -----

    def _emit(self, ev: dict) -> None:
        raw = time.perf_counter_ns() - self.t0
        ts = int(raw - self.overhead_fraction * self.overhead_ns * self.n)
        ts = max(ts, self.last_ts)
        self.last_ts = ts
        self.n += 1
        ev["ts"] = ts
        self.events.write(json.dumps(ev, separators=(",", ":")))
        self.events.write("\n")

    def _fn(self, code) -> int:
        fid = self.fn_ids.get(code)
        if fid is None:
            fid = len(self.functions) + 1
            self.fn_ids[code] = fid
            self.functions.append(
                {
                    "id": fid,
                    "name": f"{code.co_qualname} ({os.path.basename(code.co_filename)}:{code.co_firstlineno})",
                    "can_suspend": bool(code.co_flags & _COROUTINE_FLAGS),
                }
            )
        return fid

    # ----- context attribution -----

    def _ctx_for_new_frame(self, frame) -> int:
        parent = frame.f_back
        if parent is not None:
            info = self.frames.get(id(parent))
            if info is not None and info[0] != 0:
                return info[0]
        if frame.f_code.co_flags & _COROUTINE_FLAGS:
            try:
                task = asyncio.current_task()
            except RuntimeError:
                task = None
            if task is not None:
                coro = task.get_coro()
                if getattr(coro, "cr_frame", None) is frame:
                    ctx = self.task_ctx.get(id(task))
                    if ctx is None:
                        ctx = self.next_ctx
                        self.next_ctx += 1
                        self.task_ctx[id(task)] = ctx
                        self.tasks[id(task)] = task
                        self.stacks[ctx] = []
                        self.roots[ctx] = id(frame)
                        self._emit({"ev": "spawn", "ctx": ctx, "parent": 0})
                    return ctx
        return 0

    def _wake(self, ctx: int) -> None:
        """Frame activity in a suspended context means it is running again
        (for example ``coro.close()`` or a cancellation ``throw()``)."""
        if ctx in self.suspended:
            self.suspended.discard(ctx)
            self._emit({"ev": "resume", "ctx": ctx})

    def _push(self, frame) -> None:
        ctx = self._ctx_for_new_frame(frame)
        if ctx in self.exited:
            return
        self._wake(ctx)
        act = self.next_act
        self.next_act += 1
        code = frame.f_code
        self.frames[id(frame)] = (ctx, act, frame)
        self.stacks[ctx].append(act)
        self._emit({"ev": "call", "ctx": ctx, "act": act, "fn": self._fn(code), "frame_bytes": _frame_bytes(code)})

    def _pop(self, frame) -> None:
        info = self.frames.pop(id(frame), None)
        if info is None:
            return
        ctx, act, _ = info
        if ctx in self.exited:
            return
        self._wake(ctx)
        stack = self.stacks[ctx]
        # Unwind anything above it (frames we missed returns for).
        while stack and stack[-1] != act:
            dangling = stack.pop()
            self._emit({"ev": "return", "ctx": ctx, "act": dangling})
        if not stack:
            return
        stack.pop()
        self._emit({"ev": "return", "ctx": ctx, "act": act})
        if ctx != 0 and not stack and self.roots.get(ctx) == id(frame):
            self._emit({"ev": "exit", "ctx": ctx})
            self.exited.add(ctx)
            for tid, c in list(self.task_ctx.items()):
                if c == ctx:
                    del self.task_ctx[tid]
                    self.tasks.pop(tid, None)

    # ----- sys.monitoring callbacks -----

    def _frame(self, code):
        f = sys._getframe(2)
        return f if f.f_code is code else None

    def on_start(self, code, offset):
        if code.co_filename == _THIS_FILE:
            return sys.monitoring.DISABLE
        f = self._frame(code)
        if f is not None:
            self._push(f)

    def on_resume(self, code, offset):
        f = self._frame(code)
        if f is None:
            return
        info = self.frames.get(id(f))
        if info is None:
            # Detached generator (or untracked frame) resuming: a new activation.
            self._push(f)
            return
        self._wake(info[0])

    def on_throw(self, code, offset, exc):
        f = self._frame(code)
        if f is None:
            return
        info = self.frames.get(id(f))
        if info is not None:
            self._wake(info[0])

    def on_yield(self, code, offset, value):
        f = self._frame(code)
        if f is None:
            return
        info = self.frames.get(id(f))
        if info is None:
            return
        ctx = info[0]
        if ctx != 0 and code.co_flags & _COROUTINE_FLAGS:
            if self.roots.get(ctx) == id(f):
                # Unwind detached generators still recorded above the chain.
                self.suspended.add(ctx)
                self._emit({"ev": "suspend", "ctx": ctx})
            return
        self._pop(f)

    def on_return(self, code, offset, value):
        f = self._frame(code)
        if f is not None:
            self._pop(f)

    def on_unwind(self, code, offset, exc):
        f = self._frame(code)
        if f is not None:
            self._pop(f)

    # ----- lifecycle -----

    def _measure_overhead(self) -> float:
        # Compiled under another file name: this file is excluded from tracing.
        ns: dict = {}
        exec(compile("def leaf(x):\n    return x + 1\n"
                     "def loop(n):\n    for i in range(n):\n        leaf(i)\n",
                     "<morphstack-calibrate>", "exec"), ns)
        loop = ns["loop"]

        n = 20000
        t = time.perf_counter_ns()
        loop(n)
        plain = time.perf_counter_ns() - t
        saved = self.events
        self.events = open(os.devnull, "w")
        self._install()
        t = time.perf_counter_ns()
        loop(n)
        traced = time.perf_counter_ns() - t
        self._uninstall()
        self.events.close()
        self.events = saved
        self.frames.clear()
        self.stacks = {0: []}
        self.next_act = 0
        self.n = 0
        self.last_ts = 0
        self.fn_ids.clear()
        self.functions.clear()
        # Two events (start, return) per leaf call.
        return max(0.0, (traced - plain) / (2 * n))

    def _install(self):
        m = sys.monitoring
        m.use_tool_id(self.tool, "morphstack")
        E = m.events
        m.register_callback(self.tool, E.PY_START, self.on_start)
        m.register_callback(self.tool, E.PY_RESUME, self.on_resume)
        m.register_callback(self.tool, E.PY_YIELD, self.on_yield)
        m.register_callback(self.tool, E.PY_RETURN, self.on_return)
        m.register_callback(self.tool, E.PY_UNWIND, self.on_unwind)
        m.register_callback(self.tool, E.PY_THROW, self.on_throw)
        m.set_events(self.tool, E.PY_START | E.PY_RESUME | E.PY_YIELD | E.PY_RETURN | E.PY_UNWIND | E.PY_THROW)

    def _uninstall(self):
        m = sys.monitoring
        m.set_events(self.tool, 0)
        for ev in (m.events.PY_START, m.events.PY_RESUME, m.events.PY_YIELD, m.events.PY_RETURN,
                   m.events.PY_UNWIND, m.events.PY_THROW):
            m.register_callback(self.tool, ev, None)
        m.free_tool_id(self.tool)

    def __enter__(self):
        self.events = open(self.path + ".events", "w")
        self.overhead_ns = self._measure_overhead()
        self.t0 = time.perf_counter_ns()
        self._emit({"ev": "spawn", "ctx": 0})
        self._install()
        return self

    def __exit__(self, *exc):
        self._uninstall()
        self.events.close()
        header = {
            "format": "morphstack-trace",
            "version": 0,
            "source": self.source,
            "description": (self.description + f"; python {sys.version.split()[0]}; "
                            f"tracer overhead {self.overhead_ns:.0f} ns/event x {self.overhead_fraction} subtracted").lstrip("; "),
            "functions": self.functions,
        }
        with open(self.path, "w") as out, open(self.path + ".events") as evs:
            out.write(json.dumps(header, separators=(",", ":")) + "\n")
            for line in evs:
                out.write(line)
        os.remove(self.path + ".events")
        print(f"morphstack trace: {self.n} events, {len(self.functions)} functions -> {self.path}", file=sys.stderr)
        return False
