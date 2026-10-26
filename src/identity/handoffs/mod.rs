pub(crate) mod model;

#[cfg(test)]
mod tests {
    use super::model::Request;
    #[test]
    fn handoff_input_has_no_caller_selected_principal_parent_destination_or_extra_authority() {
        let value = serde_json::json!({"key":"supplier-review","agent":"11111111-1111-4111-8111-111111111111","task":"Review the current facts."});
        serde_json::from_value::<Request>(value.clone())
            .unwrap()
            .validate()
            .unwrap();
        for field in ["requester", "parent", "workspace", "chat", "rules"] {
            let mut forged = value.clone();
            forged[field] = serde_json::json!("forged");
            assert!(serde_json::from_value::<Request>(forged).is_err());
        }
        let mut oversized = serde_json::from_value::<Request>(value).unwrap();
        oversized.task = "x".repeat(4001);
        assert!(oversized.validate().is_err());
    }
}
