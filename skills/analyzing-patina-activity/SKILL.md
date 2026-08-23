---
name: analyzing-patina-activity
description: Use when a user asks to inspect, summarize, compare, or diagnose personal activity recorded by Patina, including desktop sessions, browser activity, focus trends, current tracking health, Tools state, or explicit app classification changes through Patina MCP or its localhost HTTP API.
---

# Analyzing Patina Activity

## Overview

Analyze Patina's local activity data without inventing missing context or exposing more private detail than the task needs. Prefer the configured MCP server; use direct HTTP when MCP is unavailable or the task needs an endpoint not exposed as an MCP tool.

## Select a Transport

1. Read [references/analysis.md](references/analysis.md) for interpretation and safety rules on every task.
2. If Patina MCP tools are available, read [references/mcp.md](references/mcp.md).
3. Otherwise, or when the MCP tool set lacks the required query, read [references/http.md](references/http.md).
4. Do not switch transports merely to bypass authentication, privacy settings, or missing user consent.

## Workflow

1. Check diagnostics before trusting activity conclusions.
2. Use the AI activity context for a fast overview when available.
3. Drill into summaries, sessions, trends, or web activity only as needed.
4. State the time range and whether current active time is included.
5. Separate recorded facts from interpretations about intent, focus, or productivity.
6. Execute app classification, rename, exclusion, tracker, audio, browser-bridge, reminder, timer, or pomodoro writes only on explicit user intent; report exactly what changed. Treat browser port, Token, and URL privacy replacement as a high-impact configuration change.

## Quick Reference

| Need | Preferred operation |
|---|---|
| Tracking or browser health | diagnostics |
| Audio/browser configuration | sanitized runtime settings |
| Current foreground window | current activity |
| Current running segment | active session |
| Today or this week | summary |
| Week/month comparison | trend |
| Exact custom range | HTTP summary range |
| Browser domain/title detail | web activity |
| Timer/reminder/pomodoro state | Tools snapshot |
| Create or control reminders/timers/pomodoro | explicit Tools write tools |
| App names/categories/exclusions | app list and explicit write tools |
| Idle threshold or tracking pause | explicit tracker setting tools |
| Audio participation or browser bridge configuration | explicit runtime setting tools |

## Common Mistakes

- Do not add active-session duration to today/week summaries; those summaries already include it.
- Do not treat closed-session queries as complete realtime totals.
- Do not assume missing web activity means no browser use; inspect bridge diagnostics first.
- Do not print, log, summarize, or persist API or browser extension bearer tokens.
- Do not expose full URLs or window titles unless they are needed for the user's request.
- Do not infer consent to create or control Tools state from a request that only asks to inspect or analyze it.
