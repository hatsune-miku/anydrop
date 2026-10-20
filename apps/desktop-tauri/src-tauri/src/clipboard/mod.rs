mod io;

use super::{emit_snapshot, outgoing, AppHandle, Backend};
use anydrop::service::{
    anydrop_service::AnyDropServiceConfig, discovery_service::DiscoveryService,
};
use image::RgbaImage;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use tauri::Manager;

pub use io::{decode, Content};

#[derive(Default)]
pub struct ClipboardState {
    pub last_text: String,
    pub last_received_text: String,
    /// Only writes performed by AnyDrop arm suppression. An outgoing send
    /// doesn't touch the OS clipboard and must never suppress the next copy.
    written_revision: Option<u64>,
    #[cfg(not(any(windows, target_os = "macos")))]
    written_fingerprint: Option<(u64, Instant)>,
}

fn fingerprint(content: &Content) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    match content {
        Content::Text(text) => {
            add(&[0]);
            add(text.as_bytes());
        }
        Content::Image(image) => {
            add(&[1]);
            add(&image.width().to_le_bytes());
            add(&image.height().to_le_bytes());
            add(image);
        }
    }
    hash
}

/// Revisions, not content hashes, identify copy actions. Reading failures leave
/// a revision pending so a busy clipboard or delayed renderer can recover.
struct Changes {
    revision: Option<u64>,
    attempts: u8,
    handled: bool,
    previous_copy: Option<(u64, Instant)>,
}

impl Changes {
    fn new(revision: Option<u64>) -> Self {
        Self {
            revision,
            attempts: 0,
            handled: true,
            previous_copy: None,
        }
    }

    fn needs_read(&mut self, revision: u64, written_revision: Option<u64>) -> bool {
        if self.revision != Some(revision) {
            let initial = self.revision.is_none();
            self.revision = Some(revision);
            self.attempts = 0;
            self.handled = initial;
        }
        if written_revision == Some(revision) {
            self.handled = true;
            self.previous_copy = None;
        }
        !self.handled && self.attempts < 15
    }

    fn failed(&mut self) {
        self.attempts += 1;
    }

    fn copied(&mut self, fingerprint: u64, now: Instant, double_only: bool) -> bool {
        self.handled = true;
        let paired = self.previous_copy.is_some_and(|(previous, at)| {
            previous == fingerprint
                && now.saturating_duration_since(at) <= Duration::from_millis(600)
        });
        self.previous_copy = if double_only && paired {
            None
        } else {
            Some((fingerprint, now))
        };
        !double_only || paired
    }
}

fn retry_write(
    mut write: impl FnMut(&mut arboard::Clipboard) -> Result<(), arboard::Error>,
) -> Result<(), String> {
    let mut error = String::new();
    for attempt in 0..6 {
        match io::with_pool(|| {
            arboard::Clipboard::new().and_then(|mut clipboard| write(&mut clipboard))
        }) {
            Ok(()) => return Ok(()),
            Err(err) => error = err.to_string(),
        }
        if attempt < 5 {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Err(format!("写入剪贴板失败：{error}"))
}

pub fn write_text(state: &Mutex<ClipboardState>, text: &str) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    retry_write(|clipboard| clipboard.set_text(text))?;
    state.written_revision = io::revision();
    state.last_text = text.chars().take(200).collect();
    state.last_received_text = text.to_owned();
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        state.written_fingerprint =
            Some((fingerprint(&Content::Text(text.to_owned())), Instant::now()));
    }
    Ok(())
}

pub fn write_image(state: &Mutex<ClipboardState>, image: &RgbaImage) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    retry_write(|clipboard| {
        clipboard.set_image(arboard::ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: std::borrow::Cow::Borrowed(image),
        })
    })?;
    state.written_revision = io::revision();
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        state.written_fingerprint =
            Some((fingerprint(&Content::Image(image.clone())), Instant::now()));
    }
    Ok(())
}

fn send(
    app: &AppHandle,
    service: Arc<DiscoveryService>,
    config: &AnyDropServiceConfig,
    content: Content,
) -> Result<bool, String> {
    match content {
        Content::Text(text) => {
            if text.len() > anydrop::MAX_CLIPBOARD_BYTES {
                outgoing::clipboard(app, text.as_bytes(), "txt")?;
                return Ok(false);
            }
            anydrop::lib_util::shared_anydrop_broadcast_text(text, service, config);
        }
        Content::Image(image) => {
            let png = io::encode(&image)?;
            if png.len() > anydrop::MAX_CLIPBOARD_BYTES {
                outgoing::clipboard(app, &png, "png")?;
                return Ok(false);
            }
            anydrop::lib_util::shared_anydrop_broadcast_image(png, service, config);
        }
    }
    emit_snapshot(app);
    Ok(true)
}

pub fn send_current(app: &AppHandle, backend: &Backend) -> Result<bool, String> {
    if backend.installing.load(Ordering::SeqCst) {
        return Err("正在安装更新".into());
    }
    let service = backend.service().ok_or("服务未启动")?;
    let config = backend.config().ok_or("服务未启动")?;
    let content = io::read()?;
    if matches!(content, Content::Image(_)) && !backend.settings.lock().unwrap().sync_image_enabled
    {
        return Err("请先开启同步剪贴板图片".into());
    }
    if let Content::Text(text) = &content {
        backend.clipboard.lock().unwrap().last_text = text.chars().take(200).collect();
    }
    let sent = send(app, service.discovery_service(), &config, content)?;
    if sent {
        backend.set_status("Clipboard sent to LAN peers");
    }
    emit_snapshot(app);
    Ok(sent)
}

#[cfg(any(windows, target_os = "macos"))]
pub fn run(
    app: AppHandle,
    state: Arc<Mutex<ClipboardState>>,
    service: Arc<DiscoveryService>,
    config: AnyDropServiceConfig,
    stop: Arc<AtomicBool>,
) {
    let mut changes = Changes::new(io::revision());
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
        let Some(backend) = app.try_state::<Backend>() else {
            break;
        };
        let settings = backend.settings.lock().unwrap().clone();
        // Serialize reads with our writes, including recording their revision.
        // There is no interval in which a receive can be mistaken for a copy.
        let mut state = state.lock().unwrap();
        let Some(revision) = io::revision() else {
            continue;
        };
        if !changes.needs_read(revision, state.written_revision) {
            continue;
        }
        let content = match io::read() {
            Ok(content) => content,
            Err(error) => {
                changes.failed();
                if changes.attempts == 15 {
                    backend.log(format!(
                        "clipboard read unavailable after retries: {error}; formats={:?}",
                        io::format_names()
                    ));
                    drop(state);
                    emit_snapshot(&app);
                }
                continue;
            }
        };
        // A producer may finish delayed rendering during get(). Read the final
        // generation on the next tick rather than sending a mixed snapshot.
        if io::revision() != Some(revision) {
            continue;
        }
        if let Content::Text(text) = &content {
            state.last_text = text.chars().take(200).collect();
        }
        let should_send = changes.copied(
            fingerprint(&content),
            Instant::now(),
            settings.send_only_on_double_copy,
        );
        drop(state);
        if should_send
            && settings.send_clipboard_enabled
            && (settings.sync_image_enabled || matches!(content, Content::Text(_)))
        {
            if let Err(err) = send(&app, service.clone(), &config, content) {
                backend.log(format!("clipboard send: {err}"));
                backend.set_status(&err);
                emit_snapshot(&app);
            }
        }
    }
}

// Linux keeps its native X11 event subscription. Windows/macOS use their OS
// counters, which remain usable after sleep even if a message listener dies.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn run(
    app: AppHandle,
    state: Arc<Mutex<ClipboardState>>,
    service: Arc<DiscoveryService>,
    config: AnyDropServiceConfig,
    stop: Arc<AtomicBool>,
) {
    use clipboard_master::{CallbackResult, ClipboardHandler, Master};
    struct Listener {
        app: AppHandle,
        state: Arc<Mutex<ClipboardState>>,
        service: Arc<DiscoveryService>,
        config: AnyDropServiceConfig,
        changes: Changes,
    }
    impl ClipboardHandler for Listener {
        fn on_clipboard_change(&mut self) -> CallbackResult {
            let Some(backend) = self.app.try_state::<Backend>() else {
                return CallbackResult::Stop;
            };
            let settings = backend.settings.lock().unwrap().clone();
            let mut state = self.state.lock().unwrap();
            let Ok(content) = io::read() else {
                return CallbackResult::Next;
            };
            let digest = fingerprint(&content);
            if state
                .written_fingerprint
                .take()
                .is_some_and(|(d, at)| d == digest && at.elapsed() < Duration::from_secs(2))
            {
                return CallbackResult::Next;
            }
            if let Content::Text(text) = &content {
                state.last_text = text.chars().take(200).collect();
            }
            drop(state);
            if self
                .changes
                .copied(digest, Instant::now(), settings.send_only_on_double_copy)
                && settings.send_clipboard_enabled
                && (settings.sync_image_enabled || matches!(content, Content::Text(_)))
            {
                let _ = send(&self.app, self.service.clone(), &self.config, content);
            }
            CallbackResult::Next
        }
    }
    let handler = Listener {
        app,
        state,
        service,
        config,
        changes: Changes::new(None),
    };
    if let Ok(mut master) = Master::new(handler) {
        let shutdown = master.shutdown_channel();
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(250));
            }
            let _ = shutdown.signal();
        });
        let _ = master.run();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recopied_text_is_sent_including_after_receive() {
        let mut changes = Changes::new(Some(1));
        let now = Instant::now();
        for revision in 2..20 {
            assert!(changes.needs_read(revision, None));
            assert!(changes.copied(42, now, false));
            assert!(!changes.needs_read(revision, None));
        }
        assert!(!changes.needs_read(20, Some(20)));
        assert!(changes.needs_read(21, Some(20)));
        assert!(changes.copied(42, now, false));
    }

    #[test]
    fn two_devices_do_not_echo_received_writes_over_many_wake_cycles() {
        let mut a = Changes::new(Some(1));
        let mut b = Changes::new(Some(1));
        let mut sent = 0;
        for day in 0..30 {
            let copy = 2 + day * 2;
            assert!(a.needs_read(copy, Some(copy - 1)));
            if a.copied(99, Instant::now(), false) {
                sent += 1;
            }
            // Peer writes may produce multiple OS events; all read the same
            // final generation. None of those events may start a new send.
            for _ in 0..5 {
                assert!(!b.needs_read(copy, Some(copy)));
            }
            assert!(b.needs_read(copy + 1, Some(copy)));
            if b.copied(99, Instant::now(), false) {
                sent += 1;
            }
            assert!(!a.needs_read(copy + 1, Some(copy + 1)));
        }
        assert_eq!(sent, 60);
    }

    #[test]
    fn busy_clipboard_retries_and_a_later_revision_recovers() {
        let mut changes = Changes::new(Some(7));
        for _ in 0..10 {
            assert!(changes.needs_read(8, None));
            changes.failed();
        }
        assert!(changes.needs_read(8, None));
        assert!(changes.copied(9, Instant::now(), false));
        assert!(!changes.needs_read(8, None));
        for _ in 0..15 {
            assert!(changes.needs_read(9, None));
            changes.failed();
        }
        assert!(!changes.needs_read(9, None));
        assert!(changes.needs_read(10, None));
        assert!(changes.copied(9, Instant::now(), false));
    }

    #[test]
    fn double_copy_requires_same_content_and_ignores_receives() {
        let mut changes = Changes::new(Some(1));
        let now = Instant::now();
        assert!(!changes.copied(1, now, true));
        assert!(!changes.copied(2, now + Duration::from_millis(100), true));
        assert!(changes.copied(2, now + Duration::from_millis(200), true));
        assert!(!changes.copied(2, now + Duration::from_millis(300), true));
        assert!(!changes.needs_read(8, Some(8)));
        assert!(!changes.copied(2, now + Duration::from_millis(400), true));
        assert!(!changes.copied(2, now + Duration::from_secs(8), true));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "Uses the system clipboard; run only on an isolated test desktop"]
    fn native_clipboard_formats_and_recopy() {
        let state = Mutex::new(ClipboardState::default());
        let mut clipboard = arboard::Clipboard::new().unwrap();
        clipboard.set_text("initial").unwrap();
        let mut changes = Changes::new(io::revision());
        for _ in 0..30 {
            clipboard.set_text("same copied text").unwrap();
            let copied = io::revision().unwrap();
            assert!(changes.needs_read(copied, state.lock().unwrap().written_revision));
            let content = io::read().unwrap();
            assert!(changes.copied(fingerprint(&content), Instant::now(), false));
            write_text(&state, "remote text").unwrap();
            let received = io::revision().unwrap();
            assert!(!changes.needs_read(received, state.lock().unwrap().written_revision));
            clipboard.set_text("remote text").unwrap();
            assert!(changes.needs_read(
                io::revision().unwrap(),
                state.lock().unwrap().written_revision
            ));
            assert!(changes.copied(fingerprint(&io::read().unwrap()), Instant::now(), false));
        }
        let original = RgbaImage::from_pixel(3, 2, image::Rgba([201, 40, 19, 255]));
        write_image(&state, &original).unwrap();
        assert!(!changes.needs_read(
            io::revision().unwrap(),
            state.lock().unwrap().written_revision
        ));
        let Content::Image(roundtrip) = io::read().unwrap() else {
            panic!("missing received image")
        };
        assert_eq!(roundtrip.dimensions(), original.dimensions());
        #[cfg(target_os = "macos")]
        let cases = [
            ("public.png", image::ImageFormat::Png),
            ("public.jpeg", image::ImageFormat::Jpeg),
            ("public.tiff", image::ImageFormat::Tiff),
        ];
        #[cfg(windows)]
        let cases = [
            ("image/png", image::ImageFormat::Png),
            ("JFIF", image::ImageFormat::Jpeg),
            ("CF_DIB", image::ImageFormat::Bmp),
        ];
        for (format_name, format) in cases {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(original.clone())
                .to_rgb8()
                .write_to(&mut bytes, format)
                .unwrap();
            let bytes = bytes.into_inner();
            let bytes = if format_name == "CF_DIB" {
                &bytes[14..]
            } else {
                &bytes
            };
            io::publish_encoded(format_name, bytes);
            let Content::Image(read) = io::read().unwrap() else {
                panic!("unreadable {format_name}")
            };
            assert_eq!(read.dimensions(), (3, 2), "{format_name}");
        }
        let png = io::encode(&original).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat image.png");
        std::fs::write(&path, &png).unwrap();
        clipboard.set().file_list(&[&path]).unwrap();
        assert!(matches!(io::read().unwrap(), Content::Image(_)));
        use base64::Engine;
        let html = format!(
            "<img src=\"data:image/png;base64,{}\">",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        clipboard.set().html(html, None).unwrap();
        assert!(matches!(io::read().unwrap(), Content::Image(_)));
        clipboard
            .set()
            .html("<p>HTML 文本 &amp; text</p>", None)
            .unwrap();
        let Content::Text(text) = io::read().unwrap() else {
            panic!("missing HTML text")
        };
        assert_eq!(text, "HTML 文本 & text");
    }
}
