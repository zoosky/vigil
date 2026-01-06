use std::path::PathBuf;
use std::process::Command;

const PLIST_LABEL: &str = "com.kapptec.networkmonitor";

/// Get the path to the launchd plist file
fn plist_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join("Library/LaunchAgents").join(format!("{}.plist", PLIST_LABEL)))
}

/// Get the path to the installed binary
fn binary_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Use the current executable path, or fall back to expected install location
    std::env::current_exe().or_else(|_| Ok(PathBuf::from("/usr/local/bin/networkmonitor")))
}

/// Generate the launchd plist content
fn generate_plist() -> Result<String, Box<dyn std::error::Error>> {
    let binary = binary_path()?;
    let binary_str = binary.to_str().ok_or("Invalid binary path")?;

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>start</string>
        <string>--foreground</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/tmp/networkmonitor.out.log</string>

    <key>StandardErrorPath</key>
    <string>/tmp/networkmonitor.err.log</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>
"#,
        PLIST_LABEL, binary_str
    ))
}

/// Install the launchd service
pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    let plist = plist_path()?;

    // Check if already installed
    if plist.exists() {
        println!("Service is already installed at:");
        println!("  {}", plist.display());
        println!("\nTo reinstall, first run: networkmonitor service uninstall");
        return Ok(());
    }

    // Ensure LaunchAgents directory exists
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Generate and write plist
    let content = generate_plist()?;
    std::fs::write(&plist, &content)?;

    println!("Created launchd plist at:");
    println!("  {}\n", plist.display());

    // Load the service
    let status = Command::new("launchctl")
        .args(["load", plist.to_str().unwrap()])
        .status()?;

    if status.success() {
        println!("Service installed and started successfully.");
        println!("\nThe monitor will now:");
        println!("  - Start automatically on login");
        println!("  - Restart if it crashes");
        println!("  - Log output to /tmp/networkmonitor.*.log");
        println!("\nTo check status: networkmonitor service status");
        println!("To view logs:    networkmonitor service logs");
    } else {
        println!("Warning: launchctl load failed. Try manually:");
        println!("  launchctl load {}", plist.display());
    }

    Ok(())
}

/// Uninstall the launchd service
pub fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    let plist = plist_path()?;

    if !plist.exists() {
        println!("Service is not installed.");
        return Ok(());
    }

    // Unload the service first
    let status = Command::new("launchctl")
        .args(["unload", plist.to_str().unwrap()])
        .status()?;

    if !status.success() {
        println!("Warning: launchctl unload may have failed (service might not be running)");
    }

    // Remove the plist file
    std::fs::remove_file(&plist)?;

    println!("Service uninstalled successfully.");
    println!("  Removed: {}", plist.display());

    Ok(())
}

/// Check the service status
pub fn status() -> Result<(), Box<dyn std::error::Error>> {
    let plist = plist_path()?;

    println!("Service Status");
    println!("══════════════════════════════════════════════════════════\n");

    // Check if plist exists
    if !plist.exists() {
        println!("Installed: No");
        println!("\nTo install: networkmonitor service install");
        return Ok(());
    }

    println!("Installed: Yes");
    println!("  Plist: {}", plist.display());

    // Check if service is running via launchctl
    let output = Command::new("launchctl")
        .args(["list"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let is_running = stdout.lines().any(|line| line.contains(PLIST_LABEL));

    if is_running {
        println!("\nStatus: Running");

        // Try to get PID
        for line in stdout.lines() {
            if line.contains(PLIST_LABEL) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(pid) = parts.first() {
                    if *pid != "-" {
                        println!("  PID: {}", pid);
                    }
                }
                break;
            }
        }
    } else {
        println!("\nStatus: Not running");
        println!("\nTo start: launchctl load {}", plist.display());
    }

    // Check log files
    println!("\nLog files:");
    let stdout_log = PathBuf::from("/tmp/networkmonitor.out.log");
    let stderr_log = PathBuf::from("/tmp/networkmonitor.err.log");

    if stdout_log.exists() {
        let meta = std::fs::metadata(&stdout_log)?;
        println!("  stdout: {} ({} bytes)", stdout_log.display(), meta.len());
    } else {
        println!("  stdout: (not created yet)");
    }

    if stderr_log.exists() {
        let meta = std::fs::metadata(&stderr_log)?;
        println!("  stderr: {} ({} bytes)", stderr_log.display(), meta.len());
    } else {
        println!("  stderr: (not created yet)");
    }

    Ok(())
}

/// View service logs
pub fn logs(lines: usize, follow: bool) -> Result<(), Box<dyn std::error::Error>> {
    let stdout_log = PathBuf::from("/tmp/networkmonitor.out.log");
    let stderr_log = PathBuf::from("/tmp/networkmonitor.err.log");

    if !stdout_log.exists() && !stderr_log.exists() {
        println!("No log files found. Is the service running?");
        println!("\nExpected log locations:");
        println!("  {}", stdout_log.display());
        println!("  {}", stderr_log.display());
        return Ok(());
    }

    if follow {
        // Use tail -f to follow logs
        println!("Following logs (Ctrl+C to stop)...\n");

        let mut cmd = Command::new("tail");
        cmd.arg("-f");

        if stdout_log.exists() {
            cmd.arg(&stdout_log);
        }
        if stderr_log.exists() {
            cmd.arg(&stderr_log);
        }

        cmd.status()?;
    } else {
        // Show last N lines
        if stdout_log.exists() {
            println!("=== stdout log (last {} lines) ===", lines);
            let output = Command::new("tail")
                .args(["-n", &lines.to_string(), stdout_log.to_str().unwrap()])
                .output()?;
            print!("{}", String::from_utf8_lossy(&output.stdout));
            println!();
        }

        if stderr_log.exists() {
            println!("=== stderr log (last {} lines) ===", lines);
            let output = Command::new("tail")
                .args(["-n", &lines.to_string(), stderr_log.to_str().unwrap()])
                .output()?;
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plist_path() {
        let path = plist_path().unwrap();
        assert!(path.to_str().unwrap().contains("LaunchAgents"));
        assert!(path.to_str().unwrap().ends_with(".plist"));
    }

    #[test]
    fn test_generate_plist() {
        let plist = generate_plist().unwrap();
        assert!(plist.contains("com.kapptec.networkmonitor"));
        assert!(plist.contains("RunAtLoad"));
        assert!(plist.contains("KeepAlive"));
        assert!(plist.contains("--foreground"));
    }
}
