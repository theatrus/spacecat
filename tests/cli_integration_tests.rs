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
    }

    #[test]
    fn test_basic_commands_available() {
        let output = chatstronomy()
            .arg("--help")
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        #[cfg(windows)]
        assert!(stdout.contains("plugin-runtime"));
        #[cfg(feature = "hub")]
        assert!(stdout.contains("hub"));
    }
}
