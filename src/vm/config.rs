use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmConfig {
    pub version: u32,
    pub agent_id: String,
    pub qemu: PathBuf,
    pub qemu_img: PathBuf,
    pub library_path: String,
    pub firmware_code: PathBuf,
    pub firmware_data: PathBuf,
    pub directory: PathBuf,
    pub memory_mib: u32,
    pub vcpus: u32,
    pub base_sha256: String,
    pub guest_sha256: String,
    pub goose_version: String,
    #[serde(default)]
    pub browser_version: Option<String>,
}

impl VmConfig {
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_for(path, "local-agent")
    }

    pub fn load_for(path: &Path, agent_id: &str) -> Result<Self> {
        let config: Self = serde_json::from_slice(
            &std::fs::read(path).context("Prepare a local VM before selecting VM execution")?,
        )?;
        ensure!(
            config.version == 1 && config.agent_id == agent_id && config.goose_version == "1.54.0",
            "VM manifest identity or version does not match this local agent"
        );
        ensure!(
            (256..=8192).contains(&config.memory_mib) && (1..=8).contains(&config.vcpus),
            "Invalid VM resource limits"
        );
        ensure!(
            config.directory.is_absolute()
                && config.qemu.is_absolute()
                && config.firmware_code.is_absolute(),
            "VM paths must be absolute"
        );
        ensure!(
            config.qemu.is_file()
                && config.firmware_code.is_file()
                && config.firmware_data.is_dir(),
            "VM runtime artifacts are missing"
        );
        ensure!(
            !config.directory.to_string_lossy().contains(','),
            "VM directory cannot contain a comma"
        );
        let metadata = std::fs::metadata(&config.directory)?;
        ensure!(
            metadata.uid() == unsafe { libc::getuid() } && metadata.mode() & 0o077 == 0,
            "VM directory must be private and owned by this user"
        );
        for name in ["client_key", "known_hosts", "disk.qcow2", "vars.fd"] {
            ensure!(
                config.directory.join(name).is_file(),
                "VM files are missing; rerun preparation in a fresh directory"
            );
        }
        ensure!(
            config.directory.join("qmp.sock").as_os_str().len() < 104,
            "VM directory path is too long for a private socket"
        );
        Ok(config)
    }

    pub fn ssh(&self, port: u16) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("ssh");
        command
            .args([
                "-T",
                "-F",
                "/dev/null",
                "-o",
                "BatchMode=yes",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "ConnectTimeout=2",
                "-o",
                "ConnectionAttempts=1",
                "-o",
                "LogLevel=ERROR",
            ])
            .arg("-o")
            .arg(format!(
                "UserKnownHostsFile={}",
                self.directory.join("known_hosts").display()
            ))
            .arg("-o")
            .arg(format!("HostKeyAlias=ayati-{}", self.agent_id))
            .arg("-i")
            .arg(self.directory.join("client_key"))
            .arg("-p")
            .arg(port.to_string())
            .arg("ayati@127.0.0.1");
        command
    }
}

#[cfg(test)]
mod tests;
