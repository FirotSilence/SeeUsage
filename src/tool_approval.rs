use crate::{
    i18n::{self, Language, key},
    settings,
};
use std::{
    collections::HashSet,
    path::Path,
    sync::{Mutex, OnceLock},
};

pub fn allow_broadly_writable_tool(
    provider_key: &str,
    provider_name: &str,
    executable: &Path,
    writable_path: &Path,
    interactive: bool,
) -> Result<bool, String> {
    if settings::unsafe_tool_approved(provider_key, executable) {
        return Ok(true);
    }

    let decision = decision_key(provider_key, executable);
    if !interactive
        || declined_paths()
            .lock()
            .is_ok_and(|paths| paths.contains(&decision))
    {
        return Ok(false);
    }

    if show_warning(provider_name, executable, writable_path) {
        settings::approve_unsafe_tool(provider_key, executable)?;
        Ok(true)
    } else {
        lock_declined_paths().insert(decision);
        Ok(false)
    }
}

pub fn reset_session_decisions() {
    lock_declined_paths().clear();
}

fn decision_key(provider: &str, executable: &Path) -> String {
    let path = executable.to_string_lossy();
    if cfg!(target_os = "windows") {
        format!("{provider}:{}", path.to_lowercase())
    } else {
        format!("{provider}:{path}")
    }
}

fn declined_paths() -> &'static Mutex<HashSet<String>> {
    static DECLINED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    DECLINED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn lock_declined_paths() -> std::sync::MutexGuard<'static, HashSet<String>> {
    declined_paths()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn warning_text(
    provider: &str,
    executable: &Path,
    writable_path: &Path,
    language: Language,
) -> String {
    format!(
        "{}\n\n{}\n\n{}\n\n{}\n\n{} {}",
        i18n::template(
            language,
            key::SECURITY_FOUND,
            &[("provider", provider.to_string())],
        ),
        friendly_path(executable),
        i18n::text(language, key::SECURITY_WRITABLE),
        friendly_path(writable_path),
        i18n::template(
            language,
            key::SECURITY_REPLACED,
            &[("provider", provider.to_string())],
        ),
        i18n::text(language, key::SECURITY_QUESTION),
    )
}

fn friendly_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(target_os = "windows") {
        if let Some(path) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{path}");
        }
        if let Some(path) = value.strip_prefix(r"\\?\") {
            return path.to_string();
        }
    }
    value.into_owned()
}

#[cfg(target_os = "windows")]
fn show_warning(provider: &str, executable: &Path, writable_path: &Path) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MessageBoxW,
    };

    let language = settings::language().effective();
    let text = wide(&warning_text(provider, executable, writable_path, language));
    let caption = wide(i18n::text(language, key::SECURITY_TITLE));
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND | MB_TOPMOST,
        ) == IDYES
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(target_os = "macos")]
fn show_warning(provider: &str, executable: &Path, writable_path: &Path) -> bool {
    use std::process::{Command, Stdio};

    const SCRIPT: &str = r#"on run argv
set cancelLabel to item 3 of argv
set allowLabel to item 4 of argv
display alert (item 1 of argv) message (item 2 of argv) as warning buttons {cancelLabel, allowLabel} default button cancelLabel cancel button cancelLabel
end run"#;
    let language = settings::language().effective();
    let message = warning_text(provider, executable, writable_path, language);
    Command::new("/usr/bin/osascript")
        .args(["-e", SCRIPT, "--"])
        .arg(i18n::text(language, key::SECURITY_TITLE))
        .arg(message)
        .arg(i18n::text(language, key::CANCEL))
        .arg(i18n::text(language, key::ALLOW))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn show_warning(_: &str, _: &Path, _: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_names_the_provider_and_risky_path() {
        let text = warning_text(
            "Codex",
            Path::new("/private/bin/codex"),
            Path::new("/private/bin"),
            Language::English,
        );
        assert!(text.contains("Codex credentials"));
        assert!(text.contains("/private/bin"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn warning_hides_windows_extended_path_prefix() {
        assert_eq!(
            friendly_path(Path::new(r"\\?\C:\tools\codex.cmd")),
            r"C:\tools\codex.cmd"
        );
    }
}
