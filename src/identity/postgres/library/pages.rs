use super::*;
impl PgStore {
    pub async fn library_save(&self, user: &User, w: &str, input: &SavePage) -> Result<Value> {
        uuid(&input.id)?;
        title(&input.title)?;
        let text = document(&input.document)?;
        let payload = json!({"save":input});
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if input.revision > 0 {
            ensure!(
                item(&tx, user, w, &input.id, false).await?["editable"] == true,
                "Only editors can save this page"
            );
        }
        if let Some(saved) = retry(&tx, user, w, &input.request, &payload).await? {
            item(&tx, user, w, &input.id, false).await?;
            tx.commit().await?;
            return Ok(saved);
        }
        if input.revision == 0 {
            let count: i64 = query(
                &tx,
                "SELECT (SELECT count(*) FROM relay_library_items WHERE workspace=$1)+(SELECT count(*) FROM relay_library_uploads WHERE workspace=$1 AND state='pending' AND expires>$2) AS n",
                vec![w.into(),now().into()],
            )
            .await?
            .unwrap()
            .try_get("", "n")?;
            ensure!(
                count < 1000,
                "This workspace's initial Library limit is 1,000 items"
            );
            ensure!(
                query(
                    &tx,
                    "SELECT id FROM relay_library_items WHERE workspace=$1 AND id=$2 UNION ALL SELECT id FROM relay_library_uploads WHERE workspace=$1 AND id=$2 LIMIT 1",
                    vec![w.into(), input.id.clone().into()]
                )
                .await?
                .is_none(),
                "Item ID already exists"
            );
            if let Some(folder) = &input.folder {
                super::folders::open_folder(&tx, user, w, folder).await?;
            }
            execute(&tx,"INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,document,text,created,updated,folder) VALUES($1,$2,'page',$3,$4,$4,$5,$6,$7,$7,$8)",vec![w.into(),input.id.clone().into(),input.title.trim().into(),user.id.clone().into(),input.document.clone().into(),text.clone().into(),now().into(),input.folder.clone().into()]).await?;
        } else {
            ensure!(input.folder.is_none(), "Move a page to change its folder");
            let current = item(&tx, user, w, &input.id, false).await?;
            ensure!(current["kind"] == "page", "This item is not a page");
            if current["revision"].as_i64() != Some(input.revision) {
                return Err(Conflict.into());
            }
            execute(&tx,"UPDATE relay_library_items SET title=$3,document=$4,text=$5,revision=revision+1,updated=$6 WHERE workspace=$1 AND id=$2",vec![w.into(),input.id.clone().into(),input.title.trim().into(),input.document.clone().into(),text.clone().into(),now().into()]).await?;
        }
        super::passages::index_item(&tx, w, &input.id).await?;
        let result = json!({"id":input.id,"revision":input.revision+1});
        execute(&tx,"INSERT INTO relay_library_versions(workspace,item,revision,author,title,document,text,created) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",vec![w.into(),input.id.clone().into(),(input.revision+1).into(),user.id.clone().into(),input.title.trim().into(),input.document.clone().into(),text.into(),now().into()]).await?;
        prune_versions(&tx, w, &input.id).await?;
        remember(&tx, user, w, &input.request, payload, &result).await?;
        audit(&tx, &user.id, Some(w), "library_page_saved", &input.id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn library_versions(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        ensure!(
            item(&tx, user, w, id, false).await?["editable"] == true,
            "History requires edit access"
        );
        // Before schema 74 versions had no agent.
        let agent = if super::agent_tools::reads_exist(&tx).await?
            && query(
                &tx,
                "SELECT to_regclass('relay_library_suggestions') IS NOT NULL AS present",
                vec![],
            )
            .await?
            .context("Schema check")?
            .try_get::<bool>("", "present")?
        {
            ",'agent',(SELECT a.name FROM relay_shared_agents a WHERE a.workspace=v.workspace AND a.id=v.agent)"
        } else {
            ""
        };
        let versions=json_rows(&tx,&format!("SELECT jsonb_build_object('revision',v.revision,'title',v.title,'created',v.created,'author',COALESCE(u.display_name,u.email){agent}) AS data FROM relay_library_versions v JOIN relay_users u ON u.id=v.author WHERE v.workspace=$1 AND v.item=$2 ORDER BY revision DESC LIMIT 50"),vec![w.into(),id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"versions":versions}))
    }
    pub async fn library_version(
        &self,
        user: &User,
        w: &str,
        id: &str,
        version: i64,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        ensure!(
            current["editable"] == true || current["revision"].as_i64() == Some(version),
            "History requires edit access"
        );
        let value=query(&tx,"SELECT to_jsonb(v)-'text' AS data FROM relay_library_versions v WHERE workspace=$1 AND item=$2 AND revision=$3",vec![w.into(),id.into(),version.into()]).await?.context("Version unavailable")?.try_get::<Value>("","data")?;
        tx.commit().await?;
        Ok(value)
    }
}

pub(super) async fn prune_versions(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    // Everything on legal hold keeps every version (LB7).
    if super::keeping::held(db, w, id).await? {
        return Ok(());
    }
    // Versions cited by Ask sources or agent reads (schema 69) stay verifiable.
    let reads = if super::agent_tools::reads_exist(db).await? {
        " AND NOT EXISTS(SELECT 1 FROM relay_library_reads r WHERE r.workspace=v.workspace AND r.item=v.item AND r.revision=v.revision)"
    } else {
        ""
    };
    execute(db,&format!("DELETE FROM relay_library_versions v WHERE workspace=$1 AND item=$2 AND revision NOT IN (SELECT revision FROM relay_library_versions WHERE workspace=$1 AND item=$2 ORDER BY revision DESC LIMIT 50) AND NOT EXISTS(SELECT 1 FROM relay_library_sources s WHERE s.workspace=v.workspace AND s.item=v.item AND s.revision=v.revision){reads}"),vec![w.into(),id.into()]).await?;
    Ok(())
}
