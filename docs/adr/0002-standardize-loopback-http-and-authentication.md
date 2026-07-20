---
status: accepted
---

# Standardize loopback HTTP and authentication

Stage 2F.1 will replace the custom HTTP parser, server loops, and SSE transport with Axum and Tower while preserving Patina's existing domain handlers, DTOs, API surfaces, and separate browser-extension bridge policy. MCP, CLI, and Agent integrations continue to use the owner-only Bearer Token, while the future local browser UI uses a same-origin HttpOnly session and never receives the long-lived token; strict origin and loopback host checks replace permissive API CORS. This accepts a bounded migration now to avoid maintaining a bespoke Web server as static UI, sessions, concurrency control, and additional clients are added.
