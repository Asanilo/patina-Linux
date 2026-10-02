# patina-client

Independent Rust loopback HTTP/SSE transport used by the existing Desktop facade
and future native clients. It has no Tauri, GTK, SQLite or tracking dependency.
The initial extraction shares capabilities, protocol version and error envelopes
from the serde-only `patina-protocol` crate;
the remaining domain-specific typed methods are still in the Desktop facade.
This is a transport foundation, not a complete multi-client SDK or a TUI.

The host supplies the port and credential. Requests only target `127.0.0.1`, do
not use environment proxies or follow redirects, have time and response-size
limits, and never automatically retry writes. Negotiate capabilities before
using runtime data. SSE parsing has a bound on incomplete frames; domain event
validation and reconnect/snapshot coordination remain in the existing runtime
adapter until their dedicated migration.

```bash
cargo test --manifest-path crates/patina-client/Cargo.toml --locked
cargo clippy --manifest-path crates/patina-client/Cargo.toml --locked --all-targets -- -D warnings
cargo run --manifest-path crates/patina-client/Cargo.toml --example inspect -- <port> <token-file> --watch
```

Use an explicit isolated daemon for development. The `inspect` example reads an
owner-only credential file, negotiates, and optionally prints event names and
cursors; it does not print activity bodies or modify data. It does not reconnect
automatically and must not be presented as the planned TUI.

`npm run check:full` includes these checks. This crate retains an independent
lockfile so it can be built without the Desktop dependency graph. No complete
workspace rearrangement or separate release channel is introduced.

For an explicit process-level probe, first build `--example inspect`, then run
`python3 scripts/acceptance/independent-client.py /absolute/path/patinad /absolute/path/inspect`.
It creates a new temporary Local profile with hardware integrations disabled,
starts two independent SDK processes, and checks their shared classification
event. It neither installs a package nor uses the Production profile. Evidence
records the exact daemon and probe hashes; it is not GUI or hardware acceptance.
