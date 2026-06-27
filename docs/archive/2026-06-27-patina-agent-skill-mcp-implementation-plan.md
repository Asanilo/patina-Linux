# Patina Agent Skill and MCP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repair Patina's MCP interoperability, complete its integration references, and add a validated Agent Skill with MCP and HTTP variants.

**Architecture:** The Rust localhost API remains authoritative. The TypeScript stdio wrapper maps a fixed MCP tool set to that API. A versioned Skill uses progressive disclosure to select MCP or HTTP without duplicating the OpenAPI schema.

**Tech Stack:** TypeScript, Node.js stdio/JSON-RPC, Rust/OpenAPI 3.1, Markdown Agent Skills.

---

### Task 1: MCP protocol contract

**Files:**
- Modify: `tests/patinaMcpScript.test.ts`
- Modify: `scripts/patina-mcp.ts`

- [x] Add failing tests for newline-delimited messages, notification suppression, buffer consumption, required write arguments, and tool-result errors.
- [x] Run `node --experimental-strip-types tests/patinaMcpScript.test.ts` and confirm the new assertions fail for the expected reasons.
- [x] Implement the minimal protocol and schema corrections.
- [x] Re-run the focused test and confirm it passes.

### Task 2: API and MCP reference completeness

**Files:**
- Modify: `docs/api-index.md`
- Modify: `docs/mcp-wrapper.md`
- Modify: `README.md`
- Modify: `src-tauri/src/engine/api/handlers/openapi.rs`

- [x] Add a failing documentation contract test for every implemented endpoint section and MCP tool.
- [x] Document AI activity context, Tools snapshot, transport framing, direct Node launch, and error behavior.
- [x] Make the OpenAPI server URL explicitly describe the default port instead of claiming it follows runtime configuration.
- [x] Run the focused documentation and Rust OpenAPI tests.

### Task 3: Agent Skill

**Files:**
- Create: `skills/analyzing-patina-activity/SKILL.md`
- Create: `skills/analyzing-patina-activity/agents/openai.yaml`
- Create: `skills/analyzing-patina-activity/references/mcp.md`
- Create: `skills/analyzing-patina-activity/references/http.md`
- Create: `skills/analyzing-patina-activity/references/analysis.md`
- Create: `tests/patinaAgentSkill.test.ts`
- Modify: `package.json`

- [x] Add a failing Skill contract test covering transport selection, privacy, active-session handling, diagnostics, and write confirmation.
- [x] Initialize the Skill with `skill-creator/scripts/init_skill.py`.
- [x] Write the minimal Skill and transport references needed to pass the contract.
- [x] Run `skill-creator/scripts/quick_validate.py` and the Skill contract test.

### Task 4: Completion validation

**Files:**
- Move: `docs/working/2026-06-27-patina-agent-skill-mcp-*.md` to `docs/archive/`

- [x] Run `npm test`.
- [x] Run `npm run test:replay`.
- [x] Run `npm run build`.
- [x] Run focused MCP, Skill, and Rust OpenAPI tests.
- [x] Check `git diff --check` and review the final diff.
- [x] Archive the completed working documents.
