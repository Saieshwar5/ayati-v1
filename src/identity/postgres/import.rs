//! Offline, all-or-nothing import. The source is read-only and is never deleted.
use super::*;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::path::Path;

struct Table {
    source: &'static str,
    target: &'static str,
    // Explicit SQL identifiers, never supplied by a user.
    columns: &'static [&'static str],
}
const TABLES: &[Table] = &[
    Table {
        source: "users",
        target: "relay_users",
        columns: &["id", "provider", "email", "display_name"],
    },
    Table {
        source: "workspaces",
        target: "relay_workspaces",
        columns: &["id", "name"],
    },
    Table {
        source: "members",
        target: "relay_members",
        columns: &["workspace", "user", "role"],
    },
    Table {
        source: "invitations",
        target: "relay_invitations",
        columns: &[
            "id",
            "workspace",
            "email",
            "role",
            "inviter",
            "expires",
            "status",
            "provider",
        ],
    },
    Table {
        source: "chats",
        target: "relay_chats",
        columns: &[
            "id",
            "workspace",
            "name",
            "creator",
            "kind",
            "description",
            "direct_key",
        ],
    },
    Table {
        source: "chat_people",
        target: "relay_chat_people",
        columns: &["workspace", "chat", "user"],
    },
    Table {
        source: "chat_agents",
        target: "relay_chat_agents",
        columns: &["workspace", "chat", "agent"],
    },
    Table {
        source: "chat_access",
        target: "relay_chat_access",
        columns: &["workspace", "user", "agent"],
    },
    Table {
        source: "chat_messages",
        target: "relay_chat_messages",
        columns: &[
            "id",
            "workspace",
            "chat",
            "author",
            "kind",
            "content",
            "state",
            "at",
            "target",
            "requester",
            "thread",
        ],
    },
    Table {
        source: "audit",
        target: "relay_audit",
        columns: &["actor", "workspace", "action", "target", "at"],
    },
];

impl PgStore {
    pub async fn import_sqlite(&self, path: &Path) -> Result<Value> {
        let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        source.execute_batch("BEGIN")?;
        let integrity: String = source.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "SQLite source failed integrity check");
        let tx = self.db.begin().await?;
        // Serialize import and reject any existing target data; never merge or overwrite accounts.
        execute(&tx, "SELECT pg_advisory_xact_lock(7826301020)", vec![]).await?;
        for table in TABLES {
            let sql = format!("SELECT count(*) AS total FROM {}", table.target);
            let count: i64 = query(&tx, &sql, vec![])
                .await?
                .context("Missing count")?
                .try_get("", "total")?;
            ensure!(count == 0, "PostgreSQL target is not empty; import refused");
        }
        let has_chat_kinds = source
            .prepare("PRAGMA table_info(chats)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|c| c == "kind");
        let has_display_names = source
            .prepare("PRAGMA table_info(users)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|c| c == "display_name");
        let has_threads = source
            .prepare("PRAGMA table_info(chat_messages)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|c| c == "thread");
        let mut counts = serde_json::Map::new();
        for table in TABLES {
            let select = match table.source {
                "users" if !has_display_names => {
                    "SELECT id,provider,email,NULL FROM users ORDER BY rowid".into()
                }
                "chats" if !has_chat_kinds => {
                    "SELECT id,workspace,name,creator,'group','',NULL FROM chats ORDER BY rowid"
                        .into()
                }
                "chat_people" | "chat_agents" => format!(
                    "SELECT c.workspace,s.* FROM {} s JOIN chats c ON c.id=s.chat ORDER BY s.rowid",
                    table.source
                ),
                "chat_messages" => format!(
                    "SELECT s.id,c.workspace,s.chat,s.author,s.kind,s.content,s.state,s.at,s.target,s.requester,{} FROM chat_messages s JOIN chats c ON c.id=s.chat ORDER BY s.rowid",
                    if has_threads { "s.thread" } else { "NULL" }
                ),
                _ => format!(
                    "SELECT {} FROM {} ORDER BY rowid",
                    table.columns.join(","),
                    table.source
                ),
            };
            let mut statement = source.prepare(&select)?;
            let mut records = statement.query([])?;
            let columns = table
                .columns
                .iter()
                .map(|c| if *c == "user" { "user_id" } else { c })
                .collect::<Vec<_>>()
                .join(",");
            let placeholders = (1..=table.columns.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>()
                .join(",");
            let insert = format!(
                "INSERT INTO {} ({columns}) VALUES({placeholders})",
                table.target
            );
            let mut imported = 0i64;
            while let Some(record) = records.next()? {
                let mut values = Vec::new();
                for (index, column) in table.columns.iter().enumerate() {
                    let value = match record.get_ref(index)? {
                        ValueRef::Null => SqlValue::String(None),
                        ValueRef::Integer(v) => v.into(),
                        ValueRef::Text(v) => {
                            let text = std::str::from_utf8(v)?;
                            if *column == "role"
                                && ["members", "invitations"].contains(&table.source)
                                && ["Admin", "Member"].contains(&text)
                            {
                                "User".into()
                            } else if table.source == "audit"
                                && *column == "workspace"
                                && text.is_empty()
                            {
                                SqlValue::String(None)
                            } else {
                                text.into()
                            }
                        }
                        _ => anyhow::bail!("Unsupported value in identity import"),
                    };
                    values.push(value);
                }
                execute(&tx, &insert, values).await?;
                imported += 1;
            }
            // A missing chat parent must not silently disappear through the import join.
            let original: i64 =
                source.query_row(&format!("SELECT count(*) FROM {}", table.source), [], |r| {
                    r.get(0)
                })?;
            ensure!(
                original == imported,
                "Import count mismatch in {}",
                table.source
            );
            counts.insert(table.source.into(), imported.into());
        }
        // Old SQLite pending replies have no durable job payload; never invent or replay one.
        execute(&tx,"UPDATE relay_chat_messages SET state='failed',content='Request interrupted by storage migration. Send a new request to retry.' WHERE kind='agent' AND state='queued'",vec![]).await?;
        let counts = Value::Object(counts);
        execute(&tx,"INSERT INTO relay_imports(source_id,completed_at,counts) VALUES('sqlite-identity-v1',$1,$2)",vec![now().into(),counts.clone().into()]).await?;
        tx.commit().await?;
        // Browser and App session tokens intentionally do not cross the storage cutover.
        Ok(counts)
    }
}
