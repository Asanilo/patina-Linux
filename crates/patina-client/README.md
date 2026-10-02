# patina-client

Independent Rust loopback HTTP/SSE transport used by the existing Desktop facade
and future native clients. It has no Tauri, GTK, SQLite or tracking dependency.
The initial extraction shares capabilities, protocol version and error envelopes
from the serde-only `patina-protocol` crate;
runtime event and alert types also share that protocol source. The remaining
domain-specific typed methods are still in the Desktop facade. This is a client
foundation, not a complete multi-client SDK or a TUI.

`classification_snapshot()` reads a bounded configuration snapshot with a content
revision. `commit_classification(revision, mutations)` negotiates the
`classification-conditional` capability and uses its separate conditional endpoint.
An old server cannot silently treat this as an unconditional write. Conflicts
return HTTP 409 to the caller without automatic retry or rebase; a successful
commit returns the new revision. Configuration reads expose only the documented
classification namespaces, never arbitrary settings or credentials. These are
configuration wire types; classification rules and activity read models still
need further migration to the backend.

The host supplies the port and credential. Requests only target `127.0.0.1`, do
not use environment proxies or follow redirects, have time and response-size
limits, and never automatically retry writes. Negotiate capabilities before
using runtime data. SSE parsing bounds incomplete frames and validates typed
event names and sequence IDs. `state::ClientState` atomically versions connection
configuration; `sync::SnapshotSession` supplies cancellable negotiation,
subscription-before-snapshot, refresh, reconnect and replay-gap handling. The
Desktop adapter uses this coordinator with its own snapshot reader and output.

The coordinator checks the event-hub response identity before consuming replay.
A new instance or legacy server without an identity causes snapshot recovery
without replaying old alerts. A reconnect/gap notifies the output to invalidate
all dependent read models. Output callbacks are synchronous; do not change the
configuration or send shutdown inside a callback. Schedule those actions after
the callback returns, so they cannot deadlock the publication guard.

```bash
cargo test --manifest-path crates/patina-client/Cargo.toml --locked
cargo clippy --manifest-path crates/patina-client/Cargo.toml --locked --all-targets -- -D warnings
cargo run --manifest-path crates/patina-client/Cargo.toml --example inspect -- <port> <token-file> --watch
```

Use an explicit isolated daemon for development. The `inspect` example reads an
owner-only credential file, negotiates, and optionally prints event names and
cursors; it does not print activity bodies or modify data. Watch mode uses the
same reconnect/snapshot coordinator as Desktop and stops on Ctrl-C. It must not
be presented as the planned TUI.

`npm run check:full` includes these checks. This crate retains an independent
lockfile so it can be built without the Desktop dependency graph. No complete
workspace rearrangement or separate release channel is introduced.

For an explicit process-level probe, first build `--example inspect`, then run
`python3 scripts/acceptance/independent-client.py /absolute/path/patinad /absolute/path/inspect`.
It creates a new temporary Local profile with hardware integrations disabled,
starts two independent SDK processes, and checks their shared classification
event. It neither installs a package nor uses the Production profile. Evidence
records the exact daemon and probe hashes; it is not GUI or hardware acceptance.
