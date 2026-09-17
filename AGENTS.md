# AGENTS.md

This repository uses `Quiet Pro` as the only long-term UI design baseline.

This file is the top-level collaboration entry point for repository-aware agents.

These instructions apply to repository work; UI sections apply when UI is touched. Explicit task-specific user instructions take precedence.

## Documentation Routing

- Use `docs/README.md` as the documentation map.
- Domain terminology must follow `CONTEXT.md`.
- Product direction and scope must follow `docs/product-principles-and-scope.md`.
- Roadmap and priority decisions must follow `docs/roadmap-and-prioritization.md`.
- Engineering quality direction should follow `docs/engineering-quality.md`.
- UI work must follow `docs/quiet-pro-component-guidelines.md`.
- Architecture refactors, boundary decisions, and new modules must align with `docs/architecture.md`.
- Stable-period issue fixes and boundary triage must follow `docs/issue-fix-boundary-guardrails.md`.
- Versioning, changelog, and release work must follow `docs/versioning-and-release-policy.md`.
- Treat the top-level long-lived docs under `docs/` as the current source of truth.
- Read the documentation map and task-relevant references first; do not reload every long document for each small change. Use the active working document for execution evidence, with its latest status taking precedence over historical progress entries.

## Execution And Communication

- Check the current branch, working tree and existing implementation before planning new work; do not duplicate completed features.
- Continue authorized work through implementation and verification. Routine reversible choices do not require repeated confirmation; ask when missing information materially changes scope, product behavior or data safety.
- Reassessing an ownership boundary means choosing and documenting the owner, not automatically stopping for permission. Escalate genuine unresolved decisions.
- Default to one agent. Use subagents only when the user requests them or explicitly approves their cost for the current task.
- Keep updates concise and evidence-based. Distinguish implemented, automatically tested, manually verified, packaged, pushed and publicly released states.
- A user-paused feature stays deferred until resumed; do not let optional polish block the next agreed release milestone.

## Quiet Pro Baseline

- Build calm, professional, restrained desktop-product UI.
- Prefer typography, spacing, alignment, and hierarchy over decoration.
- Keep the interface neutral and durable rather than flashy or brand-heavy.
- New UI should feel native to the existing Dashboard, History, App Mapping, and Settings surfaces.

## Hard Rules

- Do not introduce glassmorphism, blur-heavy panels, neon glow, or large gradient backgrounds.
- Do not hardcode new colors, radii, shadows, or border styles when a token or semantic variable should exist.
- Do not add one-off visual treatments that only work on a single page.
- Do not make components louder than the information they present.
- Do not trade readability or efficiency for "design feeling".

## Token And Styling Rules

- Reuse existing semantic tokens first.
- If a new visual role is needed, add or extend a token instead of hardcoding a value in a component.
- Keep radius, border, elevation, and motion within the existing Quiet Pro scale.
- Category or status colors may vary by feature, but surrounding chrome must stay within the Quiet Pro system.

## Component Rules

- New components must define clear `default`, `hover`, `active`, `focus`, `disabled`, and where relevant `loading` and `empty` states.
- Prefer existing component archetypes: `panel`, `control`, `chip`, `status`.
- Icons support recognition; they should not become the main visual focus.
- Dense pages may be efficient, but they must still scan cleanly at a glance.

## Implementation Preference

- Extend the design system before inventing a page-local workaround.
- Preserve existing product behavior unless the user explicitly asks for interaction changes.
- If a proposed UI change conflicts with Quiet Pro or requires a new visual direction, pause and confirm before proceeding.

## Architecture Direction

- Follow `docs/architecture.md` as the architecture mother document.
- Frontend long-term structure is `app / features / shared / platform`.
- Rust long-term structure is `lib.rs + app / commands / platform / engine / data / domain`.
- Keep Tauri command handlers thin; do not let `commands/*` or `lib.rs` regrow thick business logic.
- Prefer owner-first placement: decide the real owner before deciding the file or layer.
- `shared/*` is only for stable shared capability, not a temporary bucket.
- `platform/*` is for explicit external-environment boundaries, not a generic dump for hard problems.
- Do not reintroduce exited root layers such as `src/lib/` or `src/types/`.
- Treat compatibility shells and forwarding layers as explicit exceptions that should stay thin.
- Treat files under `docs/archive/` as historical context, not the default source of truth.
- Follow the current `patinad` roadmap: the daemon owns background tracking and runtime writes; Desktop and future browser/TUI clients use supported boundaries. Do not expand parallel embedded and daemon implementations without a migration need.

## Product And Priority Direction

- Keep the product centered on personal, local-first, Linux desktop time tracking, with GNOME Wayland as the primary supported environment.
- Prioritize trust, readability, control, and long-term usability over feature count.
- Do not quietly expand the product toward team SaaS, cloud-first workflows, mobile-first usage, or gamified productivity unless the user explicitly changes product direction.
- When multiple directions compete, prefer correctness, data safety, and high-frequency core flows before expansion work.

## Stable-Period Fixing

- In the stable period, fix problems by deciding owner first and implementation second.
- Use the lightest mode that fits the issue: small fix, boundary judgment, or execution plan.
- If a fix requires a new shared abstraction, cross-layer relocation, or a new compatibility shell, stop and reassess before implementing.
- Keep `app/*`, `shared/*`, `platform/*`, `lib.rs`, and `commands/*` under extra scrutiny because they are high-attraction layers.

## Release And Validation

- For release work, keep version files, Git tags, GitHub Release titles, and updater artifacts consistent.
- Do not skip the minimum validation bar for architecture-affecting or release-affecting changes.
- Follow the validation matrix in `docs/issue-fix-boundary-guardrails.md`: `npm run check` for frontend delivery, `npm run check:full` for architecture/Rust runtime work, and release-policy gates for releases. Documentation-only edits need relevant consistency checks, not a full application rebuild.
- After required checks pass, repeat or broaden them only for new changes, failures or unresolved risks. Native lifecycle tests validate lifecycle, not full React behavior or user-observed flicker.
- Do not bump versions or build a DEB for every patch; group work into an agreed test candidate or releasable milestone. Installation and public release are separate actions.
- Treat code quality, software performance, and reliability/validation as related but different concerns; do not optimize one by accidentally damaging the others.

## GitHub Push And Issue Rules

- When asked to push, use the current user-authorized branch and verify its upstream. The ongoing daemon migration stays on `feature/patinad-daemon`; do not switch, merge or push it into `main` implicitly. Use `origin/main` when the confirmed task is on main.
- Do not create a branch or pull request unless the user explicitly asks for one.
- Do not use issue-closing keywords such as `Closes`, `Fixes`, or `Resolves` in commits, changelog entries, pull request descriptions, or GitHub comments unless the user explicitly asks to close the issue.
- When a change relates to an issue, reference it without changing its state, for example with `Refs #3` or a Markdown issue link.
- Do not close, reopen, label, or otherwise mutate GitHub issues unless the user explicitly requests that issue action.

## Documentation Hygiene

- Top-level `docs/` is for active long-lived reference documents only.
- One-off execution plans, temporary fix plans, and completed task documents should not stay in top-level `docs/`.
- Temporary execution plans may live under a dedicated subdirectory such as `docs/working/`, but should be archived once they stop being the active execution basis.
- When a one-off document is no longer the current source of truth, move it to `docs/archive/`.
- When a long-lived rule changes, update the relevant top-level doc instead of scattering the new rule across temporary notes.
- Do not update or rely on `docs/archive/*` as the default execution basis unless the user explicitly asks for historical context.
- Do not try to reconstruct long-lived docs from old mojibake terminal output or archived one-off plans when a current top-level source-of-truth document already exists.

## Encoding Rules

- Markdown and documentation files must be saved as UTF-8.
- When editing Chinese documentation on Windows, preserve readable UTF-8 text and do not introduce mojibake.
- Do not rewrite `.md` files through shell output or redirection patterns that may change encoding implicitly.
- Do not rewrite source files or documentation through PowerShell text-output commands or redirection, including `>`, `>>`, `Set-Content`, and `Out-File`.
- When a task touches Chinese text in `.md`, `.ts`, `.tsx`, or `.rs` files, prefer normal code edits only; if encoding damage is detected, stop and repair encoding first before continuing the task.
- If a documentation file appears garbled in terminal output, verify the file bytes before assuming the content is corrupted.
