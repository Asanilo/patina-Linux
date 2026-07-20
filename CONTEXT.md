# Patina

Canonical product language for Patina's local runtime and clients. Product and architecture rules remain in the active documents under `docs/`.

## Language

**Patina Product**:
The complete local-first Linux desktop time-tracking system, including its runtime, clients, integrations, and local data.
_Avoid_: Patina Desktop when referring to the whole system

**Patina Desktop**:
The graphical desktop client of the Patina product; it is not the long-term runtime owner.
_Avoid_: Patina when the distinction from the whole product matters

**`patinad`**:
The local background runtime intended to be the sole runtime owner for a profile.
_Avoid_: desktop backend, second tracker

**Runtime Owner**:
The single active authority allowed to advance tracking state and write live activity for one profile.
_Avoid_: client, UI process

**Client**:
A product surface that reads runtime state or requests supported operations without owning tracking.
_Avoid_: runtime owner, independent tracker

**Profile**:
An isolated local boundary for data, settings, credentials, and runtime ownership.
_Avoid_: account, workspace

**Product Package**:
The installable release unit that delivers mutually compatible Patina product components together.
_Avoid_: desktop package when referring to the complete installation

**Background Tracking at Login**:
The preference that starts the runtime owner for a user session without requiring a client to open.
_Avoid_: launch Patina, start minimized

**Desktop Launch at Login**:
The separate preference that opens Patina Desktop for a user session; it does not determine whether background tracking runs.
_Avoid_: background tracking, daemon autostart

**Local Browser UI**:
A graphical client served on loopback by `patinad`; it is neither a public website nor a runtime owner.
_Avoid_: cloud dashboard, hosted web app, second backend
