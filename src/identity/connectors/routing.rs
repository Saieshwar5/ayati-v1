//! Which provider serves each catalog app for a workspace: the workspace's own
//! route (set by Cuckoo's administrators), then the providers file's route, then
//! the catalog's default (the first configured provider that maps the app,
//! Composio preferred). Only new connections follow a route; a saved connection
//! keeps the provider it was made with.
use super::{App, ConnectorProvider, curated};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The routes that apply to one workspace.
#[derive(Default)]
pub(crate) struct Routes {
    /// App to provider, from `relay_connector_routes`.
    pub workspace: BTreeMap<String, String>,
    /// App to providers in order, from the providers file.
    pub file: BTreeMap<String, Vec<String>>,
}

/// The provider for `app` among `configured`, or `None` when none maps it.
pub(crate) fn resolve<'a>(
    app: &curated::CatalogApp,
    routes: &Routes,
    configured: &[&'a str],
) -> Option<&'a str> {
    let serving = |name: &str| {
        configured
            .iter()
            .copied()
            .find(|c| *c == name && app.providers.contains_key(name))
    };
    if let Some(found) = routes.workspace.get(&app.id).and_then(|p| serving(p)) {
        return Some(found);
    }
    let file = routes.file.get(&app.id).into_iter().flatten();
    if let Some(found) = file.filter_map(|p| serving(p)).next() {
        return Some(found);
    }
    serving("composio").or_else(|| app.providers.keys().find_map(|p| serving(p)))
}

/// One app per id: catalog apps through their routed provider, then the
/// providers' own apps (operator MCP servers). An operator server named like a
/// catalog app is offered only while no provider serves that catalog app.
pub(crate) fn apps_with(
    providers: &[Arc<dyn ConnectorProvider>],
    workspace: &str,
    routes: &Routes,
) -> Result<Vec<App>> {
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    let mut apps: Vec<App> = vec![];
    for provider in providers {
        for app in provider.apps(workspace)? {
            let routed = curated::app(&app.id).and_then(|c| resolve(c, routes, &names));
            if routed.is_some_and(|r| r != provider.name()) || apps.iter().any(|a| a.id == app.id) {
                continue;
            }
            apps.push(app);
        }
    }
    Ok(apps)
}

/// The provider that connects `app` for this workspace.
pub(crate) fn provider_with(
    providers: &[Arc<dyn ConnectorProvider>],
    workspace: &str,
    app: &str,
    routes: &Routes,
) -> Result<Arc<dyn ConnectorProvider>> {
    let name = apps_with(providers, workspace, routes)?
        .into_iter()
        .find(|a| a.id == app)
        .map(|a| a.provider)
        .context("This connector is not offered on this server")?;
    providers
        .iter()
        .find(|p| p.name() == name)
        .cloned()
        .context("This connector is not offered on this server")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::connectors::{Account, Call, Connect, Saved};
    use crate::identity::control::model::Operation;
    use serde_json::Value;

    /// A provider that offers every catalog app it is mapped for, by name only.
    struct Named(&'static str, Vec<&'static str>);
    #[async_trait::async_trait]
    impl ConnectorProvider for Named {
        fn name(&self) -> &'static str {
            self.0
        }
        fn apps(&self, _: &str) -> Result<Vec<App>> {
            Ok(self
                .1
                .iter()
                .map(|id| App {
                    id: (*id).into(),
                    name: (*id).into(),
                    account: None,
                    operations: vec![],
                    provider: self.0,
                })
                .collect())
        }
        async fn connect(&self, _: &str, _: &str, _: &str) -> Result<Connect> {
            unreachable!()
        }
        async fn finish(&self, _: &str, _: &str, _: &Value) -> Result<Account> {
            unreachable!()
        }
        async fn disconnect(&self, _: &str, _: &str, _: Option<&Value>) -> Result<()> {
            unreachable!()
        }
        async fn prepare(&self, _: &Saved<'_>, _: &Operation) -> Result<Box<dyn Call>> {
            unreachable!()
        }
    }

    /// Gmail as if Pipedream mapped it too.
    fn gmail() -> curated::CatalogApp {
        serde_json::from_value(serde_json::json!({
            "id":"gmail","name":"Gmail","category":"email_calendar","description":"Email",
            "providers":{"composio":"gmail","pipedream":"gmail"},"actions":[]}))
        .unwrap()
    }

    #[test]
    fn a_workspace_route_beats_the_file_which_beats_the_catalog_default() {
        let app = gmail();
        let both = ["mcp", "pipedream", "composio"];
        let mut routes = Routes::default();
        assert_eq!(
            resolve(&app, &routes, &both),
            Some("composio"),
            "Composio is preferred"
        );
        assert_eq!(
            resolve(&app, &routes, &["mcp", "pipedream"]),
            Some("pipedream")
        );
        assert_eq!(resolve(&app, &routes, &["mcp"]), None);
        routes
            .file
            .insert("gmail".into(), vec!["pipedream".into(), "composio".into()]);
        assert_eq!(resolve(&app, &routes, &both), Some("pipedream"));
        // The first configured provider in the file's route.
        assert_eq!(resolve(&app, &routes, &["composio"]), Some("composio"));
        routes.workspace.insert("gmail".into(), "composio".into());
        assert_eq!(resolve(&app, &routes, &both), Some("composio"));
        // A workspace route to a provider that isn't configured falls through.
        routes.workspace.insert("gmail".into(), "pipedream".into());
        assert_eq!(resolve(&app, &routes, &["composio"]), Some("composio"));
        // A provider that doesn't map the app never serves it.
        routes.workspace.insert("gmail".into(), "mcp".into());
        assert_eq!(resolve(&app, &routes, &both), Some("pipedream"));
    }

    #[test]
    fn each_app_is_offered_once_through_its_route_beside_operator_servers() {
        let providers: Vec<Arc<dyn ConnectorProvider>> = vec![
            Arc::new(Named("mcp", vec!["orders", "google_drive"])),
            Arc::new(Named("composio", vec!["gmail", "google_drive"])),
        ];
        let apps = apps_with(&providers, "w", &Routes::default()).unwrap();
        let listed: Vec<(&str, &str)> = apps.iter().map(|a| (a.id.as_str(), a.provider)).collect();
        assert_eq!(
            listed,
            [
                ("orders", "mcp"),
                ("gmail", "composio"),
                ("google_drive", "composio")
            ]
        );
        let found = provider_with(&providers, "w", "gmail", &Routes::default()).unwrap();
        assert_eq!(found.name(), "composio");
        assert!(provider_with(&providers, "w", "slack", &Routes::default()).is_err());
        // Without a catalog provider, the operator's server of that name is offered.
        let operator: Vec<Arc<dyn ConnectorProvider>> =
            vec![Arc::new(Named("mcp", vec!["google_drive"]))];
        let apps = apps_with(&operator, "w", &Routes::default()).unwrap();
        assert_eq!(apps[0].provider, "mcp");
    }
}
