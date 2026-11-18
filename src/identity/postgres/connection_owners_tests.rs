//! Schema 132: an agent's owner connects an app for that agent alone; the
//! connection is the agent's own, with one rule set its owner changes.
use super::super::{tests::*, *};
use crate::identity::agents::config::{SaveAgent, Settings};
use crate::identity::connectors::{
    self, ConnectorProvider, connect::connect_app_for, fake_composio::FakeComposio,
};
use crate::identity::control::{
    connections_model::{ConnectorRule, SystemApproval, WorkspaceConnection},
    model::AgentConnection,
};
use std::sync::Arc;

const ORIGIN: &str = "http://127.0.0.1:1";

struct Setup {
    db: PgStore,
    admin: User,
    ravi: User,
    priya: User,
    w: String,
    sales: String,
    hr: String,
    fake: FakeComposio,
    provider: Arc<dyn ConnectorProvider>,
}

async fn agent(db: &PgStore, owner: &User, w: &str, name: &str) -> String {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        role: "Worker".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    input.id
}

async fn setup() -> Setup {
    let db = fixture().await;
    let admin = user(&db, "owner-admin").await;
    let ravi = user(&db, "owner-ravi").await;
    let priya = user(&db, "owner-priya").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &ravi, "Member").await;
    join(&db, &admin, &w, &priya, "Member").await;
    let sales = agent(&db, &ravi, &w, "Sales Desk").await;
    let hr = agent(&db, &priya, &w, "HR Desk").await;
    let fake = FakeComposio::start().await;
    let provider = connectors::composio_for_tests(&fake);
    Setup {
        db,
        admin,
        ravi,
        priya,
        w,
        sales,
        hr,
        fake,
        provider,
    }
}

impl Setup {
    /// Connects Gmail for `agent` as `who` and signs in at the provider.
    async fn connect(&self, who: &User, agent: &str) -> anyhow::Result<String> {
        let request = uuid::Uuid::new_v4().to_string();
        let answer = connect_app_for(
            &self.db,
            ORIGIN,
            who,
            &self.w,
            "gmail",
            &request,
            Some(agent),
            self.provider.as_ref(),
        )
        .await?;
        assert!(
            answer["redirect_url"].is_string(),
            "Gmail signs in at its page"
        );
        let row = query(
            &self.db.db,
            "SELECT id,connection,binding->>'account' AS account FROM relay_connector_sign_ins WHERE workspace=$1 AND agent=$2 AND state='waiting'",
            vec![self.w.clone().into(), agent.into()],
        )
        .await?
        .expect("a sign-in is waiting");
        let id: String = row.try_get("", "id")?;
        self.fake.sign_in(&row.try_get::<String>("", "account")?);
        let sign_in = self.db.connector_sign_in(who, &id).await?;
        connectors::routes::finish_sign_in(&self.db, who, &id, sign_in, self.provider.as_ref())
            .await?;
        Ok(row.try_get("", "connection")?)
    }
    async fn tools(&self, who: &User, agent: &str) -> Vec<Value> {
        self.db.control_snapshot(who, &self.w, agent).await.unwrap()["connections"]
            .as_array()
            .unwrap()
            .clone()
    }
    async fn stored(&self, id: &str) -> Value {
        query(
            &self.db.db,
            "SELECT to_jsonb(c) AS data FROM relay_control_connections c WHERE id=$1",
            vec![id.into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "data")
        .unwrap()
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_agents_owner_connects_an_app_that_only_that_agent_uses() {
    let s = setup().await;
    // Ravi is a member, not an admin: he connects Gmail for his own agent.
    let mine = s.connect(&s.ravi, &s.sales).await.unwrap();
    let saved = s.stored(&mine).await;
    assert_eq!(saved["agent"], s.sales.as_str());
    assert_eq!(saved["server"], "gmail");
    assert_eq!(saved["connected_by"], s.ravi.id.as_str());
    let own = s.tools(&s.ravi, &s.sales).await;
    assert_eq!(own.len(), 1);
    assert_eq!(
        (own[0]["on"].as_bool(), own[0]["own"].as_bool()),
        (Some(true), Some(true))
    );
    // Reads are allowed and everything else asks first, as for any new app.
    let modes: Vec<&str> = own[0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["mode"].as_str().unwrap())
        .collect();
    assert!(modes.contains(&"allow") && !modes.is_empty());
    // Priya's agent does not see it, and it is not offered to new agents.
    assert!(s.tools(&s.priya, &s.hr).await.is_empty());
    let offered = s.db.control_available(&s.priya, &s.w).await.unwrap();
    assert_eq!(offered["connections"], serde_json::json!([]));

    // Priya connects her own Gmail for her agent: a second account of the app.
    let hers = s.connect(&s.priya, &s.hr).await.unwrap();
    assert_ne!(hers, mine);
    assert_eq!(s.stored(&hers).await["agent"], s.hr.as_str());
    assert_ne!(
        s.stored(&hers).await["binding"]["account"],
        saved["binding"]["account"]
    );
    assert_eq!(s.tools(&s.priya, &s.hr).await.len(), 1);
    assert_eq!(s.tools(&s.ravi, &s.sales).await[0]["id"], mine.as_str());

    // Nobody connects for an agent they don't own, and nobody, not even an
    // admin, gives one agent's connection to another agent.
    assert!(s.connect(&s.priya, &s.sales).await.is_err());
    let give = AgentConnection {
        connection: mine.clone(),
        enabled: true,
    };
    for who in [&s.priya, &s.admin] {
        let refused =
            s.db.control_agent_connection(who, &s.w, &s.hr, &give)
                .await
                .unwrap_err();
        assert!(refused.to_string().contains("belongs to another agent"));
    }
    // An admin may connect for any agent, and sees every agent's tools.
    assert_eq!(s.tools(&s.admin, &s.sales).await.len(), 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_owner_sets_its_rules_and_disconnects_it_and_an_admin_can_block_the_app() {
    let s = setup().await;
    let mine = s.connect(&s.ravi, &s.sales).await.unwrap();
    let first = s.tools(&s.ravi, &s.sales).await[0]["operations"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let rule = |mode: &str| ConnectorRule {
        connection: mine.clone(),
        operation: first.clone(),
        mode: mode.into(),
        needs_other: false,
        revision: 0,
    };
    // One rule set, the owner's; another member can't change it.
    assert!(
        s.db.control_connector_rule(&s.priya, &s.w, &rule("block"))
            .await
            .is_err()
    );
    s.db.control_connector_rule(&s.ravi, &s.w, &rule("block"))
        .await
        .unwrap();
    assert_eq!(
        s.tools(&s.ravi, &s.sales).await[0]["operations"][0]["mode"],
        "block"
    );

    // The owner disconnects it; another member can't.
    let revision = s.stored(&mine).await["revision"].as_i64().unwrap();
    let off = WorkspaceConnection {
        id: mine.clone(),
        server: "gmail".into(),
        enabled: false,
        revision,
        agent: Some(s.sales.clone()),
    };
    assert!(
        s.db.workspace_connection_bind(&s.priya, &s.w, &off, None)
            .await
            .is_err()
    );
    // A request can't claim someone else's connection as the workspace's.
    let claimed = WorkspaceConnection {
        agent: None,
        ..off.clone()
    };
    assert!(
        s.db.workspace_connection_bind(&s.admin, &s.w, &claimed, None)
            .await
            .is_err()
    );
    s.db.workspace_connection_bind(&s.ravi, &s.w, &off, None)
        .await
        .unwrap();
    assert_eq!(s.stored(&mine).await["enabled"], false);
    assert!(s.tools(&s.ravi, &s.sales).await.is_empty());

    // An admin switches the app off for the workspace: owners can't connect it.
    let system = SystemApproval {
        server: "gmail".into(),
        approved: false,
        revision: 1,
    };
    s.db.control_system(&s.admin, &s.w, &system).await.unwrap();
    let refused = s.connect(&s.priya, &s.hr).await.unwrap_err();
    assert!(refused.to_string().contains("switched this app off"));
}
