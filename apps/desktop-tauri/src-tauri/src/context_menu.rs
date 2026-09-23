//! OS-owned file actions. Never interpolate selected filenames into shell code.
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

pub fn handle_arguments(app: &AppHandle, args: &[String], cwd: &str) -> bool {
    let Some(index) = args.iter().position(|arg| arg == "--send-files") else {
        return false;
    };
    if args.get(index + 1).map(String::as_str) != Some("--") {
        return false;
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
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = super::outgoing::files(&app, paths) {
            app.state::<super::Backend>().set_status(&error);
            let _ = app.emit("desktop-notice", &error);
            super::show_main_window(&app);
        }
    });
    true
}

pub fn register(app: &AppHandle, enabled: bool, repair: bool) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("ANYDROP_TEST_CONFIG_DIR").is_some() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (app, repair);
        return macos::register(enabled);
    }
    #[cfg(windows)]
    {
        return windows::register(app, enabled, repair);
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (app, enabled, repair);
        Err("当前系统尚不支持文件右键菜单".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use serde_json::json;
    use std::{fs, path::Path, process::Command};
    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
    fn write_plist(path: &Path, value: &serde_json::Value) -> Result<(), String> {
        let file = fs::File::create(path).map_err(|e| e.to_string())?;
        plist::to_writer_xml(file, value).map_err(|e| e.to_string())
    }
    pub fn register(enabled: bool) -> Result<(), String> {
        let home = dirs::home_dir().ok_or("无法定位用户目录")?;
        let services = home.join("Library/Services");
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        write_services(&services, &exe, enabled)?;
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
    pub fn register(app: &AppHandle, enabled: bool, repair: bool) -> Result<(), String> {
        #[cfg(debug_assertions)]
        if std::env::var_os("ANYDROP_TEST_CONFIG_DIR").is_some() {
            return Ok(());
        }
        use std::os::windows::process::CommandExt;
        let script = app
            .path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("shell/register.ps1");
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .arg("-InstallRoot")
            .arg(exe.parent().ok_or("无法定位安装目录")?)
            .arg("-Executable")
            .arg(&exe)
            .arg("-Registration")
            .arg(if repair { "Repair" } else { "Normal" })
            .arg("-Mode")
            .arg(if enabled { "Register" } else { "Unregister" })
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("右键菜单注册失败：{e}"))?;
        if !output.status.success() {
            return Err(format!(
                "右键菜单注册失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
}
