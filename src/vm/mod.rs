mod browser;
mod browser_network;
pub mod config;
mod gateway;
mod qmp;

use anyhow::{Context, Result, ensure};
use config::VmConfig;
use gateway::ModelGateway;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::Mutex,
};

pub struct VmManager {
    path: PathBuf,
    provider: String,
    model: String,
    memory: Option<std::sync::Arc<crate::memory::MemoryAccess>>,
    process: Mutex<Option<VmProcess>>,
    stop_requested: AtomicBool,
}

struct VmProcess {
    config: VmConfig,
    child: Child,
    gateway: ModelGateway,
    _browser_network: Option<browser_network::BrowserNetwork>,
    ssh_port: u16,
    group: i32,
    boot_seconds: f64,
    _lock: File,
}

impl Drop for VmProcess {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.group, libc::SIGKILL);
        }
        let _ = self.child.start_kill();
    }
}

impl VmManager {
    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    pub fn new(path: PathBuf, provider: String, model: String) -> Self {
        Self {
            path,
            provider,
            model,
            memory: None,
            process: Mutex::new(None),
            stop_requested: AtomicBool::new(false),
        }
    }

    pub fn with_memory(
        mut self,
        memory: Option<std::sync::Arc<crate::memory::MemoryAccess>>,
    ) -> Self {
        self.memory = memory;
        self
    }

    pub async fn status(&self) -> Value {
        let Ok(mut process) = self.process.try_lock() else {
            return json!({"state":"changing"});
        };
        if process
            .as_mut()
            .is_some_and(|p| p.child.try_wait().ok().flatten().is_some())
        {
            process.take();
        }
        match process.as_ref() {
            None => json!({"state":"off","configured":self.path.is_file()}),
            Some(p) => {
                let rss = std::fs::read_to_string(format!("/proc/{}/status", p.group))
                    .ok()
                    .and_then(|s| {
                        s.lines()
                            .find(|l| l.starts_with("VmRSS:"))
                            .and_then(|l| l.split_whitespace().nth(1))
                            .and_then(|n| n.parse::<u64>().ok())
                    });
                let cpu = std::fs::read_to_string(format!("/proc/{}/stat", p.group))
                    .ok()
                    .and_then(|s| {
                        let tail = s.rsplit_once(')')?.1;
                        let fields: Vec<_> = tail.split_whitespace().collect();
                        let ticks = fields.get(11)?.parse::<u64>().ok()?
                            + fields.get(12)?.parse::<u64>().ok()?;
                        let rate = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
                        (rate > 0).then_some(ticks as f64 / rate as f64)
                    });
                json!({"state":"ready","agent_id":p.config.agent_id,"pid":p.group,"memory_mib":p.config.memory_mib,"vcpus":p.config.vcpus,"boot_seconds":p.boot_seconds,"host_rss_kib":rss,"ssh_port":p.ssh_port,"host_cpu_seconds":cpu})
            }
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut process = self.process.lock().await;
        if let Some(p) = process.as_mut() {
            if p.child.try_wait()?.is_none() {
                return Ok(());
            }
            process.take();
        }
        self.stop_requested.store(false, Ordering::SeqCst);
        let config = VmConfig::load(&self.path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(config.directory.join("vm.lock"))?;
        lock.try_lock()
            .context("This VM is already owned by another Ayati process")?;
        let gateway =
            ModelGateway::start_with_memory(&self.provider, &self.model, self.memory.clone())
                .await?;
        let browser_network = if config.browser_version.is_some() {
            Some(browser_network::BrowserNetwork::start().await?)
        } else {
            None
        };
        let browser_forward = browser_network
            .as_ref()
            .map(|n| {
                format!(
                    ",guestfwd=tcp:10.0.2.101:8081-cmd:/usr/bin/socat - TCP:127.0.0.1:{}",
                    n.port
                )
            })
            .unwrap_or_default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let qmp = config.directory.join("qmp.sock");
        if qmp.exists() {
            std::fs::remove_file(&qmp)?;
        }
        let mut command = Command::new(&config.qemu);
        command.env_clear().env("PATH", "/usr/bin:/bin");
        if !config.library_path.is_empty() {
            command.env("LD_LIBRARY_PATH", &config.library_path);
        }
        command.args(["-machine","q35,accel=kvm,dump-guest-core=off,mem-merge=off","-cpu","host","-nodefaults","-no-user-config","-display","none","-monitor","none"])
            .arg("-m").arg(config.memory_mib.to_string()).arg("-smp").arg(config.vcpus.to_string())
            .arg("-L").arg(&config.firmware_data)
            .arg("-serial").arg(format!("file:{}",config.directory.join("console.log").display()))
            .arg("-qmp").arg(format!("unix:{},server=on,wait=off",qmp.display()))
            .arg("-drive").arg(format!("if=pflash,format=raw,readonly=on,file={}",qemu_path(&config.firmware_code)))
            .arg("-drive").arg(format!("if=pflash,format=raw,file={}",qemu_path(&config.directory.join("vars.fd"))))
            .arg("-drive").arg(format!("file={},if=virtio,format=qcow2",qemu_path(&config.directory.join("disk.qcow2"))))
            .arg("-netdev").arg(format!("user,id=n1,restrict=on,ipv6=off,hostfwd=tcp:127.0.0.1:{port}-:22,guestfwd=tcp:10.0.2.100:8080-cmd:/usr/bin/socat - TCP:127.0.0.1:{}{browser_forward}",gateway.port))
            .args(["-device","virtio-net-pci,netdev=n1,romfile="])
            .stdin(Stdio::null()).stdout(Stdio::null())
            .stderr(OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(config.directory.join("qemu.log"))?)
            .kill_on_drop(true).process_group(0);
        // A host crash must not leave an unowned VM writing its disk.
        let parent = unsafe { libc::getpid() };
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("VM parent exited"));
                }
                Ok(())
            });
        }
        let started = Instant::now();
        let child = command.spawn().context("Could not start QEMU/KVM")?;
        let group = child.id().unwrap() as i32;
        let mut vm = VmProcess {
            config,
            child,
            gateway,
            _browser_network: browser_network,
            ssh_port: port,
            group,
            boot_seconds: 0.0,
            _lock: lock,
        };
        while started.elapsed() < Duration::from_secs(90) {
            ensure!(
                !self.stop_requested.load(Ordering::SeqCst),
                "VM start was cancelled"
            );
            ensure!(
                vm.child.try_wait()?.is_none(),
                "VM exited during boot; inspect its private qemu.log"
            );
            if let Ok(result) = probe(
                &vm,
                json!({"version":1,"op":"probe","agent_id":vm.config.agent_id}),
            )
            .await
                && result.get("ready") == Some(&json!(true))
                && result.get("agent_id") == Some(&json!(vm.config.agent_id))
            {
                vm.boot_seconds = started.elapsed().as_secs_f64();
                *process = Some(vm);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        anyhow::bail!("VM boot timed out; inspect its private console.log")
    }

    pub async fn connect(&self, run: &str) -> Result<Child> {
        self.start().await?;
        let process = self.process.lock().await;
        let p = process.as_ref().context("VM stopped before connection")?;
        let mut command = p.config.ssh(p.ssh_port);
        command.env_clear().env("PATH", "/usr/bin:/bin");
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .process_group(0)
            .spawn()?;
        let mut config = json!({"version":1,"op":"acp","agent_id":p.config.agent_id,"model":self.model,"token":p.gateway.token});
        if self.memory.is_some() {
            config["memory_token"] = json!(run);
        }
        if p.config.browser_version.is_some() {
            config["run"] = json!(run);
        }
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(format!("{config}\n").as_bytes())
            .await?;
        child.stdin.as_mut().unwrap().flush().await?;
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            read_reply(child.stdout.as_mut().unwrap()),
        )
        .await??;
        ensure!(
            response.get("ready") == Some(&json!(true))
                && response.get("agent_id") == Some(&json!(p.config.agent_id))
                && response.get("version") == Some(&json!(1)),
            "Guest bridge identity or protocol mismatch"
        );
        Ok(child)
    }

    pub async fn storage_probe(&self, value: Option<&str>) -> Result<Value> {
        self.start().await?;
        let process = self.process.lock().await;
        let p = process.as_ref().context("VM stopped")?;
        probe(
            p,
            json!({"version":1,"op":"storage_probe","agent_id":p.config.agent_id,"value":value}),
        )
        .await
    }

    pub async fn model_failed(&self) -> bool {
        self.process
            .lock()
            .await
            .as_ref()
            .is_none_or(|p| p.gateway.failed.load(Ordering::SeqCst))
    }

    pub async fn stop(&self) -> Result<()> {
        self.stop_requested.store(true, Ordering::SeqCst);
        let browser = self
            .process
            .lock()
            .await
            .as_ref()
            .is_some_and(|p| p.config.browser_version.is_some());
        let parked = if browser {
            self.browser(json!({"op":"park"})).await.map(|_| ())
        } else {
            Ok(())
        };
        if let Some(mut p) = self.process.lock().await.take() {
            let requested = qmp::power_down(&p.config.directory.join("qmp.sock")).await;
            if requested.is_ok()
                && matches!(
                    tokio::time::timeout(Duration::from_secs(15), p.child.wait()).await,
                    Ok(Ok(status)) if status.success()
                )
            {
                return parked.context("VM stopped but browser profile flush was not confirmed");
            }
            drop(p);
            anyhow::bail!("VM required forced termination; its interrupted work was not replayed")
        }
        Ok(())
    }
}

fn qemu_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace(',', ",,")
}

async fn read_reply(read: &mut (impl AsyncReadExt + Unpin)) -> Result<Value> {
    let mut line = Vec::new();
    loop {
        ensure!(line.len() < 4096, "Guest reply exceeded its limit");
        let byte = read.read_u8().await?;
        if byte == b'\n' {
            break;
        }
        line.push(byte);
    }
    Ok(serde_json::from_slice(&line)?)
}

async fn probe(vm: &VmProcess, input: Value) -> Result<Value> {
    let mut command = vm.config.ssh(vm.ssh_port);
    command.env_clear().env("PATH", "/usr/bin:/bin");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{input}\n").as_bytes())
        .await?;
    let output = tokio::time::timeout(Duration::from_secs(3), child.wait_with_output()).await??;
    ensure!(output.status.success(), "Guest probe unavailable");
    ensure!(
        output.stdout.len() < 4096,
        "Guest probe reply exceeded its limit"
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}
