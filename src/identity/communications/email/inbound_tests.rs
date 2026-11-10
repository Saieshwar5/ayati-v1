use super::*;
use serde_json::{Value, json};

/// A reply from Priya, shaped like Postmark's documented inbound example.
fn sample() -> Value {
    json!({
        "MessageID": "22c74902-a0c1-4511-804f-341342852c90",
        "From": "Priya@Acme.com",
        "FromFull": {"Email": "Priya@Acme.com", "Name": "Priya", "MailboxHash": ""},
        "To": "research-k7q2@agents.cuckoo.test",
        "ToFull": [{"Email": "research-k7q2@agents.cuckoo.test", "Name": "", "MailboxHash": ""}],
        "CcFull": [],
        "OriginalRecipient": "research-k7q2@agents.cuckoo.test",
        "Subject": "Re: Onboarding steps",
        "TextBody": "Please add the laptop step.\n\nOn Tue, 10 Nov 2026 at 09:00, Research wrote:\n> Here are the steps.",
        "StrippedTextReply": "",
        "HtmlBody": "<p>Please add the laptop step.</p>",
        "Headers": [
            {"Name": "Received-SPF", "Value": "Pass (sender SPF authorized) identity=mailfrom; client-ip=209.85.160.180; helo=mail.acme.com; envelope-from=bounces@acme.com; receiver=x@inbound.postmarkapp.com"},
            {"Name": "X-Spam-Tests", "Value": "DKIM_SIGNED,DKIM_VALID,DKIM_VALID_AU,SPF_PASS"},
            {"Name": "Message-ID", "Value": "<reply-2@acme.com>"},
            {"Name": "In-Reply-To", "Value": "<agent-1@agents.cuckoo.test>"},
            {"Name": "References", "Value": "<root-0@acme.com> <agent-1@agents.cuckoo.test>"}
        ],
        "Attachments": []
    })
}
fn parsed(v: &Value) -> Parsed {
    parse(&serde_json::to_vec(v).unwrap()).unwrap()
}
fn with_headers(headers: Value) -> Value {
    let mut v = sample();
    v["Headers"] = headers;
    v
}

#[test]
fn a_reply_keeps_its_thread_and_loses_its_quotes() {
    let p = parsed(&sample());
    assert_eq!(p.event, "22c74902-a0c1-4511-804f-341342852c90");
    assert_eq!(p.from, "priya@acme.com");
    assert_eq!(p.to, "research-k7q2@agents.cuckoo.test");
    assert!(!p.others && p.authenticated && !p.automated && !p.attachments);
    assert_eq!(p.message_id, "<reply-2@acme.com>");
    assert_eq!(p.root, "<root-0@acme.com>");
    assert_eq!(
        p.references,
        [
            "<root-0@acme.com>",
            "<agent-1@agents.cuckoo.test>",
            "<reply-2@acme.com>"
        ]
    );
    assert_eq!(p.text.as_deref(), Some("Please add the laptop step."));
    assert_eq!(p.subject, "Re: Onboarding steps");
}

#[test]
fn a_new_email_starts_its_own_thread() {
    let p = parsed(&with_headers(json!([
        {"Name": "X-Spam-Tests", "Value": "DKIM_VALID_AU"},
        {"Name": "Message-ID", "Value": "<new-1@acme.com>"}
    ])));
    assert_eq!(p.root, "<new-1@acme.com>");
    assert_eq!(p.references, ["<new-1@acme.com>"]);
    // Without a Message-ID, Postmark's own ID stands in.
    let p = parsed(&with_headers(
        json!([{"Name": "X-Spam-Tests", "Value": "DKIM_VALID_AU"}]),
    ));
    assert_eq!(
        p.root,
        "<22c74902-a0c1-4511-804f-341342852c90@postmark.invalid>"
    );
}

#[test]
fn postmarks_stripped_reply_wins_when_present() {
    let mut v = sample();
    v["StrippedTextReply"] = json!("  Just the new line.  ");
    assert_eq!(parsed(&v).text.as_deref(), Some("Just the new line."));
}

#[test]
fn only_the_senders_own_domain_vouches_for_it() {
    let spf = |value: &str| {
        parsed(&with_headers(
            json!([{"Name": "Received-SPF", "Value": value}]),
        ))
        .authenticated
    };
    assert!(spf("pass (ok) envelope-from=bounces@acme.com; receiver=x"));
    assert!(spf("Pass envelope-from=\"Bounces@ACME.com\";"));
    assert!(!spf("Pass envelope-from=bounces@mailer.example;"));
    assert!(!spf("SoftFail envelope-from=bounces@acme.com;"));
    assert!(!spf("Fail envelope-from=bounces@acme.com;"));
    let dkim = |value: &str| {
        parsed(&with_headers(
            json!([{"Name": "X-Spam-Tests", "Value": value}]),
        ))
        .authenticated
    };
    assert!(dkim("DKIM_SIGNED, DKIM_VALID_AU"));
    // A valid signature from some other domain says nothing about the sender.
    assert!(!dkim("DKIM_SIGNED,DKIM_VALID"));
    assert!(!parsed(&with_headers(json!([]))).authenticated);
}

#[test]
fn a_forged_header_below_postmarks_is_not_believed() {
    let p = parsed(&with_headers(json!([
        {"Name": "Received-SPF", "Value": "Fail envelope-from=bounces@acme.com;"},
        {"Name": "X-Spam-Tests", "Value": "SPF_FAIL"},
        {"Name": "Received-SPF", "Value": "Pass envelope-from=bounces@acme.com;"},
        {"Name": "X-Spam-Tests", "Value": "DKIM_VALID_AU"}
    ])));
    assert!(!p.authenticated);
}

#[test]
fn robots_are_recognised() {
    for header in [
        json!({"Name": "Auto-Submitted", "Value": "auto-replied"}),
        json!({"Name": "Precedence", "Value": "Bulk"}),
        json!({"Name": "List-Id", "Value": "<team.acme.com>"}),
        json!({"Name": "List-Unsubscribe", "Value": "<mailto:x@acme.com>"}),
        json!({"Name": "X-Autoreply", "Value": "yes"}),
        json!({"Name": "Return-Path", "Value": "<>"}),
    ] {
        assert!(
            parsed(&with_headers(json!([header.clone()]))).automated,
            "{header}"
        );
    }
    assert!(
        !parsed(&with_headers(
            json!([{"Name": "Auto-Submitted", "Value": "no"}])
        ))
        .automated
    );
    for sender in [
        "MAILER-DAEMON@acme.com",
        "noreply@acme.com",
        "no-reply-x@acme.com",
    ] {
        let mut v = sample();
        v["FromFull"]["Email"] = json!(sender);
        assert!(parsed(&v).automated, "{sender}");
    }
}

#[test]
fn other_recipients_make_it_shared() {
    let mut v = sample();
    v["CcFull"] = json!([{"Email": "raj@acme.com", "Name": "Raj"}]);
    assert!(parsed(&v).others);
    let mut v = sample();
    v["ToFull"] = json!([{"Email": "team@acme.com"}]);
    assert!(parsed(&v).others, "a Bcc copy addressed to someone else");
}

#[test]
fn quoted_history_is_left_out() {
    assert_eq!(new_text("Thanks!\n\n> old\n> older"), "Thanks!");
    assert_eq!(
        new_text("Yes.\nOn Mon, 9 Nov 2026, Research\n<r@a.test> wrote:\nold"),
        "Yes."
    );
    assert_eq!(
        new_text("Sure\n-----Original Message-----\nFrom: x"),
        "Sure"
    );
    assert_eq!(
        new_text("Ok\n________________________________\nFrom: Research"),
        "Ok"
    );
    assert_eq!(new_text("> only quotes"), "");
}

#[test]
fn nothing_to_read_and_long_mail() {
    let mut v = sample();
    v["TextBody"] = json!("");
    v["Attachments"] = json!([{"Name": "a.pdf"}]);
    let p = parsed(&v);
    assert_eq!(p.text, None);
    assert!(p.attachments);
    assert!(admit(&p).is_err());
    let mut v = sample();
    v["TextBody"] = json!("é".repeat(10_000));
    let text = parsed(&v).text.unwrap();
    assert!(text.len() <= 12_000 && text.ends_with(CUT));
}

#[test]
fn bad_payloads_are_refused() {
    assert!(parse(b"{}").is_err());
    let mut v = sample();
    v["MessageID"] = json!("not an id!");
    assert!(parse(&serde_json::to_vec(&v).unwrap()).is_err());
    let mut v = sample();
    v["FromFull"]["Email"] = json!("not-an-address");
    assert!(parse(&serde_json::to_vec(&v).unwrap()).is_err());
}

#[test]
fn ids_and_subjects() {
    assert_eq!(
        ids("<a@x> <b@x>\r\n <a@x> <bad id@x> <>"),
        ["<a@x>", "<b@x>"]
    );
    assert_eq!(reply_subject("Plan"), "Re: Plan");
    assert_eq!(reply_subject("RE: Plan"), "RE: Plan");
    assert_eq!(reply_subject(""), "Re: your message");
    assert_eq!(subject("a\r\nb"), "a  b");
}
