---
status: accepted
---

# Standardize loopback HTTP and authentication

Stage 2F.2 replaced the custom API/SSE and browser-bridge HTTP parsers and server loops with Axum and Tower while preserving Patina's existing domain handlers, DTOs, API surfaces, and separate browser-extension bridge policy. MCP, CLI, and Agent integrations continue to use the owner-only Bearer Token, while the future local browser UI uses a same-origin HttpOnly session and never receives the long-lived token. The API accepts only loopback origins when an Origin header is present; the separate bridge accepts Firefox/Zen and Chromium extension origins. Neither transport returns wildcard CORS. This bounded migration avoids maintaining bespoke Web servers as static UI, sessions, concurrency control, and additional clients are added.
