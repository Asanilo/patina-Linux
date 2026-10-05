//! Identity of this compiled artifact. Protocol and database schemas evolve separately.
//! Desktop/legacy bundles use their product version; standalone builds use daemon/VERSION.
pub const ARTIFACT_VERSION: &str = env!("PATINA_ARTIFACT_VERSION");

#[cfg(test)]
#[path = "../../build_support/artifact_version.rs"]
mod version_selection;
