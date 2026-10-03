//! Explicit export surface; no file writes, runtime startup or environment config.
use patina_protocol::{
    configuration::*, icons::*, product_settings::*, web_history::WebActivityUrlPrivacyMode,
};
use ts_rs::{Config, TS};

fn main() {
    // JSON uses numbers, not BigInt. Receivers must still reject unsafe integers.
    let config = Config::new().with_large_int("number");
    println!("// Generated from patina-protocol. Run npm run generate:protocol; do not edit.");
    println!("// Wire types only: untrusted input still requires runtime validation.\n");
    macro_rules! export {
        ($($ty:ty),+ $(,)?) => { $(println!("export {}\n", <$ty>::decl(&config));)+ };
    }
    export!(
        WebActivityUrlPrivacyMode,
        ProductSettings,
        ProductSettingsSnapshot,
        ProductSettingsPatch,
        ProductSettingsCommitRequest,
        ClassificationEntry,
        ClassificationSnapshot,
        ClassificationMutationRequest,
        ClassificationMutationsRequest,
        ClassificationCommitResult,
        CachedIcon,
        IconPage,
        IconLookup,
    );
}
