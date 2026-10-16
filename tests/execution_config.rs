use std::process::Command;

#[test]
fn requesting_vm_execution_cannot_select_the_host_prototype() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ayati"))
        .arg("--help")
        .current_dir(directory.path())
        .env_clear()
        .env("AYATI_RUNTIME", "prototype")
        .env("AYATI_EXECUTION", "vm")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("VM execution requires"));
    assert!(!directory.path().join(".ayati-local").exists());
}

#[test]
fn requesting_browser_execution_cannot_run_it_on_the_host() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ayati"))
        .arg("--help")
        .current_dir(directory.path())
        .env_clear()
        .env("AYATI_BROWSER", "1")
        .env("AYATI_EXECUTION", "host")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Browser execution requires"));
    assert!(!directory.path().join(".ayati-local").exists());
}
