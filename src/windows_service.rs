//! Windows service implementation for SpaceCat
//!
//! This module provides Windows service functionality when compiled on Windows platforms.

#[cfg(windows)]
#[allow(unused_must_use)]
mod implementation {
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_service::service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceDependency,
        ServiceErrorControl, ServiceExitCode, ServiceInfo, ServiceStartType, ServiceState,
        ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    use windows_service::{Result as WindowsServiceResult, define_windows_service};

    use crate::config::Config;
    use crate::service_wrapper::ServiceWrapper;

    // Service configuration constants
    const SERVICE_NAME: &str = "SpaceCat";
    const SERVICE_DISPLAY_NAME: &str = "SpaceCat Chat Updater";
    const SERVICE_DESCRIPTION: &str =
        "SpaceCat astronomical observation system chat updater service for Discord and Matrix";

    pub fn install_service() -> Result<(), Box<dyn std::error::Error>> {
        let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service_binary_path = std::env::current_exe()?;

        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: service_binary_path,
            launch_arguments: vec![OsString::from("windows-service"), OsString::from("run")],
            dependencies: vec![ServiceDependency::Service(OsString::from("Tcpip"))], // Network dependency
            account_name: None, // Run as Local System
            account_password: None,
        };

        let service = service_manager.create_service(&service_info, ServiceAccess::all())?;

        // Set service description
        service.set_description(SERVICE_DESCRIPTION)?;

        println!("Service '{}' installed successfully.", SERVICE_NAME);
        println!("Service will start automatically on system boot.");
        println!("To start the service now, run: spacecat windows-service start");

        Ok(())
    }

    pub fn uninstall_service() -> Result<(), Box<dyn std::error::Error>> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service_access =
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
        let service = service_manager.open_service(SERVICE_NAME, service_access)?;

        // Stop the service if it's running
        let service_status = service.query_status()?;
        if service_status.current_state != ServiceState::Stopped {
            println!("Stopping service...");
            service.stop()?;

            // Wait for the service to stop
            let mut attempts = 0;
            loop {
                let status = service.query_status()?;
                if status.current_state == ServiceState::Stopped {
                    break;
                }
                if attempts > 30 {
                    return Err("Service did not stop within 30 seconds".into());
                }
                std::thread::sleep(Duration::from_secs(1));
                attempts += 1;
            }
        }

        service.delete()?;
        println!("Service '{}' uninstalled successfully.", SERVICE_NAME);

        Ok(())
    }

    pub fn start_service() -> Result<(), Box<dyn std::error::Error>> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::START;
        let service = service_manager.open_service(SERVICE_NAME, service_access)?;

        let service_status = service.query_status()?;
        if service_status.current_state == ServiceState::Running {
            println!("Service '{}' is already running.", SERVICE_NAME);
            return Ok(());
        }

        service.start(&[] as &[&OsString])?;
        println!("Service '{}' started successfully.", SERVICE_NAME);

        Ok(())
    }

    pub fn stop_service() -> Result<(), Box<dyn std::error::Error>> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP;
        let service = service_manager.open_service(SERVICE_NAME, service_access)?;

        let service_status = service.query_status()?;
        if service_status.current_state == ServiceState::Stopped {
            println!("Service '{}' is already stopped.", SERVICE_NAME);
            return Ok(());
        }

        service.stop()?;
        println!("Service '{}' stopped successfully.", SERVICE_NAME);

        Ok(())
    }

    pub fn service_status() -> Result<(), Box<dyn std::error::Error>> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service_access = ServiceAccess::QUERY_STATUS;
        let service = service_manager.open_service(SERVICE_NAME, service_access)?;

        let service_status = service.query_status()?;

        let state_str = match service_status.current_state {
            ServiceState::Stopped => "Stopped",
            ServiceState::StartPending => "Start Pending",
            ServiceState::StopPending => "Stop Pending",
            ServiceState::Running => "Running",
            ServiceState::ContinuePending => "Continue Pending",
            ServiceState::PausePending => "Pause Pending",
            ServiceState::Paused => "Paused",
        };

        println!("Service '{}' status: {}", SERVICE_NAME, state_str);

        Ok(())
    }

    pub fn run_service() -> WindowsServiceResult<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }

    define_windows_service!(ffi_service_main, service_main);

    fn service_main(_arguments: Vec<OsString>) -> WindowsServiceResult<()> {
        // Create a channel to communicate with the system service event loop
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        // Define system service event handler that will be receiving service events.
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    // Handle stop event and return control back to the system.
                    let _ = shutdown_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        // Register system service event handler
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        let next_status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        };

        // Tell the system that the service is running now
        status_handle.set_service_status(next_status)?;

        // Run the actual service logic
        let service_result = run_chat_updater_service(shutdown_rx);

        // Tell the system that service has stopped.
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: match service_result {
                Ok(_) => ServiceExitCode::Win32(0),
                Err(_) => ServiceExitCode::Win32(1),
            },
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        service_result
            .map_err(|_e| windows_service::Error::Winapi(std::io::Error::from_raw_os_error(1)))
    }

    fn run_chat_updater_service(
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Load configuration from Windows service location
        let config_path = get_service_config_path()?;
        let config = if config_path.exists() {
            Config::load_from_file(&config_path)?
        } else {
            // Create default config and save it
            let default_config = Config::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            default_config.save_to_file(&config_path)?;
            println!(
                "Created default configuration at: {}",
                config_path.display()
            );
            default_config
        };

        // Create service wrapper
        let service_wrapper = ServiceWrapper::new(config)
            .map_err(|e| format!("Failed to create service wrapper: {}", e))?;

        // Run the chat updater with graceful shutdown support
        service_wrapper.run_with_shutdown(shutdown_rx)
    }

    fn get_service_config_path()
    -> Result<std::path::PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        use std::path::PathBuf;

        // Use %ProgramData%\SpaceCat\config.json for service installations
        let program_data =
            std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());

        let mut config_path = PathBuf::from(program_data);
        config_path.push("SpaceCat");
        config_path.push("config.json");

        Ok(config_path)
    }
}

// Re-export the implementation functions on Windows
#[cfg(windows)]
pub use implementation::*;

// Provide stub functions for non-Windows platforms
#[cfg(not(windows))]
mod stubs {
    pub fn install_service() -> Result<(), Box<dyn std::error::Error>> {
        Err("Windows service support is not available on this platform".into())
    }

    pub fn uninstall_service() -> Result<(), Box<dyn std::error::Error>> {
        Err("Windows service support is not available on this platform".into())
    }

    pub fn start_service() -> Result<(), Box<dyn std::error::Error>> {
        Err("Windows service support is not available on this platform".into())
    }

    pub fn stop_service() -> Result<(), Box<dyn std::error::Error>> {
        Err("Windows service support is not available on this platform".into())
    }

    pub fn service_status() -> Result<(), Box<dyn std::error::Error>> {
        Err("Windows service support is not available on this platform".into())
    }

    pub fn run_service() -> Result<(), Box<dyn std::error::Error>> {
        Err("Windows service support is not available on this platform".into())
    }
}

#[cfg(not(windows))]
pub use stubs::*;
