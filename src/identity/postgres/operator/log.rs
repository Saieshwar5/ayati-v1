//! The change log: every operator change, newest first, 50 at a time.
use super::*;

const AREAS: &[&str] = &[
    "workspaces",
    "providers",
    "models",
    "routing",
    "settings",
    "codes",
];

impl PgStore {
    pub(crate) async fn operator_log(
        &self,
        area: Option<&str>,
        before: Option<i64>,
        limit: i64,
    ) -> Result<Value> {
        if let Some(area) = area {
            ensure!(AREAS.contains(&area), "Unknown change log area");
        }
        let limit = limit.clamp(1, 200);
        let entries = json_rows(&self.db, "SELECT jsonb_build_object('id',l.id,'at_ms',l.at_ms,'via',l.via,'area',l.area,'action',l.action,
              'target',l.target,'summary',l.summary,'reason',l.reason,'by',COALESCE(u.display_name,u.email),'email',u.email) AS data
            FROM relay_operator_log l JOIN relay_users u ON u.id=l.actor
            WHERE ($1::text IS NULL OR l.area=$1) AND ($2::bigint IS NULL OR l.id<$2)
            ORDER BY l.id DESC LIMIT $3",
            vec![area.map(str::to_owned).into(), before.into(), (limit + 1).into()]).await?;
        let more = entries.len() as i64 > limit;
        Ok(
            json!({"entries": entries.into_iter().take(limit as usize).collect::<Vec<_>>(), "more": more}),
        )
    }

    /// Codes are issued through the existing store; this records them in the
    /// change log too, so the console shows one history.
    pub(crate) async fn operator_record(
        &self,
        by: &Operator,
        area: &str,
        action: &str,
        target: &str,
        summary: &str,
        why: &str,
    ) -> Result<()> {
        ensure!(AREAS.contains(&area), "Unknown change log area");
        record(&self.db, by, area, action, target, summary, why).await
    }
}

impl PgStore {
    /// The person `ayati admin` acts as: someone who has signed in to Cuckoo.
    /// Several provider identities may share an email; the earliest is used.
    pub(crate) async fn operator_user(&self, email: &str) -> Result<Option<User>> {
        query(&self.db, "SELECT id,email,display_name FROM relay_users WHERE lower(email)=lower($1) ORDER BY id LIMIT 1", vec![email.into()])
            .await?
            .map(|row| Ok(User { id: row.try_get("", "id")?, email: row.try_get("", "email")?, display_name: row.try_get("", "display_name")? }))
            .transpose()
    }
}
