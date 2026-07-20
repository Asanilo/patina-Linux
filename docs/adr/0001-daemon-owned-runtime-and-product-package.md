---
status: accepted
---

# Use one daemon-owned runtime and one product package

`patinad` will become the sole runtime owner for each profile, while Patina Desktop becomes a client and never starts an automatic embedded-tracker fallback. The first daemon-backed DEB will install the desktop client, daemon, and user service as one product package so protocol and data expectations upgrade atomically; the service unit is enabled in the user's session during first-launch migration rather than globally from the package installer. That beta does not publish an AppImage because a movable image cannot safely own a persistent service until versioned extraction and atomic update semantics are designed. A daemon failure is surfaced and repaired explicitly because silently switching owners would reintroduce split-brain tracking and unreliable duration data.
