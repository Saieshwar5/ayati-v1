use super::*;
use serde_json::json;

// Used for both snapshot and live payloads, including updated receipts and media.
pub(super) const MESSAGE: &str = "SELECT to_jsonb(m) - 'target' - 'requester' || jsonb_build_object('name',CASE WHEN m.kind='agent' THEN (SELECT a.name FROM relay_shared_agents a WHERE a.workspace=m.workspace AND a.id=m.author) ELSE COALESCE(u.display_name,u.email) END,'attachments',coalesce((SELECT jsonb_agg(jsonb_build_object('id',o.id,'name',o.name,'mime',o.mime,'size',o.size,'checksum',o.checksum)) FROM relay_objects o WHERE o.workspace=m.workspace AND o.chat=m.chat AND m.attachments @> jsonb_build_array(o.id)),'[]'::jsonb),'delivered',CASE WHEN m.author=$3 THEN (SELECT count(*) FROM relay_message_receipts r WHERE r.workspace=m.workspace AND r.message=m.id) ELSE 0 END,'seen',CASE WHEN m.author=$3 THEN (SELECT count(*) FROM relay_message_receipts r WHERE r.workspace=m.workspace AND r.message=m.id AND seen_at IS NOT NULL) ELSE 0 END) AS data FROM relay_chat_messages m LEFT JOIN relay_users u ON u.id=m.author WHERE m.workspace=$1 AND m.id=$2";

impl PgStore {
    pub async fn accepted_message(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let messages = json_rows(
            &tx,
            MESSAGE,
            vec![w.into(), id.into(), user.id.clone().into()],
        )
        .await?;
        let message = messages.into_iter().next().context("Message missing")?;
        ensure!(
            message["chat"] == chat && message["author"] == user.id,
            "Message access denied"
        );
        tx.commit().await?;
        Ok(message)
    }
    pub async fn listen_changes(&self) -> Result<()> {
        let mut listener = sea_orm::sqlx::postgres::PgListener::connect_with(
            self.db.get_postgres_connection_pool(),
        )
        .await?;
        listener.listen("relay_message_changes").await?;
        loop {
            let notification = listener.recv().await?;
            let _ = self.changes.send(notification.payload().to_owned());
        }
    }
    pub async fn sync_page(&self, user: &User, w: &str, after: Option<i64>) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let clock = query(&tx,"SELECT coalesce((SELECT cursor FROM relay_sync_clocks WHERE workspace=$1),0) AS cursor,
            coalesce((SELECT floor FROM relay_sync_clocks WHERE workspace=$1),0) AS floor",vec![w.into()]).await?.context("Missing clock")?;
        let (high, floor): (i64, i64) = (clock.try_get("", "cursor")?, clock.try_get("", "floor")?);
        // Notices before the floor were removed (D226): reload rather than miss them.
        let Some(after) = after.filter(|n| *n >= floor && *n <= high) else {
            tx.commit().await?;
            return Ok(json!({"cursor":high,"reset":true,"events":[]}));
        };
        let events=rows(&tx,"SELECT cursor,chat,message,kind,to_jsonb(e)->>'audience' AS audience FROM relay_sync_events e WHERE workspace=$1 AND cursor>$2 ORDER BY cursor LIMIT 100",vec![w.into(),after.into()]).await?;
        let mut output = vec![];
        let mut cursor = after;
        let mut ids = std::collections::BTreeSet::new();
        let mut access_changed = false;
        let mut sample_access_changed = false;
        let mut panel_access_changed = false;
        for row in events {
            cursor = row.try_get("", "cursor")?;
            if row.try_get::<String>("", "kind")? == "access" {
                access_changed = true;
            } else if row.try_get::<String>("", "kind")? == "panel_access" {
                panel_access_changed |=
                    row.try_get::<Option<String>>("", "audience")?.as_deref() == Some(&user.id);
            } else if row.try_get::<String>("", "kind")? == "sample_access" {
                sample_access_changed |=
                    row.try_get::<Option<String>>("", "audience")?.as_deref() == Some(&user.id);
            } else if row.try_get::<String>("", "kind")? == "event_access" {
                let chat: String = row.try_get("", "chat")?;
                if super::chat::access(&tx, user, w, &chat).await.is_ok() {
                    output.push(json!({"kind":"event_access","chat":chat,"source":row.try_get::<String>("","message")?}));
                }
            } else if let Some(id) = row.try_get::<Option<String>>("", "message")? {
                ids.insert(id);
            }
        }
        if access_changed {
            output.push(json!({"kind":"access"}));
        }
        if panel_access_changed {
            output.push(json!({"kind":"panel_access"}));
        }
        if sample_access_changed {
            output.push(json!({"kind":"sample_access"}));
        }
        if !ids.is_empty() {
            // One authorized batch read, not three queries per message per recipient.
            let sql = MESSAGE.replace("m.id=$2", "m.id IN (SELECT jsonb_array_elements_text($2::jsonb)) AND EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=m.workspace AND c.id=m.chat AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)))");
            let mut messages = json_rows(
                &tx,
                &sql,
                vec![w.into(), json!(ids).into(), user.id.clone().into()],
            )
            .await?;
            super::reply_views::hydrate(&tx, user, w, &mut messages).await?;
            for message in messages {
                output.push(json!({"kind":"message","chat":message["chat"],"message":message}));
            }
        }
        tx.commit().await?;
        Ok(json!({"cursor":cursor,"more":cursor<high,"events":output}))
    }
    pub async fn acknowledge(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        ids: &[String],
        seen: bool,
    ) -> Result<()> {
        ensure!(
            !ids.is_empty() && ids.len() <= 100,
            "Acknowledge 1–100 messages"
        );
        let tx = self.workspace_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        for id in ids {
            let row = query(
                &tx,
                "SELECT author,state FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3",
                vec![w.into(), chat.into(), id.clone().into()],
            )
            .await?
            .context("Message is unavailable in this conversation")?;
            if row.try_get::<String>("", "author")? == user.id
                || !["sent", "complete"].contains(&row.try_get::<String>("", "state")?.as_str())
            {
                continue;
            }
            execute(&tx,"INSERT INTO relay_message_receipts(workspace,chat,message,user_id,delivered_at,seen_at) VALUES($1,$2,$3,$4,$5,CASE WHEN $6 THEN $5 ELSE NULL END) ON CONFLICT(workspace,message,user_id) DO UPDATE SET seen_at=EXCLUDED.seen_at WHERE relay_message_receipts.seen_at IS NULL AND EXCLUDED.seen_at IS NOT NULL",vec![w.into(),chat.into(),id.clone().into(),user.id.clone().into(),now().into(),seen.into()]).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn receipt_details(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        ensure!(query(&tx,"SELECT id FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3 AND author=$4 AND kind='user'",vec![w.into(),chat.into(),id.into(),user.id.clone().into()]).await?.is_some(),"Only the sender can inspect receipts");
        let receipts=json_rows(&tx,"SELECT jsonb_build_object('name',COALESCE(u.display_name,u.email),'delivered_at',r.delivered_at,'seen_at',r.seen_at) AS data FROM relay_message_receipts r JOIN relay_users u ON u.id=r.user_id WHERE r.workspace=$1 AND r.chat=$2 AND r.message=$3 ORDER BY r.delivered_at LIMIT 200",vec![w.into(),chat.into(),id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"receipts":receipts}))
    }
    pub async fn unread(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let items=json_rows(&tx,"SELECT jsonb_build_object('chat',m.chat,'count',count(*)) AS data FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat WHERE m.workspace=$1 AND m.author<>$2 AND m.state IN ('sent','complete') AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2)) AND NOT EXISTS(SELECT 1 FROM relay_message_receipts r WHERE r.workspace=m.workspace AND r.message=m.id AND r.user_id=$2 AND r.seen_at IS NOT NULL) GROUP BY m.chat",vec![w.into(),user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"unread":items}))
    }
}
