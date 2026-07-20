# Documentation Map

Use this page as the entry point for active Patina documentation. Files under `archive/` are historical evidence, not current execution guidance.

## Start Here

- [`../CONTEXT.md`](../CONTEXT.md): canonical product and runtime terminology
- [`product-principles-and-scope.md`](./product-principles-and-scope.md): product purpose, scope, and non-goals
- [`roadmap-and-prioritization.md`](./roadmap-and-prioritization.md): current priorities and implementation order
- [`architecture.md`](./architecture.md): long-term ownership and module boundaries
- [`engineering-quality.md`](./engineering-quality.md): quality, performance, and validation rules

## Product And UI

- [`quiet-pro-component-guidelines.md`](./quiet-pro-component-guidelines.md): Quiet Pro design system and component behavior
- [`linux-platform-support.md`](./linux-platform-support.md): supported Linux environments, providers, and fallbacks

## Engineering Reference

- [`linux-development-setup.md`](./linux-development-setup.md): Linux development, extension, and packaging setup
- [`api-index.md`](./api-index.md): local HTTP API contract and examples
- [`mcp-wrapper.md`](./mcp-wrapper.md): MCP integration contract and usage
- [`issue-fix-boundary-guardrails.md`](./issue-fix-boundary-guardrails.md): stable-period issue triage and fix boundaries
- [`versioning-and-release-policy.md`](./versioning-and-release-policy.md): version, changelog, updater, and release rules

## Current Work

- [`working/2026-07-10-patinad-runtime-design.md`](./working/2026-07-10-patinad-runtime-design.md): active `patinad` migration design and acceptance gates

Only current execution documents belong under `working/`. Move them to `archive/` when they stop being the active implementation basis.

## Decisions

Architecture decisions live under [`adr/`](./adr/). They explain durable choices and trade-offs; the active architecture and roadmap documents remain the normative rules.

## Supplemental And Historical Material

- [`examples/`](./examples/): optional integration examples, not required reading
- [`archive/`](./archive/): completed plans, superseded designs, reviews, and historical targets

Do not use archived documents to reconstruct current behavior when an active document above already defines it.
