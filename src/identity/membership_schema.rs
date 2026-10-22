//! Forward migration of the SQLite membership constraint, retaining dependent records.
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension};

pub(super) fn migrate(db: &mut Connection) -> Result<()> {
    let schema: String = db.query_row(
        "SELECT sql FROM sqlite_master WHERE name='members'",
        [],
        |r| r.get(0),
    )?;
    if !schema.contains("'Admin'") {
        return Ok(());
    }
    // SQLite's documented table-rebuild procedure: disable FKs outside the transaction,
    // replace the table without renaming its dependents, then verify all references.
    db.execute_batch("PRAGMA foreign_keys=OFF")?;
    let result = (|| -> Result<()> {
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE members_new(workspace TEXT REFERENCES workspaces(id), user TEXT REFERENCES users(id), role TEXT NOT NULL CHECK(role IN ('Owner','User')), PRIMARY KEY(workspace,user));
            INSERT INTO audit(actor,workspace,action,target,at) SELECT user,workspace,'legacy_role_migrated',role || ' -> User',unixepoch() FROM members WHERE role IN ('Admin','Member');
            INSERT INTO members_new SELECT workspace,user,CASE WHEN role='Owner' THEN 'Owner' ELSE 'User' END FROM members;
            DROP TABLE members;
            ALTER TABLE members_new RENAME TO members;
            UPDATE invitations SET role='User' WHERE role IN ('Admin','Member');")?;
        let violation = tx
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()?;
        ensure!(
            violation.is_none(),
            "Membership migration found invalid references"
        );
        tx.commit()?;
        Ok(())
    })();
    db.execute_batch("PRAGMA foreign_keys=ON")?;
    result
}
