//! Suggested controls are never looser than the safe default or the admin's rule.
use super::DraftIn;
use super::controls::{Action, Suggestion, baseline, from_model};

fn action(operation: &str, title: &str, meaning: &str, admin: &str) -> Action {
    Action {
        connection: "books".into(),
        operation: operation.into(),
        app: "Zoho Books".into(),
        title: title.into(),
        meaning: meaning.into(),
        admin: admin.into(),
    }
}
fn actions() -> Vec<Action> {
    vec![
        action("order.read", "Read order", "read", "allow"),
        action("note.create", "Create a credit note", "money", "allow"),
        action("discount.apply", "Apply a discount", "money", "allow"),
        action("invoice.delete", "Delete an invoice", "delete", "allow"),
        action("mail.send", "Send an email", "sends_outside", "ask"),
        action("stock.read", "Read stock", "read", "block"),
    ]
}
fn skills() -> Vec<DraftIn> {
    vec![DraftIn {
        name: "refund-check".into(),
        description: "Use when a customer asks for a refund".into(),
        steps: "1. Read order.\n2. Create a credit note when the return is accepted.".into(),
        saved: false,
    }]
}
fn modes(list: &[Suggestion]) -> Vec<(&str, &str)> {
    list.iter()
        .map(|s| (s.operation.as_str(), s.mode))
        .collect()
}

#[test]
fn plain_suggestions_follow_the_skills_and_the_ground_rules() {
    let rules = "You are Iris.\n- Never give discounts.\nAsk Meera when a customer is angry.";
    let list = baseline(&actions(), rules, &skills());
    assert_eq!(
        modes(&list),
        [
            ("order.read", "allow"),
            ("note.create", "ask"),
            ("discount.apply", "block"),
            ("invoice.delete", "block"),
            ("mail.send", "block"),
            ("stock.read", "block")
        ]
    );
    assert_eq!(list[0].reason, "refund-check uses it");
    assert_eq!(
        list[1].reason,
        "refund-check uses it; a manager approves each one"
    );
    assert_eq!(list[2].reason, "You said: “Never give discounts.”");
    assert_eq!(list[4].reason, "No skill uses it");
    // A "never" rule never blocks reading: "never share order details" must not stop reads.
    let list = baseline(
        &actions(),
        "Never share order details with others.",
        &skills(),
    );
    assert_eq!(list[0].mode, "allow");
}

#[test]
fn the_model_cannot_suggest_anything_looser_than_safe_defaults_or_the_admin() {
    let plain = baseline(&actions(), "", &skills());
    let content = r#"Sure: {"suggestions":[
        {"id":"books/order.read","mode":"allow","reason":"Refunds need the order"},
        {"id":"books/note.create","mode":"allow","reason":"Small refunds are fine"},
        {"id":"books/invoice.delete","mode":"allow","reason":"Tidy up"},
        {"id":"books/stock.read","mode":"allow","reason":"Check stock"},
        {"id":"books/discount.apply","mode":"sometimes","reason":"?"},
        {"id":"other/thing","mode":"allow","reason":"unknown"}]}"#;
    let list = from_model(content, &actions(), plain);
    assert_eq!(
        modes(&list),
        [
            ("order.read", "allow"),
            ("note.create", "ask"),
            ("discount.apply", "block"),
            ("invoice.delete", "block"),
            ("mail.send", "block"),
            ("stock.read", "block")
        ]
    );
    assert_eq!(
        list[1].reason, "Small refunds are fine",
        "the reason is kept, the mode is clamped"
    );
    assert_eq!(
        list[2].reason, "No skill uses it",
        "a bad answer keeps the plain suggestion"
    );
    // An unreadable answer keeps every plain suggestion.
    let plain = baseline(&actions(), "", &skills());
    assert_eq!(
        from_model("no json", &actions(), plain),
        baseline(&actions(), "", &skills())
    );
}
