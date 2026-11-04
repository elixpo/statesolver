"""A real aiohttp server and client in one process, traced with MorphTracer.

Three phases, so that the best representation can change over time:

  A  CPU-heavy: concurrent clients hit /render (recursive templating plus JSON)
  B  Parked: many long-poll requests wait on the server while light /db traffic runs
  C  CPU-heavy again

Usage:
  python aiohttp_workload.py OUT.jsonl [--scale 1.0] [--seed 1]
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from aiohttp import web, ClientSession, TCPConnector  # noqa: E402

from py_trace import MorphTracer  # noqa: E402


def render(node, depth):
    if depth == 0:
        return {"leaf": node, "text": f"item-{node}"}
    return {"node": node, "children": [render(node * 3 + i, depth - 1) for i in range(3)]}


async def handle_render(request):
    depth = int(request.query.get("d", "5"))
    body = json.dumps(render(1, depth))
    return web.Response(text=body, content_type="application/json")


async def fake_db(rng: random.Random):
    await asyncio.sleep(rng.lognormvariate(-6.0, 0.8))  # median about 2.5 ms
    return {"rows": [rng.random() for _ in range(20)]}


async def handle_db(request):
    rng = request.app["rng"]
    out = []
    for _ in range(rng.randint(1, 3)):
        out.append(await fake_db(rng))
    return web.json_response(out)


async def handle_longpoll(request):
    rng = request.app["rng"]
    await asyncio.sleep(rng.uniform(1.0, 2.0))
    return web.json_response({"event": "tick"})


async def client_loop(session, url, n):
    for _ in range(n):
        async with session.get(url) as r:
            await r.read()


async def run(scale: float, seed: int):
    rng = random.Random(seed)
    app = web.Application()
    app["rng"] = rng
    app.add_routes([web.get("/render", handle_render), web.get("/db", handle_db), web.get("/longpoll", handle_longpoll)])
    runner = web.AppRunner(app, access_log=None)
    await runner.setup()
    site = web.TCPSite(runner, "127.0.0.1", 0)
    await site.start()
    port = site._server.sockets[0].getsockname()[1]
    base = f"http://127.0.0.1:{port}"

    clients = max(1, int(20 * scale))
    per_client = max(1, int(20 * scale))
    parked = max(1, int(300 * scale))
    async with ClientSession(connector=TCPConnector(limit=0)) as session:
        # Phase A
        await asyncio.gather(*(client_loop(session, f"{base}/render?d=5", per_client) for _ in range(clients)))
        # Phase B
        polls = [asyncio.create_task(client_loop(session, f"{base}/longpoll", 1)) for _ in range(parked)]
        await asyncio.gather(*(client_loop(session, f"{base}/db", per_client) for _ in range(max(1, clients // 4))))
        await asyncio.gather(*polls)
        # Phase C
        await asyncio.gather(*(client_loop(session, f"{base}/render?d=5", per_client) for _ in range(clients)))
    await runner.cleanup()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("out")
    p.add_argument("--scale", type=float, default=1.0)
    p.add_argument("--seed", type=int, default=1)
    a = p.parse_args()
    with MorphTracer(a.out, source="python-aiohttp/phases", description=f"aiohttp server+client, scale={a.scale}, seed={a.seed}"):
        asyncio.run(run(a.scale, a.seed))


if __name__ == "__main__":
    main()
