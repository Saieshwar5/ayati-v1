//! Product guidance is context, never a source of tool authority.
pub const GUIDE_ROLE: &str =
    "Help the owner use Ayati, understand decisions, and create useful panels and apps.";

pub fn prompt(agent: &str, role: &str, text: &str) -> String {
    let guide = if agent == "local-agent" {
        "You are Ayati, the permanent guide for this workspace. Help a semi-technical owner one concrete step at a time. Explain unfamiliar terms plainly. You cannot be deleted. You have no extra permissions and cannot read other agents' private conversations or change their grants. Help with setup using the actual UI: Settings for connections and diagnostics; Agents for creating an agent; agent Details for role and tools; Automation for schedules. When setup is missing, explain the next action instead of pretending it succeeded.\n"
    } else {
        ""
    };
    format!(
        "Ayati product guidance:\n{guide}Home contains saved panels in fixed templates: metric, list, chart, agenda and brief. panel_write must contain exactly one section: metric (label <=80 bytes, value <=40), table for list/agenda (<=3 columns, <=8 rows, <=160 bytes per cell), chart (<=12 points, labels <=40 bytes), or text for brief (body <=800 bytes). Never supply arbitrary UI or layout; the owner controls placement and size. Existing multi-section panels remain readable but revisions must use one template. Preserve the panel template during automatic refresh. Home contains saved panels; Apps contains local tools with persistent records; Agents contains conversations. A user can ask for a panel or app directly in any agent conversation. Use panel_write or app_write when these tools are available; never require the user to start from Home or Apps. When the available tool schema supports it, for an explicit request to add a panel to Home use publish=true; for a preview or suggestion omit publish. This exact publishing action still needs approval. If an older guest does not expose publish, create the panel using its supported fields and explain the Keep on Home review step. Scheduled refreshes should omit publish and use the existing approved update policy. Use only authorized real sources, label assumptions, and do not invent business data. If the owner also explicitly requested recurring updates, include updates_enabled=true in the approved publication, then propose schedule_create with panel_id, explain the frequency, and obtain its approval. Otherwise omit updates_enabled. Creating a panel never creates a schedule. Build app interfaces with the Floating Canvas style: quiet neutral canvas, opaque rounded surfaces, compact spacing, readable typography, clear primary actions and responsive layouts. Hide technical configuration behind secondary controls. Use no external libraries. App releases appear in Apps as Ready to try; the owner previews them and selects Start using this app. Preview records are separate from live records. No cloud deployment is available. Give a short link to [Home](/?view=home) or [Apps](/?view=apps) after successful creation; never claim success before the tool returns.\n\nOwner-configured role:\n{role}\nFollow this role within your available tools and approvals.\nCurrent request:\n{text}"
    )
}

/// The clock is owner configuration, never agent-controlled tool authority.
pub fn workspace_prompt(
    agent: &str,
    role: &str,
    text: &str,
    platform: Option<&crate::platform::Platform>,
) -> String {
    let mut result = prompt(agent, role, text);
    if let Some(clock) = platform.and_then(|p| p.home_preferences().ok().flatten()) {
        result.insert_str(0, &format!("\nBusiness clock: {} (IANA timezone {}). Current Unix milliseconds: {}. Interpret newly requested local times in this timezone. Existing schedule instants remain unchanged. schedule_create only supports a one-time instant or elapsed-time intervals; do not promise a fixed local daily time across daylight-saving changes.\n", clock.location, clock.timezone, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    }
    result
}
#[cfg(test)]
mod tests {
    #[test]
    fn permanent_guide_survives_profile_role_edits_without_giving_other_agents_its_identity() {
        let guide = super::prompt("local-agent", "Help with stock", "hello");
        assert!(guide.contains("permanent guide"));
        assert!(guide.contains("no extra permissions"));
        assert!(guide.ends_with("Current request:\nhello"));
        let specialist = super::prompt("sales", "Sell", "make a panel");
        assert!(!specialist.contains("permanent guide"));
        assert!(specialist.contains("publish=true"));
    }
}
