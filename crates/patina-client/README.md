# patina-client

Independent Rust loopback HTTP/SSE transport used by the existing Desktop facade
and future native clients. It has no Tauri, GTK, SQLite or tracking dependency.
The initial extraction shares capabilities, protocol version and error envelopes
from the serde-only `patina-protocol` crate;
runtime event and alert types also share that protocol source. Typed product
reads, conditional settings writes and Tools now live here; some maintenance
and host-specific methods remain in the Desktop facade. This is a client
foundation, not a complete multi-client SDK or a TUI.

`classification_snapshot()` reads a bounded configuration snapshot with a content
revision. `commit_classification(revision, mutations)` negotiates the
`classification-conditional` capability and uses its separate conditional endpoint.
An old server cannot silently treat this as an unconditional write. Conflicts
return HTTP 409 to the caller without automatic retry or rebase; a successful
commit returns the new revision. Configuration reads expose only the documented
classification namespaces, never arbitrary settings or credentials. These are
configuration wire types; product activity responses below already apply backend
classification rules. Remaining migrations are tracked in the active stage plan.

`daily_product(from, to, language)` reads host-local daily totals with final
product categories, transaction-consistent display-name overrides, and a
classification configuration revision. This uses its own development endpoint;
old runtimes fail explicitly, without falling back to a legacy projection.
The client validates totals and identity completeness. Consumers must not apply
local classification or exclusion rules to these confirmed totals again.
The snapshot also carries validated `tracking_health` and a backend live cutoff:
stale owner heartbeats stop open-session growth, while closed facts retain their
stored boundaries. Consumers must not extrapolate these totals with local clocks.
Presentation labels and colors remain client concerns.

`dashboard(date, language)` returns one selected/previous-day product snapshot and
24 host-local display-hour quantities. It validates hourly totals against final
product categories. Missing/repeated DST hours retain accurate day totals, and
imported bucket quantities must not be presented as exact observed intervals.
The existing Desktop Dashboard command delegates to this method; its frontend
formats confirmed quantities and refreshes from the owner instead of extrapolating
with a local clock. Exact History and cached icons have separate typed methods.

`exact_history(from_ms, to_ms, language)` reads precise native/imported fragments
with final classification and clipped title samples. It never includes hour
buckets, fabricates dated observations from record captions, or extends open rows
past owner liveness. Source IDs require their origin; imported facts can produce
multiple fragments. The SDK validates the response within an 8 MiB budget and
rejects unsupported endpoints instead of using legacy sessions or local SQLite.
Desktop History/Details consume these owner facts. `history_product` additionally
supplies backend display-hour quantities for History.

`tools_snapshot()` and the typed reminder, software-reminder, timer and Pomodoro
methods use `protocol::tools` wire types. `tools_action(ToolsAction)` selects an
explicit action rather than accepting an arbitrary path. Writes require a
compatible tracking daemon advertising ready Tools ownership and the `tools`
write scope. These methods preserve the existing three-second request and 64 KiB
response limits; enum/required-field decoding errors and oversized responses are
errors, never fallback triggers. An error can follow a committed action: refresh
the snapshot before deciding what to do, and never blindly repeat a lap or start.

Tools scheduling and phase transitions remain in the backend; wire DTOs contain
no clock or storage algorithm. Subscribe before reading, refresh on
`tools-runtime-changed`, and use the existing session coordinator for reconnect
and gaps. New live subscriptions must not replay old alerts. Desktop uses these
same SDK methods through a mechanical domain/wire conversion; the frontend keeps
its runtime validators and consumes generated wire types. This does not deliver
a new Tools UI or change the native notification owner.

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
