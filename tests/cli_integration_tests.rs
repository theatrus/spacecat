//! CLI integration tests

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn test_help_command() {
        let output = Command::new("cargo")
            .args(&["run", "--", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("SpaceCat"));
        assert!(stdout.contains("discord-updater"));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_service_help() {
        let output = Command::new("cargo")
            .args(&["run", "--features", "windows-service", "--", "windows-service", "--help"])
            .output()
            .expect("Failed to execute command");

        // Should succeed and show Windows service commands
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("install"));
        assert!(stdout.contains("uninstall"));
        assert!(stdout.contains("start"));
        assert!(stdout.contains("stop"));
        assert!(stdout.contains("status"));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_windows_service_unavailable() {
        let output = Command::new("cargo")
            .args(&["run", "--features", "windows-service", "--", "--help"])
            .output()
            .expect("Failed to execute command");

        // Should succeed but Windows service commands should not be available
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // On non-Windows platforms, windows-service command should not appear
        assert!(!stdout.contains("windows-service"));
    }

    #[test] 
    fn test_basic_commands_available() {
        let output = Command::new("cargo")
            .args(&["run", "--", "--help"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Basic commands should always be available
        assert!(stdout.contains("sequence"));
        assert!(stdout.contains("events"));
        assert!(stdout.contains("images"));
        assert!(stdout.contains("discord-updater"));
        assert!(stdout.contains("mount-info"));
    }
}