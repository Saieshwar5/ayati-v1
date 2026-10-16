use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn manifest(directory: &Path, agent: &str) -> PathBuf {
    for name in [
        "client_key",
        "known_hosts",
        "disk.qcow2",
        "vars.fd",
        "qemu",
        "firmware",
    ] {
        std::fs::write(directory.join(name), "synthetic").unwrap();
    }
    let value = json!({"version":1,"agent_id":agent,"qemu":directory.join("qemu"),"qemu_img":directory.join("qemu"),"library_path":"","firmware_code":directory.join("firmware"),"firmware_data":directory,"directory":directory,"memory_mib":768,"vcpus":1,"base_sha256":"test-only","guest_sha256":"test-only","goose_version":"1.54.0"});
    let path = directory.join("vm.json");
    std::fs::write(&path, value.to_string()).unwrap();
    path
}

#[test]
fn a_foreign_agent_manifest_cannot_be_claimed_by_the_local_agent() {
    let directory = tempfile::tempdir().unwrap();
    let path = manifest(directory.path(), "another-agent");
    assert!(
        VmConfig::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("identity")
    );
}

#[test]
fn shared_vm_directory_is_rejected_before_reading_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let path = manifest(directory.path(), "local-agent");
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        VmConfig::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("private")
    );
}

#[test]
fn agent_manifest_is_checked_against_the_selected_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = manifest(directory.path(), "operations-agent");
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(VmConfig::load_for(&path, "operations-agent").is_ok());
    assert!(VmConfig::load_for(&path, "sales-agent").is_err());
    assert!(VmConfig::load(&path).is_err());
}
