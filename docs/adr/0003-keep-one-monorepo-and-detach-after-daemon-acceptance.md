---
status: accepted
---

# Keep one monorepo and detach after daemon acceptance

Patina Desktop, `patinad`, the local browser UI, extensions, MCP, and future TUI remain in one repository because they share a versioned protocol, schema, release, and product identity; `patinad` will not become a separate repository. The repository will leave the upstream Windows fork network after the first daemon-backed package passes beta acceptance, preserving Git history, MIT licensing, and attribution. Cargo workspace extraction is deferred until measured build, binary, resource, or independent-package needs justify it, so runtime-owner migration is not combined with a broad source-tree rewrite.
