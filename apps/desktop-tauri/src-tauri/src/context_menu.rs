//! OS-owned file actions. Never interpolate selected filenames into shell code.
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum FileContextMenuLocation {
    Finder,
    Services,
}

pub fn initial_location() -> Option<FileContextMenuLocation> {
    #[cfg(target_os = "macos")]
    {
        let services = dirs::home_dir().map(|home| home.join("Library/Services"));
        return Some(location_for_existing_services(services.as_deref()));
    }
    #[cfg(not(target_os = "macos"))]
    None
}

#[cfg(any(target_os = "macos", test))]
fn location_for_existing_services(services: Option<&std::path::Path>) -> FileContextMenuLocation {
    if services.is_some_and(|root| {
        ["AnyDrop-Send.workflow", "AnyDrop-CopyPaths.workflow"]
            .iter()
            .any(|name| root.join(name).exists())
    }) {
        FileContextMenuLocation::Services
    } else {
        FileContextMenuLocation::Finder
    }
}

#[cfg(target_os = "macos")]
pub fn handle_urls(app: &AppHandle, urls: &[tauri::Url]) {
    for url in urls {
        if let Some(paths) = paths_from_url(url) {
            dispatch_files(app, paths);
        }
    }
}

#[cfg(target_os = "macos")]
fn paths_from_url(url: &tauri::Url) -> Option<Vec<PathBuf>> {
    if url.scheme() != "anydrop"
        || url.host_str() != Some("send-files")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
    {
        return None;
    }
    let paths: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key == "path")
        .map(|(_, path)| PathBuf::from(path.as_ref()))
        .collect();
    if paths.is_empty()
        || paths
            .iter()
            .any(|path| !path.is_absolute() || path.to_string_lossy().contains('\0'))
    {
        return None;
    }
    Some(paths)
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn existing_services_keep_their_location_and_fresh_installs_use_finder() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            location_for_existing_services(Some(directory.path())),
            FileContextMenuLocation::Finder
        );
        for name in ["AnyDrop-Send.workflow", "AnyDrop-CopyPaths.workflow"] {
            let existing = directory.path().join(name);
            std::fs::create_dir(&existing).unwrap();
            assert_eq!(
                location_for_existing_services(Some(directory.path())),
                FileContextMenuLocation::Services
            );
            std::fs::remove_dir(existing).unwrap();
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn finder_urls_preserve_multiselect_and_special_characters() {
        let paths = [
            "/Users/test/空格 ' + & # % ?.txt",
            "/Volumes/Disk/line\nbreak.txt",
        ];
        let mut url = tauri::Url::parse("anydrop://send-files").unwrap();
        for path in paths {
            url.query_pairs_mut().append_pair("path", path);
        }
        assert_eq!(
            paths_from_url(&url),
            Some(paths.iter().map(PathBuf::from).collect())
        );
        // URL produced by NSURLComponents uses %20 for spaces and must encode +.
        assert_eq!(
            paths_from_url(&tauri::Url::parse("anydrop://send-files?path=/tmp/a%2Bb%20c").unwrap()),
            Some(vec![PathBuf::from("/tmp/a+b c")])
        );
        for invalid in [
            "https://send-files?path=/tmp/a",
            "anydrop://other?path=/tmp/a",
            "anydrop://send-files?path=relative",
            "anydrop://send-files",
            "anydrop://send-files?path=/a%00b",
        ] {
            assert!(paths_from_url(&tauri::Url::parse(invalid).unwrap()).is_none());
        }
    }
}

pub(crate) fn selected_paths(args: &[String], cwd: &str) -> Option<Vec<PathBuf>> {
    let index = args.iter().position(|arg| arg == "--send-files")?;
    if args.get(index + 1).map(String::as_str) != Some("--") {
        return None;
    }
    let paths: Vec<_> = args[index + 2..]
        .iter()
        .map(|path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                PathBuf::from(cwd).join(path)
            }
        })
        .collect();
    Some(paths)
}

pub fn handle_arguments(app: &AppHandle, args: &[String], cwd: &str) -> bool {
    let Some(paths) = selected_paths(args, cwd) else {
        return false;
    };
    dispatch_files(app, paths);
    true
}

fn dispatch_files(app: &AppHandle, paths: Vec<PathBuf>) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = super::outgoing::files(&app, paths) {
            app.state::<super::Backend>().set_status(&error);
            let _ = app.emit("desktop-notice", &error);
            super::show_main_window(&app);
        }
    });
}

pub fn register(
    app: &AppHandle,
    enabled: bool,
    repair: bool,
    location: Option<FileContextMenuLocation>,
) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("ANYDROP_TEST_CONFIG_DIR").is_some() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (app, repair);
        return macos::register(
            enabled,
            location.unwrap_or(FileContextMenuLocation::Finder),
            repair,
        );
    }
    #[cfg(windows)]
    {
        let _ = location;
        return windows::register(app, enabled, repair);
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (app, enabled, repair, location);
        Err("当前系统尚不支持文件右键菜单".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::FileContextMenuLocation;
    use serde_json::json;
    use std::{fs, path::Path, process::Command};
    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
    fn write_plist(path: &Path, value: &serde_json::Value) -> Result<(), String> {
        let file = fs::File::create(path).map_err(|e| e.to_string())?;
        plist::to_writer_xml(file, value).map_err(|e| e.to_string())
    }
    const EXTENSION_ID: &str = "com.anydrop.desktop.FinderSync";

    fn plugin_command(args: &[&std::ffi::OsStr]) -> Result<String, String> {
        let output = Command::new("/usr/bin/pluginkit")
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "访达扩展注册失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn finder_registration(enabled: bool, exe: &Path, repair: bool) -> Result<(), String> {
        let extension = exe
            .parent()
            .and_then(Path::parent)
            .map(|contents| contents.join("PlugIns/AnyDropFinder.appex"))
            .filter(|path| path.is_dir());
        if let Some(extension) = extension {
            plugin_command(&["-a".as_ref(), extension.as_os_str()])?;
        } else if enabled {
            return Err("当前应用缺少访达扩展，请安装完整的 AnyDrop.app".into());
        }
        let status = plugin_command(&["-m".as_ref(), "-i".as_ref(), EXTENSION_ID.as_ref()])?;
        if enabled || !status.trim().is_empty() {
            // Do not undo a system-level disable on every launch. An explicit
            // enable / repair is allowed to request activation again.
            if !enabled || repair || !status.trim_start().starts_with('-') {
                plugin_command(&[
                    "-e".as_ref(),
                    if enabled { "use" } else { "ignore" }.as_ref(),
                    "-i".as_ref(),
                    EXTENSION_ID.as_ref(),
                ])?;
            }
        }
        if enabled {
            let status = plugin_command(&["-m".as_ref(), "-i".as_ref(), EXTENSION_ID.as_ref()])?;
            if !status.trim_start().starts_with('+') {
                return Err("请在系统设置的访达扩展中启用 AnyDrop".into());
            }
        }
        Ok(())
    }

    pub fn register(
        enabled: bool,
        location: FileContextMenuLocation,
        repair: bool,
    ) -> Result<(), String> {
        let home = dirs::home_dir().ok_or("无法定位用户目录")?;
        let services = home.join("Library/Services");
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // Activate the destination first. If it fails, preserve the working
        // registration so the user can repair or choose the other location.
        if enabled && location == FileContextMenuLocation::Finder {
            finder_registration(true, &exe, repair)?;
        }
        write_services(
            &services,
            &exe,
            enabled && location == FileContextMenuLocation::Services,
        )?;
        let output = Command::new("/System/Library/CoreServices/pbs")
            .arg("-update")
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "服务菜单刷新失败：{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        if !enabled || location == FileContextMenuLocation::Services {
            finder_registration(false, &exe, repair)?;
        }
        Ok(())
    }
    fn write_services(services: &Path, exe: &Path, enabled: bool) -> Result<(), String> {
        fs::create_dir_all(&services).map_err(|e| e.to_string())?;
        for (id, name, script) in [
            (
                "Send",
                "使用 AnyDrop 发送",
                format!(
                    "{} --send-files -- \"$@\" >/dev/null 2>&1 &",
                    shell_quote(&exe.to_string_lossy())
                ),
            ),
            (
                "CopyPaths",
                "复制绝对路径",
                "printf '%s\\n' \"$@\" | /usr/bin/pbcopy".into(),
            ),
        ] {
            let package = services.join(format!("AnyDrop-{id}.workflow"));
            if !enabled {
                if package.exists() {
                    fs::remove_dir_all(&package).map_err(|e| e.to_string())?;
                }
                continue;
            }
            let contents = package.join("Contents");
            fs::create_dir_all(contents.join("Resources")).map_err(|e| e.to_string())?;
            write_plist(
                &contents.join("Info.plist"),
                &json!({
                    "CFBundleIdentifier": format!("com.anydrop.services.{id}"), "CFBundleName": name,
                    "CFBundleShortVersionString": "1.0",
                    "NSServices": [{ "NSMenuItem": {"default": name}, "NSMessage": "runWorkflowAsService",
                        "NSRequiredContext": {"NSApplicationIdentifier": "com.apple.finder"},
                        "NSSendFileTypes": ["public.item"] }]
                }),
            )?;
            write_plist(
                &contents.join("Resources/document.wflow"),
                &json!({
                    "AMDocumentVersion": "2", "AMApplicationVersion": "2.10", "AMApplicationBuild": "523",
                    "actions": [{"action": {
                        "AMAccepts": {"Container": "List", "Optional": false, "Types": ["com.apple.cocoa.string"]},
                        "AMProvides": {"Container": "List", "Types": ["com.apple.cocoa.string"]},
                        "AMActionVersion": "2.0.3", "ActionName": "Run Shell Script",
                        "ActionBundlePath": "/System/Library/Automator/Run Shell Script.action",
                        "BundleIdentifier": "com.apple.RunShellScript", "Class Name": "RunShellScriptAction",
                        "ActionParameters": {"COMMAND_STRING": script, "CheckedForUserDefaultShell": true,
                            "inputMethod": 1, "shell": "/bin/sh", "source": ""},
                        "UUID": format!("AnyDrop-{id}"), "ShowWhenRun": false
                    }}],
                    "workflowMetaData": {"workflowTypeIdentifier": "com.apple.Automator.servicesMenu",
                        "serviceApplicationBundleID": "com.apple.finder",
                        "serviceInputTypeIdentifier": "com.apple.Automator.fileSystemObject",
                        "serviceOutputTypeIdentifier": "com.apple.Automator.nothing", "serviceProcessesInput": 0}
                }),
            )?;
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        #[test]
        fn automator_service_preserves_selected_paths() {
            use std::os::unix::fs::PermissionsExt;
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let spy = root.join("AnyDrop's test app");
            let result = root.join("arguments.txt");
            std::fs::write(
                &spy,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n",
                    super::shell_quote(&result.to_string_lossy())
                ),
            )
            .unwrap();
            std::fs::set_permissions(&spy, std::fs::Permissions::from_mode(0o700)).unwrap();
            super::write_services(root, &spy, true).unwrap();
            let selected = root.join("空格 ' $().txt");
            std::fs::write(&selected, "test").unwrap();
            let output = std::process::Command::new("/usr/bin/automator")
                .arg("-i")
                .arg(&selected)
                .arg(root.join("AnyDrop-Send.workflow"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            for _ in 0..30 {
                if result.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            assert_eq!(
                std::fs::read_to_string(&result).unwrap(),
                format!("--send-files\n--\n{}\n", selected.display())
            );
            super::write_services(root, &spy, false).unwrap();
            assert!(!root.join("AnyDrop-Send.workflow").exists());
            assert!(!root.join("AnyDrop-CopyPaths.workflow").exists());
            assert!(selected.exists());
        }
        #[test]
        fn quotes_executable_paths() {
            assert_eq!(
                super::shell_quote("/Users/A B/O'Reilly/AnyDrop"),
                "'/Users/A B/O'\\''Reilly/AnyDrop'"
            );
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::{ffi::OsStringExt, process::CommandExt};
    use std::process::Command;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    fn powershell_command() -> Result<Command, String> {
        // PowerShell lives below System32, which is not searched recursively.
        // A GUI app may inherit a PATH without WindowsPowerShell\v1.0. Ask the
        // OS for its real system directory instead of trusting PATH/SystemRoot
        // or accidentally executing a powershell.exe from the working directory.
        let mut buffer = vec![0u16; 260];
        let system = loop {
            // SAFETY: buffer is writable for the supplied number of UTF-16 units.
            let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
            if length == 0 {
                return Err(format!(
                    "无法定位 Windows 系统目录：{}",
                    std::io::Error::last_os_error()
                ));
            }
            if (length as usize) < buffer.len() {
                break PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
            }
            // For an insufficient buffer Windows returns the required size,
            // including the terminating null character.
            buffer.resize(length as usize + 1, 0);
        };
        let program = system.join(r"WindowsPowerShell\v1.0\powershell.exe");
        if !program.is_file() {
            return Err(format!(
                "未找到系统 Windows PowerShell（{}），请检查或修复该系统组件",
                program.display()
            ));
        }
        let mut command = Command::new(program);
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
            ])
            .creation_flags(0x08000000);
        Ok(command)
    }

    pub fn register(app: &AppHandle, enabled: bool, repair: bool) -> Result<(), String> {
        #[cfg(debug_assertions)]
        if std::env::var_os("ANYDROP_TEST_CONFIG_DIR").is_some() {
            return Ok(());
        }
        let script = app
            .path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("shell/register.ps1");
        if !script.is_file() {
            return Err(format!(
                "右键菜单注册失败：缺少注册脚本（{}），请重新安装 AnyDrop",
                script.display()
            ));
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = powershell_command().map_err(|e| format!("右键菜单注册失败：{e}"))?;
        let program = PathBuf::from(command.get_program());
        let output = command
            .arg("-File")
            .arg(&script)
            .arg("-InstallRoot")
            .arg(exe.parent().ok_or("无法定位安装目录")?)
            .arg("-Executable")
            .arg(&exe)
            .arg("-Registration")
            .arg(if repair { "Repair" } else { "Normal" })
            .arg("-Mode")
            .arg(if enabled { "Register" } else { "Unregister" })
            .output()
            .map_err(|e| format!("右键菜单注册失败：无法启动 {}：{e}", program.display()))?;
        if !output.status.success() {
            return Err(format!(
                "右键菜单注册失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn powershell_runs_without_path() {
            // Rust also searches the parent's PATH on Windows. Run this case in
            // a separate test process so neither PATH contains PowerShell, without
            // racing other tests by changing the current process's environment.
            if std::env::var("ANYDROP_POWERSHELL_TEST_CHILD")
                .ok()
                .as_deref()
                != Some("1")
            {
                let directory = tempfile::tempdir().unwrap();
                let output = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "context_menu::windows::tests::powershell_runs_without_path",
                        "--nocapture",
                    ])
                    .env("ANYDROP_POWERSHELL_TEST_CHILD", "1")
                    .env("PATH", "")
                    .current_dir(directory.path())
                    .creation_flags(0x08000000)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{:?}", output);
                return;
            }
            assert_eq!(
                std::env::var_os("PATH").unwrap_or_default(),
                OsString::new()
            );
            let directory = tempfile::tempdir().unwrap();
            // Reproduce the old launcher failure in the same environment.
            let error = Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", "exit 0"])
                .env("PATH", "")
                .current_dir(directory.path())
                .creation_flags(0x08000000)
                .output()
                .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            let output = powershell_command()
                .unwrap()
                .env("PATH", "")
                .current_dir(directory.path())
                .args(["-Command", "[Console]::Write('anydrop-shell-ok')"])
                .output()
                .unwrap();
            assert!(output.status.success(), "{:?}", output);
            assert_eq!(output.stdout, b"anydrop-shell-ok");
        }

        #[test]
        fn powershell_ignores_working_directory_and_path_shadows() {
            let directory = tempfile::Builder::new()
                .prefix("AnyDrop 空格 ")
                .tempdir()
                .unwrap();
            std::fs::write(directory.path().join("powershell.exe"), b"not a program").unwrap();
            let mut command = powershell_command().unwrap();
            assert!(std::path::Path::new(command.get_program()).is_absolute());
            let output = command
                .env("PATH", directory.path())
                .current_dir(directory.path())
                .args(["-Command", "[Console]::Write('system-powershell')"])
                .output()
                .unwrap();
            assert!(output.status.success(), "{:?}", output);
            assert_eq!(output.stdout, b"system-powershell");
        }
    }
}
