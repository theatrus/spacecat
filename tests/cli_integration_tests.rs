//! CLI integration tests

#[cfg(test)]
mod tests {
    use std::process::Command;

    fn chatstronomy() -> Command {
        // Cargo already builds the package binary for integration tests and
        // exposes its exact path here. Spawning nested `cargo run` processes
        // made these tiny help checks contend for Cargo's target lock and take
        // more than a minute on Windows release runners.
        Command::new(env!("CARGO_BIN_EXE_chatstronomy"))
    }

    #[test]
    fn test_help_command() {
        let output = chatstronomy()
            .arg("--help")
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Chatstronomy"));
        assert!(stdout.contains("chat-updater"));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_service_help() {
        let output = chatstronomy()
            .args(["windows-service", "--help"])
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
        let output = chatstronomy()
            .arg("--help")
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
        let output = chatstronomy()
            .arg("--help")
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Basic commands should always be available
        assert!(stdout.contains("sequence"));
        assert!(stdout.contains("events"));
        assert!(stdout.contains("images"));
        assert!(stdout.contains("chat-updater"));
        assert!(stdout.contains("mount-info"));
    }
}
