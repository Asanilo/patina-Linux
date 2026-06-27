# Patina Agent Skill and MCP Design

## Goal

Provide a reliable external-agent integration for Patina by repairing the stdio MCP wrapper, completing the active API/MCP references, and publishing one Agent Skill with separate MCP and HTTP workflows.

## Boundaries

- Keep the local HTTP API as the source of truth for data, authentication, and field schemas.
- Keep the MCP wrapper thin and dependency-free.
- Do not add built-in chat or remote AI processing.
- Keep all services bound to localhost and never place API tokens in Skill content or command output.
- Treat app classification, rename, and exclusion as writes that require explicit user intent.

## Components

### MCP transport

Use newline-delimited UTF-8 JSON-RPC messages as required by MCP stdio. Handle initialization notifications without responding, process each input message exactly once, keep logs off stdout, and expose accurate required/optional tool arguments.

### API and MCP references

Keep `docs/api-index.md` as the human behavior reference and `/api/v1/openapi.json` as the machine-readable schema. Add the missing AI activity-context and Tools snapshot sections. Keep `docs/mcp-wrapper.md` focused on standards-compliant launch configuration, tools, errors, and client setup.

### Agent Skill

Create one versioned Skill under `skills/analyzing-patina-activity/`. Its `SKILL.md` selects a transport and routes detailed work to:

- `references/mcp.md` for MCP-first usage.
- `references/http.md` for direct localhost HTTP usage.
- `references/analysis.md` for interpretation, privacy, active-session, and write-safety rules shared by both transports.

The Skill must prefer MCP when configured, fall back to HTTP when MCP is unavailable, diagnose data-source health before analysis, and distinguish closed sessions from the realtime active session.

## Validation

- Add protocol-level tests for newline framing, notification handling, one-time input consumption, required schemas, and tool execution errors.
- Add a repository test that checks the Skill structure and critical safety instructions.
- Run the skill validator from `skill-creator`.
- Run the existing MCP tests, minimum frontend validation, and focused Rust OpenAPI tests.

