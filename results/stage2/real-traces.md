# Real traces

## python-aiohttp-phases

- **Source:** `scripts/capture/aiohttp_workload.py --scale 1.0 --seed 1`, traced with `scripts/capture/py_trace.py` (`sys.monitoring`). The workload is a real aiohttp 3.14.5 server and client on Python 3.14.4, run in three phases: CPU rendering, then 300 parked long-polls alongside /db traffic, then CPU rendering again.
- **File:** `results/traces/python-aiohttp-phases.msb` (not in git, 12 MB; sha256 prefix `fb6964f420c4e6d6`). Python timing is not deterministic, so a re-capture gives a statistically similar trace, not an identical one.
- **Shape:** 1,543,095 events; 534 real functions; 1,265 task contexts, with at most 693 parked at once; call depth up to 17; frames of 162 B on average. Suspensions have a median of 55 ms and a maximum of 1.5 s.
- **Tracer overhead:** a measured 4.1 µs per event, 80% of which is subtracted from timestamps. Compute durations are approximate; suspension durations are mostly real.

### Oracle on the calibrated x86-64 profile

| Memory price | Best fixed | Oracle gain | from heterogeneity | from switching | Contexts migrated |
|---|---|---|---|---|---|
| 0.1× | segmented | 1.7% | 1.7% | 0.0% | 0% |
| 1× (cloud default) | segmented | 9.9% | 0.8% | 9.1% | 27% |
| 10× (memory-bound) | segmented | 50.7% | 9.0% | 41.6% | 71% |
| 100× | heap | 37.4% | 11.5% | 25.9% | 76% |

**Mechanism:** run on segmented stacks, compact parked keep-alive connections into heap frames, and switch back when they wake.
