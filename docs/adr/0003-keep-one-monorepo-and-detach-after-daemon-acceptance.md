---
status: accepted
---

# Keep one monorepo; defer fork identity changes

Patina Desktop, `patinad`, the local browser UI, extensions, MCP, and future TUI remain in one repository because they share a versioned protocol, schema, release, and product identity; `patinad` will not become a separate repository. Cargo workspace extraction is deferred until measured build, binary, resource, or independent-package needs justify it.

Amended 2026-09-19: the original automatic fork detachment after beta acceptance is superseded. Product architecture and GitHub repository identity are separate decisions. Keep the current fork, Git history, MIT licensing and attribution unless a later explicit decision changes identity. A future branch based on current upstream may contribute selected Linux support without proposing the daemon architecture. Upstream acceptance does not block independent daemon/client development, and maintaining two complete products in lockstep is not a goal. The filename is retained for existing links.

Clarified 2026-09-20: this repository is `Asanilo/patina-Linux`; `main` remains its primary Linux development/product branch. `feature/patinad-daemon` is the separation experiment, not an automatic replacement for `main`. A future upstream Linux contribution branch starts from a recorded current upstream commit in the Linux repository, using suitable `main` code as a reference or rewriting it for upstream's current boundaries. Its support matrix is a separate agreement from the fork's current Debian-family validation.

Implementation sequence accepted 2026-09-20: concentrate on the daemon gap audit,
blocking fixes and acceptance, then converge into `main` when the documented
merge criteria pass. During this interval, prioritize necessary product fixes on
`main` and forward them into the daemon candidate. The upstream contribution
branch `feat/linux-desktop`, based on `80204c73`, now exists but its unfinished
draft is on hold. Reuse suitable validated Linux modules selectively after
convergence; do not merge the upstream-based product history into the fork.
This sequencing does not waive release gates, authorize production service
changes, or imply that daemon acceptance is complete. The active Todo and
evidence remain in the daemon working checklist.

Converged 2026-09-21 with explicit user authorization: local `main` fast-forwarded
from `a13a64a6` to the accepted daemon candidate `bc5c2e9c`, preserving all existing
development and beta history. The experiment branch is retained as merged history;
ongoing daemon and Desktop product work now shares `main`. This supersedes the
temporary two-branch development arrangement above. Upstream work remains separate,
and source integration does not publish a release or waive the DEB-only beta and
AppImage acceptance gates.
