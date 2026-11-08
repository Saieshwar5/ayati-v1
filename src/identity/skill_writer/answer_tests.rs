//! Reading the writer's answer into drafts people can save.
use super::answer::{read, slug};
use super::{DraftIn, LibrarySkill, Message, Request, WriterContext};

fn request(skills: Vec<DraftIn>) -> Request {
    Request {
        name: "Iris".into(),
        role: "Customer support".into(),
        agent: None,
        connections: vec![],
        ground_rules: String::new(),
        skills,
        messages: vec![Message {
            role: "person".into(),
            text: "Answer order questions.".into(),
        }],
        focus: None,
    }
}
fn context() -> WriterContext {
    WriterContext {
        names: vec!["quote-builder".into(), "refund-check".into()],
        library: vec![LibrarySkill {
            id: "s1".into(),
            name: "complaint-reply".into(),
            description: "Use when a customer complains".into(),
        }],
        apps: vec![],
    }
}

#[test]
fn names_become_valid_skill_names() {
    assert_eq!(slug("Refund check"), "refund-check");
    assert_eq!(
        slug("  Delivery: date / question!! "),
        "delivery-date-question"
    );
    assert_eq!(slug("--a__b--"), "a-b");
    assert_eq!(slug("रिफंड"), "");
    assert_eq!(slug(&"x".repeat(80)).len(), 64);
}

#[test]
fn drafts_pass_the_upload_checks_and_never_take_a_used_name() {
    let content = "Here you go:\n```json\n{\"reply\":\"Wrote two skills.\",\"ground_rules\":\"  You are Iris.\\nNever give discounts.  \",\
        \"skills\":[{\"name\":\"Refund Check\",\"description\":\"Use when a customer\\n asks for a refund\",\"steps\":\"1. Find the order.\\n2. Check the date.\"},\
        {\"name\":\"delivery-date\",\"description\":\"Use when someone asks when an order arrives\",\"steps\":\"1. Track it.\"},\
        {\"name\":\"empty\",\"description\":\"\",\"steps\":\"1. Nothing\"}],\
        \"remove\":[\"old-draft\",\"someone-else\"],\"reuse\":[\"Complaint reply\",\"unknown\"],\"missing_apps\":[\"Shiprocket\"]}\n```";
    let own = vec![DraftIn {
        name: "old-draft".into(),
        description: "Use when".into(),
        steps: "1.".into(),
        saved: false,
    }];
    let answer = read(content, &request(own), &context()).unwrap();
    assert_eq!(answer.reply, "Wrote two skills.");
    assert_eq!(
        answer.ground_rules.as_deref(),
        Some("You are Iris.\nNever give discounts.")
    );
    // refund-check exists in the workspace, so the new one gets the agent's name.
    let names: Vec<&str> = answer.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["refund-check-iris", "delivery-date"]);
    let refund = &answer.skills[0];
    assert_eq!(refund.description, "Use when a customer asks for a refund");
    assert_eq!(
        refund.text,
        "---\nname: refund-check-iris\ndescription: Use when a customer asks for a refund\n---\n1. Find the order.\n2. Check the date.\n"
    );
    assert!(
        answer
            .notes
            .iter()
            .any(|n| n.contains("refund-check already exists"))
    );
    assert!(
        answer
            .notes
            .iter()
            .any(|n| n.starts_with("empty was left out"))
    );
    // Only the request's own drafts can be removed; reuse is only library skills.
    assert_eq!(answer.remove, ["old-draft"]);
    assert_eq!(answer.reuse.len(), 1);
    assert_eq!(answer.reuse[0]["id"], "s1");
    assert_eq!(answer.missing_apps, ["Shiprocket"]);
}

#[test]
fn a_skill_the_agent_already_has_keeps_its_name_when_changed() {
    let saved = vec![DraftIn {
        name: "refund-check".into(),
        description: "Use when".into(),
        steps: "1.".into(),
        saved: true,
    }];
    let content = r#"{"reply":"Changed it.","ground_rules":null,"skills":[{"name":"refund-check","description":"Use when a refund is asked","steps":"1. Ask Meera above 3000."}]}"#;
    let answer = read(content, &request(saved), &context()).unwrap();
    assert_eq!(answer.skills[0].name, "refund-check");
    assert!(answer.ground_rules.is_none() && answer.notes.is_empty());
}

#[test]
fn an_unreadable_answer_is_an_error_and_an_empty_one_still_replies() {
    assert!(read("Sorry, I can't.", &request(vec![]), &context()).is_err());
    let answer = read("{}", &request(vec![]), &context()).unwrap();
    assert!(answer.reply.starts_with("I couldn't write anything"));
    let too_long = format!(
        r#"{{"skills":[{{"name":"long","description":"Use when long","steps":"{}"}}]}}"#,
        "1. Step\\n".repeat(600)
    );
    let answer = read(&too_long, &request(vec![]), &context()).unwrap();
    assert_eq!(
        answer.skills[0].steps.lines().count(),
        480,
        "kept under the 500-line limit"
    );
}
