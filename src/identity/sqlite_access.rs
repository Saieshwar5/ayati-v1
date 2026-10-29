//! Existing local storage operations extracted from HTTP handlers for backend parity.
use super::store::{Store, User, role};
use anyhow::Result;
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
        let users=db.prepare("SELECT u.id,COALESCE(u.display_name,u.email),m.role,u.email FROM users u JOIN members m ON m.user=u.id WHERE m.workspace=?1")?.query_map([workspace],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?,"email":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
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
}
