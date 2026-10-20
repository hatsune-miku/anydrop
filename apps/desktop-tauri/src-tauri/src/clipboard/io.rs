//! Clipboard access is short lived: never retain a pasteboard connection across
//! sleep/session changes. Read only when the OS change counter advances.
use base64::Engine;
use image::{ImageReader, RgbaImage};
use std::io::{Cursor, Read};
use std::path::Path;

pub const MAX_ENCODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;

pub enum Content {
    Text(String),
    Image(RgbaImage),
}

pub fn with_pool<T>(f: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "macos")]
    {
        objc2::rc::autoreleasepool(|_| f())
    }
    #[cfg(not(target_os = "macos"))]
    {
        f()
    }
}

pub fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    if bytes.len() > MAX_ENCODED_BYTES {
        return Err("剪贴板图片过大".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    // Some decoders enforce their allocation limit before conversion to RGBA.
    if u64::from(image.width()) * u64::from(image.height()) * 4 > MAX_DECODED_BYTES {
        return Err("剪贴板图片尺寸过大".into());
    }
    Ok(image.into_rgba8())
}

pub fn encode(image: &RgbaImage) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image,
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(png)
}

fn local_image(path: &Path) -> Option<RgbaImage> {
    // A clipboard URL must not cause an HTTP or network-share fetch.
    let value = path.to_string_lossy();
    if !path.is_absolute() || value.starts_with("\\\\") || value.starts_with("//") {
        return None;
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tif" | "tiff" | "webp"
    ) {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ENCODED_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_ENCODED_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    decode(&bytes).ok()
}

/// Parse HTML instead of matching `<img>` with a regex: quoted attributes,
/// entities and fragments from Chromium/Electron are all valid inputs.
#[derive(Default)]
struct HtmlContent {
    images: Vec<String>,
    text: String,
    hidden_depth: usize,
}

fn html_content(html: &str) -> HtmlContent {
    use html5ever::tendril::StrTendril;
    use html5ever::tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
    };
    use std::cell::RefCell;
    struct Sink(RefCell<HtmlContent>);
    impl TokenSink for Sink {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            let mut out = self.0.borrow_mut();
            match token {
                Token::TagToken(tag) => {
                    let name = tag.name.as_ref();
                    if matches!(name, "script" | "style") {
                        if tag.kind == TagKind::StartTag {
                            out.hidden_depth += 1;
                        } else {
                            out.hidden_depth = out.hidden_depth.saturating_sub(1);
                        }
                    } else if out.hidden_depth == 0 {
                        if name == "img" && tag.kind == TagKind::StartTag {
                            if let Some(src) =
                                tag.attrs.iter().find(|a| a.name.local.as_ref() == "src")
                            {
                                out.images.push(src.value.to_string());
                            }
                        }
                        if matches!(name, "br" | "p" | "div" | "li" | "tr")
                            && !out.text.ends_with('\n')
                        {
                            out.text.push('\n');
                        }
                    }
                }
                Token::CharacterTokens(text) if out.hidden_depth == 0 => out.text.push_str(&text),
                _ => {}
            }
            TokenSinkResult::Continue
        }
    }
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from(html));
    let tokenizer = Tokenizer::new(
        Sink(RefCell::new(HtmlContent::default())),
        Default::default(),
    );
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    tokenizer.sink.0.into_inner()
}

fn image_source(src: &str) -> Option<RgbaImage> {
    if let Some(data) = src.strip_prefix("data:image/") {
        let (header, data) = data.split_once(',')?;
        if !header.ends_with(";base64") || data.len() > MAX_ENCODED_BYTES * 4 / 3 + 4 {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .ok()?;
        return decode(&bytes).ok();
    }
    let url = url::Url::parse(src).ok()?;
    if url.scheme() != "file" || url.host_str().is_some_and(|h| h != "localhost") {
        return None;
    }
    local_image(&url.to_file_path().ok()?)
}

pub fn read() -> Result<Content, String> {
    with_pool(read_inner)
}

fn read_inner() -> Result<Content, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    // Native image formats take precedence over their textual fallback.
    if let Some(image) = native_image() {
        return Ok(Content::Image(image));
    }
    if let Ok(paths) = clipboard.get().file_list() {
        if paths.len() == 1 {
            if let Some(image) = local_image(&paths[0]) {
                return Ok(Content::Image(image));
            }
        }
    }
    let text = clipboard.get_text().ok().filter(|text| !text.is_empty());
    let html = clipboard
        .get()
        .html()
        .ok()
        .filter(|html| html.len() <= MAX_ENCODED_BYTES);
    if let Some(html) = html {
        let parsed = html_content(&html);
        if parsed.images.len() == 1 && parsed.text.trim().is_empty() {
            if let Some(image) = image_source(&parsed.images[0]) {
                return Ok(Content::Image(image));
            }
        }
        if text.is_none() && !parsed.text.trim().is_empty() {
            return Ok(Content::Text(parsed.text.trim().to_owned()));
        }
    }
    text.map(Content::Text)
        .ok_or_else(|| "剪贴板没有可读取的文本或图片".into())
}

/// CF_DIB and CF_DIBV5 have a BMP info header but no 14-byte file header.
/// Include palette entries and external bit masks in the pixel offset.
#[cfg(any(windows, test))]
fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |offset| {
        Some(u32::from_le_bytes(
            dib.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let header = u32_at(0)? as usize;
    if !matches!(header, 40 | 52 | 56 | 108 | 124)
        || dib.len() < header
        || dib.len() > MAX_ENCODED_BYTES
    {
        return None;
    }
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    if !matches!(bits, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    let colors = u32_at(32)? as usize;
    let palette = if colors > 0 {
        colors
    } else if bits <= 8 {
        1usize << bits
    } else {
        0
    };
    let masks = if header == 40 {
        match u32_at(16)? {
            3 => 12,
            6 => 16,
            _ => 0,
        }
    } else {
        0
    };
    let offset = header
        .checked_add(masks)?
        .checked_add(palette.checked_mul(4)?)?;
    if offset >= dib.len() {
        return None;
    }
    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&((14 + offset) as u32).to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

#[cfg(windows)]
pub fn revision() -> Option<u64> {
    let value = unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
    // Zero means unavailable (e.g. a locked/switched desktop); retry later.
    (value != 0).then_some(u64::from(value))
}

#[cfg(windows)]
fn native_image() -> Option<RgbaImage> {
    use windows_sys::Win32::System::{DataExchange::*, Memory::*};
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let _open = Open;
        let mut formats = Vec::new();
        for name in [
            "PNG",
            "image/png",
            "JFIF",
            "image/jpeg",
            "image/webp",
            "GIF",
            "image/gif",
            "TIFF",
            "image/tiff",
            "image/bmp",
        ] {
            let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            formats.push((RegisterClipboardFormatW(name.as_ptr()), false));
        }
        formats.extend([(17, true), (8, true)]); // CF_DIBV5, CF_DIB (including synthesized CF_BITMAP)
        for (format, dib) in formats {
            if format == 0 || IsClipboardFormatAvailable(format) == 0 {
                continue;
            }
            let handle = GetClipboardData(format);
            if handle.is_null() {
                continue;
            }
            let len = GlobalSize(handle);
            if len == 0 || len > MAX_ENCODED_BYTES {
                continue;
            }
            let ptr = GlobalLock(handle) as *const u8;
            if ptr.is_null() {
                continue;
            }
            let bytes = std::slice::from_raw_parts(ptr, len);
            let result = if dib {
                dib_to_bmp(bytes).and_then(|bmp| decode(&bmp).ok())
            } else {
                decode(bytes).ok()
            };
            GlobalUnlock(handle);
            if result.is_some() {
                return result;
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn revision() -> Option<u64> {
    use objc2::{
        msg_send,
        rc::{autoreleasepool, Retained},
        ClassType,
    };
    use objc2_app_kit::NSPasteboard;
    autoreleasepool(|_| unsafe {
        let board: Option<Retained<NSPasteboard>> =
            msg_send![NSPasteboard::class(), generalPasteboard];
        board.map(|b| b.changeCount() as u64)
    })
}

#[cfg(target_os = "macos")]
fn native_image() -> Option<RgbaImage> {
    use objc2::{
        msg_send,
        rc::{autoreleasepool, Retained},
        ClassType,
    };
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    autoreleasepool(|_| unsafe {
        let board: Option<Retained<NSPasteboard>> =
            msg_send![NSPasteboard::class(), generalPasteboard];
        let board = board?;
        for name in [
            "public.png",
            "public.tiff",
            "public.jpeg",
            "com.compuserve.gif",
            "com.microsoft.bmp",
            "org.webmproject.webp",
        ] {
            if let Some(data) = board.dataForType(&NSString::from_str(name)) {
                if let Ok(image) = decode(data.as_bytes_unchecked()) {
                    return Some(image);
                }
            }
        }
        None
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn revision() -> Option<u64> {
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
fn native_image() -> Option<RgbaImage> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image = clipboard.get_image().ok()?;
    if image.bytes.len() as u64 > MAX_DECODED_BYTES {
        return None;
    }
    RgbaImage::from_raw(
        image.width.try_into().ok()?,
        image.height.try_into().ok()?,
        image.bytes.into_owned(),
    )
}

/// Format names only: diagnostic logs must never contain clipboard contents.
pub fn format_names() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::{
            msg_send,
            rc::{autoreleasepool, Retained},
            ClassType,
        };
        use objc2_app_kit::NSPasteboard;
        return autoreleasepool(|_| unsafe {
            let board: Option<Retained<NSPasteboard>> =
                msg_send![NSPasteboard::class(), generalPasteboard];
            board
                .and_then(|board| board.types())
                .map(|types| types.iter().take(32).map(|name| name.to_string()).collect())
                .unwrap_or_default()
        });
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::DataExchange::*;
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return vec!["busy".into()];
        }
        let mut names = Vec::new();
        let mut format = 0;
        loop {
            format = EnumClipboardFormats(format);
            if format == 0 || names.len() >= 32 {
                break;
            }
            let mut name = [0u16; 128];
            let len = GetClipboardFormatNameW(format, name.as_mut_ptr(), name.len() as i32);
            names.push(if len > 0 {
                String::from_utf16_lossy(&name[..len as usize])
            } else {
                format!("CF_{format}")
            });
        }
        CloseClipboard();
        return names;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    Vec::new()
}

#[cfg(all(test, target_os = "macos"))]
pub(super) fn publish_encoded(format: &str, bytes: &[u8]) {
    use objc2::{
        msg_send,
        rc::{autoreleasepool, Retained},
        ClassType,
    };
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSData, NSString};
    autoreleasepool(|_| unsafe {
        let board: Retained<NSPasteboard> = msg_send![NSPasteboard::class(), generalPasteboard];
        board.clearContents();
        assert!(board.setData_forType(
            Some(&NSData::with_bytes(bytes)),
            &NSString::from_str(format)
        ));
    });
}

#[cfg(all(test, windows))]
pub(super) fn publish_encoded(format: &str, bytes: &[u8]) {
    use windows_sys::Win32::{
        Foundation::*,
        System::{DataExchange::*, Memory::*},
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        assert!(!window.is_null());
        struct Guard(HWND);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    CloseClipboard();
                    DestroyWindow(self.0);
                }
            }
        }
        let _guard = Guard(window);
        assert_ne!(OpenClipboard(window), 0);
        assert_ne!(EmptyClipboard(), 0);
        let format = if format == "CF_DIB" {
            8
        } else {
            let name: Vec<u16> = format.encode_utf16().chain(Some(0)).collect();
            RegisterClipboardFormatW(name.as_ptr())
        };
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        assert!(!memory.is_null());
        let pointer = GlobalLock(memory);
        assert!(!pointer.is_null());
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
        GlobalUnlock(memory);
        if SetClipboardData(format, memory).is_null() {
            GlobalFree(memory);
            panic!("SetClipboardData failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_native_and_chat_image_representations() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3,
            2,
            image::Rgb([255, 42, 9]),
        ));
        for format in [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::Tiff,
            image::ImageFormat::Bmp,
            image::ImageFormat::Gif,
            image::ImageFormat::WebP,
        ] {
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            assert_eq!(
                decode(bytes.get_ref()).unwrap().dimensions(),
                (3, 2),
                "{format:?}"
            );
        }
    }

    #[test]
    fn decodes_classic_dib_with_padding_palette_and_bit_masks() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3,
            2,
            image::Rgb([255, 42, 9]),
        ));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Bmp).unwrap();
        let bmp = dib_to_bmp(&bytes.get_ref()[14..]).unwrap();
        assert_eq!(decode(&bmp).unwrap().get_pixel(2, 1).0, [255, 42, 9, 255]);
        // A 1x1, 8-bit indexed DIB with two palette entries and a padded row.
        let mut dib = vec![0; 52];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1u32.to_le_bytes());
        dib[8..12].copy_from_slice(&1u32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&8u16.to_le_bytes());
        dib[32..36].copy_from_slice(&2u32.to_le_bytes());
        dib[44..48].copy_from_slice(&[0, 0, 255, 0]);
        dib[48] = 1;
        assert_eq!(
            decode(&dib_to_bmp(&dib).unwrap())
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [255, 0, 0, 255]
        );
        dib[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(dib_to_bmp(&dib).is_none());
        // 16-bit RGB565 with external bit masks and a top-down scanline.
        let mut dib = vec![0; 56];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&16u16.to_le_bytes());
        dib[16..20].copy_from_slice(&3u32.to_le_bytes());
        dib[40..44].copy_from_slice(&0xf800u32.to_le_bytes());
        dib[44..48].copy_from_slice(&0x07e0u32.to_le_bytes());
        dib[48..52].copy_from_slice(&0x001fu32.to_le_bytes());
        dib[52..54].copy_from_slice(&0xf800u16.to_le_bytes());
        assert_eq!(
            decode(&dib_to_bmp(&dib).unwrap())
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [255, 0, 0, 255]
        );
        assert!(decode(b"broken image").is_err());
    }

    #[test]
    fn oversized_decoded_dimensions_are_rejected() {
        let bytes = encode(&RgbaImage::new(16385, 1)).unwrap();
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn html_images_and_text_without_remote_fetches() {
        let png = encode(&RgbaImage::from_pixel(2, 3, image::Rgba([1, 2, 3, 255]))).unwrap();
        let src = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        let html = html_content(&format!(
            "<html><body><!--StartFragment--><IMG SRC='{src}'><!--EndFragment--></body>"
        ));
        assert_eq!(image_source(&html.images[0]).unwrap().dimensions(), (2, 3));
        assert!(html.text.trim().is_empty());
        let text =
            html_content("<p>你好 &amp; hello</p><style>hidden</style><script>secret</script>");
        assert_eq!(text.text.trim(), "你好 & hello");
        assert!(image_source("https://example.invalid/image.png").is_none());
        assert!(image_source("file://server/share/image.png").is_none());
    }

    #[test]
    fn reads_only_single_local_image_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("聊天 图片.png");
        std::fs::write(&path, encode(&RgbaImage::new(2, 2)).unwrap()).unwrap();
        assert_eq!(local_image(&path).unwrap().dimensions(), (2, 2));
        let url = url::Url::from_file_path(&path).unwrap();
        assert!(image_source(url.as_str()).is_some());
        assert!(local_image(dir.path()).is_none());
        assert!(local_image(Path::new("relative.png")).is_none());
    }
}
