---
status: accepted
---

# Keep one monorepo; defer fork identity changes

Patina Desktop, `patinad`, the local browser UI, extensions, MCP, and future TUI remain in one repository because they share a versioned protocol, schema, release, and product identity; `patinad` will not become a separate repository. Cargo workspace extraction is deferred until measured build, binary, resource, or independent-package needs justify it.

Amended 2026-09-19: the original automatic fork detachment after beta acceptance is superseded. Product architecture and GitHub repository identity are separate decisions. Keep the current fork, Git history, MIT licensing and attribution unless a later explicit decision changes identity. A future branch based on current upstream may contribute selected Linux support without proposing the daemon architecture. Upstream acceptance does not block independent daemon/client development, and maintaining two complete products in lockstep is not a goal. The filename is retained for existing links.
