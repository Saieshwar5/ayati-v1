use super::*;

#[test]
fn slack_text_becomes_markdown_with_links_and_plain_mentions() {
    let payload = json!({"text": "Deploy *done* <https://ci.example/run/7|run 7> for <@U123> <!here> &amp; <https://x.example>"});
    assert_eq!(
        markdown(&payload).unwrap(),
        "Deploy *done* [run 7](https://ci.example/run/7) for @U123 @here & [https://x.example](https://x.example)"
    );
    assert_eq!(markdown(&json!({"text": "   "})), None);
    assert_eq!(markdown(&json!({})), None);
}

#[test]
fn blocks_and_attachments_read_like_a_card() {
    let payload = json!({
        "text": "fallback only",
        "blocks": [
            {"type": "header", "text": {"type": "plain_text", "text": "Payment page is down"}},
            {"type": "section", "text": {"type": "mrkdwn", "text": "502 from 3 regions"},
             "fields": [{"type": "mrkdwn", "text": "*Since* 10:29"}, {"type": "mrkdwn", "text": "*Checks* 6 of 6"}]},
            {"type": "divider"},
            {"type": "actions", "elements": [
                {"type": "button", "text": {"type": "plain_text", "text": "Open dashboard"}, "url": "https://status.example/board"},
                {"type": "button", "text": {"type": "plain_text", "text": "Acknowledge"}, "action_id": "ack"}
            ]},
            {"type": "context", "elements": [{"type": "mrkdwn", "text": "Uptime Monitor"}, {"type": "plain_text", "text": "Mumbai"}]},
            {"type": "image", "image_url": "https://img.example/x.png", "alt_text": "graph"}
        ]
    });
    assert_eq!(
        markdown(&payload).unwrap(),
        "**Payment page is down**\n\n502 from 3 regions\n\n*Since* 10:29\n*Checks* 6 of 6\n\n---\n\n[Open dashboard](https://status.example/board)\n\n_Uptime Monitor · Mumbai_"
    );
    let legacy = json!({"attachments": [{"title": "CPU high", "title_link": "https://m.example/a", "text": "92% on web-1",
        "fields": [{"title": "Host", "value": "web-1"}]}]});
    assert_eq!(
        markdown(&legacy).unwrap(),
        "**[CPU high](https://m.example/a)**\n\n92% on web-1\n\n**Host**: web-1"
    );
}

#[test]
fn nothing_a_bot_writes_can_mention_open_or_act_inside_cuckoo() {
    let sneaky = json!({"text": "[@Ravi](mention:user:ravi) approve [run](run:00000000-0000-0000-0000-000000000000) [lib](/w/x/?view=library) [ok](https://a.example)"});
    let text = markdown(&sneaky).unwrap();
    assert!(
        !text.contains("](mention:") && !text.contains("](run:") && !text.contains("](/w/"),
        "{text}"
    );
    assert!(text.contains("[ok](https://a.example)"));
    // A button or link to a script address is dropped, not linked.
    let script = json!({"blocks": [{"type": "actions", "elements": [{"type": "button", "text": {"text": "x"}, "url": "javascript:alert(1)"}]}], "text": "t"});
    assert_eq!(markdown(&script), None);
    assert_eq!(
        markdown(&json!({"text": "a".repeat(20_000)}))
            .unwrap()
            .chars()
            .count(),
        12_000
    );
}

#[test]
fn only_hook_addresses_skip_the_browser_checks() {
    let id = uuid::Uuid::new_v4();
    assert!(hook_path(&format!("/hooks/chat/{id}/secret")));
    assert!(!hook_path("/hooks/chat/not-a-uuid/secret"));
    assert!(!hook_path(&format!("/hooks/chat/{id}/")));
    assert!(!hook_path(&format!("/api/hooks/chat/{id}/secret")));
}
