use super::*;
impl PgStore {
    pub async fn library_action(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &ItemAction,
    ) -> Result<Value> {
        ensure!(
            [
                "star",
                "unstar",
                "trash",
                "restore",
                "restore_version",
                "claim"
            ]
            .contains(&input.action.as_str()),
            "Unknown Library action"
        );
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, true).await?;
        let payload = json!({"action":id,"input":input});
        if let Some(result) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(result);
        }
        if input.action == "star" || input.action == "unstar" {
            execute(
                &tx,
                "DELETE FROM relay_library_stars WHERE workspace=$1 AND item=$2 AND user_id=$3",
                vec![w.into(), id.into(), user.id.clone().into()],
            )
            .await?;
            if input.action == "star" {
                execute(
                    &tx,
                    "INSERT INTO relay_library_stars VALUES($1,$2,$3)",
                    vec![w.into(), id.into(), user.id.clone().into()],
                )
                .await?;
            }
        } else {
            if current["revision"].as_i64() != Some(input.revision) {
                return Err(Conflict.into());
            }
            if input.action == "claim" {
                ensure!(
                    current["steward"].is_null() && current["trashed"] == false,
                    "This item already has a sharing manager"
                );
                ensure!(
                    (current["visibility"] == "workspace"
                        && role(&tx, w, &user.id).await? == "Admin")
                        || (current["visibility"] == "selected" && current["editable"] == true),
                    "Only a workspace admin or an existing selected editor can take over this shared item"
                );
                execute(&tx,"UPDATE relay_library_items SET steward=$3,access_revision=access_revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),user.id.clone().into()]).await?;
            } else if input.action == "restore_version" {
                ensure!(
                    current["editable"] == true
                        && current["kind"] == "page"
                        && current["trashed"] == false,
                    "Only page editors can restore versions"
                );
                let version=query(&tx,"SELECT title,document,text FROM relay_library_versions WHERE workspace=$1 AND item=$2 AND revision=$3",vec![w.into(),id.into(),input.version.into()]).await?.context("Version unavailable")?;
                execute(&tx,"UPDATE relay_library_items SET title=$3,document=$4,text=$5,revision=revision+1,updated=$6 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),version.try_get::<String>("","title")?.into(),version.try_get::<Value>("","document")?.into(),version.try_get::<String>("","text")?.into(),now().into()]).await?;
                execute(&tx,"INSERT INTO relay_library_versions SELECT workspace,id,revision,$3,title,document,text,$4 FROM relay_library_items WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),user.id.clone().into(),now().into()]).await?;
            } else {
                ensure!(
                    current["manageable"] == true,
                    "Only the creator can trash or restore an item"
                );
                execute(&tx,"UPDATE relay_library_items SET trashed=$3,access_revision=access_revision+1,revision=revision+1,updated=$4 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),(input.action=="trash").into(),now().into()]).await?;
                execute(&tx,"INSERT INTO relay_library_versions SELECT workspace,id,revision,$3,title,document,text,$4 FROM relay_library_items WHERE workspace=$1 AND id=$2 AND kind='page'",vec![w.into(),id.into(),user.id.clone().into(),now().into()]).await?;
            }
            audit(
                &tx,
                &user.id,
                Some(w),
                &format!("library_{}", input.action),
                id,
            )
            .await?;
        }
        super::pages::prune_versions(&tx, w, id).await?;
        let result = json!({"ok":true});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}
