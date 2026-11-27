use super::*;
use crate::identity::Pending;

/// Started sign-ins kept at once, for every workspace together. This only bounds
/// storage (each row is under 2 KB and lasts ten minutes); each client address
/// is limited first (`identity::sign_in_limit`). It was 100, which let anyone
/// lock every person out of signing in with 100 requests every ten minutes.
pub(in crate::identity) const LOGIN_FLOW_LIMIT: i64 = 50_000;

impl PgStore {
    pub(in crate::identity) async fn save_login_flow(
        &self,
        hash: &str,
        flow: &Pending,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        execute(
            &tx,
            "DELETE FROM relay_login_flows WHERE expires <= $1",
            vec![now().into()],
        )
        .await?;
        // A storage guard, so it needn't be exact across replicas: no table lock.
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM (SELECT 1 FROM relay_login_flows LIMIT $1) f",
            vec![LOGIN_FLOW_LIMIT.into()],
        )
        .await?
        .context("Cannot count sign-in attempts")?
        .try_get("", "n")?;
        ensure!(
            count < LOGIN_FLOW_LIMIT,
            "Too many sign-in attempts. Try again later"
        );
        // return_to (schema 42) is only named when used, so older schemas keep working.
        let (column, value) = if flow.return_to.is_some() {
            (",return_to", ",$6")
        } else {
            ("", "")
        };
        let mut args: Vec<sea_orm::Value> = vec![
            hash.into(),
            flow.state.clone().into(),
            flow.verifier.clone().into(),
            flow.invitation.clone().into(),
            flow.expires.into(),
        ];
        if let Some(path) = &flow.return_to {
            args.push(path.clone().into());
        }
        execute(&tx, &format!("INSERT INTO relay_login_flows(hash,state,verifier,invitation,expires{column}) VALUES($1,$2,$3,$4,$5{value})"), args).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(in crate::identity) async fn take_login_flow(&self, hash: &str) -> Result<Option<Pending>> {
        let row = query(&self.db, "DELETE FROM relay_login_flows f WHERE hash=$1 RETURNING state,verifier,invitation,expires,to_jsonb(f)->>'return_to' AS return_to", vec![hash.into()]).await?;
        row.map(|r| {
            Ok(Pending {
                state: r.try_get("", "state")?,
                verifier: r.try_get("", "verifier")?,
                invitation: r.try_get("", "invitation")?,
                return_to: r.try_get("", "return_to")?,
                expires: r.try_get("", "expires")?,
            })
        })
        .transpose()
    }
    pub(crate) async fn ping(&self) -> Result<()> {
        query(&self.db, "SELECT 1", vec![]).await?;
        Ok(())
    }
}
