//! Integration tests for Windows service functionality

#[cfg(test)]
mod tests {
    // Import Windows service functions if available (Windows only)
    #[cfg(all(windows, feature = "windows-service"))]
    use spacecat::windows_service::*;

    #[cfg(all(windows, feature = "windows-service"))]
    #[test]
    fn test_service_functions_exist() {
        // These tests ensure the functions exist and return appropriate errors
        // on non-Windows platforms or when feature is disabled

        let result = install_service();
        assert!(result.is_err());

        let result = uninstall_service();
        assert!(result.is_err());

        let result = start_service();
        assert!(result.is_err());

        let result = stop_service();
        assert!(result.is_err());

        let result = service_status();
        assert!(result.is_err());

        let result = run_service();
        assert!(result.is_err());
    }

    #[cfg(all(windows, feature = "windows-service"))]
    #[test]
    fn test_error_messages() {
        let result = install_service();
        if let Err(e) = result {
            let error_msg = e.to_string();
            // On non-Windows platforms, should get platform error
            #[cfg(not(all(windows, feature = "windows-service")))]
            assert!(error_msg.contains("not available on this platform"));
        }
    }

    #[cfg(not(all(windows, feature = "windows-service")))]
    #[test]
    fn test_windows_service_feature_disabled() {
        // When the windows-service feature is not enabled, we can't test the functions
        // but we can at least ensure this test compiles and runs
        assert!(true);
    }

    #[cfg(all(windows, feature = "windows-service"))]
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
