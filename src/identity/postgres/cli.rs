use super::*;

/// Explicit migrations; never run destructive schema synchronization on API startup.
pub async fn run(url: &str, action: &str) -> Result<()> {
    let db = PgStore::connect(url).await?;
    match action {
        "migrate" => {
            db.migrate().await?;
            println!("PostgreSQL workspace schema is current.");
        }
        "check" => {
            db.verify_schema().await?;
            println!("PostgreSQL workspace schema is ready.");
        }
        // Reports from "Report a problem", newest first, as JSON lines.
        "problem-reports" => {
            db.verify_schema().await?;
            for report in db.problem_reports(100).await? {
                println!("{report}");
            }
        }
        _ => anyhow::bail!("Use workspace-db migrate, check or problem-reports"),
    }
    Ok(())
}
impl PgStore {
    pub async fn verify_schema(&self) -> Result<()> {
        let applied = rows(
            &self.db,
            "SELECT version FROM seaql_migrations ORDER BY version",
            vec![],
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!("Run workspace-db migrate before starting Cuckoo PostgreSQL")
        })?;
        ensure!(
            applied.len() == 56
                && applied[0].try_get::<String>("", "version")?
                    == "m20261020_000001_workspace_foundation"
                && applied[1].try_get::<String>("", "version")?
                    == "m20261020_000002_conversation_kinds"
                && applied[2].try_get::<String>("", "version")? == "m20261021_000003_display_names"
                && applied[3].try_get::<String>("", "version")?
                    == "m20261021_000004_company_collaboration"
                && applied[4].try_get::<String>("", "version")?
                    == "m20261021_000005_queue_telemetry"
                && applied[5].try_get::<String>("", "version")?
                    == "m20261021_000006_reliable_messaging"
                && applied[6].try_get::<String>("", "version")?
                    == "m20261021_000007_agent_profiles"
                && applied[7].try_get::<String>("", "version")?
                    == "m20261022_000008_workspace_roles"
                && applied[8].try_get::<String>("", "version")? == "m20261022_000009_library"
                && applied[9].try_get::<String>("", "version")? == "m20261023_000010_login_flows"
                && applied[10].try_get::<String>("", "version")?
                    == "m20261023_000011_worker_assignments"
                && applied[11].try_get::<String>("", "version")?
                    == "m20261023_000012_worker_control"
                && applied[12].try_get::<String>("", "version")?
                    == "m20261023_000013_executor_slots"
                && applied[13].try_get::<String>("", "version")? == "m20261023_000014_reply_output"
                && applied[14].try_get::<String>("", "version")?
                    == "m20261023_000015_harness_preparation"
                && applied[15].try_get::<String>("", "version")?
                    == "m20261024_000016_interactive_reserve"
                && applied[16].try_get::<String>("", "version")?
                    == "m20261024_000017_cloud_executors"
                && applied[17].try_get::<String>("", "version")?
                    == "m20261025_000018_shared_control"
                && applied[18].try_get::<String>("", "version")?
                    == "m20261025_000019_connection_authority"
                && applied[19].try_get::<String>("", "version")?
                    == "m20261025_000020_guided_permissions"
                && applied[20].try_get::<String>("", "version")?
                    == "m20261025_000021_channel_approvals"
                && applied[21].try_get::<String>("", "version")?
                    == "m20261026_000022_durable_approval_waits"
                && applied[22].try_get::<String>("", "version")?
                    == "m20261026_000023_agent_questions"
                && applied[23].try_get::<String>("", "version")?
                    == "m20261026_000024_chat_organization"
                && applied[24].try_get::<String>("", "version")?
                    == "m20261026_000025_workspace_attention"
                && applied[25].try_get::<String>("", "version")?
                    == "m20261026_000026_agent_rehearsals"
                && applied[26].try_get::<String>("", "version")?
                    == "m20261026_000027_shared_agent_schedules"
                && applied[27].try_get::<String>("", "version")?
                    == "m20261026_000028_shared_agent_events"
                && applied[28].try_get::<String>("", "version")?
                    == "m20261026_000029_channel_agent_handoffs"
                && applied[29].try_get::<String>("", "version")?
                    == "m20261026_000030_workspace_home_panels"
                && applied[30].try_get::<String>("", "version")?
                    == "m20261026_000031_conversation_context"
                && applied[31].try_get::<String>("", "version")?
                    == "m20261026_000032_native_communications"
                && applied[32].try_get::<String>("", "version")?
                    == "m20261027_000033_home_panel_maintenance"
                && applied[33].try_get::<String>("", "version")?
                    == "m20261029_000034_executor_tasks"
                && applied[34].try_get::<String>("", "version")?
                    == "m20261029_000035_data_lifecycle"
                && applied[35].try_get::<String>("", "version")? == "m20261030_000036_chat_people"
                && applied[36].try_get::<String>("", "version")?
                    == "m20261030_000037_message_actions"
                && applied[37].try_get::<String>("", "version")?
                    == "m20261030_000038_chat_notifications"
                && applied[38].try_get::<String>("", "version")?
                    == "m20261030_000039_agent_receipts"
                && applied[39].try_get::<String>("", "version")?
                    == "m20261030_000040_message_replies"
                && applied[40].try_get::<String>("", "version")?
                    == "m20261030_000041_workspace_codes"
                && applied[41].try_get::<String>("", "version")? == "m20261030_000042_invite_links"
                && applied[42].try_get::<String>("", "version")?
                    == "m20261030_000043_admin_member_roles"
                && applied[43].try_get::<String>("", "version")?
                    == "m20261030_000044_audit_summaries"
                && applied[44].try_get::<String>("", "version")?
                    == "m20261031_000045_agent_audience"
                && applied[45].try_get::<String>("", "version")?
                    == "m20261031_000046_agent_instructions"
                && applied[46].try_get::<String>("", "version")?
                    == "m20261031_000047_connector_safety"
                && applied[47].try_get::<String>("", "version")? == "m20261031_000048_activity"
                && applied[48].try_get::<String>("", "version")?
                    == "m20261031_000049_workspace_settings"
                && applied[49].try_get::<String>("", "version")? == "m20261031_000050_skills"
                && applied[50].try_get::<String>("", "version")? == "m20261101_000051_agent_pause"
                && applied[51].try_get::<String>("", "version")?
                    == "m20261101_000052_agent_updates"
                && applied[52].try_get::<String>("", "version")?
                    == "m20261101_000053_connector_providers"
                && applied[53].try_get::<String>("", "version")?
                    == "m20261101_000054_communication_gateway"
                && applied[54].try_get::<String>("", "version")? == "m20261101_000055_shared_rooms"
                && applied[55].try_get::<String>("", "version")? == "m20261101_000056_work_rooms",
            "Run workspace-db migrate: unsupported workspace schema version"
        );
        Ok(())
    }
}
