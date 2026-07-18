use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub enum ExecutableTrust {
    Trusted(PathBuf),
    BroadlyWritable {
        executable: PathBuf,
        writable_path: PathBuf,
    },
}

pub fn inspect_executable(path: &Path) -> Result<ExecutableTrust, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    if !canonical.is_file() {
        return Err(format!("tool path is not a file: {}", canonical.display()));
    }
    match broadly_writable_component(&canonical)? {
        Some(writable_path) => Ok(ExecutableTrust::BroadlyWritable {
            executable: canonical,
            writable_path,
        }),
        None => Ok(ExecutableTrust::Trusted(canonical)),
    }
}

pub fn trusted_directory(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "search path is not a directory: {}",
            canonical.display()
        ));
    }
    if let Some(writable_path) = broadly_writable_component(&canonical)? {
        return Err(unsafe_path_error(&writable_path));
    }
    Ok(canonical)
}

pub fn trusted_autostart_path(path: &Path) -> Result<(), String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    if let Some(writable_path) = broadly_writable_component(&canonical)? {
        return Err(unsafe_path_error(&writable_path));
    }
    Ok(())
}

fn broadly_writable_component(path: &Path) -> Result<Option<PathBuf>, String> {
    for candidate in path.ancestors().take(2) {
        if is_broadly_writable(candidate)? {
            return Ok(Some(candidate.to_path_buf()));
        }
    }
    Ok(None)
}

fn unsafe_path_error(path: &Path) -> String {
    format!("refusing broadly writable path: {}", path.display())
}

#[cfg(unix)]
fn is_broadly_writable(path: &Path) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    Ok(metadata.mode() & 0o022 != 0)
}

#[cfg(target_os = "windows")]
fn is_broadly_writable(path: &Path) -> Result<bool, String> {
    use std::{
        mem::size_of,
        os::windows::ffi::OsStrExt,
        ptr::{addr_of, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_SUCCESS, GENERIC_ALL, GENERIC_WRITE},
        Security::{
            ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, IsWellKnownSid,
            PSECURITY_DESCRIPTOR, WinAuthenticatedUserSid, WinBuiltinGuestsSid, WinBuiltinUsersSid,
            WinWorldSid,
        },
        Storage::FileSystem::{
            DELETE, FILE_APPEND_DATA, FILE_DELETE_CHILD, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
            FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
        },
        System::SystemServices::ACCESS_ALLOWED_ACE_TYPE,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut dacl: *mut ACL = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!(
            "could not inspect permissions for {} (Windows error {status})",
            path.display()
        ));
    }
    let _descriptor = LocalSecurityDescriptor(descriptor);
    if dacl.is_null() {
        return Ok(true);
    }

    let mut information = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut information as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(format!("could not read permissions for {}", path.display()));
    }

    const WRITE_MASK: u32 = FILE_WRITE_DATA
        | FILE_APPEND_DATA
        | FILE_WRITE_EA
        | FILE_WRITE_ATTRIBUTES
        | FILE_DELETE_CHILD
        | DELETE
        | WRITE_DAC
        | WRITE_OWNER
        | GENERIC_WRITE
        | GENERIC_ALL;
    for index in 0..information.AceCount {
        let mut entry = null_mut();
        if unsafe { GetAce(dacl, index, &mut entry) } == 0 || entry.is_null() {
            return Err(format!("could not read permissions for {}", path.display()));
        }
        let ace = entry.cast::<ACCESS_ALLOWED_ACE>();
        if unsafe { (*ace).Header.AceType } as u32 != ACCESS_ALLOWED_ACE_TYPE
            || unsafe { (*ace).Mask } & WRITE_MASK == 0
        {
            continue;
        }
        let sid = unsafe { addr_of!((*ace).SidStart).cast_mut().cast() };
        if [
            WinWorldSid,
            WinAuthenticatedUserSid,
            WinBuiltinUsersSid,
            WinBuiltinGuestsSid,
        ]
        .into_iter()
        .any(|kind| unsafe { IsWellKnownSid(sid, kind) } != 0)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "windows")]
struct LocalSecurityDescriptor(windows_sys::Win32::Security::PSECURITY_DESCRIPTOR);

#[cfg(target_os = "windows")]
impl Drop for LocalSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { windows_sys::Win32::Foundation::LocalFree(self.0) };
        }
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
fn is_broadly_writable(_: &Path) -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_executable_is_rejected() {
        let missing = std::env::temp_dir().join("seeusage-definitely-missing-tool");
        assert!(inspect_executable(&missing).is_err());
    }
}
