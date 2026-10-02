# Documentation Map

Use this page as the entry point for active Patina documentation. Files under `archive/` are historical evidence, not current execution guidance.

## Start Here

This is `Asanilo/patina-Linux`: `main` is the primary Linux development/product
branch and includes the accepted daemon separation. `feature/patinad-daemon`
is retained as merged history. Check the current branch and candidate evidence
before applying runtime or release instructions; source integration is not a release.

For current priorities, read the roadmap and the relevant working checklist below.
For implementation contracts, use the engineering references. Archives are only
for tracing past decisions and test evidence, not a backlog to execute.

- [`../CONTEXT.md`](../CONTEXT.md): canonical product and runtime terminology
- [`product-principles-and-scope.md`](./product-principles-and-scope.md): product purpose, scope, and non-goals
- [`roadmap-and-prioritization.md`](./roadmap-and-prioritization.md): current priorities and implementation order
- [`architecture.md`](./architecture.md): long-term ownership and module boundaries
- [`engineering-quality.md`](./engineering-quality.md): quality, performance, and validation rules

Read only the references needed for the task:

| Task | References |
| --- | --- |
| Local bug fix | Relevant owner code/tests; [fix guardrails](./issue-fix-boundary-guardrails.md); architecture only if ownership is unclear |
| UI change | [Quiet Pro](./quiet-pro-component-guidelines.md) and the affected feature |
| Runtime, persistence, or cross-layer change | Relevant sections of [architecture](./architecture.md) and [engineering quality](./engineering-quality.md) |
| Scope, priority, or upstream Linux proposal | [Product scope](./product-principles-and-scope.md), [roadmap](./roadmap-and-prioritization.md), [Linux support](./linux-platform-support.md) |
| Release or packaging | [Release policy](./versioning-and-release-policy.md), [development setup](./linux-development-setup.md), candidate evidence |
| Documentation only | Changed references, links, terminology, commands, and branch consistency; no full build |

## Product And UI

- [`quiet-pro-component-guidelines.md`](./quiet-pro-component-guidelines.md): Quiet Pro design system and component behavior
- [`linux-platform-support.md`](./linux-platform-support.md): supported Linux environments, providers, and fallbacks

## Engineering Reference

- [`linux-development-setup.md`](./linux-development-setup.md): Linux development, extension, and packaging setup
- [`api-index.md`](./api-index.md): local HTTP API contract and examples
- [`activity-import-format.md`](./activity-import-format.md): supported activity import format
- [`mcp-wrapper.md`](./mcp-wrapper.md): MCP integration contract and usage
- [`issue-fix-boundary-guardrails.md`](./issue-fix-boundary-guardrails.md): stable-period issue triage and fix boundaries
- [`versioning-and-release-policy.md`](./versioning-and-release-policy.md): version, changelog, updater, and release rules

## Current Work

The stable-release closeout is complete. The authorized next stage is [one backend and multiple synchronized clients](./working/2026-10-03-multi-client-platform.md), developed on `feature/multi-client-platform` in its own worktree. Use its capability gaps and milestone evidence to distinguish planned clients from delivered functionality; retain unrelated paused items as deferred.

Only current execution documents belong under `working/`. Move them to `archive/` when they stop being the active implementation basis.

## Completed Stage Evidence

- [`archive/2026-10-02-stable-closeout.md`](./archive/2026-10-02-stable-closeout.md): 1.9.2 public release, actual 1.8.4 upgrade with a manual reopen, host installation, and final stage evidence
- [`archive/2026-07-10-patinad-runtime-design.md`](./archive/2026-07-10-patinad-runtime-design.md): daemon ownership, integration and installed acceptance history
- [`archive/2026-09-21-linux-platform-reuse.md`](./archive/2026-09-21-linux-platform-reuse.md): Linux provider reuse, GNOME protocol and initial AppImage acceptance history
- [`archive/2026-09-23-sustainable-linux-release.md`](./archive/2026-09-23-sustainable-linux-release.md): GNOME 46/Fedora technical acceptance, core-flow checks and client-read inventory
- [`archive/2026-09-27-tauri-runtime-refresh.md`](./archive/2026-09-27-tauri-runtime-refresh.md): completed Tauri refresh and beta.21 publication evidence
- [`archive/2026-09-27-appimage-stable-release.md`](./archive/2026-09-27-appimage-stable-release.md): completed AppImage public-channel acceptance and 1.9.1 stable-release evidence

The roadmap now places daemon and Desktop product work together on Linux `main`,
with upstream Linux contributions kept separate. `feat/linux-desktop` was created
from upstream `80204c73`; its unfinished draft remains paused. The daemon checklist
records completed acceptance, integration and later platform work. The archived release
execution document records the 1.9.1 stable AppImage and DEB publication and acceptance.

Historical pending items are not the current task queue. Use the roadmap for current scope; the two-week observation and other user-paused features remain deferred.

## Decisions

Architecture decisions live under [`adr/`](./adr/). They explain durable choices and trade-offs; the active architecture and roadmap documents remain the normative rules.

## Supplemental And Historical Material

- [`examples/`](./examples/): optional integration examples, not required reading
- [`archive/`](./archive/): completed plans, superseded designs, reviews, and historical targets
- [`archive/2026-09-19-patinad-runtime-history.md`](./archive/2026-09-19-patinad-runtime-history.md): preserved daemon migration experiments and acceptance timeline

Do not use archived documents to reconstruct current behavior when an active document above already defines it.
