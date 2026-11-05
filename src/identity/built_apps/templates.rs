//! Apps that start from a template (Apps AP10): Leave requests, Expenses and Stock.
//! Each is a manifest with actions for agents and a small config, run by one shared
//! records screen, so a template is ready to publish as it is and easy to change.
use super::files::{escape, starter};
use anyhow::{Result, bail};
use serde_json::Value;

/// The files an app starts with for a template; "blank" is the plain starter.
pub fn files(template: &str, name: &str, description: &str) -> Result<Vec<(String, String)>> {
    let (manifest, config) = match template {
        "" | "blank" => return Ok(starter(name, description)),
        "leave" => (
            include_str!("templates/leave/cuckoo-app.json"),
            include_str!("templates/leave/config.js"),
        ),
        "expenses" => (
            include_str!("templates/expenses/cuckoo-app.json"),
            include_str!("templates/expenses/config.js"),
        ),
        "stock" => (
            include_str!("templates/stock/cuckoo-app.json"),
            include_str!("templates/stock/config.js"),
        ),
        _ => bail!("Start from blank, leave, expenses or stock"),
    };
    let mut manifest: Value = serde_json::from_str(manifest)?;
    manifest["name"] = name.into();
    if !description.is_empty() {
        manifest["description"] = description.into();
    }
    let about = manifest["description"].as_str().unwrap_or("").to_owned();
    let page = include_str!("templates/index.html")
        .replace("{{name}}", &escape(name))
        .replace("{{description}}", &escape(&about));
    Ok(vec![
        (
            "cuckoo-app.json".into(),
            serde_json::to_string_pretty(&manifest)?,
        ),
        ("index.html".into(), page),
        (
            "style.css".into(),
            include_str!("templates/style.css").into(),
        ),
        ("config.js".into(), config.into()),
        (
            "records.js".into(),
            include_str!("templates/records.js").into(),
        ),
        (
            "README.md".into(),
            format!(
                "# {name}\n\n{about}\n\nStarted from the {template} template. config.js names the collection and its fields; records.js draws the screen.\n"
            ),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::super::{files, manifest::Manifest};

    #[test]
    fn every_template_passes_its_checks_and_offers_agent_actions() {
        for template in ["leave", "expenses", "stock"] {
            let app = super::files(template, "Our <app>", "").unwrap();
            assert!(
                files::checks(&app).is_empty(),
                "{template}: {:?}",
                files::checks(&app)
            );
            let manifest = Manifest::parse(&app[0].1).unwrap();
            assert!(!manifest.actions.is_empty(), "{template}");
            let doc = files::runnable(&app).unwrap();
            assert!(
                doc.contains("window.APP") && doc.contains("cuckoo.list(app.collection)"),
                "{template}"
            );
            assert!(doc.contains("<h1>Our &lt;app&gt;</h1>"));
        }
        assert_eq!(super::files("blank", "A", "B").unwrap().len(), 5);
        assert!(super::files("payroll", "A", "B").is_err());
    }
}
