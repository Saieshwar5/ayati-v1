use super::*;
use crate::identity::Pending;

impl PgStore {
    pub(in crate::identity) async fn save_login_flow(
        &self,
        hash: &str,
        flow: &Pending,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        // Serialize the bounded anonymous intake across API replicas, not just one process.
        tx.execute_unprepared("LOCK TABLE relay_login_flows IN EXCLUSIVE MODE")
            .await?;
        execute(
            &tx,
            "DELETE FROM relay_login_flows WHERE expires <= $1",
            vec![now().into()],
        )
        .await?;
        let count: i64 = query(&tx, "SELECT count(*) AS n FROM relay_login_flows", vec![])
            .await?
            .context("Cannot count sign-in attempts")?
            .try_get("", "n")?;
        ensure!(count < 100, "Too many sign-in attempts. Try again later");
        execute(&tx, "INSERT INTO relay_login_flows(hash,state,verifier,invitation,expires) VALUES($1,$2,$3,$4,$5)",
            vec![hash.into(),flow.state.clone().into(),flow.verifier.clone().into(),flow.invitation.clone().into(),flow.expires.into()]).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(in crate::identity) async fn take_login_flow(&self, hash: &str) -> Result<Option<Pending>> {
        let row = query(&self.db, "DELETE FROM relay_login_flows WHERE hash=$1 RETURNING state,verifier,invitation,expires", vec![hash.into()]).await?;
        row.map(|r| {
            Ok(Pending {
                state: r.try_get("", "state")?,
                verifier: r.try_get("", "verifier")?,
                invitation: r.try_get("", "invitation")?,
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
