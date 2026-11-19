//! Which parts of the controller one `ayati identity` process runs
//! (runtime/controller-roles.md). The roles share one binary and one database
//! and never call each other: one writes to PostgreSQL and its notifications
//! wake the other.
use super::Reader;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// Everything in one process: a laptop, and the default.
    All,
    /// Serves people: sign-in, pages, the browser API and live updates.
    Web,
    /// Serves executors (worker control, the model gateway) and runs the
    /// scheduler and the other background jobs. Not reached by browsers.
    Work,
}

impl Role {
    pub(super) fn read(env: &mut Reader) -> Self {
        match env.text("AYATI_CONTROLLER_ROLE").as_deref() {
            None | Some("all") => Self::All,
            Some("web") => Self::Web,
            Some("work") => Self::Work,
            Some(_) => {
                env.problem("AYATI_CONTROLLER_ROLE must be all, web or work");
                Self::All
            }
        }
    }

    /// Serves the browser.
    pub fn web(self) -> bool {
        self != Self::Work
    }

    /// Serves executors and runs background jobs.
    pub fn work(self) -> bool {
        self != Self::Web
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Web => "web",
            Self::Work => "work",
        }
    }

    /// Each role refuses the other's secrets, so a process that is broken into
    /// holds no more than it needs.
    pub(super) fn refuse_other_secrets(self, env: &mut Reader) {
        match self {
            Self::All => {}
            Self::Web => env.refuse(
                &[
                    "AYATI_WORKER_CONTROL_URL",
                    "AYATI_WORKER_CONTROL_KEY_FILE",
                    "AYATI_WORKER_CONTROL_LISTEN",
                    "AYATI_WORKER_CONTROL_TLS_CERT",
                    "AYATI_WORKER_CONTROL_TLS_KEY",
                ],
                "A web controller",
            ),
            Self::Work => env.refuse(
                &[
                    "WORKOS_API_KEY",
                    "WORK_OS_API_KEY",
                    "WORKOS_CLIENT_ID",
                    "WORK_OS_CLIENT_ID",
                ],
                "A work controller",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Controller, Execution};
    use anyhow::Result;

    type Pairs = [(&'static str, &'static str)];
    fn with(
        base: &Pairs,
        extra: (&'static str, &'static str),
    ) -> Vec<(&'static str, &'static str)> {
        base.iter().copied().chain([extra]).collect()
    }
    const LOCAL: &Pairs = &[
        DATABASE,
        ("WORKOS_CLIENT_ID", "client_fixture"),
        ("WORKOS_API_KEY", "fixture-key"),
        ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
        (
            "AYATI_WORKER_CONTROL_KEY_FILE",
            "/private/worker-control.key",
        ),
    ];
    fn role(pairs: &[(&str, &str)]) -> Result<Controller> {
        Controller::read(&mut Reader::from_pairs(pairs), false)
    }
    const DATABASE: (&str, &str) = (
        "RELAY_DATABASE_URL",
        "postgresql://relay_app@localhost/relay",
    );
    const WEB: &[(&str, &str)] = &[
        DATABASE,
        ("AYATI_CONTROLLER_ROLE", "web"),
        ("WORKOS_CLIENT_ID", "client_fixture"),
        ("WORKOS_API_KEY", "fixture-key"),
    ];
    const WORK: &[(&str, &str)] = &[
        DATABASE,
        ("AYATI_CONTROLLER_ROLE", "work"),
        ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
        (
            "AYATI_WORKER_CONTROL_KEY_FILE",
            "/private/worker-control.key",
        ),
    ];

    #[test]
    fn one_process_runs_every_role_unless_told_otherwise() {
        let config = role(LOCAL).unwrap();
        assert_eq!(config.role, Role::All);
        assert!(config.role.web() && config.role.work());
        assert_eq!(config.database_connections, 10);
        let error = role(&with(LOCAL, ("AYATI_CONTROLLER_ROLE", "api")))
            .err()
            .unwrap();
        assert!(error.to_string().contains("all, web or work"), "{error}");
    }

    #[test]
    fn a_web_controller_signs_people_in_and_holds_no_executor_secrets() {
        let config = role(WEB).unwrap();
        assert!(config.role.web() && !config.role.work());
        assert!(config.workos.is_some() && config.worker_control.is_none());
        // It still accepts agent work into the queue for executors to run.
        assert_eq!(config.execution, Execution::Pooled);
        for secret in [
            (
                "AYATI_WORKER_CONTROL_KEY_FILE",
                "/private/worker-control.key",
            ),
            ("AYATI_WORKER_CONTROL_TLS_KEY", "/private/server.key"),
            ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
        ] {
            let error = role(&with(WEB, secret)).err().unwrap().to_string();
            assert!(
                error.contains("A web controller must not receive"),
                "{error}"
            );
        }
        let no_sign_in = [DATABASE, ("AYATI_CONTROLLER_ROLE", "web")];
        assert!(role(&no_sign_in).is_err());
    }

    #[test]
    fn a_work_controller_serves_executors_and_holds_no_sign_in_keys() {
        let config = role(WORK).unwrap();
        assert!(config.role.work() && !config.role.web());
        assert!(config.workos.is_none() && config.worker_control.is_some());
        let keyed = with(WORK, ("WORKOS_API_KEY", "fixture-key"));
        let error = role(&keyed).err().unwrap().to_string();
        assert!(
            error.contains("A work controller must not receive WORKOS_API_KEY"),
            "{error}"
        );
        let no_listener = [DATABASE, ("AYATI_CONTROLLER_ROLE", "work")];
        assert!(role(&no_listener).is_err());
    }

    #[test]
    fn each_role_sets_its_own_database_pool_and_scripted_replies_need_every_role() {
        let sized = with(WEB, ("AYATI_DATABASE_CONNECTIONS", "25"));
        assert_eq!(role(&sized).unwrap().database_connections, 25);
        assert!(role(&with(LOCAL, ("AYATI_DATABASE_CONNECTIONS", "1"))).is_err());
        // Scripted replies are written inside the one simulation process.
        let mut simulation = Reader::from_pairs(&[
            (
                "RELAY_SIMULATION_DATABASE_URL",
                "postgresql://localhost/sim",
            ),
            ("AYATI_CONTROLLER_ROLE", "web"),
        ]);
        assert!(Controller::read(&mut simulation, true).is_err());
    }
}
