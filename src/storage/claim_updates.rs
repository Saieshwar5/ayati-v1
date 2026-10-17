use super::{Store, work::RunClaim};
use anyhow::{Result, ensure};
use rusqlite::params;

impl Store {
    /// Consume a matching exact approval once, independently of MCP transport claims.
    pub fn consume_workspace_permission(
        &self,
        claim: &RunClaim,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<String> {
        ensure!(self.claim_active(claim)?, "Run no longer owns this work");
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let candidates: Vec<(String, String, String)> = {
            let mut query=tx.prepare("SELECT p.id,p.action,a.call_id FROM permissions p JOIN tool_attempts a ON a.permission_id=p.id WHERE p.task_id=?1 AND p.run_token=?2 AND p.decision='allowed' AND EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=p.task_id AND t.status='running' AND c.token=p.run_token AND c.owner_id=?3) AND a.status IN ('authorized','running') AND NOT EXISTS(SELECT 1 FROM permission_uses u WHERE u.permission_id=p.id)")?;
            query
                .query_map(params![claim.task_id, claim.token, claim.owner_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?
                .collect::<rusqlite::Result<_>>()?
        };
        for (id, action, call) in candidates {
            let action: serde_json::Value = serde_json::from_str(&action)?;
            if action["tool"] == tool && action["request"]["rawInput"] == *arguments {
                tx.execute("INSERT INTO permission_uses VALUES(?1)", [id])?;
                tx.commit()?;
                return Ok(call);
            }
        }
        anyhow::bail!("No unused exact owner approval for this business operation")
    }
    pub fn add_claim_reply(&self, claim: &RunClaim) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let changed = self.connection()?.execute(
            "INSERT INTO messages(id,task_id,role,content,state) SELECT ?4,?1,'assistant','','streaming' WHERE EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id,id],
        )?;
        ensure!(changed == 1, "This run no longer owns the work");
        Ok(id)
    }

    pub fn save_claim_reply(
        &self,
        claim: &RunClaim,
        id: &str,
        text: &str,
        state: &str,
    ) -> Result<bool> {
        Ok(self.connection()?.execute(
            "UPDATE messages SET content=?5,state=?6 WHERE id=?4 AND task_id=?1 AND state='streaming' AND EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id,id,text,state],
        )? == 1)
    }

    pub fn set_claim_detail(&self, claim: &RunClaim, detail: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE tasks SET detail=?4 WHERE id=?1 AND status='running' AND EXISTS(SELECT 1 FROM run_claims WHERE task_id=?1 AND token=?2 AND owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id,detail],
        )?;
        Ok(())
    }

    pub fn finish_claim_reply(&self, claim: &RunClaim, id: &str, state: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE messages SET state=?5 WHERE id=?4 AND task_id=?1 AND state='streaming' AND EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id,id,state],
        )?;
        Ok(())
    }

    pub fn add_claim_permission(
        &self,
        claim: &RunClaim,
        call: &str,
        title: &str,
        action: &serde_json::Value,
    ) -> Result<String> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let id = uuid::Uuid::new_v4().to_string();
        let changed = tx.execute(
            "INSERT INTO permissions(id,task_id,title,action,run_token) SELECT ?4,?1,?5,?6,?2 WHERE EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id,id,title,action.to_string()],
        )?;
        ensure!(changed == 1, "This run no longer owns the work");
        let linked = tx.execute(
            "UPDATE tool_attempts SET permission_id=?4 WHERE task_id=?1 AND run_token=?2 AND call_id=?3 AND status='pending' AND permission_id IS NULL",
            params![claim.task_id,claim.token,call,id],
        )?;
        ensure!(linked == 1, "Tool approval lacks a matching pending action");
        tx.commit()?;
        Ok(id)
    }

    pub fn claim_permission_decision(&self, claim: &RunClaim, id: &str) -> Result<String> {
        if !self.claim_active(claim)? {
            return Ok("cancelled".into());
        }
        Ok(self.connection()?.query_row(
            "SELECT decision FROM permissions WHERE id=?1 AND task_id=?2 AND run_token=?3",
            params![id, claim.task_id, claim.token],
            |r| r.get(0),
        )?)
    }
}
