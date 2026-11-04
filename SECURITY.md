# Security policy

## Private reporting

Please use [GitHub private vulnerability reporting](https://github.com/elixpo/statesolver/security/advisories/new)
for vulnerabilities. If that route is unavailable, email `hello@elixpo.com`
with the repository name, affected version or commit, impact, reproduction
steps, and any suggested mitigation.

Do not disclose an unresolved vulnerability in a public issue or discussion.
We will acknowledge a usable report, investigate it, coordinate a fix, and
publish an advisory when disclosure is appropriate. No response-time guarantee
is made while StateSolver is pre-release.

## High-priority boundaries

StateSolver runs and controls other programs, so reports are especially useful when they concern:

- the recorder or replayer affecting processes, files, or the network beyond the explicitly traced target (for example, replay re-executing side-effecting syscalls instead of injecting recorded results);
- crashes, code execution, or unbounded resource use when parsing untrusted `.ss` traces or ELF/DWARF files;
- memory-safety violations in `statesolver-record` or `statesolver-replay`;
- secrets from traced programs (memory, syscall payloads) leaking through traces, the harness, or agent output; and
- dependency, release, workflow, or artifact supply-chain compromise.

Traces contain the traced program's memory and inputs. Treat them as sensitive.

Agent prompt behavior without a demonstrated security boundary failure may be handled
as a normal bug. Never include real secrets or third-party personal data in a
reproduction.
