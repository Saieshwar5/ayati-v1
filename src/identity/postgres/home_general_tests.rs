use super::{home_maintenance_tests::*, tests::*};
use crate::{
    identity::home_panels::{maintenance::Refresh, model::Review},
    panels::PanelDraft,
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn general_panels_round_trip_private_review_and_target_preserving_refresh() {
    let (mut db, owner, w, original, read, _, _) = setup().await;
    db.pooled = false;
    db.chat_send(
        &owner,
        &w,
        &original.chat,
        &message("Prepare general views", Some(&original.agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    observed(&db, &job, &read).await;
    let sections = [
        json!({"kind":"metric","label":"Count","value":"12","comparison":{"label":"Previous period","value":-3,"unit":"%","favorable":"decrease"}}),
        json!({"kind":"metrics","items":[{"id":"a","label":"A","value":"12"}]}),
        json!({"kind":"chart","title":"Trend","variant":"stacked_bar","unit":"count","series":[{"name":"A","points":[{"label":"Mon","value":2}]},{"name":"B","points":[{"label":"Mon","value":3}]}]}),
        json!({"kind":"table","columns":["Name","Count"],"column_types":["text","number"],"rows":[["A","12"]]}),
        json!({"kind":"collection","items":[{"id":"a","title":"A","description":"Description","status":"Active"}]}),
        json!({"kind":"progress","items":[{"id":"a","label":"Goal","actual":72,"target":100,"unit":"count"}]}),
        json!({"kind":"timeline","events":[{"id":"a","at_ms":1000,"label":"Changed"}]}),
        json!({"kind":"text","heading":"Context","body":"Saved finding"}),
    ];
    let mut progress_id = String::new();
    for (index, section) in sections.into_iter().enumerate() {
        let draft: PanelDraft = serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),
            "expected_revision":0,"title":format!("General {index}"),"purpose":"Use the reviewed source",
            "sections":[section],"sources":[]})).unwrap();
        db.save_personal_panel(&job, &draft).await.unwrap();
        db.review_home_panel(
            &owner,
            &w,
            &draft.id,
            &Review {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
            },
        )
        .await
        .unwrap();
        let result = db.home_panels(&owner, &w).await.unwrap();
        let saved = result["panels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == draft.id)
            .unwrap();
        assert_eq!(saved["active"]["definition"], draft.definition());
        if section["kind"] == "progress" {
            progress_id = draft.id;
        }
    }
    let other = user(&db, "general-panel-other-owner").await;
    join(&db, &owner, &w, &other, "Member").await;
    db.change(&owner, &w, &other.id, Some("Admin"))
        .await
        .unwrap();
    assert_eq!(
        db.home_panels(&other, &w).await.unwrap()["panels"],
        json!([])
    );
    assert!(
        db.review_home_panel(
            &other,
            &w,
            &progress_id,
            &Review {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1
            }
        )
        .await
        .is_err()
    );
    db.chat_finish(&job, "Prepared views", "complete")
        .await
        .unwrap();
    db.pooled = true;
    db.save_home_agreement(&owner, &w, &progress_id, &agreement(0, None))
        .await
        .unwrap();
    let accepted = db
        .update_home_panel(&owner, &w, &progress_id, &update(1))
        .await
        .unwrap();
    let refresh_job = claim(&db, &w, accepted["reply"].as_str().unwrap()).await;
    observed(&db, &refresh_job, &read).await;
    let update: Refresh = serde_json::from_value(json!({"sections":[{"kind":"progress","items":[
        {"id":"a","label":"Goal","actual":80,"target":200,"unit":"count"}]}]}))
    .unwrap();
    assert!(db.refresh_home_panel(&refresh_job, &update).await.is_err());
    let update: Refresh = serde_json::from_value(json!({"sections":[{"kind":"progress","items":[
        {"id":"a","label":"Goal","actual":80,"target":100,"unit":"count"}]}]}))
    .unwrap();
    db.refresh_home_panel(&refresh_job, &update).await.unwrap();
    db.chat_finish(&refresh_job, "Refreshed", "complete")
        .await
        .unwrap();
    let result = db.home_panels(&owner, &w).await.unwrap();
    let saved = result["panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == progress_id)
        .unwrap();
    assert_eq!(
        saved["active"]["definition"]["sections"][0]["items"][0]["actual"],
        80.0
    );
    assert_eq!(
        saved["active"]["definition"]["sections"][0]["items"][0]["target"],
        100.0
    );
}
