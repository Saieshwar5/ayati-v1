use super::{Journal, work::RunClaim};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

impl Journal {
    pub(super) fn close_pending_actions(
        connection: &rusqlite::Connection,
        task: Option<&str>,
    ) -> Result<()> {
        connection.execute(
            "UPDATE tool_attempts SET status=CASE WHEN EXISTS(SELECT 1 FROM permissions p WHERE p.id=tool_attempts.permission_id AND p.decision='cancelled') THEN 'cancelled' ELSE 'uncertain' END WHERE (?1 IS NULL OR task_id=?1) AND status IN ('pending','authorized','running')", [task],
        )?;
        Ok(())
    }

    // ACP events describe runtime observations, not independently verified
    // external receipts. Preserve unknown outcomes instead of inferring success.
    pub fn observe_tool(
        &self,
        claim: &RunClaim,
        call: &str,
        name: &str,
        input: &Value,
    ) -> Result<()> {
        ensure!(
            call.len() <= 256 && name.len() <= 256 && input.to_string().len() <= 16000,
            "Tool identity or arguments exceeded the journal limit"
        );
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id], |r| r.get(0),
        )?;
        ensure!(active, "This run no longer owns the work");
        let old: Option<(String,String)> = tx.query_row(
            "SELECT name,arguments FROM tool_attempts WHERE task_id=?1 AND call_id=?2 AND run_token=?3",
            params![claim.task_id,call,claim.token], |r| Ok((r.get(0)?,r.get(1)?)),
        ).optional()?;
        if let Some((previous, arguments)) = old {
            ensure!(
                previous == name && serde_json::from_str::<Value>(&arguments)? == *input,
                "Tool call identity changed after being recorded"
            );
        } else {
            tx.execute(
                "INSERT INTO tool_attempts(id,task_id,call_id,name,arguments,status,run_token) VALUES(?1,?2,?3,?4,?5,'pending',?6)",
                params![uuid::Uuid::new_v4().to_string(),claim.task_id,call,name,input.to_string(),claim.token],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn observe_tool_status(
        &self,
        claim: &RunClaim,
        call: &str,
        status: &str,
        output: Option<&Value>,
    ) -> Result<()> {
        ensure!(
            matches!(
                status,
                "pending" | "running" | "completed" | "failed" | "uncertain"
            ),
            "Invalid observed tool status"
        );
        let output = output.map(|v| {
            let encoded = v.to_string();
            if encoded.len() <= 64 * 1024 { encoded }
            else { serde_json::json!({"omitted":true,"reason":"Tool result exceeded journal limit","bytes":encoded.len()}).to_string() }
        });
        self.connection()?.execute(
            "UPDATE tool_attempts SET status=?4,result=COALESCE(?5,result) WHERE task_id=?1 AND run_token=?2 AND call_id=?3 AND status IN ('pending','authorized','running') AND EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?6)",
            params![claim.task_id,claim.token,call,status,output,claim.owner_id],
        )?;
        Ok(())
    }
}
