//! The AWS services template writes the controller's providers files from
//! values it carries. A file the controller can't read stops it at start, so
//! each value is checked here against the same parser the controller uses.
use super::{computers, connectors::providers_file, web_search};

const TEMPLATE: &str = include_str!("../../deploy/aws/stacks/services.yaml");

/// The single-quoted JSON the template's `prepare` container gets as `name`.
fn value(name: &str) -> &'static str {
    let start = format!("{{Name: {name}, Value: '");
    let from = TEMPLATE.find(&start).expect("the template names the file") + start.len();
    let length = TEMPLATE[from..].find("'}").expect("the value ends");
    &TEMPLATE[from..from + length]
}

#[test]
fn the_cloud_template_writes_providers_files_the_controller_accepts() {
    let computers = computers::ProvidersFile::parse_bytes(value("COMPUTERS").as_bytes()).unwrap();
    for size in computers::SIZES {
        assert!(
            computers.size(size).is_some(),
            "{size} computers are offered"
        );
    }
    let connectors = providers_file::ProvidersFile::parse(value("CONNECTORS")).unwrap();
    assert!(connectors.composio().is_some());
    web_search::ProvidersFile::parse(value("WEB_SEARCH")).unwrap();
    // The key files are the ones the same container writes.
    for (name, key) in [
        ("COMPUTERS", "e2b-key"),
        ("CONNECTORS", "composio-key"),
        ("WEB_SEARCH", "exa-key"),
    ] {
        assert!(value(name).contains(&format!("/run/ayati/{key}")));
        assert!(TEMPLATE.contains(&format!("> /run/ayati/{key};")));
    }
}
