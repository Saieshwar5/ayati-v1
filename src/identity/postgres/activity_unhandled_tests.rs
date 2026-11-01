//! Events no agent acted on: listed with their reason for the person who set up the
//! trigger, the agent's managers and admins; never their contents.
use super::super::tests::{chat, fixture, join, user, workspace};
use super::super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn unhandled_events_show_their_reason_to_the_trigger_owner_managers_and_admins() {
    let db = fixture().await;
    let admin = user(&db, "events-admin").await;
    let setter = user(&db, "events-setter").await;
    let manager = user(&db, "events-manager").await;
    let other = user(&db, "events-other").await;
    let w = workspace(&db, &admin).await;
    for person in [&setter, &manager, &other] {
        join(&db, &admin, &w, person, "Member").await;
    }
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    let room = chat(
        &db,
        &admin,
        &w,
        vec![setter.id.clone()],
        vec![agent.clone()],
    )
    .await;
    execute(&db.db, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$4,1)", vec![w.clone().into(), agent.clone().into(), manager.id.clone().into(), admin.id.clone().into()]).await.unwrap();
    db.db
        .execute_unprepared(&format!(
            "INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES('{w}','shop',true,1);
             INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES('{w}','shopify','shop','{{\"id\":\"shop\",\"operations\":[]}}','fp');
             INSERT INTO relay_event_triggers(workspace,id,agent,principal,source,binding,source_definition,connection,read_operation,chat,input,request_hash,connection_revision,read_revision)
              VALUES('{w}','new-orders','{agent}','{setter}','orders','b1','{{\"name\":\"New Shopify order\"}}','shopify','order.read','{room}','{{}}','h',1,1);
             INSERT INTO relay_event_inbox(workspace,source,id,fingerprint,binding,data) VALUES('{w}','orders','evt-1','fp','b1','{{\"customer\":\"PRIVATE_CUSTOMER\"}}');
             INSERT INTO relay_event_deliveries(workspace,id,trigger_id,source,event_id,authority,state,detail)
              VALUES('{w}','d1','new-orders','orders','evt-1','{{}}','cancelled','All agents were paused when this event arrived, so it was not handled.');",
            setter = setter.id,
        ))
        .await
        .unwrap();

    for (person, sees) in [
        (&setter, true),
        (&manager, true),
        (&admin, true),
        (&other, false),
    ] {
        let page = db
            .activity_unhandled(person, &w, None, None, None, None)
            .await
            .unwrap();
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), usize::from(sees), "{}", person.id);
        if sees {
            assert_eq!(items[0]["title"], "New Shopify order");
            assert_eq!(items[0]["agent"], agent.as_str());
            assert!(
                items[0]["reason"]
                    .as_str()
                    .unwrap()
                    .contains("All agents were paused")
            );
            assert!(
                !page.to_string().contains("PRIVATE_CUSTOMER"),
                "never the event's contents"
            );
        }
    }
    // My agents keeps it for the person who set the trigger up and the managers.
    let mine = db
        .activity_unhandled(&setter, &w, None, None, Some("mine"), None)
        .await
        .unwrap();
    assert_eq!(mine["items"].as_array().unwrap().len(), 1);
    assert!(
        db.activity_unhandled(&setter, &w, None, None, Some("team"), None)
            .await
            .is_err()
    );
    // Now counts today's unhandled events for the line that opens this list.
    let now = db
        .activity_now(&manager, &w, 1, Some("mine"))
        .await
        .unwrap();
    assert_eq!(now["unhandled_today"], 1);
    let none = db.activity_now(&other, &w, 1, None).await.unwrap();
    assert_eq!(none["unhandled_today"], 0);
}
