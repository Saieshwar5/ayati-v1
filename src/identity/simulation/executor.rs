use super::*;
use crate::identity::chat::model::Job;
pub(in crate::identity) async fn execute(app: &App, job: &Job) -> Result<String> {
    check(
        app.simulation.is_some(),
        "Scripted execution requires simulation mode",
    )?;
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    check(
        app.store.chat_job_allowed(job).await,
        "Request access changed",
    )?;
    let last = job.prompt.lines().last().unwrap_or("");
    if last.contains("[quote]") || last.contains("[complete:") {
        return Ok(format!(
            "{}\n\n[Simulation · deterministic agent]",
            app.store.scripted_work(job).await?
        ));
    }
    let text = if last.contains("[stock]") {
        "Stock checked against the scenario fixture: 12 desks available; 8 require replenishment. Please request purchase approval before promising the full delivery."
    } else if last.contains("[quote]") {
        "Draft quotation: 20 desks × ₹4,000 = ₹80,000 before tax. Delivery depends on Operations confirming stock. Finance approval is still required."
    } else if last.contains("[delivery]") {
        "Delivery update recorded in this thread. The attached recording is a fixture; no speech recognition was performed. Please confirm the signed delivery note."
    } else if last.contains("[support]") {
        "The complaint is assigned for human review. Please check the damaged-item photo and agree a replacement with Operations before closing the case."
    } else if last.contains("[summary]") {
        "Scenario summary: quotation prepared; stock shortage identified; approval and delivery evidence must be confirmed by the responsible employees. This scripted reply does not modify business records."
    } else {
        "Acknowledged. I will keep this request in its current conversation and thread. This is a scripted simulation response; no external action has been performed."
    };
    Ok(format!("{text}\n\n[Simulation · deterministic agent]"))
}
