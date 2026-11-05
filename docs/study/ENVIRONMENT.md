# Study environment: what is needed before the pilot (#4)

Status as of 2026-11-05: **laptop-scale plan adopted** (see DIRECTION.md, Changes). About 74 GB was freed (Rust build outputs and unused Docker data), so **92 GB is free**, which is enough for the one-bug-at-a-time pipeline. Still open: the model provider and budget, and a second annotator.

Original assessment (2026-11-04): Everything below is needed for the **agent runs**. Building the MCP server and the study harness does not need any of it.

## 1. Model access: **resolved 2026-11-05 (Pollinations)**

- Provider: Pollinations, OpenAI-compatible: `POST https://gen.pollinations.ai/v1/chat/completions`, header `Authorization: Bearer $POLLINATIONS_KEY` (an `sk_` key). The `pk_` app key is identification only; Pollinations rejects it for generation since 2026-11-01.
- Keys: plaintext in `.env.local` (git-ignored); the SOPS-encrypted `.env` is committed, following the Crumb pattern. Never print or log keys.
- Models, verified in the live catalog and with a tool-calling test on 2026-11-05:

  | Alias | Model | Pollen per 1M tokens (in / out) | Tools |
  |---|---|---|---|
  | `nova-fast` | `amazon/nova-micro-v1` | 0.035 / 0.14 | yes |
  | `qwen-coder` | `qwen/qwen3-coder-30b-a3b-instruct` (open-weight) | 0.074 / 0.30 | yes |

  Both are `paid_only`, so they draw on paid Pollen. The balance was 86.5 Pollen (42.1 paid). At about 50k tokens per agent run, the whole pilot costs **under 1 Pollen**.

### Original requirements (2026-11-04)

The design ([DIRECTION.md](../DIRECTION.md) §2) needs **3 or more models, at least one open-weight**. The pilot uses one.

| Need | Options | Notes |
|---|---|---|
| Pilot model | Any one strong tool-using model | The pilot is A1 against A4 (plus A5) on about 20 bugs |
| Full study | 3 or more models, at least 1 open-weight (for example DeepSeek, Qwen, or GLM via a provider) | DebugHarness used DeepSeek-V3.2 and GLM-5; overlapping keeps the A1 replication comparable |
| Runner | Crumb (`crumb-harness`, provider-neutral) plus one other harness | API keys go in environment variables and are **never committed** |

**Budget estimate** (DebugHarness reports about $0.09–0.64 per bug and run):
- pilot: 20 bugs × 3 arms × 3 seeds ≈ 180 runs, roughly **$20–120**;
- full study: about 6 arms × 3 models × 3 seeds × about 300 bugs, roughly **$1.5k–10k**, depending on the models.

## 2. Machine (maintainer)

rr needs hardware performance counters, which rules out most cloud VMs unless they expose a vPMU.

| Resource | This laptop | Needed |
|---|---|---|
| CPU | i5-13450HX (hybrid; rr pinned to a P-core) | Intel or AMD with a working PMU; bare metal is preferred |
| RAM | 6 GB | **32 GB or more** (benchmark builds run in Docker) |
| Free disk | **13 GB (98% full)** | **300–500 GB** for SEC-bench and ARVO images plus rr traces |
| Docker | 29.7.1 | rr inside containers needs `--cap-add=SYS_PTRACE --security-opt seccomp=unconfined` and host `perf_event_paranoid ≤ 1` |

Options:
- **(a)** free about 300 GB here: the pilot only;
- **(b)** a bare-metal Linux box or workstation (recommended for the full study);
- **(c)** a cloud instance with vPMU or a metal instance type (check rr support before paying).

## 3. Data (scripted by us once 1 and 2 exist)

- **SEC-bench**, primary: Docker image per instance, root-cause ground truth added by us.
- **ARVO** with **T2L-ARVO** (50 expert-labelled cases), **VulnLoc** as a cross-check.
- Root-cause annotation needs two annotators (κ reported). The maintainer plus one more person, or a second pass under a written guideline.
