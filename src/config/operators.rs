//! Settings about Cuckoo's operators, shared by the controller and `ayati admin`.
use super::reader::Reader;

pub(super) fn control_environment(env: &mut Reader, hosted: bool) -> String {
    let name = env
        .text("AYATI_CONTROL_ENVIRONMENT")
        .unwrap_or_else(|| if hosted { "production" } else { "local" }.into())
        .to_lowercase();
    if !(1..=24).contains(&name.len())
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        env.problem("AYATI_CONTROL_ENVIRONMENT must be a short name such as production or staging");
    }
    name
}

pub(super) fn platform_admins(env: &mut Reader) -> Vec<String> {
    let admins: Vec<String> = env
        .text("AYATI_PLATFORM_ADMINS")
        .map(|value| {
            value
                .split(',')
                .map(|e| e.trim().to_lowercase())
                .filter(|e| !e.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if admins
        .iter()
        .any(|e| !e.contains('@') || e.chars().any(char::is_whitespace))
    {
        env.problem("AYATI_PLATFORM_ADMINS must be a comma-separated list of email addresses");
    }
    admins
}
