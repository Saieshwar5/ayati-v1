//! Company apps open through Cuckoo only for the people allowed; members ask for
//! access or suggest apps, and the app's owner or an admin decides.
use super::super::tests::{fixture, join, user, workspace};
use super::{AppDecision, AppRequest, CompanyApp};
use serde_json::Value;

fn app(
    id: &str,
    audience: &str,
    people: Vec<String>,
    owner: Option<String>,
    revision: i64,
) -> CompanyApp {
    CompanyApp {
        id: id.into(),
        name: "Order portal".into(),
        description: "Orders and deliveries".into(),
        url: "https://orders.example.test/home".into(),
        audience: audience.into(),
        people,
        owner,
        revision,
    }
}
fn ask(app: Option<&str>, name: &str) -> AppRequest {
    AppRequest {
        id: uuid::Uuid::new_v4().to_string(),
        app: app.map(str::to_owned),
        name: name.into(),
        url: String::new(),
        reason: "For deliveries".into(),
    }
}
fn system<'a>(view: &'a Value, id: &str) -> &'a Value {
    view["systems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn company_apps_open_only_for_people_allowed_and_requests_go_to_their_owner() {
    let db = fixture().await;
    let admin = user(&db, "apps-admin").await;
    let ravi = user(&db, "apps-ravi").await;
    let mina = user(&db, "apps-mina").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &ravi, "Member").await;
    join(&db, &admin, &w, &mina, "Member").await;
    let id = uuid::Uuid::new_v4().to_string();
    // Only admins add apps, over https only.
    assert!(
        db.company_app_save(&ravi, &w, &app(&id, "everyone", vec![], None, 0))
            .await
            .is_err()
    );
    let mut plain = app(&id, "everyone", vec![], None, 0);
    plain.url = "http://orders.example.test".into();
    assert!(db.company_app_save(&admin, &w, &plain).await.is_err());
    plain.url = "javascript:alert(1)".into();
    assert!(db.company_app_save(&admin, &w, &plain).await.is_err());
    db.company_app_save(
        &admin,
        &w,
        &app(&id, "selected", vec![], Some(ravi.id.clone()), 0),
    )
    .await
    .unwrap();
    assert!(
        db.company_app_save(
            &admin,
            &w,
            &app(&id, "selected", vec![], Some(ravi.id.clone()), 0)
        )
        .await
        .is_err(),
        "an add can't overwrite"
    );
    // Mina sees the tile, not the address, and can't open it.
    let view = db.workspace_apps(&mina, &w).await.unwrap();
    let tile = system(&view, &id);
    assert_eq!(
        (
            tile["can_open"].as_bool(),
            tile["url"].is_null(),
            tile["people"].is_null()
        ),
        (Some(false), true, true)
    );
    assert!(db.company_app_open(&mina, &w, &id).await.is_err());
    // Ravi owns it: he opens it and decides Mina's request.
    assert_eq!(
        db.company_app_open(&ravi, &w, &id).await.unwrap(),
        "https://orders.example.test/home"
    );
    let request = ask(Some(&id), "");
    db.app_request(&mina, &w, &request).await.unwrap();
    assert_eq!(
        db.app_request(&mina, &w, &request).await.unwrap()["state"],
        "pending",
        "a retry doesn't ask twice"
    );
    assert!(
        db.app_request(&mina, &w, &ask(Some(&id), ""))
            .await
            .is_err(),
        "one waiting request per app"
    );
    assert_eq!(
        system(&db.workspace_apps(&mina, &w).await.unwrap(), &id)["requested"],
        true
    );
    let waiting = db.app_requests(&ravi, &w).await.unwrap();
    assert_eq!(
        waiting["requests"][0]["requester"],
        "apps-mina@example.test"
    );
    assert_eq!(
        db.workspace_apps(&ravi, &w).await.unwrap()["requests_waiting"],
        1
    );
    assert!(
        db.app_request_decide(
            &mina,
            &w,
            &request.id,
            &AppDecision {
                action: "approve".into()
            }
        )
        .await
        .is_err()
    );
    db.app_request_decide(
        &ravi,
        &w,
        &request.id,
        &AppDecision {
            action: "approve".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.app_request_decide(
            &admin,
            &w,
            &request.id,
            &AppDecision {
                action: "decline".into()
            }
        )
        .await
        .unwrap()["state"],
        "approved",
        "the first decision stands"
    );
    assert_eq!(
        db.company_app_open(&mina, &w, &id).await.unwrap(),
        "https://orders.example.test/home"
    );
    assert!(
        db.app_request(&mina, &w, &ask(Some(&id), ""))
            .await
            .is_err(),
        "no request once she can open it"
    );
    // A member suggests an app; only admins see and decide suggestions.
    let suggestion = ask(None, "Delivery tracker");
    db.app_request(&mina, &w, &suggestion).await.unwrap();
    assert!(
        db.app_requests(&ravi, &w).await.unwrap()["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let admin_view = db.app_requests(&admin, &w).await.unwrap();
    assert_eq!(admin_view["requests"][0]["name"], "Delivery tracker");
    assert!(
        db.app_request_decide(
            &ravi,
            &w,
            &suggestion.id,
            &AppDecision {
                action: "approve".into()
            }
        )
        .await
        .is_err()
    );
    db.app_request_decide(
        &admin,
        &w,
        &suggestion.id,
        &AppDecision {
            action: "approve".into(),
        },
    )
    .await
    .unwrap();
    // Removing the app closes it for everyone; every step is audited.
    db.company_app_remove(&admin, &w, &id).await.unwrap();
    assert!(db.company_app_open(&ravi, &w, &id).await.is_err());
    assert!(
        db.workspace_apps(&ravi, &w).await.unwrap()["systems"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let audit: Vec<String> = super::rows(&db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND summary IS NOT NULL ORDER BY at,summary", vec![w.clone().into()])
        .await.unwrap().into_iter().map(|r| r.try_get("", "summary").unwrap()).collect();
    for line in [
        "Added the company app \"Order portal\" (0 people can open it)",
        "Opened \"Order portal\"",
        "Asked to open \"Order portal\"",
        "Approved apps-mina@example.test's request to open \"Order portal\"",
        "Suggested the app \"Delivery tracker\"",
        "Accepted apps-mina@example.test's suggestion \"Delivery tracker\"",
        "Removed the company app \"Order portal\"",
    ] {
        assert!(
            audit.iter().any(|a| a == line),
            "missing audit line: {line}\n{audit:#?}"
        );
    }
}
