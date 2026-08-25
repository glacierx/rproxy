#![recursion_limit="512"]
#![warn(rust_2018_idioms)]

use chrono::Local;
use std::sync::Arc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::path::Path;
use log::*;
use argh::FromArgs;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod tcp;
mod udp;
mod dns;
use rproxy::{Proxy, Settings};

/// Return a per-user runtime directory for the PID / reload files, verified to
/// be owned by the current user and inaccessible to other local users.
///
/// A shared world-writable temp dir (e.g. `/tmp`) must never be used on Unix:
/// another local user could pre-position a PID file and induce us to signal an
/// arbitrary process on `-s reload`. We prefer `$XDG_RUNTIME_DIR` and otherwise
/// create a `rproxy-<uid>` subdirectory with mode 0700, refusing to use either
/// unless it is owned by us and not group/other accessible.
#[cfg(unix)]
fn runtime_dir() -> std::io::Result<PathBuf> {
    use std::io::{Error, ErrorKind};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    let euid = nix::unistd::geteuid().as_raw();

    let verify = |p: &Path| -> std::io::Result<()> {
        // lstat, so a symlink is rejected rather than followed.
        let meta = std::fs::symlink_metadata(p)?;
        if !meta.is_dir() {
            return Err(Error::new(ErrorKind::Other,
                format!("{} is not a directory", p.display())));
        }
        if meta.uid() != euid {
            return Err(Error::new(ErrorKind::PermissionDenied,
                format!("{} is not owned by uid {}", p.display(), euid)));
        }
        if meta.mode() & 0o077 != 0 {
            return Err(Error::new(ErrorKind::PermissionDenied,
                format!("{} is accessible to other users (mode {:o})",
                    p.display(), meta.mode() & 0o7777)));
        }
        Ok(())
    };

    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(dir);
        if p.is_absolute() && verify(&p).is_ok() {
            return Ok(p);
        }
    }

    let dir = std::env::temp_dir().join(format!("rproxy-{}", euid));
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    verify(&dir)?;
    Ok(dir)
}

#[cfg(not(unix))]
fn runtime_dir() -> std::io::Result<PathBuf> {
    Ok(std::env::temp_dir())
}

fn pid_file_path() -> std::io::Result<PathBuf> {
    Ok(runtime_dir()?.join("rproxy.pid"))
}

fn reload_signal_path() -> std::io::Result<PathBuf> {
    Ok(runtime_dir()?.join("rproxy.reload"))
}

#[allow(dead_code)]
#[derive(FromArgs)]
#[argh(description = "rproxy is a platform independent UDP TCP high performance async proxy")]
struct Options {
    /// remote service endpoint (UDP & TCP)
    #[argh(option, short='r', default="\"\".to_string()")]
    remote: String,
    /// local endpoint to be binded by rproxy
    #[argh(option, short='b', default="\"\".to_string()")]
    bind: String,
    /// protocol of the remote service(UDP|TCP)
    #[argh(option, short='p', default="\"UDP\".to_string()")]
    protocol: String,
    /// set logger level to debug or not
    #[argh(switch, short='d')]
    debug: bool,

    /// logger settings rotating etc...
    #[argh(option, short = 'l', default = "\"rproxy.logger.yaml\".to_string()")]
    logger_settings: String,

    /// configuration for multiple proxy instances
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,

    /// send signal to running rproxy process (reload)
    #[argh(option, short = 's')]
    signal: Option<String>,

    /// max concurrent TCP connections (default: 1024)
    #[argh(option)]
    max_connections: Option<usize>,

    /// max concurrent UDP client tunnels (default: 1024)
    #[argh(option)]
    max_client_tunnels: Option<usize>,

    /// TCP keepalive idle time in seconds (default: 60)
    #[argh(option)]
    keepalive_idle: Option<u64>,

    /// TCP keepalive probe interval in seconds (default: 30)
    #[argh(option)]
    keepalive_interval: Option<u64>,
}

static MY_LOGGER: MyLogger = MyLogger;

struct MyLogger;

impl log::Log for MyLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            println!("[{}][{}] - {}", record.level(), Local::now(), record.args());
        }
    }
    fn flush(&self) {}
}

fn build_settings(options: &Options, base: Settings) -> Settings {
    Settings {
        max_connections: options.max_connections.unwrap_or(base.max_connections),
        max_client_tunnels: options.max_client_tunnels.unwrap_or(base.max_client_tunnels),
        keepalive_idle: options.keepalive_idle.unwrap_or(base.keepalive_idle),
        keepalive_interval: options.keepalive_interval.unwrap_or(base.keepalive_interval),
    }
}

fn write_pid_file() {
    let path = match pid_file_path() {
        Ok(p) => p,
        Err(e) => {
            warn!("Refusing to write PID file: no secure runtime directory: {}", e);
            return;
        }
    };
    let pid = std::process::id();

    #[cfg(unix)]
    let result = {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut f| f.write_all(pid.to_string().as_bytes()))
    };

    #[cfg(not(unix))]
    let result = std::fs::write(&path, pid.to_string());

    if let Err(e) = result {
        warn!("Failed to write PID file {}: {}", path.display(), e);
    } else {
        info!("PID {} written to {}", pid, path.display());
    }
}

fn remove_pid_file() {
    if let Ok(p) = pid_file_path() {
        let _ = std::fs::remove_file(p);
    }
    if let Ok(p) = reload_signal_path() {
        let _ = std::fs::remove_file(p);
    }
}

fn send_reload_signal() {
    let pid_path = match pid_file_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Cannot locate a secure runtime directory: {}", e);
            std::process::exit(1);
        }
    };

    // The PID file lives in a per-user 0700 directory, but verify the file
    // itself is a regular file owned by us and not writable by other users
    // before trusting its contents — defence in depth against being induced to
    // signal an attacker-chosen process.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let euid = nix::unistd::geteuid().as_raw();
        match std::fs::symlink_metadata(&pid_path) {
            Ok(meta) => {
                if !meta.is_file() {
                    eprintln!("Refusing to use PID file {}: not a regular file", pid_path.display());
                    std::process::exit(1);
                }
                if meta.uid() != euid {
                    eprintln!("Refusing to use PID file {}: not owned by uid {}", pid_path.display(), euid);
                    std::process::exit(1);
                }
                if meta.mode() & 0o022 != 0 {
                    eprintln!("Refusing to use PID file {}: writable by other users", pid_path.display());
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Failed to stat PID file {}: {}. Is rproxy running?", pid_path.display(), e);
                std::process::exit(1);
            }
        }
    }

    // Verify rproxy is running by checking PID file
    match std::fs::read_to_string(&pid_path) {
        Ok(content) => {
            match content.trim().parse::<u32>() {
                Ok(pid) => {
                    // On Unix, send SIGHUP directly
                    #[cfg(unix)]
                    {
                        // On Linux, confirm the target is actually an rproxy
                        // process before signalling, so a stale or foreign PID
                        // is never signalled.
                        #[cfg(target_os = "linux")]
                        {
                            let comm_path = format!("/proc/{}/comm", pid);
                            match std::fs::read_to_string(&comm_path) {
                                Ok(comm) => {
                                    if comm.trim() != "rproxy" {
                                        eprintln!("Refusing to signal PID {}: not an rproxy process (comm={:?})",
                                            pid, comm.trim());
                                        std::process::exit(1);
                                    }
                                }
                                Err(_) => {
                                    eprintln!("rproxy (PID {}) does not appear to be running", pid);
                                    std::process::exit(1);
                                }
                            }
                        }

                        use nix::sys::signal::{kill, Signal};
                        use nix::unistd::Pid;
                        match kill(Pid::from_raw(pid as i32), Signal::SIGHUP) {
                            Ok(()) => {
                                println!("Reload signal sent to rproxy (PID {})", pid);
                                return;
                            }
                            Err(e) => {
                                eprintln!("Failed to send signal to PID {}: {}", pid, e);
                                std::process::exit(1);
                            }
                        }
                    }

                    // On Windows (or fallback), write a signal file that the running process polls
                    #[cfg(not(unix))]
                    {
                        let signal_path = match reload_signal_path() {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("Cannot locate runtime directory: {}", e);
                                std::process::exit(1);
                            }
                        };
                        match std::fs::write(&signal_path, "reload") {
                            Ok(()) => {
                                println!("Reload signal written for rproxy (PID {})", pid);
                            }
                            Err(e) => {
                                eprintln!("Failed to write reload signal file: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Invalid PID in {}: {}", pid_path.display(), e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read PID file {}: {}. Is rproxy running?", pid_path.display(), e);
            std::process::exit(1);
        }
    }
}

struct ProxyTask {
    #[allow(dead_code)]
    handle: JoinHandle<Result<(), std::io::Error>>,
    cancel: CancellationToken,
    proxy: Proxy,
}

fn spawn_proxy(proxy: &Proxy, settings: &Arc<Settings>) -> ProxyTask {
    let bind = proxy.bind.clone();
    let remote = proxy.remote.clone();
    let protocol = proxy.protocol.clone();
    let s = settings.clone();
    let cancel = CancellationToken::new();
    let c = cancel.clone();

    let handle = if protocol == "UDP" {
        tokio::spawn(async move { udp::udp_proxy(&bind, &remote, s, c).await })
    } else {
        tokio::spawn(async move { tcp::tcp_proxy(&bind, &remote, s, c).await })
    };

    ProxyTask {
        handle,
        cancel,
        proxy: proxy.clone(),
    }
}

fn reload_config(
    config_path: &Path,
    options: &Options,
    running: &mut HashMap<String, ProxyTask>,
) {
    let config = match rproxy::load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to reload configuration: {:?}", e);
            return;
        }
    };

    let settings = Arc::new(build_settings(options, config.settings));

    // Build new proxy map keyed by bind address
    let mut new_proxies: HashMap<String, &Proxy> = HashMap::new();
    for proxy in &config.proxies {
        new_proxies.insert(proxy.bind.clone(), proxy);
    }

    // Find removed and changed proxies
    let old_keys: Vec<String> = running.keys().cloned().collect();
    for key in &old_keys {
        match new_proxies.get(key) {
            None => {
                // Removed
                info!("[reload] Stopping proxy on {}", key);
                if let Some(task) = running.remove(key) {
                    task.cancel.cancel();
                }
            }
            Some(new_proxy) => {
                let old_task = running.get(key).unwrap();
                if old_task.proxy.remote != new_proxy.remote || old_task.proxy.protocol != new_proxy.protocol {
                    // Changed — cancel old, will spawn new below
                    info!("[reload] Restarting proxy on {} ({}->{})",
                        key, old_task.proxy.remote, new_proxy.remote);
                    if let Some(task) = running.remove(key) {
                        task.cancel.cancel();
                    }
                } else {
                    // Unchanged — keep running
                    debug!("[reload] Proxy on {} unchanged, keeping", key);
                }
            }
        }
    }

    // Spawn new and changed proxies
    for (bind, proxy) in &new_proxies {
        if !running.contains_key(bind) {
            info!("[reload] Starting proxy on {} -> {} ({})", bind, proxy.remote, proxy.protocol);
            let task = spawn_proxy(proxy, &settings);
            running.insert(bind.clone(), task);
        }
    }

    info!("[reload] Configuration reloaded: {} proxies active", running.len());
}

/// Wait for a reload signal. On Unix uses SIGHUP, on Windows polls a signal file.
async fn wait_for_reload_signal() {
    #[cfg(unix)]
    {
        let mut sighup = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::hangup()
        ).expect("Failed to register SIGHUP handler");
        sighup.recv().await;
    }

    #[cfg(not(unix))]
    {
        let signal_path = reload_signal_path()
            .unwrap_or_else(|_| std::env::temp_dir().join("rproxy.reload"));
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if signal_path.exists() {
                let _ = std::fs::remove_file(&signal_path);
                break;
            }
        }
    }
}

#[tokio::main]
async fn main(){
    let options: Options = argh::from_env();

    // Handle -s reload
    if let Some(ref sig) = options.signal {
        match sig.as_str() {
            "reload" => {
                send_reload_signal();
                return;
            }
            _ => {
                eprintln!("Unknown signal: {}. Supported: reload", sig);
                std::process::exit(1);
            }
        }
    }

    // Setup logging
    if Path::new(&options.logger_settings).exists(){
        log4rs::init_file(&options.logger_settings,
            Default::default()).unwrap();
        debug!("NICE");
    } else {
        log::set_logger(&MY_LOGGER).unwrap();
        if options.debug {
            log::set_max_level(log::LevelFilter::Debug);
        } else {
            log::set_max_level(log::LevelFilter::Info);
        }
    }

    match options.config {
        None => {
            // Single proxy mode — no hot reload
            let settings = Arc::new(build_settings(&options, Settings::default()));
            let cancel = CancellationToken::new();
            if options.protocol == "UDP"{
                udp::udp_proxy(&options.bind, &options.remote, settings, cancel).await.unwrap();
            } else if options.protocol == "TCP" {
                tcp::tcp_proxy(&options.bind, &options.remote, settings, cancel).await.unwrap();
            }
        },
        Some(ref config_path) => {
            if !config_path.as_path().exists() {
                error!("Invalid configuration file path {}", config_path.as_path().display());
                return;
            }

            // Write PID file for -s reload
            write_pid_file();

            let config = match rproxy::load_config(config_path.as_path()) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to parse configuration: {:?}", e);
                    remove_pid_file();
                    return;
                }
            };

            let settings = Arc::new(build_settings(&options, config.settings));
            info!("Settings: max_connections={}, max_client_tunnels={}, keepalive_idle={}s, keepalive_interval={}s",
                settings.max_connections, settings.max_client_tunnels,
                settings.keepalive_idle, settings.keepalive_interval);

            // Spawn initial proxies
            let mut running: HashMap<String, ProxyTask> = HashMap::new();
            for proxy in &config.proxies {
                let task = spawn_proxy(proxy, &settings);
                running.insert(proxy.bind.clone(), task);
            }

            info!("Started {} proxies, listening for reload signal", running.len());

            loop {
                tokio::select! {
                    _ = wait_for_reload_signal() => {
                        info!("[reload] Reload signal received, reloading configuration from {}", config_path.display());
                        reload_config(config_path.as_path(), &options, &mut running);
                    },
                    _ = tokio::signal::ctrl_c() => {
                        info!("Ctrl+C received, shutting down all proxies...");
                        for (bind, task) in running.drain() {
                            info!("Stopping proxy on {}", bind);
                            task.cancel.cancel();
                        }
                        break;
                    }
                }
            }

            remove_pid_file();
        }
    }
}
