use crate::path_trust;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

pub struct AutoStart {
    inner: AutoLaunch,
}

impl AutoStart {
    pub fn new() -> Result<Self, String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("could not locate SeeUsage: {error}"))?;
        path_trust::trusted_autostart_path(&executable)?;
        let executable = executable.to_string_lossy().into_owned();

        #[cfg(target_os = "windows")]
        let launch_path = format!("\"{executable}\"");

        #[cfg(not(target_os = "windows"))]
        let launch_path = executable;

        let mut builder = AutoLaunchBuilder::new();
        builder.set_app_name("SeeUsage").set_app_path(&launch_path);

        #[cfg(target_os = "windows")]
        builder.set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);

        #[cfg(target_os = "macos")]
        builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);

        let inner = builder
            .build()
            .map_err(|error| format!("could not configure launch at login: {error}"))?;
        Ok(Self { inner })
    }

    pub fn is_enabled(&self) -> Result<bool, String> {
        self.inner
            .is_enabled()
            .map_err(|error| format!("could not read launch-at-login setting: {error}"))
    }

    pub fn toggle(&self) -> Result<bool, String> {
        let enable = !self.is_enabled()?;
        if enable {
            self.inner
                .enable()
                .map_err(|error| format!("could not enable launch at login: {error}"))?;
        } else {
            self.inner
                .disable()
                .map_err(|error| format!("could not disable launch at login: {error}"))?;
        }
        Ok(enable)
    }
}
