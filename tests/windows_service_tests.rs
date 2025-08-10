//! Integration tests for Windows service functionality

#[cfg(test)]
mod tests {
    // Import Windows service functions if available (Windows only)
    #[cfg(windows)]
    use spacecat::windows_service::*;

    #[cfg(windows)]
    #[test]
    fn test_service_functions_exist() {
        // These tests ensure the functions exist and return appropriate errors
        // on non-Windows platforms

        let result = install_service();
        eprintln!("Result {:?}", result);
        assert!(result.is_err());

        let result = uninstall_service();
        eprintln!("Result {:?}", result);
        assert!(result.is_err());

        let result = start_service();
        eprintln!("Result {:?}", result);
        assert!(result.is_err());

        let result = stop_service();
        eprintln!("Result {:?}", result);
        assert!(result.is_err());

        let result = service_status();
        eprintln!("Result {:?}", result);
        assert!(result.is_err());

        let result = run_service();
        eprintln!("Result {:?}", result);
        assert!(result.is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn test_windows_service_unavailable() {
        // On non-Windows platforms, we can't test the actual functions
        // but we can at least ensure this test compiles and runs
        // No assertions needed - successful compilation is the test
    }

    #[cfg(windows)]
    mod windows_only_tests {
        use super::*;

        #[test]
        fn test_service_functions_compile() {
            // These tests just ensure Windows service functions compile properly
            // We won't actually install/run services in CI, just test compilation

            // Test that functions exist and have correct signatures
            let _: fn() -> Result<(), Box<dyn std::error::Error>> = install_service;
            let _: fn() -> Result<(), Box<dyn std::error::Error>> = uninstall_service;
            let _: fn() -> Result<(), Box<dyn std::error::Error>> = start_service;
            let _: fn() -> Result<(), Box<dyn std::error::Error>> = stop_service;
            let _: fn() -> Result<(), Box<dyn std::error::Error>> = service_status;
        }
    }
}
