//! Starting sd-server processes, or attaching to ones already running
//! (DESIGN.md, Backend).

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::{SdServerConfig, ServerConfig};
use crate::sdserver::Client;

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("{name}: no sd-server answers on {url}")]
    NotRunning { name: String, url: String },
    #[error("{name}: can't start {binary}: {source}")]
    Spawn { name: String, binary: String, source: std::io::Error },
    #[error("{name}: sd-server exited ({status}) during start-up; log {}:\n{tail}", log.display())]
    Exited { name: String, status: String, log: PathBuf, tail: String },
    #[error("{name}: opening cancelled")]
    Cancelled { name: String },
    #[error("{name}: sd-server didn't answer within {secs} s; log {}", log.display())]
    Timeout { name: String, secs: u64, log: PathBuf },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// A server the app talks to. Dropping it stops the process if the app
/// started it; an attached server is left running.
#[derive(Debug)]
pub struct ServerProcess {
    pub name: String,
    pub device: String,
    pub client: Client,
    child: Option<Child>,
    /// The log file, for a launched server.
    pub log: Option<PathBuf>,
}

impl ServerProcess {
    /// Whether the app started this process (vs attached to it).
    pub fn launched(&self) -> bool {
        self.child.is_some()
    }

    /// Lets a started server outlive the app: dropping this no longer stops it.
    pub fn detach(&mut self) {
        self.child = None;
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The command line for a `Start` server (empty for `Connect`).
pub fn command_args(server: &ServerConfig, shared: &SdServerConfig) -> Vec<String> {
    let ServerConfig::Start { host, port, device, args: own, .. } = server else { return vec![] };
    let mut args: Vec<String> = vec![
        "--listen-ip".into(),
        host.clone(),
        "--listen-port".into(),
        port.to_string(),
        "--backend".into(),
        device.clone(),
    ];
    for dir in &shared.model_dirs {
        args.extend(["--model-dir".into(), dir.clone()]);
    }
    args.extend(shared.args.iter().cloned());
    args.extend(own.iter().cloned());
    args
}

/// Opens one server of the chosen preset. `Connect` checks that it answers.
/// `Start` launches sd-server, logging to `log_dir/sd-server-<name>.log`,
/// and waits up to `wait` for it to answer; if an sd-server already answers
/// on that port (e.g. left over from a crash) it is used as is.
pub fn open(
    server: &ServerConfig,
    shared: &SdServerConfig,
    log_dir: &Path,
    wait: Duration,
) -> Result<ServerProcess, LaunchError> {
    open_cancellable(server, shared, log_dir, wait, &std::sync::atomic::AtomicBool::new(false))
}

/// As [`open`]; setting `cancel` stops the wait (and a server it started)
/// within 200 ms.
pub fn open_cancellable(
    server: &ServerConfig,
    shared: &SdServerConfig,
    log_dir: &Path,
    wait: Duration,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<ServerProcess, LaunchError> {
    let name = server.name().to_string();
    let client = Client::new(&server.base_url());
    let mut proc = ServerProcess {
        name: name.clone(),
        device: server.device().to_string(),
        client: client.clone(),
        child: None,
        log: None,
    };
    if client.capabilities().is_ok() {
        return Ok(proc);
    }
    if matches!(server, ServerConfig::Connect { .. }) {
        return Err(LaunchError::NotRunning { name, url: server.base_url() });
    }
    fs::create_dir_all(log_dir)?;
    let log = log_path(log_dir, server);
    let out = OpenOptions::new().create(true).append(true).open(&log)?;
    let start = out.metadata()?.len();
    let child = Command::new(&shared.binary)
        .args(command_args(server, shared))
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()
        .map_err(|source| LaunchError::Spawn { name: name.clone(), binary: shared.binary.clone(), source })?;
    proc.child = Some(child);
    proc.log = Some(log.clone());
    let deadline = Instant::now() + wait;
    loop {
        if client.capabilities().is_ok() {
            return Ok(proc);
        }
        if let Some(status) = proc.child.as_mut().unwrap().try_wait()? {
            proc.child = None;
            return Err(LaunchError::Exited { name, status: status.to_string(), tail: tail(&log, start), log });
        }
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            proc.stop();
            return Err(LaunchError::Cancelled { name });
        }
        if Instant::now() >= deadline {
            return Err(LaunchError::Timeout { name, secs: wait.as_secs(), log });
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Where a `Start` server logs.
pub fn log_path(log_dir: &Path, server: &ServerConfig) -> PathBuf {
    log_dir.join(format!("sd-server-{}.log", server.name()))
}

/// What a starting server is doing, from its log since byte `start`: the
/// latest line, tidied ("loading tensors 294/396 · 4.38GB/s", "loading
/// tensors completed, taking 1.32s"). Progress bars redraw with `\r`.
pub fn latest_line(log: &Path, start: u64) -> Option<String> {
    let mut f = fs::File::open(log).ok()?;
    let len = f.metadata().ok()?.len();
    let from = start.max(len.saturating_sub(4096));
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut last = None;
    for line in text.split(['\r', '\n']) {
        let line = line.replace("\x1b[K", "").replace("[K", "");
        let line = line.trim();
        if !line.is_empty() {
            last = Some(line.to_string());
        }
    }
    Some(tidy_line(&last?))
}

/// "[I] loading tensors completed --- model_loader.cpp:1405" → "loading
/// tensors completed"; "|####   | 294/396 - 4.38GB/s" → "294/396 · 4.38GB/s".
fn tidy_line(line: &str) -> String {
    let line = line.split(" --- ").next().unwrap_or(line);
    let line = ["[I] ", "[W] ", "[E] ", "[D] "].iter().find_map(|p| line.strip_prefix(p)).unwrap_or(line);
    let line = match line.strip_prefix('|').and_then(|l| l.split_once('|')) {
        Some((_, rest)) => rest.trim().replace(" - ", " · "),
        None => line.trim().to_string(),
    };
    line.chars().take(120).collect()
}

/// The last 2 KB written to `log` since byte `start`.
fn tail(log: &Path, start: u64) -> String {
    let Ok(mut f) = fs::File::open(log) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = start.max(len.saturating_sub(2048));
    let mut s = String::new();
    let _ = f.seek(SeekFrom::Start(from)).and_then(|_| f.read_to_string(&mut s));
    s
}

/// `sd-server --list-devices`, one line per device as printed.
pub fn list_devices(binary: &str) -> std::io::Result<Vec<String>> {
    let out = Command::new(binary).arg("--list-devices").stdin(Stdio::null()).output()?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// The device name a `--list-devices` line starts with (`ROCm0` in
/// "ROCm0\tAMD Radeon …").
pub fn device_name(line: &str) -> &str {
    line.split_whitespace().next().unwrap_or("").trim_end_matches(':')
}

/// The configured device name as `--list-devices` spells it (matched
/// case-insensitively), if listed.
pub fn match_device<'a>(listed: &'a [String], wanted: &str) -> Option<&'a str> {
    let w = wanted.to_lowercase();
    listed.iter().map(String::as_str).find(|l| device_name(l).to_lowercase() == w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_starting_servers_latest_log_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("sd-server-gpu0.log");
        fs::write(&log, "old run\n").unwrap();
        let start = fs::metadata(&log).unwrap().len();
        assert_eq!(latest_line(&log, start), None, "nothing since the start");
        let bars = "\n  |######     |  41/396 - 158.91MB/s\x1b[K\r  |#######    | 294/396 -   4.38GB/s\x1b[K";
        fs::write(&log, format!("old run\n[I] loading model from 'q.gguf' --- model.cpp:12{bars}")).unwrap();
        assert_eq!(latest_line(&log, start).unwrap(), "294/396 ·   4.38GB/s");
        let done = "\n[I] loading tensors completed, taking 1.32s (read: 0.08s) --- model_loader.cpp:1405\n\n";
        fs::write(&log, format!("old run\n{bars}{done}")).unwrap();
        assert_eq!(latest_line(&log, start).unwrap(), "loading tensors completed, taking 1.32s (read: 0.08s)");
    }

    #[test]
    fn args() {
        let server = ServerConfig::Start {
            name: "gpu1".into(),
            host: "127.0.0.1".into(),
            port: 1235,
            device: "ROCm1".into(),
            args: vec!["--offload-to-cpu".into()],
        };
        let shared = SdServerConfig { model_dirs: vec!["/m".into()], ..Default::default() };
        assert_eq!(
            command_args(&server, &shared).join(" "),
            "--listen-ip 127.0.0.1 --listen-port 1235 --backend ROCm1 --model-dir /m --fa --conditioning-cache-size 32 --offload-to-cpu"
        );
        let remote = ServerConfig::Connect { name: "box".into(), url: "http://h:1".into(), device: String::new() };
        assert!(command_args(&remote, &shared).is_empty());
    }

    #[test]
    fn devices() {
        let listed = vec![
            "CPU: cpu".to_string(),
            "ROCm0: AMD Radeon AI PRO R9700".into(),
            "ROCm1: AMD Radeon AI PRO R9700".into(),
        ];
        assert_eq!(match_device(&listed, "rocm1"), Some("ROCm1: AMD Radeon AI PRO R9700"));
        assert_eq!(match_device(&listed, "Vulkan0"), None);
    }
}
