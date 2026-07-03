pub mod backup;
pub mod product_identity;
pub mod settings;
pub mod storage;
pub mod tools;
pub mod tracking;
pub mod update;
pub mod web_activity;
pub mod widget;

#[cfg(test)]
mod product_identity_contract {
    #[test]
    fn keeps_window_display_names_consistent() {
        assert_eq!(crate::domain::product_identity::DISPLAY_NAME, "Patina Linux");
        assert_eq!(
            crate::domain::product_identity::WIDGET_DISPLAY_NAME,
            "Patina Linux Widget"
        );
    }
}
