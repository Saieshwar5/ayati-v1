use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub async fn power_down(path: &Path) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let stream = UnixStream::connect(path).await?;
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        ensure!(
            lines.next_line().await?.is_some(),
            "QMP closed before greeting"
        );
        for (id, command) in [(1, "qmp_capabilities"), (2, "system_powerdown")] {
            write
                .write_all(format!("{}\n", json!({"execute":command,"id":id})).as_bytes())
                .await?;
            loop {
                let line = lines
                    .next_line()
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("QMP closed"))?;
                let value: Value = serde_json::from_str(&line)?;
                if value.get("id") == Some(&json!(id)) {
                    ensure!(value.get("error").is_none(), "QMP power control failed");
                    break;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}
