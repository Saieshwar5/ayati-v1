//! Workspace profiles: job title, short introduction and photo. Every member
//! can read every profile; only the person can change their own.
use super::*;
use serde_json::json;

pub const PHOTO_LIMIT: usize = 2 * 1024 * 1024;

fn clean(value: &str, limit: usize, label: &str) -> Result<String> {
    let value = value.trim();
    ensure!(
        value.chars().count() <= limit && !value.chars().any(|c| c.is_control() && c != '\n'),
        "Use a {label} of at most {limit} characters"
    );
    Ok(value.into())
}

impl PgStore {
    /// Saves the caller's name (account-wide) with their title and introduction
    /// (this workspace only) in one change.
    pub async fn save_member_profile(
        &self,
        user: &User,
        w: &str,
        display_name: &str,
        title: &str,
        about: &str,
    ) -> Result<Value> {
        let name = crate::identity::profile::validate(display_name)?;
        let title = clean(title, 80, "job title")?;
        let about = clean(about, 200, "short introduction")?;
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        execute(
            &tx,
            "UPDATE relay_users SET display_name=$1 WHERE id=$2",
            vec![name.into(), user.id.clone().into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_member_profiles(workspace,user_id,title,about) VALUES($1,$2,$3,$4)
             ON CONFLICT(workspace,user_id) DO UPDATE SET title=EXCLUDED.title,about=EXCLUDED.about,
             revision=relay_member_profiles.revision+1",
            vec![w.into(), user.id.clone().into(), title.into(), about.into()],
        )
        .await?;
        audit(&tx, &user.id, Some(w), "profile_updated", &user.id).await?;
        let profile = member_profile(&tx, w, &user.id).await?;
        tx.commit().await?;
        Ok(profile)
    }

    /// Replaces the caller's photo. The new ID changes the photo's address so
    /// browsers never show a cached older picture.
    pub async fn save_member_photo(
        &self,
        user: &User,
        w: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<Value> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= PHOTO_LIMIT,
            "Choose a photo of at most 2 MB"
        );
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let photo = uuid::Uuid::new_v4().to_string();
        execute(
            &tx,
            "INSERT INTO relay_member_profiles(workspace,user_id,photo,photo_mime,photo_bytes) VALUES($1,$2,$3,$4,$5)
             ON CONFLICT(workspace,user_id) DO UPDATE SET photo=EXCLUDED.photo,photo_mime=EXCLUDED.photo_mime,
             photo_bytes=EXCLUDED.photo_bytes,revision=relay_member_profiles.revision+1",
            vec![
                w.into(),
                user.id.clone().into(),
                photo.into(),
                mime.into(),
                bytes.into(),
            ],
        )
        .await?;
        audit(&tx, &user.id, Some(w), "profile_photo_updated", &user.id).await?;
        let profile = member_profile(&tx, w, &user.id).await?;
        tx.commit().await?;
        Ok(profile)
    }

    pub async fn remove_member_photo(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        execute(
            &tx,
            "UPDATE relay_member_profiles SET photo=NULL,photo_mime=NULL,photo_bytes=NULL,revision=revision+1
             WHERE workspace=$1 AND user_id=$2 AND photo IS NOT NULL",
            vec![w.into(), user.id.clone().into()],
        )
        .await?;
        let profile = member_profile(&tx, w, &user.id).await?;
        tx.commit().await?;
        Ok(profile)
    }

    /// Any member may view any member's current photo.
    pub async fn member_photo(
        &self,
        user: &User,
        w: &str,
        member: &str,
        photo: &str,
    ) -> Result<(String, Vec<u8>)> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let row = query(
            &tx,
            "SELECT photo_mime,photo_bytes FROM relay_member_profiles WHERE workspace=$1 AND user_id=$2 AND photo=$3",
            vec![w.into(), member.into(), photo.into()],
        )
        .await?
        .context("Photo unavailable")?;
        let result = (
            row.try_get("", "photo_mime")?,
            row.try_get("", "photo_bytes")?,
        );
        tx.commit().await?;
        Ok(result)
    }
}

async fn member_profile(db: &impl ConnectionTrait, w: &str, user: &str) -> Result<Value> {
    let row = query(
        db,
        "SELECT COALESCE(u.display_name,u.email) AS name,COALESCE(p.title,'') AS title,
         COALESCE(p.about,'') AS about,p.photo FROM relay_users u
         LEFT JOIN relay_member_profiles p ON p.workspace=$1 AND p.user_id=u.id WHERE u.id=$2",
        vec![w.into(), user.into()],
    )
    .await?
    .context("Account unavailable")?;
    Ok(json!({
        "id": user,
        "name": row.try_get::<String>("", "name")?,
        "title": row.try_get::<String>("", "title")?,
        "about": row.try_get::<String>("", "about")?,
        "photo": row.try_get::<Option<String>>("", "photo")?,
    }))
}
