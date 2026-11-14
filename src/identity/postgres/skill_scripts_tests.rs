//! Skill scripts run on the agent's computer (B2, D248), under its Programs rule.
use super::super::computer_tests::{ask, computer, setup};
use super::NewSkill;
use super::files::SkillFile;
use super::*;
use crate::identity::control::broker::invoke;
use serde_json::json;

/// A skill with a tested script that writes an invoice line for an order.
async fn invoice_skill(s: &super::super::computer_tests::Setup) -> String {
    let skill = s
        .db
        .create_skill(
            &s.owner,
            &s.w,
            &NewSkill {
                id: uuid::Uuid::new_v4().to_string(),
                shared: true,
                files: vec![
                    SkillFile {
                        path: "SKILL.md".into(),
                        text: "---\nname: invoice-line\ndescription: Use when an invoice line is needed\n---\n1. Run scripts/line.py with the order number.\n".into(),
                    },
                    SkillFile {
                        path: "scripts/line.py".into(),
                        text: "import sys\nprint(f'INVOICE {sys.argv[1]} GST 18%')\n".into(),
                    },
                ],
            },
        )
        .await
        .unwrap();
    let id = skill["id"].as_str().unwrap().to_owned();
    s.db.assign_skill(&s.owner, &s.w, &s.agent, &id, true)
        .await
        .unwrap();
    id
}

fn run() -> Value {
    json!({"skill":"invoice-line","script":"scripts/line.py","arguments":["1042"]})
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_skill_script_runs_on_the_agents_computer_under_its_programs_rule() {
    let s = setup().await;
    invoice_skill(&s).await;
    // Without a computer nothing runs; the script stays text to read.
    let job = ask(&s, "Make the invoice line for order 1042").await;
    let none = invoke(&s.db, &job, "run_skill_script", run())
        .await
        .unwrap();
    assert_eq!(none["state"], "denied");
    assert!(none["message"].as_str().unwrap().contains("no computer"));

    // With programs allowed, it runs, with the skill's files copied over.
    computer(&s, json!({"on": true, "size": "small"})).await;
    let ran = invoke(&s.db, &job, "run_skill_script", run())
        .await
        .unwrap();
    assert_eq!(ran["state"], "completed", "{ran}");
    assert!(ran.to_string().contains("INVOICE 1042 GST 18%"));
    assert_eq!(ran["skill"], "invoice-line");
    assert_eq!(ran["version"], 1);
    let steps = super::runtime::steps(&s.db.db, &s.w, &job.reply)
        .await
        .unwrap();
    assert!(
        steps.iter().any(|step| step["title"]
            == "Ran scripts/line.py from invoice-line (version 1) on its computer"),
        "{steps:?}"
    );

    // Only the skill's own scripts, under scripts/, with a known interpreter.
    for (input, state) in [
        (
            json!({"skill":"invoice-line","script":"SKILL.md"}),
            "not_found",
        ),
        (
            json!({"skill":"invoice-line","script":"scripts/../SKILL.md"}),
            "not_found",
        ),
        (
            json!({"skill":"other-skill","script":"scripts/line.py"}),
            "not_found",
        ),
    ] {
        assert_eq!(
            invoke(&s.db, &job, "run_skill_script", input)
                .await
                .unwrap()["state"],
            state
        );
    }
    // Arguments are separate words, never shell.
    let quoted = invoke(
        &s.db,
        &job,
        "run_skill_script",
        json!({"skill":"invoice-line","script":"scripts/line.py","arguments":["x; touch hacked"]}),
    )
    .await
    .unwrap();
    assert!(quoted.to_string().contains("INVOICE x; touch hacked GST"));

    // With "Ask first" the person is asked, and the question names the skill.
    s.db.chat_finish(&job, "Done.", "complete").await.unwrap();
    computer(&s, json!({"on": true, "size": "small", "programs": "ask"})).await;
    let second = ask(&s, "Another invoice line").await;
    let asks = invoke(&s.db, &second, "run_skill_script", run())
        .await
        .unwrap();
    assert_eq!(asks["state"], "needs_confirmation", "{asks}");
    assert!(
        asks.to_string()
            .contains("skills/invoice-line/v1/scripts/line.py")
    );

    // With "Block" nothing runs.
    s.db.chat_finish(&second, "Done.", "complete")
        .await
        .unwrap();
    computer(
        &s,
        json!({"on": true, "size": "small", "programs": "block"}),
    )
    .await;
    let third = ask(&s, "One more").await;
    let blocked = invoke(&s.db, &third, "run_skill_script", run())
        .await
        .unwrap();
    assert_eq!(blocked["state"], "denied");
    assert!(!blocked.to_string().contains("INVOICE"));
}
