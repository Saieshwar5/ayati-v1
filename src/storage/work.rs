use super::{Store, Task, runtime::LOCAL_CONVERSATION};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RunClaim {
    pub task_id: String,
    pub token: String,
    pub owner_id: String,
}

#[derive(Debug)]
pub enum IntakeError {
    Conflict,
    Full,
}
impl std::fmt::Display for IntakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Conflict => "This request ID was already used for different content",
            Self::Full => "Too many pending requests; please wait or stop one",
        })
    }
}
impl std::error::Error for IntakeError {}

impl Store {
    pub(super) fn migrate_work(connection: &mut rusqlite::Connection, version: i64) -> Result<()> {
        if version < 3 {
            let tx = connection.transaction()?;
            tx.execute_batch(include_str!("work.sql"))?;
            tx.commit()?;
        }
        let tx = connection.transaction()?;
        tx.execute("UPDATE tasks SET status='interrupted',detail='Ayati restarted during work; check action outcomes before continuing.' WHERE status='running'", [])?;
        tx.execute(
            "UPDATE messages SET state='interrupted' WHERE state='streaming'",
            [],
        )?;
        tx.execute(
            "UPDATE permissions SET decision='cancelled' WHERE decision='pending'",
            [],
        )?;
        Self::close_pending_actions(&tx, None)?;
        tx.commit()?;
        Ok(())
    }

    pub fn enqueue(&self, text: &str) -> Result<Task> {
        self.accept_input(text, None)
    }

    pub fn accept_input(&self, text: &str, request_id: Option<&str>) -> Result<Task> {
        ensure!(
            !text.trim().is_empty() && text.len() <= 16000,
            "Message must contain 1–16000 bytes"
        );
        let request = request_id
            .map(Uuid::parse_str)
            .transpose()
            .context("request_id must be a UUID")?
            .unwrap_or_else(Uuid::new_v4)
            .to_string();
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, String)> = tx.query_row(
            "SELECT r.task_id,m.content FROM input_receipts r JOIN messages m ON m.task_id=r.task_id AND m.role='user' WHERE r.conversation_id=?1 AND r.request_id=?2",
            params![LOCAL_CONVERSATION, request], |r| Ok((r.get(0)?,r.get(1)?)),
        ).optional()?;
        let id = if let Some((id, previous)) = previous {
            if previous != text {
                return Err(IntakeError::Conflict.into());
            }
            id
        } else {
            let pending: i64 = tx.query_row(
                "SELECT COUNT(*) FROM tasks WHERE status IN ('queued','running')",
                [],
                |r| r.get(0),
            )?;
            if pending >= 32 {
                return Err(IntakeError::Full.into());
            }
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO tasks(id,status,context) VALUES(?1,'queued','[]')",
                [&id],
            )?;
            tx.execute("INSERT INTO messages(id,task_id,role,content,state) VALUES(?1,?2,'user',?3,'complete')", params![Uuid::new_v4().to_string(),id,text])?;
            tx.execute(
                "INSERT INTO run_scopes(task_id,conversation_id) VALUES(?1,?2)",
                params![id, LOCAL_CONVERSATION],
            )?;
            tx.execute(
                "INSERT INTO input_receipts(conversation_id,request_id,task_id) VALUES(?1,?2,?3)",
                params![LOCAL_CONVERSATION, request, id],
            )?;
            tx.execute("INSERT INTO work_transitions(task_id,to_status,detail) VALUES(?1,'queued','Durably accepted')", [&id])?;
            id
        };
        tx.commit()?;
        drop(connection);
        self.task(&id)?.context("Task was not saved")
    }

    pub fn claim_next(&self, owner: &str) -> Result<Option<RunClaim>> {
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: Option<String> = tx.query_row(
            "SELECT t.id FROM tasks t JOIN input_receipts r ON r.task_id=t.id WHERE t.status='queued' AND NOT EXISTS(SELECT 1 FROM tasks WHERE status='running') ORDER BY t.rowid LIMIT 1",
            [], |r| r.get(0),
        ).optional()?;
        let Some(id) = id else {
            return Ok(None);
        };
        let claim = RunClaim {
            task_id: id,
            token: Uuid::new_v4().to_string(),
            owner_id: owner.into(),
        };
        tx.execute(
            "INSERT INTO run_claims(task_id,token,owner_id) VALUES(?1,?2,?3)",
            params![claim.task_id, claim.token, claim.owner_id],
        )?;
        tx.execute(
            "UPDATE tasks SET status='running',detail='Thinking' WHERE id=?1 AND status='queued'",
            [&claim.task_id],
        )?;
        tx.commit()?;
        Ok(Some(claim))
    }

    pub fn claim_active(&self, claim: &RunClaim) -> Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id], |r| r.get(0),
        )?)
    }

    pub fn finish_work(&self, claim: &RunClaim, status: &str, detail: &str) -> Result<bool> {
        ensure!(
            matches!(status, "completed" | "failed" | "stopped"),
            "Invalid final work state"
        );
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks t JOIN run_claims c ON c.task_id=t.id WHERE t.id=?1 AND t.status='running' AND c.token=?2 AND c.owner_id=?3)",
            params![claim.task_id,claim.token,claim.owner_id], |r| r.get(0),
        )?;
        if !active {
            return Ok(false);
        }
        tx.execute(
            "UPDATE permissions SET decision='cancelled' WHERE task_id=?1 AND decision='pending'",
            [&claim.task_id],
        )?;
        Self::close_pending_actions(&tx, Some(&claim.task_id))?;
        let uncertain: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tool_attempts WHERE task_id=?1 AND status='uncertain')",
            [&claim.task_id],
            |r| r.get(0),
        )?;
        let (status, detail) = if status == "completed" && uncertain {
            (
                "interrupted",
                "A tool outcome is uncertain; inspect its action record before continuing.",
            )
        } else {
            (status, detail)
        };
        tx.execute(
            "UPDATE tasks SET status=?2,detail=?3 WHERE id=?1",
            params![claim.task_id, status, detail],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn stop_work(&self, task: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        tx.execute("UPDATE tasks SET status='stopped',detail='Stopped by you' WHERE id=?1 AND status IN ('queued','running')", [task])?;
        tx.execute(
            "UPDATE permissions SET decision='cancelled' WHERE task_id=?1 AND decision='pending'",
            [task],
        )?;
        Self::close_pending_actions(&tx, Some(task))?;
        tx.execute(
            "UPDATE messages SET state='stopped' WHERE task_id=?1 AND state='streaming'",
            [task],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn interrupt_active_work(&self) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        tx.execute("UPDATE tasks SET status='interrupted',detail='Service stopped during work; inspect action outcomes before continuing.' WHERE status='running'", [])?;
        tx.execute(
            "UPDATE permissions SET decision='cancelled' WHERE decision='pending'",
            [],
        )?;
        Self::close_pending_actions(&tx, None)?;
        tx.execute(
            "UPDATE messages SET state='interrupted' WHERE state='streaming'",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn stop_all_work(&self) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        tx.execute("UPDATE tasks SET status='stopped',detail='Agent VM stopped by you' WHERE status IN ('queued','running')", [])?;
        tx.execute(
            "UPDATE permissions SET decision='cancelled' WHERE decision='pending'",
            [],
        )?;
        Self::close_pending_actions(&tx, None)?;
        tx.execute(
            "UPDATE messages SET state='stopped' WHERE state='streaming'",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
}
