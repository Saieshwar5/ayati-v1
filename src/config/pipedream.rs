//! Pipedream Connect settings for the controller (runtime/connector-providers.md).
//! The OAuth client ID and secret stay in owned 0600 files that are read on
//! every use; only their paths are settings. Executors refuse all of these.
use super::{Reader, absolute};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Pipedream {
    pub client_id_file: PathBuf,
    pub client_secret_file: PathBuf,
    /// The Connect project, `proj_…`.
    pub project_id: String,
    /// `development` or `production`: each keeps its own accounts and triggers.
    pub environment: String,
    /// A loopback fixture for tests; the official API otherwise.
    pub base_url: Option<String>,
}

impl Pipedream {
    /// The client files, project and environment together, or none of them.
    pub(crate) fn read(env: &mut Reader) -> Option<Self> {
        let names = [
            "AYATI_PIPEDREAM_CLIENT_ID_FILE",
            "AYATI_PIPEDREAM_CLIENT_SECRET_FILE",
            "AYATI_PIPEDREAM_PROJECT_ID",
            "AYATI_PIPEDREAM_ENVIRONMENT",
            "AYATI_PIPEDREAM_BASE_URL",
        ];
        if !names.iter().any(|name| env.present(name)) {
            return None;
        }
        let client_id_file = absolute(env, names[0]);
        let client_secret_file = absolute(env, names[1]);
        let project_id = env.text(names[2]);
        let environment = env.text(names[3]);
        if project_id.as_deref().is_some_and(|p| !project(p)) {
            env.problem(
                "AYATI_PIPEDREAM_PROJECT_ID must be a Connect project ID such as proj_abc123",
            );
        }
        if environment.as_deref().is_some_and(|e| !environment_name(e)) {
            env.problem("AYATI_PIPEDREAM_ENVIRONMENT must be development or production");
        }
        match (client_id_file, client_secret_file, project_id, environment) {
            (
                Some(client_id_file),
                Some(client_secret_file),
                Some(project_id),
                Some(environment),
            ) => Some(Self {
                client_id_file,
                client_secret_file,
                project_id,
                environment,
                base_url: env.text(names[4]),
            }),
            _ => {
                env.problem(
                    "Pipedream needs AYATI_PIPEDREAM_CLIENT_ID_FILE, AYATI_PIPEDREAM_CLIENT_SECRET_FILE, AYATI_PIPEDREAM_PROJECT_ID and AYATI_PIPEDREAM_ENVIRONMENT together",
                );
                None
            }
        }
    }
}

/// A Connect project ID: `proj_` and letters or digits.
pub(crate) fn project(value: &str) -> bool {
    value.strip_prefix("proj_").is_some_and(|rest| {
        !rest.is_empty() && rest.len() <= 64 && rest.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

pub(crate) fn environment_name(value: &str) -> bool {
    matches!(value, "development" | "production")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(pairs: &[(&str, &str)]) -> (Option<Pipedream>, anyhow::Result<()>) {
        let mut env = Reader::from_pairs(pairs);
        let settings = Pipedream::read(&mut env);
        (settings, env.finish("controller"))
    }

    #[test]
    fn pipedream_needs_all_of_its_settings_or_none() {
        let full = [
            ("AYATI_PIPEDREAM_CLIENT_ID_FILE", "/run/secrets/pd-id"),
            (
                "AYATI_PIPEDREAM_CLIENT_SECRET_FILE",
                "/run/secrets/pd-secret",
            ),
            ("AYATI_PIPEDREAM_PROJECT_ID", "proj_abc123"),
            ("AYATI_PIPEDREAM_ENVIRONMENT", "production"),
        ];
        let (settings, result) = read(&full);
        result.unwrap();
        let settings = settings.unwrap();
        assert_eq!(
            (settings.project_id.as_str(), settings.environment.as_str()),
            ("proj_abc123", "production")
        );
        assert!(settings.base_url.is_none());
        assert!(read(&[]).0.is_none());
        // Half a configuration, a relative file, or wrong values are refused.
        assert!(read(&full[..3]).1.is_err());
        assert!(
            read(&[("AYATI_PIPEDREAM_BASE_URL", "http://127.0.0.1:9/v1/")])
                .1
                .is_err()
        );
        for (name, value) in [
            ("AYATI_PIPEDREAM_CLIENT_ID_FILE", "pd-id"),
            ("AYATI_PIPEDREAM_PROJECT_ID", "abc123"),
            ("AYATI_PIPEDREAM_ENVIRONMENT", "staging"),
        ] {
            let pairs: Vec<_> = full
                .iter()
                .map(|(n, v)| (*n, if *n == name { value } else { *v }))
                .collect();
            let error = read(&pairs).1.unwrap_err().to_string();
            assert!(error.contains(name), "{error}");
        }
    }

    #[test]
    fn executors_refuse_pipedream_settings() {
        let base = [
            ("AYATI_MODEL", "fixture"),
            ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
            (
                "AYATI_WORKER_CONTROL_KEY_FILE",
                "/private/worker-control.key",
            ),
        ];
        for name in [
            "AYATI_PIPEDREAM_CLIENT_ID_FILE",
            "AYATI_PIPEDREAM_CLIENT_SECRET_FILE",
            "AYATI_PIPEDREAM_PROJECT_ID",
            "AYATI_PIPEDREAM_ENVIRONMENT",
            "AYATI_PIPEDREAM_BASE_URL",
        ] {
            let pairs: Vec<_> = base.iter().copied().chain([(name, "x")]).collect();
            let error = super::super::Executor::read(&mut Reader::from_pairs(&pairs))
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains(name), "{error}");
        }
    }
}
