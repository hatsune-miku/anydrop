//! A second clipboard synchronizer can relay our write back as a new OS
//! revision. Do not create another revision when the content is already here.
use super::{io, retry_write, Content};
use image::RgbaImage;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy)]
pub(super) enum Incoming<'a> {
    Text(&'a str),
    Image(&'a RgbaImage),
}

impl Incoming<'_> {
    fn fingerprint(self) -> u64 {
        let mut hash = DefaultHasher::new();
        match self {
            Self::Text(text) => {
                0u8.hash(&mut hash);
                // Windows clipboard bridges commonly translate LF to CRLF.
                // Preserve all other whitespace, including lone CR and NUL.
                for part in text.split_inclusive('\n') {
                    if let Some(line) = part.strip_suffix("\r\n") {
                        hash.write(line.as_bytes());
                        hash.write_u8(b'\n');
                    } else {
                        hash.write(part.as_bytes());
                    }
                }
            }
            Self::Image(image) => {
                1u8.hash(&mut hash);
                image.dimensions().hash(&mut hash);
                image.as_raw().hash(&mut hash);
            }
        }
        hash.finish()
    }
}

impl<'a> From<&'a Content> for Incoming<'a> {
    fn from(content: &'a Content) -> Self {
        match content {
            Content::Text(text) => Self::Text(text),
            Content::Image(image) => Self::Image(image),
        }
    }
}

pub(super) enum WriteOutcome {
    Unchanged,
    Written(Option<u64>),
}

#[derive(Default)]
pub(super) struct Receiver {
    /// Only cache a successfully read, stable OS revision. Never retain the
    /// text or image, and never assume our input is the actual native output.
    observed: Option<(u64, u64)>,
}

trait Clipboard {
    fn revision(&self) -> Option<u64>;
    fn read(&mut self) -> Result<Content, String>;
    fn write(&mut self, incoming: Incoming<'_>) -> Result<(), String>;
}

struct NativeClipboard;

impl Clipboard for NativeClipboard {
    fn revision(&self) -> Option<u64> {
        io::revision()
    }

    fn read(&mut self) -> Result<Content, String> {
        io::read()
    }

    fn write(&mut self, incoming: Incoming<'_>) -> Result<(), String> {
        retry_write(|clipboard| match incoming {
            Incoming::Text(text) => clipboard.set_text(text),
            Incoming::Image(image) => clipboard.set_image(arboard::ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: std::borrow::Cow::Borrowed(image),
            }),
        })
    }
}

impl Receiver {
    pub(super) fn apply(
        &mut self,
        incoming: Incoming<'_>,
        force: bool,
    ) -> Result<WriteOutcome, String> {
        self.apply_with(&mut NativeClipboard, incoming, force)
    }

    fn apply_with(
        &mut self,
        clipboard: &mut impl Clipboard,
        incoming: Incoming<'_>,
        force: bool,
    ) -> Result<WriteOutcome, String> {
        if !force {
            let expected = incoming.fingerprint();
            let before = clipboard.revision();
            let cached = self
                .observed
                .filter(|(revision, _)| Some(*revision) == before);
            let actual = match cached {
                Some((_, actual)) => Some(actual),
                None => {
                    let content = clipboard.read();
                    let after = clipboard.revision();
                    // Never use a mixed/delayed-rendered snapshot to skip a
                    // delivery. Platforms without counters read each time.
                    if before != after {
                        None
                    } else {
                        content.ok().map(|content| {
                            let actual = Incoming::from(&content).fingerprint();
                            self.observed = after.map(|revision| (revision, actual));
                            actual
                        })
                    }
                }
            };
            if actual == Some(expected) {
                // A no-op must not claim an external/user copy as our write.
                return Ok(WriteOutcome::Unchanged);
            }
        }
        self.observed = None;
        clipboard.write(incoming)?;
        Ok(WriteOutcome::Written(clipboard.revision()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::{fingerprint, Changes};
    use std::collections::VecDeque;
    use std::time::Instant;

    fn owned(content: Incoming<'_>) -> Content {
        match content {
            Incoming::Text(text) => Content::Text(text.to_owned()),
            Incoming::Image(image) => Content::Image(image.clone()),
        }
    }

    struct MemoryClipboard {
        content: Content,
        revision: u64,
        counter_available: bool,
        reads: usize,
        writes: usize,
        fail_read: bool,
        fail_write: bool,
        change_during_read: bool,
    }

    impl MemoryClipboard {
        fn new(content: Incoming<'_>) -> Self {
            Self {
                content: owned(content),
                revision: 1,
                counter_available: true,
                reads: 0,
                writes: 0,
                fail_read: false,
                fail_write: false,
                change_during_read: false,
            }
        }
    }

    impl Clipboard for MemoryClipboard {
        fn revision(&self) -> Option<u64> {
            self.counter_available.then_some(self.revision)
        }

        fn read(&mut self) -> Result<Content, String> {
            self.reads += 1;
            if self.change_during_read {
                self.revision += 1;
            }
            if self.fail_read {
                Err("clipboard busy".into())
            } else {
                Ok(owned(Incoming::from(&self.content)))
            }
        }

        fn write(&mut self, incoming: Incoming<'_>) -> Result<(), String> {
            if self.fail_write {
                return Err("clipboard busy".into());
            }
            self.content = owned(incoming);
            self.revision += 1;
            self.writes += 1;
            Ok(())
        }
    }

    #[test]
    fn only_cross_platform_newlines_are_equivalent() {
        let key = |text| Incoming::Text(text).fingerprint();
        for (unix, windows) in [
            ("a\nb", "a\r\nb"),
            ("中文\n🙂\n", "中文\r\n🙂\r\n"),
            ("\n\n", "\r\n\r\n"),
            ("a\r\nb\nc", "a\nb\r\nc"),
        ] {
            assert_eq!(key(unix), key(windows));
        }
        for other in ["a", "a\r", "a\n", "a\0", " a", "a ", "a\n\n"] {
            assert_ne!(key("a\nb"), key(other));
        }
        assert_ne!(key("a"), key("a\r"));
        assert_ne!(key("a"), key("a\n"));
        assert_ne!(key("ab"), key("a\nb"));
        assert_ne!(key("a\rb"), key("a\nb"));
    }

    #[test]
    fn identical_receives_do_not_write_or_repeatedly_read_the_clipboard() {
        let mut clipboard = MemoryClipboard::new(Incoming::Text("same\ntext"));
        let mut receiver = Receiver::default();
        for _ in 0..100 {
            assert!(matches!(
                receiver.apply_with(&mut clipboard, Incoming::Text("same\r\ntext"), false),
                Ok(WriteOutcome::Unchanged)
            ));
        }
        assert_eq!((clipboard.writes, clipboard.reads), (0, 1));
        clipboard.write(Incoming::Text("same\ntext")).unwrap();
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Text("same\ntext"), false),
            Ok(WriteOutcome::Unchanged)
        ));
        assert_eq!((clipboard.writes, clipboard.reads), (1, 2));
        // A real intervening copy invalidates the cached revision.
        clipboard.write(Incoming::Text("different")).unwrap();
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Text("same\ntext"), false),
            Ok(WriteOutcome::Written(Some(4)))
        ));
        assert_eq!(clipboard.writes, 3);
    }

    #[test]
    fn unavailable_counters_and_busy_or_unstable_reads_never_use_stale_content() {
        let mut clipboard = MemoryClipboard::new(Incoming::Text("same"));
        let mut receiver = Receiver::default();
        receiver
            .apply_with(&mut clipboard, Incoming::Text("same"), false)
            .unwrap();
        clipboard.counter_available = false;
        clipboard.content = Content::Text("changed while locked".into());
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Text("same"), false),
            Ok(WriteOutcome::Written(None))
        ));
        for _ in 0..2 {
            receiver
                .apply_with(&mut clipboard, Incoming::Text("same"), false)
                .unwrap();
        }
        assert_eq!(clipboard.reads, 4);
        clipboard.counter_available = true;
        clipboard.change_during_read = true;
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Text("same"), false),
            Ok(WriteOutcome::Written(_))
        ));
        clipboard.change_during_read = false;
        clipboard.fail_read = true;
        clipboard.fail_write = true;
        assert!(receiver
            .apply_with(&mut clipboard, Incoming::Text("next"), false)
            .is_err());
        clipboard.fail_write = false;
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Text("next"), false),
            Ok(WriteOutcome::Written(_))
        ));
    }

    #[test]
    fn forced_local_copies_still_create_a_revision() {
        let mut clipboard = MemoryClipboard::new(Incoming::Text("same"));
        let mut receiver = Receiver::default();
        receiver
            .apply_with(&mut clipboard, Incoming::Text("same"), false)
            .unwrap();
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Text("same"), true),
            Ok(WriteOutcome::Written(Some(2)))
        ));
        assert_eq!(clipboard.writes, 1);
    }

    #[test]
    fn images_compare_decoded_pixels_including_dimensions_and_alpha() {
        let image = RgbaImage::from_pixel(3, 2, image::Rgba([201, 40, 19, 255]));
        let mut clipboard = MemoryClipboard::new(Incoming::Image(&image));
        let mut receiver = Receiver::default();
        for format in [
            image::ImageFormat::Png,
            image::ImageFormat::Bmp,
            image::ImageFormat::Tiff,
        ] {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let decoded = io::decode(bytes.get_ref()).unwrap();
            assert!(matches!(
                receiver.apply_with(&mut clipboard, Incoming::Image(&decoded), false),
                Ok(WriteOutcome::Unchanged)
            ));
        }
        let mut changed = image.clone();
        changed[(0, 0)][3] = 254;
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Image(&changed), false),
            Ok(WriteOutcome::Written(_))
        ));
        let reshaped = RgbaImage::from_raw(2, 3, changed.into_raw()).unwrap();
        assert!(matches!(
            receiver.apply_with(&mut clipboard, Incoming::Image(&reshaped), false),
            Ok(WriteOutcome::Written(_))
        ));
    }

    struct Device {
        clipboard: MemoryClipboard,
        receiver: Receiver,
        changes: Changes,
        written_revision: Option<u64>,
        sent: usize,
    }

    impl Device {
        fn new() -> Self {
            Self {
                clipboard: MemoryClipboard::new(Incoming::Text("initial")),
                receiver: Receiver::default(),
                changes: Changes::new(Some(1)),
                written_revision: None,
                sent: 0,
            }
        }

        fn poll(&mut self, peer: usize, queue: &mut VecDeque<(usize, bool, Content)>) {
            if self
                .changes
                .needs_read(self.clipboard.revision, self.written_revision)
                && self
                    .changes
                    .copied(fingerprint(&self.clipboard.content), Instant::now(), false)
            {
                self.sent += 1;
                queue.push_back((peer, false, owned(Incoming::from(&self.clipboard.content))));
            }
        }
    }

    #[test]
    fn parallel_remote_control_bridge_settles_and_deliberate_recopies_still_send() {
        let image = RgbaImage::from_pixel(4, 3, image::Rgba([3, 7, 11, 255]));
        for content in [Incoming::Text("remote\n文本"), Incoming::Image(&image)] {
            for bridge_first in [false, true] {
                let mut devices = [Device::new(), Device::new()];
                let mut queue = VecDeque::new();
                for _ in 0..30 {
                    let prior = devices[0].sent;
                    // User copies the same value again, with both sync tools active.
                    devices[0].clipboard.write(content).unwrap();
                    devices[0].poll(1, &mut queue);
                    assert_eq!(devices[0].sent, prior + 1);
                    let bridge = (1, true, owned(content));
                    if bridge_first {
                        queue.push_front(bridge);
                    } else {
                        queue.push_back(bridge);
                    }
                    let mut events = 0;
                    while let Some((destination, bridge, content)) = queue.pop_front() {
                        events += 1;
                        assert!(events <= 8, "clipboard feedback loop failed to settle");
                        let peer = 1 - destination;
                        let device = &mut devices[destination];
                        if bridge {
                            // The other tool drops AnyDrop's revision identity and
                            // rewrites even identical data, with OS-specific newlines.
                            let content = match content {
                                Content::Text(text) => {
                                    Content::Text(text.replace("\r\n", "\n").replace(
                                        '\n',
                                        if destination == 1 { "\r\n" } else { "\n" },
                                    ))
                                }
                                content => content,
                            };
                            device.clipboard.write(Incoming::from(&content)).unwrap();
                        } else {
                            let outcome = device
                                .receiver
                                .apply_with(&mut device.clipboard, Incoming::from(&content), false)
                                .unwrap();
                            if let WriteOutcome::Written(revision) = outcome {
                                device.written_revision = revision;
                                queue.push_back((
                                    peer,
                                    true,
                                    owned(Incoming::from(&device.clipboard.content)),
                                ));
                            }
                        }
                        device.poll(peer, &mut queue);
                    }
                }
            }
        }
    }
}
