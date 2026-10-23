use super::*;
use serde_json::json;
impl PgStore {
    pub async fn chat_page(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        before: Option<i64>,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let boundary = before.unwrap_or(i64::MAX);
        let mut messages=json_rows(&tx,"WITH recent AS (SELECT * FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND sequence<$3 ORDER BY sequence DESC LIMIT 200) SELECT jsonb_build_object('id',m.id,'chat',m.chat,'sequence',m.sequence,'author',m.author,'kind',m.kind,'content',m.content,'state',m.state,'progress',m.progress,'revision',m.revision,'at',m.at,'name',CASE WHEN m.kind='agent' THEN (SELECT a.name FROM relay_shared_agents a WHERE a.workspace=m.workspace AND a.id=m.author) ELSE COALESCE(u.display_name,u.email) END,'thread',m.thread,'delivered',CASE WHEN m.author=$4 THEN (SELECT count(*) FROM relay_message_receipts r WHERE r.workspace=m.workspace AND r.message=m.id) ELSE 0 END,'seen',CASE WHEN m.author=$4 THEN (SELECT count(*) FROM relay_message_receipts r WHERE r.workspace=m.workspace AND r.message=m.id AND r.seen_at IS NOT NULL) ELSE 0 END,'attachments',coalesce((SELECT jsonb_agg(jsonb_build_object('id',o.id,'name',o.name,'mime',o.mime,'size',o.size,'checksum',o.checksum)) FROM relay_objects o WHERE o.workspace=m.workspace AND o.chat=m.chat AND m.attachments @> jsonb_build_array(o.id)),'[]'::jsonb)) AS data FROM (SELECT * FROM recent UNION SELECT * FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id IN (SELECT thread FROM recent)) m LEFT JOIN relay_users u ON u.id=m.author ORDER BY m.sequence",vec![w.into(),chat.into(),boundary.into(),user.id.clone().into()]).await?;
        let cursor=query(&tx,"SELECT min(sequence) AS cursor FROM (SELECT sequence FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND sequence<$3 ORDER BY sequence DESC LIMIT 200) m",vec![w.into(),chat.into(),boundary.into()]).await?.context("Missing cursor")?.try_get::<Option<i64>>("","cursor")?;
        let has_more = if let Some(cursor) = cursor {
            query(&tx,"SELECT id FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND sequence<$3 LIMIT 1",vec![w.into(),chat.into(),cursor.into()]).await?.is_some()
        } else {
            false
        };
        let agents = rows(
            &tx,
            "SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2",
            vec![w.into(), chat.into()],
        )
        .await?
        .into_iter()
        .map(|r| r.try_get::<String>("", "agent"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
        super::reply_views::hydrate(&tx, user, w, &mut messages).await?;
        tx.commit().await?;
        Ok(
            json!({"messages":messages,"agents":agents,"next_before":if has_more {cursor}else{None}}),
        )
    }
}
