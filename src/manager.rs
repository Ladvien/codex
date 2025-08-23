use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{info, warn};

/// Default paths for manager files
pub struct ManagerPaths {
    pub pid_file: PathBuf,
    pub log_file: PathBuf,
    pub config_dir: PathBuf,
}

impl Default for ManagerPaths {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("codex-memory");

        Self {
            pid_file: config_dir.join("codex-memory.pid"),
            log_file: config_dir.join("codex-memory.log"),
            config_dir,
        }
    }
}

/// Server manager for handling daemon processes
pub struct ServerManager {
    paths: ManagerPaths,
}

impl Default for ServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerManager {
    pub fn new() -> Self {
        let paths = ManagerPaths::default();

        // Ensure config directory exists
        if !paths.config_dir.exists() {
            fs::create_dir_all(&paths.config_dir).ok();
        }

        Self { paths }
    }

    pub fn with_paths(pid_file: Option<String>, log_file: Option<String>) -> Self {
        let mut paths = ManagerPaths::default();

        if let Some(pid) = pid_file {
            paths.pid_file = PathBuf::from(pid);
        }
        if let Some(log) = log_file {
            paths.log_file = PathBuf::from(log);
        }

        Self { paths }
    }

    /// Start the server as a daemon
    pub async fn start_daemon(&self, daemon: bool) -> Result<()> {
        // Check if already running
        if let Some(pid) = self.get_running_pid()? {
            return Err(anyhow::anyhow!("Server already running with PID: {}", pid));
        }

        info!("Starting Codex Memory server...");

        if daemon {
            // Start as daemon using fork or platform-specific method
            self.start_background_process().await?;
        } else {
            // Start in foreground - this would need to be implemented differently
            // For now, we'll just start in background
            info!("Starting server in foreground mode...");
            self.start_background_process().await?;
        }

        Ok(())
    }

    /// Start server in background
    async fn start_background_process(&self) -> Result<()> {
        let exe = std::env::current_exe().context("Failed to get current executable path")?;

        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.log_file)
            .context("Failed to open log file")?;

        let cmd = Command::new(exe)
            .arg("start")
            .arg("--skip-setup")
            .stdout(Stdio::from(log_file.try_clone()?))
            .stderr(Stdio::from(log_file))
            .spawn()
            .context("Failed to spawn server process")?;

        let pid = cmd.id();

        // Write PID file
        fs::write(&self.paths.pid_file, pid.to_string()).context("Failed to write PID file")?;

        info!("Server started with PID: {}", pid);
        info!("Log file: {}", self.paths.log_file.display());

        Ok(())
    }

    /// Stop the running server
    pub async fn stop(&self) -> Result<()> {
        let pid = self
            .get_running_pid()?
            .ok_or_else(|| anyhow::anyhow!("Server is not running"))?;

        info!("Stopping server with PID: {}", pid);

        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            // Send SIGTERM for graceful shutdown
            signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
                .context("Failed to send SIGTERM")?;

            // Wait for process to exit (max 10 seconds)
            for _ in 0..10 {
                if !self.is_process_running(pid)? {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }

            // Force kill if still running
            if self.is_process_running(pid)? {
                warn!("Process didn't stop gracefully, forcing kill");
                signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL)
                    .context("Failed to send SIGKILL")?;
            }
        }

        #[cfg(windows)]
        {
            // Windows process termination
            let output = Command::new("taskkill")
                .args(&["/PID", &pid.to_string(), "/F"])
                .output()
                .context("Failed to kill process")?;

            if !output.status.success() {
                return Err(anyhow::anyhow!("Failed to stop server"));
            }
        }

        // Remove PID file
        fs::remove_file(&self.paths.pid_file).ok();
        info!("Server stopped");

        Ok(())
    }

    /// Restart the server
    pub async fn restart(&self) -> Result<()> {
        info!("Restarting server...");

        // Stop if running
        if self.get_running_pid()?.is_some() {
            self.stop().await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }

        // Start again
        self.start_daemon(true).await?;

        Ok(())
    }

    /// Get server status
    pub async fn status(&self, detailed: bool) -> Result<()> {
        match self.get_running_pid()? {
            Some(pid) => {
                println!("● Server is running");
                println!("  PID: {pid}");

                if detailed {
                    println!("  PID file: {}", self.paths.pid_file.display());
                    println!("  Log file: {}", self.paths.log_file.display());

                    // Show recent logs
                    if self.paths.log_file.exists() {
                        println!("\nRecent logs:");
                        self.show_logs(5, false).await?;
                    }
                }
            }
            None => {
                println!("○ Server is not running");

                if detailed {
                    println!("  PID file: {}", self.paths.pid_file.display());
                    println!("  Log file: {}", self.paths.log_file.display());
                }
            }
        }

        Ok(())
    }

    /// Show server logs
    pub async fn show_logs(&self, lines: usize, follow: bool) -> Result<()> {
        if !self.paths.log_file.exists() {
            return Err(anyhow::anyhow!("Log file not found"));
        }

        if follow {
            // Follow logs (like tail -f)
            let file = fs::File::open(&self.paths.log_file)?;
            let reader = BufReader::new(file);

            println!("Following logs (Ctrl+C to stop)...");
            for line in reader.lines() {
                println!("{}", line?);
            }
        } else {
            // Show last N lines
            let content = fs::read_to_string(&self.paths.log_file)?;
            let all_lines: Vec<&str> = content.lines().collect();
            let start = if all_lines.len() > lines {
                all_lines.len() - lines
            } else {
                0
            };

            for line in &all_lines[start..] {
                println!("{line}");
            }
        }

        Ok(())
    }

    /// Install as system service
    pub async fn install_service(&self, service_type: Option<String>) -> Result<()> {
        let service_type = service_type.unwrap_or_else(|| {
            #[cfg(target_os = "linux")]
            return "systemd".to_string();
            #[cfg(target_os = "macos")]
            return "launchd".to_string();
            #[cfg(target_os = "windows")]
            return "windows".to_string();
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            return "none".to_string();
        });

        match service_type.as_str() {
            "systemd" => self.install_systemd_service().await,
            "launchd" => self.install_launchd_service().await,
            "windows" => self.install_windows_service().await,
            _ => Err(anyhow::anyhow!(
                "Unsupported service type: {}",
                service_type
            )),
        }
    }

    /// Install systemd service (Linux)
    async fn install_systemd_service(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let service_content = format!(
                r#"[Unit]
Description=Codex Memory System
After=network.target postgresql.service

[Service]
Type=simple
ExecStart={} start
ExecStop={} manager stop
Restart=on-failure
RestartSec=10
StandardOutput=append:{}
StandardError=append:{}

[Install]
WantedBy=multi-user.target
"#,
                std::env::current_exe()?.display(),
                std::env::current_exe()?.display(),
                self.paths.log_file.display(),
                self.paths.log_file.display(),
            );

            let service_path = PathBuf::from("/etc/systemd/system/codex-memory.service");

            // Write service file (requires sudo)
            fs::write(&service_path, service_content)
                .context("Failed to write service file (need sudo?)")?;

            // Reload systemd
            Command::new("systemctl")
                .args(&["daemon-reload"])
                .status()
                .context("Failed to reload systemd")?;

            // Enable service
            Command::new("systemctl")
                .args(&["enable", "codex-memory.service"])
                .status()
                .context("Failed to enable service")?;

            info!("Systemd service installed successfully");
            info!("Start with: systemctl start codex-memory");
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        Err(anyhow::anyhow!("Systemd is only available on Linux"))
    }

    /// Install launchd service (macOS)
    async fn install_launchd_service(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let plist_content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.codex.memory</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>start</string>
    </array>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
    <key>RunAtLoad</key>
    <false/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
</dict>
</plist>
"#,
                std::env::current_exe()?.display(),
                self.paths.log_file.display(),
                self.paths.log_file.display(),
            );

            let plist_path = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
                .join("Library/LaunchAgents/com.codex.memory.plist");

            // Write plist file
            fs::write(&plist_path, plist_content).context("Failed to write plist file")?;

            // Load the service
            Command::new("launchctl")
                .args(["load", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to load launchd service")?;

            info!("Launchd service installed successfully");
            info!("Start with: launchctl start com.codex.memory");
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        Err(anyhow::anyhow!("Launchd is only available on macOS"))
    }

    /// Install Windows service
    async fn install_windows_service(&self) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            // Windows service installation using sc.exe
            let exe_path = std::env::current_exe()?;

            Command::new("sc")
                .args(&[
                    "create",
                    "CodexMemory",
                    &format!("binPath= \"{}\" start", exe_path.display()),
                    "DisplayName= \"Codex Memory System\"",
                    "start= auto",
                ])
                .status()
                .context("Failed to create Windows service")?;

            info!("Windows service installed successfully");
            info!("Start with: sc start CodexMemory");
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        Err(anyhow::anyhow!(
            "Windows service is only available on Windows"
        ))
    }

    /// Uninstall system service
    pub async fn uninstall_service(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            Command::new("systemctl")
                .args(&["disable", "codex-memory.service"])
                .status()?;

            fs::remove_file("/etc/systemd/system/codex-memory.service")?;

            Command::new("systemctl")
                .args(&["daemon-reload"])
                .status()?;

            info!("Systemd service uninstalled");
        }

        #[cfg(target_os = "macos")]
        {
            let plist_path = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
                .join("Library/LaunchAgents/com.codex.memory.plist");

            Command::new("launchctl")
                .args(["unload", plist_path.to_str().unwrap()])
                .status()?;

            fs::remove_file(plist_path)?;

            info!("Launchd service uninstalled");
        }

        #[cfg(target_os = "windows")]
        {
            Command::new("sc")
                .args(&["delete", "CodexMemory"])
                .status()?;

            info!("Windows service uninstalled");
        }

        Ok(())
    }

    /// Get PID of running server
    fn get_running_pid(&self) -> Result<Option<u32>> {
        if !self.paths.pid_file.exists() {
            return Ok(None);
        }

        let pid_str = fs::read_to_string(&self.paths.pid_file)?;
        let pid: u32 = pid_str.trim().parse().context("Invalid PID in file")?;

        // Check if process is actually running
        if self.is_process_running(pid)? {
            Ok(Some(pid))
        } else {
            // Clean up stale PID file
            fs::remove_file(&self.paths.pid_file).ok();
            Ok(None)
        }
    }

    /// Check if a process is running
    fn is_process_running(&self, pid: u32) -> Result<bool> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            // Send signal 0 to check if process exists
            match signal::kill(Pid::from_raw(pid as i32), Signal::SIGCONT) {
                Ok(_) => Ok(true),
                Err(nix::errno::Errno::ESRCH) => Ok(false),
                Err(e) => Err(anyhow::anyhow!("Failed to check process: {}", e)),
            }
        }

        #[cfg(windows)]
        {
            use std::process::Command;

            let output = Command::new("tasklist")
                .args(&["/FI", &format!("PID eq {}", pid)])
                .output()?;

            Ok(String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        }
    }
}
