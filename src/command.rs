use crate::{path_trust, tool_approval};
use std::{
    env,
    ffi::{OsStr, OsString},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
enum CommandTarget {
    Direct(OsString),
    #[cfg(target_os = "windows")]
    CommandScript(PathBuf),
}

#[derive(Clone, Debug)]
pub struct ToolCommand {
    target: CommandTarget,
    environment: ProviderEnvironment,
    working_directory: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderEnvironment {
    Codex,
    MiniMax,
}

pub struct CommandOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub struct ManagedChild {
    pub child: std::process::Child,
    #[cfg(target_os = "windows")]
    job: Option<ProcessJob>,
    #[cfg(unix)]
    process_group: i32,
}

impl ManagedChild {
    pub fn spawn(command: &mut Command) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command
            .spawn()
            .map_err(|error| format!("could not start command: {error}"))?;

        #[cfg(target_os = "windows")]
        let mut child = child;
        #[cfg(target_os = "windows")]
        let job = match ProcessJob::attach(&child) {
            Ok(job) => Some(job),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        #[cfg(unix)]
        let process_group = child.id() as i32;

        Ok(Self {
            child,
            #[cfg(target_os = "windows")]
            job,
            #[cfg(unix)]
            process_group,
        })
    }

    pub fn terminate(&mut self) {
        self.terminate_descendants();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn terminate_descendants(&mut self) {
        #[cfg(target_os = "windows")]
        drop(self.job.take());

        #[cfg(unix)]
        unsafe {
            libc::kill(-self.process_group, libc::SIGKILL);
        }
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(target_os = "windows")]
struct ProcessJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(target_os = "windows")]
impl ProcessJob {
    fn attach(child: &std::process::Child) -> Result<Self, String> {
        use std::{mem::size_of, os::windows::io::AsRawHandle, ptr::null};
        use windows_sys::Win32::{
            Foundation::GetLastError,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };

        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(format!(
                "could not create a process job (Windows error {})",
                unsafe { GetLastError() }
            ));
        }
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(format!(
                "could not secure the process job (Windows error {})",
                unsafe { GetLastError() }
            ));
        }
        if unsafe { AssignProcessToJobObject(handle, child.as_raw_handle() as _) } == 0 {
            return Err(format!(
                "could not contain the child process (Windows error {})",
                unsafe { GetLastError() }
            ));
        }
        Ok(job)
    }
}

#[cfg(target_os = "windows")]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

impl ToolCommand {
    pub fn resolve(
        environment_key: &str,
        fallback: &str,
        environment: ProviderEnvironment,
        interactive_approval: bool,
    ) -> Result<Self, String> {
        let requested = env::var_os(environment_key)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| OsString::from(fallback));

        #[cfg(target_os = "windows")]
        {
            let path = first_approved_tool(
                resolve_windows(&requested),
                fallback,
                environment,
                interactive_approval,
            )?;
            let execution_path = windows_execution_path(&path);
            let extension = execution_path
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let target = match extension.as_str() {
                "cmd" | "bat" => CommandTarget::CommandScript(execution_path.clone()),
                "ps1" => return Err("PowerShell tool wrappers are not supported".into()),
                _ => CommandTarget::Direct(execution_path.clone().into_os_string()),
            };
            let working_directory = tool_directory(&execution_path)?;
            Ok(Self {
                target,
                environment,
                working_directory,
            })
        }

        #[cfg(not(target_os = "windows"))]
        {
            let program = first_approved_tool(
                resolve_unix(&requested),
                fallback,
                environment,
                interactive_approval,
            )?;
            let working_directory = tool_directory(&program)?;
            Ok(Self {
                target: CommandTarget::Direct(program.into_os_string()),
                environment,
                working_directory,
            })
        }
    }

    pub fn command<I, S>(&self, args: I) -> Result<Command, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|argument| argument.as_ref().to_os_string())
            .collect();
        let mut command = match &self.target {
            CommandTarget::Direct(program) => {
                let mut command = Command::new(program);
                command.args(&args);
                command
            }
            #[cfg(target_os = "windows")]
            CommandTarget::CommandScript(path) => {
                let mut command = Command::new(windows_system_executable("cmd.exe"));
                command.args(["/D", "/S", "/C"]).arg(path).args(&args);
                command
            }
        };
        isolate_environment(&mut command, self.environment, &self.working_directory);
        command.current_dir(&self.working_directory);
        hide_window(&mut command);
        Ok(command)
    }

    pub fn run<I, S>(&self, args: I, timeout: Duration) -> Result<CommandOutput, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(args)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = ManagedChild::spawn(&mut command)?;
        let mut stdout = child
            .child
            .stdout
            .take()
            .ok_or_else(|| "could not capture command output".to_string())?;
        let mut stderr = child
            .child
            .stderr
            .take()
            .ok_or_else(|| "could not capture command errors".to_string())?;
        let stdout_reader = thread::spawn(move || read_capped(&mut stdout));
        let stderr_reader = thread::spawn(move || read_capped(&mut stderr));

        let started = Instant::now();
        let status = loop {
            if let Some(status) = child
                .child
                .try_wait()
                .map_err(|error| format!("could not wait for command: {error}"))?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                child.terminate();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("command timed out after {}s", timeout.as_secs()));
            }
            thread::sleep(Duration::from_millis(40));
        };

        child.terminate_descendants();
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();
        Ok(CommandOutput {
            success: status.success(),
            code: status.code(),
            stdout,
            stderr,
        })
    }
}

#[cfg(target_os = "windows")]
fn windows_execution_path(path: &std::path::Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    const EXTENDED: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const UNC: &[u16] = &[b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16];
    if !wide.starts_with(EXTENDED) {
        return path.to_path_buf();
    }
    let mut normalized = Vec::new();
    if wide[EXTENDED.len()..].starts_with(UNC) {
        normalized.extend([b'\\' as u16, b'\\' as u16]);
        normalized.extend_from_slice(&wide[EXTENDED.len() + UNC.len()..]);
    } else {
        normalized.extend_from_slice(&wide[EXTENDED.len()..]);
    }
    PathBuf::from(OsString::from_wide(&normalized))
}

fn tool_directory(path: &std::path::Path) -> Result<PathBuf, String> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("tool has no containing directory: {}", path.display()))
}

fn first_approved_tool(
    candidates: Vec<PathBuf>,
    fallback: &str,
    environment: ProviderEnvironment,
    interactive_approval: bool,
) -> Result<PathBuf, String> {
    let mut first_error = None;
    let mut first_unsafe = None;
    for candidate in candidates {
        match path_trust::inspect_executable(&candidate) {
            Ok(path_trust::ExecutableTrust::Trusted(path)) => return Ok(path),
            Ok(path_trust::ExecutableTrust::BroadlyWritable {
                executable,
                writable_path,
            }) if first_unsafe.is_none() => first_unsafe = Some((executable, writable_path)),
            Ok(path_trust::ExecutableTrust::BroadlyWritable { .. }) => {}
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }

    if let Some((executable, writable_path)) = first_unsafe {
        if tool_approval::allow_broadly_writable_tool(
            environment.settings_key(),
            environment.display_name(),
            &executable,
            &writable_path,
            interactive_approval,
        )? {
            return Ok(executable);
        }
        return Err(format!(
            "{} CLI path requires approval: {}",
            environment.display_name(),
            writable_path.display()
        ));
    }
    Err(first_error.unwrap_or_else(|| format!("could not find {fallback} on PATH")))
}

impl ProviderEnvironment {
    const fn settings_key(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::MiniMax => "minimax",
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::MiniMax => "MiniMax",
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn resolve_unix(requested: &OsStr) -> Vec<PathBuf> {
    let path = PathBuf::from(requested);
    if path.components().count() > 1 {
        return path.is_file().then_some(path).into_iter().collect();
    }

    env::split_paths(&augmented_path())
        .map(|directory| directory.join(requested))
        .filter(|candidate| candidate.is_file())
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn augmented_path() -> OsString {
    let current = env::var_os("PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = env::split_paths(&current).collect();
    for path in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        let path = PathBuf::from(path);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    if let Some(home) = env::var_os("HOME") {
        for suffix in [".local/bin", ".cargo/bin"] {
            let path = PathBuf::from(&home).join(suffix);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    env::join_paths(paths).unwrap_or_default()
}

fn isolate_environment(
    command: &mut Command,
    provider: ProviderEnvironment,
    tool_directory: &std::path::Path,
) {
    let allowed: Vec<_> = env::vars_os()
        .filter(|(name, _)| environment_variable_allowed(name, provider))
        .collect();
    command.env_clear();
    command.envs(allowed);
    command.env("PATH", trusted_search_path(tool_directory));

    #[cfg(target_os = "windows")]
    command.env("COMSPEC", windows_system_executable("cmd.exe"));
}

fn environment_variable_allowed(name: &OsStr, provider: ProviderEnvironment) -> bool {
    let name = name.to_string_lossy().to_ascii_uppercase();
    let common = matches!(
        name.as_str(),
        "ALL_PROXY"
            | "APPDATA"
            | "HOMEDRIVE"
            | "HOMEPATH"
            | "HOME"
            | "HTTP_PROXY"
            | "HTTPS_PROXY"
            | "LANG"
            | "LANGUAGE"
            | "LC_ALL"
            | "LOCALAPPDATA"
            | "LOGNAME"
            | "NODE_EXTRA_CA_CERTS"
            | "NO_COLOR"
            | "NO_PROXY"
            | "PATHEXT"
            | "PROGRAMDATA"
            | "PROGRAMFILES"
            | "PROGRAMFILES(X86)"
            | "PROGRAMW6432"
            | "SSL_CERT_DIR"
            | "SSL_CERT_FILE"
            | "SYSTEMDRIVE"
            | "SYSTEMROOT"
            | "TEMP"
            | "TERM"
            | "TMP"
            | "TMPDIR"
            | "USER"
            | "USERPROFILE"
            | "WINDIR"
    ) || name.starts_with("LC_")
        || name.starts_with("XDG_");
    common
        || match provider {
            ProviderEnvironment::Codex => {
                name.starts_with("CODEX_")
                    || name.starts_with("OPENAI_")
                    || name.starts_with("AZURE_OPENAI_")
            }
            ProviderEnvironment::MiniMax => {
                name.starts_with("MINIMAX_") || name.starts_with("MMX_")
            }
        }
}

fn trusted_search_path(tool_directory: &std::path::Path) -> OsString {
    #[cfg(target_os = "windows")]
    let configured = env::var_os("PATH").unwrap_or_default();
    #[cfg(not(target_os = "windows"))]
    let configured = augmented_path();

    let mut candidates = Vec::new();
    #[cfg(target_os = "windows")]
    if let Some(system) = windows_system_executable("cmd.exe").parent() {
        candidates.push(system.to_path_buf());
    }
    candidates.extend(env::split_paths(&configured));

    let mut trusted = vec![tool_directory.to_path_buf()];
    for candidate in candidates {
        if let Ok(candidate) = path_trust::trusted_directory(&candidate)
            && !trusted.contains(&candidate)
        {
            trusted.push(candidate);
        }
    }
    env::join_paths(trusted).unwrap_or_default()
}

pub fn safe_command_failure(provider: &str, code: Option<i32>, details: &[u8]) -> String {
    if authentication_error(details) {
        format!("{provider} authentication required")
    } else {
        let code = code.map_or_else(|| "unknown".into(), |code| code.to_string());
        format!("{provider} command failed (code {code})")
    }
}

pub fn safe_provider_failure(provider: &str, operation: &str, details: &str) -> String {
    if authentication_error(details.as_bytes()) {
        format!("{provider} authentication required")
    } else {
        format!("{operation} failed")
    }
}

fn authentication_error(details: &[u8]) -> bool {
    let details = String::from_utf8_lossy(details).to_ascii_lowercase();
    [
        "authentication required",
        "invalid api key",
        "invalid_api_key",
        "login required",
        "missing api key",
        "not authenticated",
        "not logged in",
        "status code 401",
        "status_code\":401",
        "unauthenticated",
        "unauthorized",
    ]
    .iter()
    .any(|marker| details.contains(marker))
}

pub fn contains_sensitive_marker(details: &str) -> bool {
    let details = details.to_ascii_lowercase();
    [
        "api-key",
        "api_key",
        "apikey",
        "authorization:",
        "bearer ",
        "password",
        "refresh_token",
        "secret",
        "sk-",
        "token=",
    ]
    .iter()
    .any(|marker| details.contains(marker))
}

pub(crate) fn read_capped(reader: &mut impl Read) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    captured
}

#[cfg(target_os = "windows")]
fn resolve_windows(requested: &OsStr) -> Vec<PathBuf> {
    let explicit = PathBuf::from(requested);
    if explicit.is_absolute() || explicit.components().count() > 1 {
        return explicit.is_file().then_some(explicit).into_iter().collect();
    }

    let mut command = Command::new(windows_system_executable("where.exe"));
    command
        .arg(requested)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let paths: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();
    let mut candidates = Vec::new();
    for extensions in [["exe"].as_slice(), ["cmd", "bat"].as_slice()] {
        candidates.extend(paths.iter().filter(|path| {
            !is_packaged_resource(path)
                && path.extension().is_some_and(|extension| {
                    extensions
                        .iter()
                        .any(|expected| extension.eq_ignore_ascii_case(expected))
                })
        }));
    }
    candidates.into_iter().cloned().collect()
}

#[cfg(target_os = "windows")]
fn is_packaged_resource(path: &std::path::Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("WindowsApps")
    })
}

#[cfg(target_os = "windows")]
fn windows_system_executable(name: &str) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = vec![0u16; 260];
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return PathBuf::from(r"C:\Windows\System32").join(name);
    }
    buffer.truncate(length);
    PathBuf::from(OsString::from_wide(&buffer)).join(name)
}

pub fn hide_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_errors_do_not_echo_details() {
        assert_eq!(
            safe_command_failure("MiniMax", Some(7), b"failed with sk-sensitive"),
            "MiniMax command failed (code 7)"
        );
        assert_eq!(
            safe_command_failure("MiniMax", Some(1), b"unauthorized"),
            "MiniMax authentication required"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolves_known_windows_command() {
        let command = ToolCommand::resolve(
            "SEEUSAGE_TEST_MISSING",
            "cmd",
            ProviderEnvironment::Codex,
            false,
        );
        assert!(command.is_ok());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn command_scripts_use_non_extended_windows_paths() {
        assert_eq!(
            windows_execution_path(std::path::Path::new(r"\\?\C:\tools\codex.cmd")),
            PathBuf::from(r"C:\tools\codex.cmd")
        );
        assert_eq!(
            windows_execution_path(std::path::Path::new(r"\\?\UNC\server\tools\codex.cmd")),
            PathBuf::from(r"\\server\tools\codex.cmd")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn runs_child_inside_a_managed_job() {
        let command = ToolCommand::resolve(
            "SEEUSAGE_TEST_MISSING",
            "cmd",
            ProviderEnvironment::Codex,
            false,
        )
        .unwrap();
        let output = command
            .run(["/D", "/C", "echo", "managed"], Duration::from_secs(5))
            .unwrap();
        assert!(output.success);
        assert!(String::from_utf8_lossy(&output.stdout).contains("managed"));
    }

    #[test]
    fn provider_environment_does_not_cross_key_boundaries() {
        assert!(environment_variable_allowed(
            OsStr::new("OPENAI_API_KEY"),
            ProviderEnvironment::Codex
        ));
        assert!(!environment_variable_allowed(
            OsStr::new("MINIMAX_API_KEY"),
            ProviderEnvironment::Codex
        ));
        assert!(environment_variable_allowed(
            OsStr::new("MINIMAX_API_KEY"),
            ProviderEnvironment::MiniMax
        ));
        assert!(!environment_variable_allowed(
            OsStr::new("OPENAI_API_KEY"),
            ProviderEnvironment::MiniMax
        ));
        assert!(!environment_variable_allowed(
            OsStr::new("NODE_OPTIONS"),
            ProviderEnvironment::MiniMax
        ));
        assert!(!environment_variable_allowed(
            OsStr::new("PATH"),
            ProviderEnvironment::Codex
        ));
    }

    #[test]
    fn captured_output_is_bounded() {
        let source = vec![b'x'; MAX_CAPTURE_BYTES + 512];
        assert_eq!(read_capped(&mut source.as_slice()).len(), MAX_CAPTURE_BYTES);
    }
}
