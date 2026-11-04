# Security policy

## Private reporting

Please use [GitHub private vulnerability reporting](https://github.com/elixpo/morphstack/security/advisories/new)
for vulnerabilities. If that route is unavailable, email `hello@elixpo.com`
with the repository name, affected version or commit, impact, reproduction
steps, and any suggested mitigation.

Do not disclose an unresolved vulnerability in a public issue or discussion.
We will acknowledge a usable report, investigate it, coordinate a fix, and
publish an advisory when disclosure is appropriate. No response-time guarantee
is made while MorphStack is pre-release.

## High-priority boundaries

Reports are especially useful when they concern:

- memory-safety violations in the coroutine runtime (`morphstack-rt`), such as
  use-after-free, dangling references after frame migration, or stack overflow
  that escapes a guard page;
- crashes or unbounded resource use when parsing untrusted trace files;
- secrets or agent-trace exposure in the optional `morphstack-agent` features; or
- dependency, release, workflow, or artifact supply-chain compromise.

Agent prompt behavior without a demonstrated security boundary failure may be handled
as a normal bug. Never include real secrets or third-party personal data in a
reproduction.
