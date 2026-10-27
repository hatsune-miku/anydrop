//! Windows file-menu requests outlive a lost single-instance window message.
//! A second process publishes an atomic, user-local request before Tauri starts.
//! The primary process acknowledges only after accepting it into the send queue.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const MAX_PENDING: usize = 128;

#[derive(Deserialize, Serialize)]
struct Request {
    version: u32,
    paths: Vec<PathBuf>,
}

#[derive(Clone)]
struct Inbox(PathBuf);

impl Inbox {
    fn new(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn write(&self, name: &str, value: &impl Serialize) -> Result<(), String> {
        let mut file = tempfile::NamedTempFile::new_in(&self.0).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(self.0.join(name)).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn submit(&self, paths: Vec<PathBuf>) -> Result<String, String> {
        if paths.is_empty() || paths.iter().any(|path| !path.is_absolute()) {
            return Err("没有可发送的绝对路径".into());
        }
        if self.pending()?.len() >= MAX_PENDING {
            return Err("文件菜单请求过多，请打开 AnyDrop 处理已有请求".into());
        }
        let request = Request { version: 1, paths };
        if serde_json::to_vec(&request)
            .map_err(|e| e.to_string())?
            .len() as u64
            > MAX_REQUEST_BYTES
        {
            return Err("所选路径过多，请分批发送".into());
        }
        let id = format!("{:020}-{:032x}", super::now_ms(), rand::random::<u128>());
        self.write(&format!("{id}.request"), &request)?;
        Ok(id)
    }

    fn pending(&self) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.0).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".request") {
                if !id.is_empty() && id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
                    ids.push(id.to_owned());
                }
            } else if name.ends_with(".ack")
                && entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|at| at.elapsed().ok())
                    .is_some_and(|age| age > Duration::from_secs(3600))
            {
                let _ = fs::remove_file(entry.path());
            }
        }
        ids.sort();
        Ok(ids)
    }

    fn process(
        &self,
        id: &str,
        deliver: impl FnOnce(Vec<PathBuf>) -> Result<(), String>,
    ) -> Result<(), String> {
        let path = self.0.join(format!("{id}.request"));
        // Recover an interruption between writing the acknowledgement and removing
        // its request without asking the user to confirm the same files twice.
        if self.0.join(format!("{id}.ack")).is_file() {
            return fs::remove_file(path).map_err(|e| e.to_string());
        }
        let result = (|| {
            let file = fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take(MAX_REQUEST_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_REQUEST_BYTES {
                return Err("文件菜单请求过大".into());
            }
            let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if request.version != 1
                || request.paths.is_empty()
                || request.paths.iter().any(|p| !p.is_absolute())
            {
                return Err("文件菜单请求格式无效".into());
            }
            deliver(request.paths)
        })();
        self.write(&format!("{id}.ack"), &result)?;
        fs::remove_file(path).map_err(|e| e.to_string())
    }

    fn wait(&self, id: &str, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        let path = self.0.join(format!("{id}.ack"));
        loop {
            match fs::read(&path) {
                Ok(bytes) => {
                    return serde_json::from_slice::<Result<(), String>>(&bytes)
                        .map_err(|e| e.to_string())?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.to_string()),
            }
            if start.elapsed() >= timeout {
                return Err("AnyDrop 尚未接收文件请求。请求已保留，请打开 AnyDrop 查看。".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(windows)]
pub enum Launch {
    Start {
        queued: bool,
        startup: Option<StartupGuard>,
    },
    Delivered,
}

/// Held on the launching thread until Tauri has installed its single-instance
/// window and started the inbox. Concurrent cold starts cannot bypass preflight.
#[cfg(windows)]
pub struct StartupGuard(isize);

#[cfg(windows)]
impl StartupGuard {
    fn acquire(identifier: &str, timeout: u32) -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
            System::Threading::{CreateMutexW, WaitForSingleObject},
        };
        let name: Vec<_> = format!("{identifier}-shell-startup")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            match WaitForSingleObject(handle, timeout) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle as isize)),
                _ => {
                    CloseHandle(handle);
                    Err("AnyDrop 启动尚未完成，请稍后打开 AnyDrop 查看文件请求。".into())
                }
            }
        }
    }
}

#[cfg(windows)]
impl Drop for StartupGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0 as _);
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

#[cfg(windows)]
fn instance_exists(identifier: &str) -> Result<bool, String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND},
        System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE},
    };
    let name: Vec<u16> = format!("{identifier}-sim")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let handle = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr());
        if !handle.is_null() {
            CloseHandle(handle);
            return Ok(true);
        }
        if GetLastError() == ERROR_FILE_NOT_FOUND {
            Ok(false)
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }
}

#[cfg(windows)]
pub fn prepare(identifier: &str) -> Result<Launch, String> {
    let args: Vec<_> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let selection = super::context_menu::selected_paths(&args, &cwd.to_string_lossy());
    let pending = selection
        .map(|paths| {
            let inbox = Inbox::new(super::config_base().join("shell-inbox"))?;
            let id = inbox.submit(paths)?;
            Ok::<_, String>((inbox, id))
        })
        .transpose()?;
    let startup = StartupGuard::acquire(identifier, 15_000)?;
    if instance_exists(identifier)? {
        drop(startup);
        // No WM_COPYDATA/FindWindow call: Explorer and the primary UI can keep
        // working while this short-lived process waits for a durable reply.
        if let Some((inbox, id)) = pending {
            inbox.wait(&id, Duration::from_secs(15))?;
            Ok(Launch::Delivered)
        } else {
            Ok(Launch::Start {
                queued: false,
                startup: None,
            })
        }
    } else {
        Ok(Launch::Start {
            queued: pending.is_some(),
            startup: Some(startup),
        })
    }
}

#[cfg(windows)]
pub fn report_error(error: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let text: Vec<_> = error.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "AnyDrop".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(windows)]
pub fn start(app: tauri::AppHandle) {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
    };
    use tauri::{Emitter, Manager};
    std::thread::spawn(move || {
        let inbox = Inbox(super::config_base().join("shell-inbox"));
        let active = Arc::new(Mutex::new(HashSet::new()));
        let mut last_error = None;
        loop {
            match fs::create_dir_all(&inbox.0)
                .map_err(|e| e.to_string())
                .and_then(|_| inbox.pending())
            {
                Ok(ids) => {
                    last_error = None;
                    for id in ids {
                        let mut locked = active.lock().unwrap();
                        if locked.len() >= 8 {
                            break;
                        }
                        if !locked.insert(id.clone()) {
                            continue;
                        }
                        drop(locked);
                        let app = app.clone();
                        let inbox = inbox.clone();
                        let active = active.clone();
                        // A slow/unavailable network path must not block other selections.
                        std::thread::spawn(move || {
                            let result = inbox.process(&id, |paths| {
                                let count = paths.len();
                                if let Err(error) = super::outgoing::files_with_id(
                                    &app,
                                    paths,
                                    format!("shell:{id}"),
                                ) {
                                    let backend = app.state::<super::Backend>();
                                    backend.set_status(&error);
                                    backend
                                        .log(format!("File menu request rejected: {id}: {error}"));
                                    let _ = app.emit("desktop-notice", &error);
                                    super::emit_snapshot(&app);
                                    super::show_main_window(&app);
                                    return Err(error);
                                }
                                app.state::<super::Backend>().log(format!(
                                    "File menu request accepted: {id} ({count} paths)"
                                ));
                                Ok(())
                            });
                            if let Err(error) = result {
                                app.state::<super::Backend>()
                                    .log(format!("File menu inbox: {error}"));
                            }
                            active.lock().unwrap().remove(&id);
                        });
                    }
                }
                Err(error) => {
                    if last_error.as_ref() != Some(&error) {
                        app.state::<super::Backend>()
                            .log(format!("File menu inbox: {error}"));
                    }
                    last_error = Some(error);
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_selection_survives_missing_wakeup_and_a_consumer_restart() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(dir.path().join("inbox")).unwrap();
        let paths = vec![
            dir.path().join("中文 空格 ' $().txt"),
            dir.path().join("folder"),
        ];
        let id = inbox.submit(paths.clone()).unwrap();
        assert!(inbox.wait(&id, Duration::ZERO).is_err());
        let reopened = Inbox::new(inbox.0.clone()).unwrap();
        assert_eq!(reopened.pending().unwrap(), [id.clone()]);
        reopened
            .process(&id, |received| {
                assert_eq!(received, paths);
                Ok(())
            })
            .unwrap();
        inbox.wait(&id, Duration::ZERO).unwrap();
        assert!(reopened.pending().unwrap().is_empty());
        // Simulate a crash after the ack was saved but before the request was removed.
        reopened
            .write(&format!("{id}.request"), &Request { version: 1, paths })
            .unwrap();
        reopened
            .process(&id, |_| panic!("acknowledged request was delivered twice"))
            .unwrap();
    }

    #[test]
    fn delivery_errors_are_returned_and_partial_requests_are_not_consumed() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(dir.path().join("inbox")).unwrap();
        fs::write(inbox.0.join("partial.tmp"), b"{").unwrap();
        assert!(inbox.pending().unwrap().is_empty());
        let id = inbox.submit(vec![dir.path().join("missing.txt")]).unwrap();
        inbox
            .process(&id, |_| Err("cannot read selected file".into()))
            .unwrap();
        assert_eq!(
            inbox.wait(&id, Duration::ZERO).unwrap_err(),
            "cannot read selected file"
        );
        assert!(inbox.pending().unwrap().is_empty());
        assert!(inbox.submit(vec![PathBuf::from("relative")]).is_err());
    }

    #[test]
    fn simultaneous_menu_clicks_keep_distinct_requests_and_acknowledgements() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(dir.path().join("inbox")).unwrap();
        let workers: Vec<_> = (0..16)
            .map(|index| {
                let inbox = inbox.clone();
                let path = dir.path().join(format!("file-{index}"));
                std::thread::spawn(move || inbox.submit(vec![path]).unwrap())
            })
            .collect();
        let ids: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(inbox.pending().unwrap().len(), ids.len());
        for id in ids {
            inbox.process(&id, |_| Ok(())).unwrap();
            inbox.wait(&id, Duration::ZERO).unwrap();
        }
        assert!(inbox.pending().unwrap().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn concurrent_cold_starts_wait_until_the_first_instance_is_ready() {
        let identifier = format!("anydrop-start-test-{:032x}", rand::random::<u128>());
        let first = StartupGuard::acquire(&identifier, 100).unwrap();
        let second_id = identifier.clone();
        assert!(
            std::thread::spawn(move || StartupGuard::acquire(&second_id, 10).is_err())
                .join()
                .unwrap()
        );
        drop(first);
        std::thread::spawn(move || {
            let _second = StartupGuard::acquire(&identifier, 100).unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_primary_without_a_message_window_can_receive_menu_requests() {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::CreateMutexW};
        let identifier = format!("anydrop-test-{:032x}", rand::random::<u128>());
        assert!(!instance_exists(&identifier).unwrap());
        let name: Vec<_> = format!("{identifier}-sim")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        assert!(!handle.is_null());
        assert!(instance_exists(&identifier).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(dir.path().join("inbox")).unwrap();
        let id = inbox.submit(vec![dir.path().join("file")]).unwrap();
        let consumer = inbox.clone();
        let request = id.clone();
        let worker = std::thread::spawn(move || consumer.process(&request, |_| Ok(())).unwrap());
        inbox.wait(&id, Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
        unsafe {
            CloseHandle(handle);
        }
        assert!(!instance_exists(&identifier).unwrap());
    }
}
