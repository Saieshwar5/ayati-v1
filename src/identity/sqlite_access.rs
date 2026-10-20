//! Existing local storage operations extracted from HTTP handlers for backend parity.
use super::store::{Store, User, now, role};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
impl Store {
    pub fn role(&self, workspace: &str, actor: &str) -> Result<String> {
        role(&self.0.lock().unwrap(), workspace, actor)
    }
    pub fn agent_access(&self, user: &str, workspace: &str, agent: &str) -> Result<bool> {
        super::chat::store::agent_access(&self.0.lock().unwrap(), user, workspace, agent)
    }
    pub fn directory(&self, user: &User, workspace: &str) -> Result<Value> {
        let db = self.0.lock().unwrap();
        let own_role = role(&db, workspace, &user.id)?;
        let users=db.prepare("SELECT u.id,u.email,m.role FROM users u JOIN members m ON m.user=u.id WHERE m.workspace=?1")?.query_map([workspace],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let grants = if own_role == "Owner" {
            db.prepare("SELECT user,agent FROM chat_access WHERE workspace=?1")?
                .query_map([workspace], |r| {
                    Ok(json!({"user":r.get::<_,String>(0)?,"agent":r.get::<_,String>(1)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            vec![]
        };
        Ok(json!({"users":users,"grants":grants,"role":own_role}))
    }
    pub fn resource_allowed(
        &self,
        user: &User,
        workspace: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<String>> {
        let db = self.0.lock().unwrap();
        role(&db, workspace, &user.id)?;
        Ok(db.query_row("SELECT access FROM resource_grants WHERE workspace=?1 AND user=?2 AND kind=?3 AND resource=?4",params![workspace,user.id,kind,id],|r|r.get(0)).optional()?)
    }
    pub fn resource_ids(&self, user: &User, workspace: &str, kind: &str) -> Result<Vec<String>> {
        let db = self.0.lock().unwrap();
        role(&db, workspace, &user.id)?;
        Ok(db
            .prepare(
                "SELECT resource FROM resource_grants WHERE workspace=?1 AND user=?2 AND kind=?3",
            )?
            .query_map(params![workspace, user.id, kind], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn resource_grants(&self, user: &User, workspace: &str) -> Result<Vec<Value>> {
        let db = self.0.lock().unwrap();
        ensure!(
            role(&db, workspace, &user.id)? == "Owner",
            "Only the owner can manage resource access"
        );
        Ok(db.prepare("SELECT user,kind,resource,access FROM resource_grants WHERE workspace=?1")?.query_map([workspace],|r|Ok(json!({"user":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"resource":r.get::<_,String>(2)?,"access":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn resource_grant(
        &self,
        user: &User,
        workspace: &str,
        target: &str,
        kind: &str,
        resource: &str,
        access: Option<&str>,
    ) -> Result<()> {
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        ensure!(
            role(&tx, workspace, &user.id)? == "Owner",
            "Only the owner can share resources"
        );
        role(&tx, workspace, target)?;
        tx.execute(
            "DELETE FROM member_app_sessions WHERE workspace=?1 AND user=?2 AND app=?3",
            params![workspace, target, resource],
        )?;
        if let Some(access) = access {
            tx.execute("INSERT INTO resource_grants VALUES(?1,?2,?3,?4,?5) ON CONFLICT(workspace,user,kind,resource) DO UPDATE SET access=excluded.access",params![workspace,target,kind,resource,access])?;
        } else {
            tx.execute("DELETE FROM resource_grants WHERE workspace=?1 AND user=?2 AND kind=?3 AND resource=?4",params![workspace,target,kind,resource])?;
        }
        tx.execute("INSERT INTO audit(actor,workspace,action,target,at) VALUES(?1,?2,'resource_access_changed',?3,?4)",params![user.id,workspace,format!("{target}/{resource}"),now()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn app_session_bound(
        &self,
        user: &User,
        workspace: &str,
        app: &str,
        value: &str,
        document: bool,
    ) -> Result<bool> {
        let db = self.0.lock().unwrap();
        role(&db, workspace, &user.id)?;
        let sql = if document {
            "SELECT EXISTS(SELECT 1 FROM member_app_sessions WHERE workspace=?1 AND user=?2 AND app=?3 AND document=?4 AND expires>?5)"
        } else {
            "SELECT EXISTS(SELECT 1 FROM member_app_sessions WHERE workspace=?1 AND user=?2 AND app=?3 AND hash=?4 AND expires>?5)"
        };
        Ok(
            db.query_row(sql, params![workspace, user.id, app, value, now()], |r| {
                r.get(0)
            })?,
        )
    }
    pub fn app_session_save(
        &self,
        user: &User,
        workspace: &str,
        app: &str,
        hash: &str,
        document: &str,
    ) -> Result<()> {
        let db = self.0.lock().unwrap();
        role(&db, workspace, &user.id)?;
        db.execute("DELETE FROM member_app_sessions WHERE expires<=?1", [now()])?;
        db.execute(
            "INSERT INTO member_app_sessions VALUES(?1,?2,?3,?4,?5,?6)",
            params![workspace, user.id, app, hash, document, now() + 900],
        )?;
        Ok(())
    }
}
