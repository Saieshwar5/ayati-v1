use super::*;
impl PgStore {
    pub async fn library_upload_start(
        &self,
        user: &User,
        w: &str,
        input: &UploadFile,
    ) -> Result<Value> {
        uuid(&input.id)?;
        title(&input.title)?;
        ensure!(
            (1..=MAX_FILE as i64).contains(&input.size),
            "Choose a file between 1 byte and 25 MiB"
        );
        ensure!(
            input.checksum.len() == 64 && input.checksum.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid file checksum"
        );
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let payload = json!({"upload":input});
        if let Some(result) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(result);
        }
        ensure!(
            query(
                &tx,
                "SELECT id FROM relay_library_items WHERE workspace=$1 AND id=$2",
                vec![w.into(), input.id.clone().into()]
            )
            .await?
            .is_none(),
            "Upload ID unavailable; start a new upload"
        );
        let count:i64=query(&tx,"SELECT (SELECT count(*) FROM relay_library_items WHERE workspace=$1)+(SELECT count(*) FROM relay_library_uploads WHERE workspace=$1 AND state='pending' AND expires>$2) AS n",vec![w.into(),now().into()]).await?.unwrap().try_get("","n")?;
        ensure!(
            count < 1000,
            "This workspace's initial Library limit is 1,000 items"
        );
        let used:i64=query(&tx,"SELECT (SELECT coalesce(sum(size),0)::bigint FROM relay_library_items WHERE workspace=$1)+(SELECT coalesce(sum(size),0)::bigint FROM relay_library_uploads WHERE workspace=$1 AND state='pending' AND expires>$2) AS used",vec![w.into(),now().into()]).await?.unwrap().try_get("","used")?;
        ensure!(
            used + input.size <= WORKSPACE_QUOTA,
            "Library storage is full (1 GiB including retained files)"
        );
        let key = format!("library/{w}/{}", input.id);
        execute(&tx,"INSERT INTO relay_library_uploads(workspace,id,actor,title,size,checksum,object_key,expires) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",vec![w.into(),input.id.clone().into(),user.id.clone().into(),input.title.clone().into(),input.size.into(),input.checksum.to_ascii_lowercase().into(),key.into(),(now()+86400).into()]).await?;
        let result = json!({"id":input.id});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn library_upload_session(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let data=query(&tx,"SELECT to_jsonb(u) AS data FROM relay_library_uploads u WHERE workspace=$1 AND id=$2 AND actor=$3 AND state IN ('pending','stored') AND expires>$4",vec![w.into(),id.into(),user.id.clone().into(),now().into()]).await?.ok_or(Denied)?.try_get("","data")?;
        tx.commit().await?;
        Ok(data)
    }
    pub async fn library_upload_finish(
        &self,
        user: &User,
        w: &str,
        id: &str,
        mime: &str,
        text: &str,
        readiness: &str,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let upload=query(&tx,"SELECT to_jsonb(u) AS data FROM relay_library_uploads u WHERE workspace=$1 AND id=$2 AND actor=$3 AND state IN ('pending','stored') AND expires>$4",vec![w.into(),id.into(),user.id.clone().into(),now().into()]).await?.ok_or(Denied)?.try_get::<Value>("","data")?;
        if upload["state"] == "pending" {
            execute(&tx,"INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,text,object_key,mime,size,checksum,readiness,created,updated) VALUES($1,$2,'file',$3,$4,$4,$5,$6,$7,$8,$9,$10,$11,$11)",vec![w.into(),id.into(),upload["title"].as_str().unwrap().into(),user.id.clone().into(),text.into(),upload["object_key"].as_str().unwrap().into(),mime.into(),upload["size"].as_i64().unwrap().into(),upload["checksum"].as_str().unwrap().into(),readiness.into(),now().into()]).await?;
            execute(
                &tx,
                "UPDATE relay_library_uploads SET state='stored' WHERE workspace=$1 AND id=$2",
                vec![w.into(), id.into()],
            )
            .await?;
            audit(&tx, &user.id, Some(w), "library_file_stored", id).await?;
        }
        let result = public(item(&tx, user, w, id, false).await?);
        tx.commit().await?;
        Ok(result)
    }
    pub async fn library_upload_cancel(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        execute(&tx,"UPDATE relay_library_uploads SET state='cancelled' WHERE workspace=$1 AND id=$2 AND actor=$3 AND state='pending'",vec![w.into(),id.into(),user.id.clone().into()]).await?;
        let state = query(
            &tx,
            "SELECT state FROM relay_library_uploads WHERE workspace=$1 AND id=$2 AND actor=$3",
            vec![w.into(), id.into(), user.id.clone().into()],
        )
        .await?
        .ok_or(Denied)?
        .try_get::<String>("", "state")?;
        tx.commit().await?;
        Ok(json!({"state":state}))
    }
}
