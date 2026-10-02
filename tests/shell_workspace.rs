use ayati::{
    files::Files,
    storage::Store,
    tools::{Shell, ShellInput},
};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn setup() -> (TempDir, Arc<Store>, Arc<Files>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
    let files = Arc::new(Files::new(dir.path(), store.clone()).unwrap());
    (dir, store, files)
}

fn input(command: &str) -> ShellInput {
    ShellInput {
        command: command.into(),
        cwd: "/workspace".into(),
        timeout_seconds: 10,
    }
}

#[tokio::test]
async fn shell_isolates_host_and_credentials_and_versions_real_files() {
    let (dir, store, files) = setup();
    let host_file = dir.path().join("host-private.txt");
    std::fs::write(&host_file, "host-only").unwrap();
    let (original, workspace_path) = files.upload("notes.txt", b"original notes").await.unwrap();
    let shell = Shell::new(files.clone());
    let command = format!(
        "cat '{workspace_path}'; printf revised > revised.txt; env; test ! -e '{}'; test ! -e /home; test ! -e /etc; test ! -e /workspace/../state.sqlite; if touch /artifacts/forbidden 2>/dev/null; then exit 1; fi",
        host_file.display()
    );
    let result = shell
        .run("", input(&command), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.status, "success", "{}", result.output);
    assert!(result.output.contains("original notes"));
    assert!(!result.output.contains("FIREWORKS_API_KEY="));
    assert!(!result.output.contains("OPENAI_API_KEY="));
    assert_eq!(std::fs::read_to_string(host_file).unwrap(), "host-only");
    let created = result
        .artifacts
        .iter()
        .find(|a| a.name == "revised.txt")
        .unwrap();
    assert_eq!(
        std::fs::read(files.archives.join(&created.stored_name)).unwrap(),
        b"revised"
    );
    assert_eq!(
        std::fs::read(files.archives.join(&original.stored_name)).unwrap(),
        b"original notes"
    );
    let changed = shell
        .run(
            "",
            input("printf second > revised.txt"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(changed.artifacts.len(), 1);
    assert_ne!(changed.artifacts[0].id, created.id);
    assert!(store.artifacts().unwrap().len() >= 4);
}

#[tokio::test]
async fn output_is_bounded_and_retained_as_a_downloadable_log() {
    let (_dir, store, files) = setup();
    let result = Shell::new(files.clone())
        .run("", input("yes x | head -c 25000"), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.status, "success");
    assert!(result.truncated);
    assert!(result.output.len() < 7000);
    let log = store
        .artifact(result.log_artifact_id.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(log.size, 25000);
    let read_log = Shell::new(files.clone())
        .run(
            "",
            input(&format!("tail -c 100 '{}'", result.log_path.unwrap())),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(read_log.status, "success");
    assert_eq!(read_log.output.len(), 100);
}

#[tokio::test]
async fn stop_kills_shell_descendants_and_preserves_completed_outputs() {
    let (_dir, _store, files) = setup();
    let shell = Shell::new(files.clone());
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        signal.cancel();
    });
    let result = shell
        .run(
            "",
            input(
                "printf saved > before.txt; (sleep 1; printf leaked > after.txt) & sleep 30; wait",
            ),
            cancel,
        )
        .await
        .unwrap();
    assert_eq!(result.status, "cancelled");
    assert!(files.workspace.join("before.txt").exists());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!files.workspace.join("after.txt").exists());
}

#[tokio::test]
async fn rejects_traversal_symlink_outputs_and_invalid_tool_inputs() {
    let (dir, _store, files) = setup();
    let shell = Shell::new(files.clone());
    let mut invalid = input("touch should-not-exist");
    invalid.cwd = "/workspace/../artifacts".into();
    assert!(
        shell
            .run("", invalid, CancellationToken::new())
            .await
            .is_err()
    );
    assert!(!files.workspace.join("should-not-exist").exists());
    let private = dir.path().join("private");
    std::fs::write(&private, "private").unwrap();
    let link = files.workspace.join("link");
    std::os::unix::fs::symlink(private, &link).unwrap();
    assert!(files.archive("", "link", "output", &link).await.is_err());
    assert!(serde_json::from_str::<ShellInput>(r#"{"command":"true","extra":1}"#).is_err());
}

#[tokio::test]
async fn timeout_is_reported_without_a_success_claim() {
    let (_dir, _store, files) = setup();
    let mut command = input("sleep 30");
    command.timeout_seconds = 1;
    let result = Shell::new(files)
        .run("", command, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.status, "timeout");
    assert_eq!(result.exit_code, None);
}

#[tokio::test]
async fn many_generated_files_have_a_bounded_result_and_explicit_omissions() {
    let (_dir, _store, files) = setup();
    let result = Shell::new(files)
        .run(
            "",
            input("for i in $(seq 1 25); do printf x > file-$i.txt; done"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.artifacts.len(), 20);
    assert_eq!(result.omitted_artifacts, 5);
    assert!(serde_json::to_vec(&result).unwrap().len() < 16000);
}
