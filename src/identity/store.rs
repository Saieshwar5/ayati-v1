use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, sync::Mutex};

pub struct Store(pub Mutex<Connection>);
#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn audit(db: &Connection, actor: &str, workspace: &str, action: &str, target: &str) -> Result<()> {
    db.execute(
        "INSERT INTO audit(actor,workspace,action,target,at) VALUES(?1,?2,?3,?4,?5)",
        params![actor, workspace, action, target, now()],
    )?;
    Ok(())
}
pub fn role(db: &Connection, workspace: &str, actor: &str) -> Result<String> {
    db.query_row(
        "SELECT role FROM members WHERE workspace=?1 AND user=?2",
        params![workspace, actor],
        |r| r.get(0),
    )
    .optional()?
    .context("Workspace access denied")
}
fn manager(db: &Connection, workspace: &str, actor: &str) -> Result<String> {
    let value = role(db, workspace, actor)?;
    ensure!(
        value == "Owner" || value == "Admin",
        "Only owners and admins can manage people"
    );
    Ok(value)
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY, provider TEXT UNIQUE NOT NULL, email TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions(hash TEXT PRIMARY KEY, user TEXT NOT NULL REFERENCES users(id),
                provider_session TEXT NOT NULL, expires INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS workspaces(id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS members(workspace TEXT REFERENCES workspaces(id), user TEXT REFERENCES users(id),
                role TEXT NOT NULL CHECK(role IN ('Owner','Admin','Member')), PRIMARY KEY(workspace,user));
            CREATE TABLE IF NOT EXISTS invitations(id TEXT PRIMARY KEY, workspace TEXT REFERENCES workspaces(id),
                email TEXT NOT NULL, role TEXT NOT NULL, inviter TEXT NOT NULL, expires INTEGER NOT NULL,
                status TEXT NOT NULL, provider TEXT);
            CREATE UNIQUE INDEX IF NOT EXISTS pending_invite ON invitations(workspace,email) WHERE status='pending';
            CREATE TABLE IF NOT EXISTS audit(id INTEGER PRIMARY KEY, actor TEXT, workspace TEXT, action TEXT, target TEXT, at INTEGER);")?;
        Ok(Self(Mutex::new(db)))
    }
    pub fn login(&self, provider: &str, email: &str, hash: &str, sid: &str) -> Result<User> {
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let id: String = tx
            .query_row("SELECT id FROM users WHERE provider=?1", [provider], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        tx.execute(
            "INSERT INTO users(id,provider,email) VALUES(?1,?2,?3)
            ON CONFLICT(provider) DO UPDATE SET email=excluded.email",
            params![id, provider, email],
        )?;
        tx.execute("DELETE FROM sessions WHERE expires<=?1", [now()])?;
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3,?4)",
            params![hash, id, sid, now() + 3600],
        )?;
        audit(&tx, &id, "", "sign_in", "")?;
        tx.commit()?;
        Ok(User {
            id,
            email: email.into(),
        })
    }
    pub fn session(&self, hash: &str) -> Result<Option<(User, String, String)>> {
        let db = self.0.lock().unwrap();
        Ok(db
            .query_row(
                "SELECT u.id,u.email,u.provider,s.provider_session FROM sessions s
            JOIN users u ON s.user=u.id WHERE s.hash=?1 AND s.expires>?2",
                params![hash, now()],
                |r| {
                    Ok((
                        User {
                            id: r.get(0)?,
                            email: r.get(1)?,
                        },
                        r.get(2)?,
                        r.get(3)?,
                    ))
                },
            )
            .optional()?)
    }
    pub fn logout(&self, hash: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .execute("DELETE FROM sessions WHERE hash=?1", [hash])?;
        Ok(())
    }
    pub fn snapshot(&self, user: &User) -> Result<Value> {
        let db = self.0.lock().unwrap();
        let mut q = db.prepare("SELECT w.id,w.name,m.role FROM workspaces w JOIN members m ON m.workspace=w.id WHERE m.user=?1 ORDER BY w.rowid")?;
        let businesses = q.query_map([&user.id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?})))?
            .collect::<Result<Vec<_>,_>>()?;
        let mut q = db.prepare("SELECT i.id,w.name,i.role FROM invitations i JOIN workspaces w ON w.id=i.workspace WHERE i.email=?1 AND i.status='pending' AND i.expires>?2 AND i.provider IS NOT NULL")?;
        let invitations = q.query_map(params![user.email,now()], |r| Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?})))?
            .collect::<Result<Vec<_>,_>>()?;
        Ok(json!({"user":user,"workspaces":businesses,"invitations":invitations}))
    }
    pub fn create(&self, user: &User, name: &str, id: &str) -> Result<String> {
        ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid request ID");
        ensure!(
            !name.trim().is_empty()
                && name.trim().len() <= 80
                && !name.chars().any(char::is_control),
            "Use a workspace name between 1 and 80 characters"
        );
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let existing: Option<String> = tx
            .query_row("SELECT name FROM workspaces WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(existing) = existing {
            ensure!(
                role(&tx, id, &user.id)? == "Owner" && existing == name.trim(),
                "Request ID already used"
            );
            return Ok(id.into());
        }
        tx.execute(
            "INSERT INTO workspaces VALUES(?1,?2)",
            params![id, name.trim()],
        )?;
        tx.execute(
            "INSERT INTO members VALUES(?1,?2,'Owner')",
            params![id, user.id],
        )?;
        audit(&tx, &user.id, id, "workspace_created", id)?;
        tx.commit()?;
        Ok(id.into())
    }
    pub fn people(&self, user: &User, workspace: &str) -> Result<Value> {
        let db = self.0.lock().unwrap();
        let own_role = role(&db, workspace, &user.id)?;
        let mut q = db.prepare("SELECT u.id,u.email,m.role FROM members m JOIN users u ON u.id=m.user WHERE m.workspace=?1 ORDER BY m.rowid")?;
        let members = q.query_map([workspace], |r| Ok(json!({"id":r.get::<_,String>(0)?,"email":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?})))?
            .collect::<Result<Vec<_>,_>>()?;
        let mut invitations = Vec::new();
        if own_role != "Member" {
            let mut q = db.prepare("SELECT id,email,role,status,expires FROM invitations WHERE workspace=?1 AND status NOT IN ('cancelled','accepted') ORDER BY rowid DESC")?;
            invitations = q.query_map([workspace], |r| Ok(json!({"id":r.get::<_,String>(0)?,"email":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"expires":r.get::<_,i64>(4)?})))?
                .collect::<Result<Vec<_>,_>>()?;
        }
        Ok(json!({"members":members,"invitations":invitations,"role":own_role}))
    }
    pub fn reserve_invite(
        &self,
        user: &User,
        workspace: &str,
        email: &str,
        assigned: &str,
    ) -> Result<String> {
        ensure!(
            email.len() <= 254 && email.contains('@') && !email.chars().any(char::is_whitespace),
            "Enter a valid email"
        );
        ensure!(["Admin", "Member"].contains(&assigned), "Invalid role");
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let own_role = manager(&tx, workspace, &user.id)?;
        ensure!(
            assigned != "Admin" || own_role == "Owner",
            "Only owners can appoint admins"
        );
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM members m JOIN users u ON u.id=m.user WHERE m.workspace=?1 AND u.email=?2)", params![workspace,email], |r|r.get(0))?;
        ensure!(!exists, "This person is already a member");
        tx.execute("UPDATE invitations SET status='expired' WHERE workspace=?1 AND expires<=?2 AND status='pending'",params![workspace,now()])?;
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO invitations VALUES(?1,?2,?3,?4,?5,?6,'pending',NULL)",
            params![id, workspace, email, assigned, user.id, now() + 604800],
        )
        .context("An invitation is already pending")?;
        audit(&tx, &user.id, workspace, "invitation_created", &id)?;
        tx.commit()?;
        Ok(id)
    }
    pub fn delivered(&self, id: &str, provider: Option<&str>) -> Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE invitations SET provider=?2,status=?3 WHERE id=?1 AND status='pending'",
            params![
                id,
                provider,
                if provider.is_some() {
                    "pending"
                } else {
                    "delivery_unknown"
                }
            ],
        )?;
        Ok(())
    }
    pub fn accept(&self, user: &User, id: &str) -> Result<String> {
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let (workspace,email,assigned,inviter,expires,status,provider): (String,String,String,String,i64,String,Option<String>) =
            tx.query_row("SELECT workspace,email,role,inviter,expires,status,provider FROM invitations WHERE id=?1", [id],
                |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
        ensure!(
            email == user.email && status == "pending" && expires > now() && provider.is_some(),
            "Invitation unavailable for this account"
        );
        let current = manager(&tx, &workspace, &inviter)?;
        ensure!(
            assigned != "Admin" || current == "Owner",
            "Inviter no longer has authority"
        );
        tx.execute(
            "INSERT INTO members VALUES(?1,?2,?3) ON CONFLICT(workspace,user) DO NOTHING",
            params![workspace, user.id, assigned],
        )?;
        tx.execute("UPDATE invitations SET status='accepted' WHERE id=?1", [id])?;
        audit(&tx, &user.id, &workspace, "invitation_accepted", id)?;
        tx.commit()?;
        Ok(workspace)
    }
    pub fn cancel(&self, user: &User, workspace: &str, id: &str) -> Result<Option<String>> {
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        manager(&tx, workspace, &user.id)?;
        let provider = tx.query_row(
            "SELECT provider FROM invitations WHERE id=?1 AND workspace=?2",
            params![id, workspace],
            |r| r.get(0),
        )?;
        tx.execute("UPDATE invitations SET status='cancelled' WHERE id=?1 AND workspace=?2 AND status!='accepted'",params![id,workspace])?;
        audit(&tx, &user.id, workspace, "invitation_cancelled", id)?;
        tx.commit()?;
        Ok(provider)
    }
    pub fn change(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        assigned: Option<&str>,
    ) -> Result<()> {
        ensure!(
            assigned.is_none_or(|r| ["Admin", "Member"].contains(&r)),
            "Invalid role"
        );
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let own_role = manager(&tx, workspace, &user.id)?;
        let target_role = role(&tx, workspace, target)?;
        ensure!(
            target_role != "Owner",
            "The owner cannot be removed or demoted"
        );
        ensure!(
            own_role == "Owner" || (target_role == "Member" && assigned != Some("Admin")),
            "Only owners can manage admins"
        );
        if let Some(assigned) = assigned {
            tx.execute(
                "UPDATE members SET role=?3 WHERE workspace=?1 AND user=?2",
                params![workspace, target, assigned],
            )?;
        } else {
            tx.execute(
                "DELETE FROM members WHERE workspace=?1 AND user=?2",
                params![workspace, target],
            )?;
        }
        audit(&tx, &user.id, workspace, "membership_changed", target)?;
        tx.commit()?;
        Ok(())
    }
}
