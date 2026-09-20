# Documentation Map

Use this page as the entry point for active Patina documentation. Files under `archive/` are historical evidence, not current execution guidance.

## Start Here

This is `Asanilo/patina-Linux`: `main` is the primary Linux development/product
branch; `feature/patinad-daemon` is the daemon-separation experiment. Check the
current branch before applying experimental runtime or release instructions.

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

- [`working/2026-07-10-patinad-runtime-design.md`](./working/2026-07-10-patinad-runtime-design.md): current daemon acceptance checklist, delivery status, and deferred work

The roadmap separates Linux `main`, the daemon experiment, and upstream Linux
contributions. `feat/linux-desktop` was created from upstream `80204c73`; its
unfinished draft is on hold while daemon work converges. Follow the ordered Todo
in the daemon checklist for the database audit, blocking fixes, validation,
merge readiness, and later selective reuse of platform work.

Only current execution documents belong under `working/`. Move them to `archive/` when they stop being the active implementation basis.

## Decisions

Architecture decisions live under [`adr/`](./adr/). They explain durable choices and trade-offs; the active architecture and roadmap documents remain the normative rules.

## Supplemental And Historical Material

- [`examples/`](./examples/): optional integration examples, not required reading
- [`archive/`](./archive/): completed plans, superseded designs, reviews, and historical targets
- [`archive/2026-09-19-patinad-runtime-history.md`](./archive/2026-09-19-patinad-runtime-history.md): preserved daemon migration experiments and acceptance timeline

Do not use archived documents to reconstruct current behavior when an active document above already defines it.
