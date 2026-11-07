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
            applied.len() == 100
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
                && applied[55].try_get::<String>("", "version")? == "m20261101_000056_work_rooms"
                && applied[56].try_get::<String>("", "version")? == "m20261101_000057_home_frame"
                && applied[57].try_get::<String>("", "version")? == "m20261101_000058_tasks"
                && applied[58].try_get::<String>("", "version")? == "m20261101_000059_home_living"
                && applied[59].try_get::<String>("", "version")? == "m20261101_000060_home_team"
                && applied[60].try_get::<String>("", "version")? == "m20261102_000061_supervising"
                && applied[61].try_get::<String>("", "version")? == "m20261102_000062_steering"
                && applied[62].try_get::<String>("", "version")?
                    == "m20261102_000063_getting_better"
                && applied[63].try_get::<String>("", "version")? == "m20261105_000064_guard"
                && applied[64].try_get::<String>("", "version")?
                    == "m20261105_000065_guard_meanings"
                && applied[65].try_get::<String>("", "version")? == "m20261105_000066_guard_facts"
                && applied[66].try_get::<String>("", "version")?
                    == "m20261105_000067_guard_proposals"
                && applied[67].try_get::<String>("", "version")? == "m20261105_000068_model_usage"
                && applied[68].try_get::<String>("", "version")?
                    == "m20261105_000069_library_folders"
                && applied[69].try_get::<String>("", "version")?
                    == "m20261105_000070_library_passages"
                && applied[70].try_get::<String>("", "version")?
                    == "m20261105_000071_library_fence"
                && applied[71].try_get::<String>("", "version")?
                    == "m20261105_000072_library_meaning"
                && applied[72].try_get::<String>("", "version")?
                    == "m20261105_000073_library_always"
                && applied[73].try_get::<String>("", "version")?
                    == "m20261105_000074_library_edits"
                && applied[74].try_get::<String>("", "version")? == "m20261105_000075_company_apps"
                && applied[75].try_get::<String>("", "version")?
                    == "m20261105_000076_library_tables"
                && applied[76].try_get::<String>("", "version")?
                    == "m20261105_000077_library_upkeep"
                && applied[77].try_get::<String>("", "version")? == "m20261105_000078_built_apps"
                && applied[78].try_get::<String>("", "version")?
                    == "m20261105_000079_built_app_data"
                && applied[79].try_get::<String>("", "version")?
                    == "m20261105_000080_built_app_agents"
                && applied[80].try_get::<String>("", "version")?
                    == "m20261105_000081_built_app_actions"
                && applied[81].try_get::<String>("", "version")?
                    == "m20261105_000082_library_keeping"
                && applied[82].try_get::<String>("", "version")?
                    == "m20261105_000083_library_watching"
                && applied[83].try_get::<String>("", "version")?
                    == "m20261105_000084_built_app_events"
                && applied[84].try_get::<String>("", "version")?
                    == "m20261105_000085_built_app_asking"
                && applied[85].try_get::<String>("", "version")?
                    == "m20261105_000086_built_app_changes"
                && applied[86].try_get::<String>("", "version")? == "m20261106_000087_model_routes"
                && applied[87].try_get::<String>("", "version")? == "m20261106_000088_model_costs"
                && applied[88].try_get::<String>("", "version")?
                    == "m20261106_000089_model_budgets"
                && applied[89].try_get::<String>("", "version")?
                    == "m20261106_000090_model_operations"
                && applied[90].try_get::<String>("", "version")?
                    == "m20261107_000091_operator_console"
                && applied[91].try_get::<String>("", "version")?
                    == "m20261107_000092_message_search"
                && applied[92].try_get::<String>("", "version")?
                    == "m20261107_000093_invitation_delivery"
                && applied[93].try_get::<String>("", "version")?
                    == "m20261107_000094_unread_from_joining"
                && applied[94].try_get::<String>("", "version")?
                    == "m20261107_000095_voice_transcripts"
                && applied[95].try_get::<String>("", "version")?
                    == "m20261107_000096_chat_activity"
                && applied[96].try_get::<String>("", "version")?
                    == "m20261107_000097_work_notifications"
                && applied[97].try_get::<String>("", "version")?
                    == "m20261107_000098_member_presence"
                && applied[98].try_get::<String>("", "version")?
                    == "m20261107_000099_several_agents_per_message"
                && applied[99].try_get::<String>("", "version")?
                    == "m20261107_000100_chat_file_reading",
            "Run workspace-db migrate: unsupported workspace schema version"
        );
        Ok(())
    }
}
