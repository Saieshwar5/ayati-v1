use super::*;

fn files(catalog: &str) -> (tempfile::TempDir, Files) {
    let dir = tempfile::tempdir().unwrap();
    let providers = dir.path().join("providers.json");
    std::fs::write(&providers, r#"[{"provider":"fireworks","base_url":"https://api.fireworks.ai/inference/v1","key_file":"/k"},{"provider":"openrouter","base_url":"https://openrouter.ai/api/v1","key_file":"/k"}]"#).unwrap();
    let path = dir.path().join("catalog.json");
    std::fs::write(&path, catalog).unwrap();
    (
        dir,
        Files {
            catalog: path,
            providers,
        },
    )
}
const LLAMA: &str = r#"{"models":[
  {"id":"llama-3.3-70b","name":"Llama 3.3 70B","family":"Meta","tags":["fast","low cost"],"context":131072,
   "supports":{"tools":true},"price":{"input":0.9,"output":0.9},
   "routes":[{"provider":"fireworks","model":"accounts/fireworks/models/llama-v3p3-70b-instruct","cost":{"input":0.9,"output":0.9}},
             {"provider":"openrouter","model":"meta-llama/llama-3.3-70b-instruct","priority":2,"cost":{"input":0.59,"output":0.79}}]},
  {"id":"old-model","name":"Old model","status":"retiring","replacement":"llama-3.3-70b",
   "routes":[{"provider":"fireworks","model":"old"}]}]}"#;

#[test]
fn a_catalog_names_models_and_routes_them_to_configured_providers() {
    let (_dir, files) = files(LLAMA);
    let catalog = files.load().unwrap();
    let llama = catalog.choosable("llama-3.3-70b").unwrap();
    assert_eq!(llama.routes[0].priority, 1, "priority defaults to first");
    assert_eq!(llama.routes[1].priority, 2);
    let public = llama.public();
    assert_eq!(public["name"], "Llama 3.3 70B");
    assert!(
        public.get("routes").is_none() && !public.to_string().contains("fireworks"),
        "people never see providers or our costs"
    );
    assert!(
        catalog
            .choosable("old-model")
            .unwrap_err()
            .to_string()
            .contains("choose Llama 3.3 70B")
    );
    assert!(catalog.choosable("gpt-9").is_err());
}

#[test]
fn a_broken_catalog_says_what_is_wrong() {
    let said = |text: &str| files(text).1.load().unwrap_err().to_string();
    assert!(
        said(
            r#"{"models":[{"id":"x","name":"X","routes":[{"provider":"together","model":"x"}]}]}"#
        )
        .contains("isn't in the model providers file")
    );
    assert!(said(r#"{"models":[{"id":"X Y","name":"X","routes":[{"provider":"fireworks","model":"x"}]}]}"#).contains("lowercase"));
    assert!(said(r#"{"models":[{"id":"x","name":"X","routes":[]}]}"#).contains("1 to 8 routes"));
    assert!(said(r#"{"models":[{"id":"x","name":"X","status":"gone","routes":[{"provider":"fireworks","model":"x"}]}]}"#).contains("available, preview or retiring"));
    assert!(said(r#"{"models":[{"id":"x","name":"X","price":{"input":-1,"output":0},"routes":[{"provider":"fireworks","model":"x"}]}]}"#).contains("prices"));
    assert!(said(r#"{"models":[{"id":"x","name":"X","replacement":"y","routes":[{"provider":"fireworks","model":"x"}]}]}"#).contains("replacement"));
    assert!(said(r#"{"models":[{"id":"x","name":"X","routes":[{"provider":"fireworks","model":"x"}]},{"id":"x","name":"Y","routes":[{"provider":"fireworks","model":"x"}]}]}"#).contains("Two models"));
}
