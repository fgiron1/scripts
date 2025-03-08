use std::error::Error;
use std::process::Command;
use std::path::{Path, PathBuf};

pub struct DriverSetup {
    ds4windows_path: Option<PathBuf>,
    ds4windows_running: bool,
    vigembus_installed: bool,
}

impl DriverSetup {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
        Self {
            ds4windows_path: Self::find_ds4windows(),
            ds4windows_running: Self::is_ds4windows_running(),
            vigembus_installed: Self::check_vigembus(),
        }
        }

        #[cfg(target_os = "linux")]
        {
            Self {
                ds4windows_path: None,
                ds4windows_running: false,
                vigembus_installed: false,
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn find_ds4windows() -> Option<PathBuf> {
        let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
        let paths = vec![
            PathBuf::from(r"C:\Program Files\DS4Windows"),
            PathBuf::from(r"C:\Program Files (x86)\DS4Windows"),
            PathBuf::from(&user_profile).join(r"AppData\Local\DS4Windows"),
            PathBuf::from(&user_profile).join(r"AppData\Roaming\DS4Windows"),
            PathBuf::from(&user_profile).join("Downloads").join("DS4Windows"),
        ];

        paths.into_iter()
            .find(|path| path.exists() && path.join("DS4Windows.exe").exists())
    }

    #[cfg(target_os = "windows")]
    fn is_ds4windows_running() -> bool {
        let output = Command::new("tasklist")
            .output()
            .ok();
            
        if let Some(output) = output {
            let processes = String::from_utf8_lossy(&output.stdout);
            processes.contains("DS4Windows.exe")
        } else {
            false
        }
    }

    #[cfg(target_os = "windows")]
    fn check_vigembus() -> bool {
        let service_check = Command::new("sc")
            .args(["query", "ViGEmBus"])
            .output()
            .map(|output| {
                let status = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout);
                status && stdout.contains("RUNNING")
            })
            .unwrap_or(false);

        let driver_path = Path::new(r"C:\Windows\System32\drivers\ViGEmBus.sys");
        
        service_check || driver_path.exists()
    }

    pub fn check_requirements(&self) -> Result<(), Box<dyn Error>> {
        #[cfg(target_os = "windows")]
        {
        println!("\n=== System Requirements Check ===");
        
        // Check DS4Windows installation
        match &self.ds4windows_path {
            Some(path) => println!("✓ DS4Windows found at: {}", path.display()),
            None => println!("ℹ️ DS4Windows not found in common locations")
        }

        // Check if DS4Windows is running
        if self.ds4windows_running {
            println!("✓ DS4Windows is currently running");
        } else {
            println!("⚠️ DS4Windows does not appear to be running");
            println!("   Please make sure DS4Windows is running for best controller support");
        }

        // Check ViGEmBus
        if self.vigembus_installed {
            println!("✓ ViGEmBus driver installed");
        } else {
            println!("⚠️ ViGEmBus driver not detected (required for DS4Windows)");
        }

        // Show summary and continue regardless
        println!("\nDriver Status: {}", 
            if self.ds4windows_running && self.vigembus_installed {
                "DS4Windows (optimal)"
            } else if self.vigembus_installed {
                "ViGEmBus only (DS4Windows recommended)"
            } else {
                "Basic XInput (limited functionality)"
            }
        );
        
        println!("\nContinuing with available driver...");
        }

        #[cfg(target_os = "linux")]
        {
            println!("✓ Linux native input detected");
        }

        Ok(())
    }
}