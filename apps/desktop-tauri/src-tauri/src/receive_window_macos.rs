//! Keep receipt clicks independent of application activation on macOS.
//!
//! `accept_first_mouse` forwards the first click but does not prevent activation.
//! Hiding an activated receipt can then bring AnyDrop's main window forward.
//! Preserve Tao's class, delegate, ivars and WebKit's KVO subclass; changing the
//! class of a live, observed window breaks AppKit tracking-area bookkeeping.

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{class, ffi, msg_send, sel, MainThreadMarker};
use std::sync::OnceLock;

type Predicate = unsafe extern "C-unwind" fn(&AnyObject, Sel) -> Bool;
type Show = unsafe extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject);
struct OriginalMethods {
    nonactivating: Predicate,
    show: Show,
}
static ORIGINAL: OnceLock<OriginalMethods> = OnceLock::new();
static RECEIPT_MARKER: u8 = 0;

unsafe fn is_receipt(window: &AnyObject) -> bool {
    !ffi::objc_getAssociatedObject(
        window as *const _ as *mut _,
        (&RECEIPT_MARKER as *const u8).cast(),
    )
    .is_null()
}

unsafe extern "C-unwind" fn nonactivating(window: &AnyObject, selector: Sel) -> Bool {
    if is_receipt(window) {
        Bool::YES
    } else {
        (ORIGINAL.get().unwrap().nonactivating)(window, selector)
    }
}

// Both Rust Window.show() and webview Window.show() use this selector. Only
// receipts order forward without taking keyboard focus; other windows delegate.
unsafe extern "C-unwind" fn show(window: &AnyObject, selector: Sel, sender: *mut AnyObject) {
    if is_receipt(window) {
        let _: () = msg_send![window, orderFrontRegardless];
    } else {
        (ORIGINAL.get().unwrap().show)(window, selector, sender);
    }
}

pub fn configure(window: &tauri::WebviewWindow) -> Result<(), String> {
    let _main_thread = MainThreadMarker::new().ok_or("popup setup requires the main thread")?;
    // Tao uses the same per-window flag for canBecomeKeyWindow/canBecomeMainWindow.
    window
        .set_focusable(false)
        .map_err(|error| error.to_string())?;
    let pointer = window.ns_window().map_err(|error| error.to_string())?;
    // SAFETY: Tauri owns this live, hidden NSWindow; all access and method
    // installation happen on the main thread before the receipt is first shown.
    unsafe {
        let native = &*pointer.cast::<AnyObject>();
        let tao = AnyClass::get(c"TaoWindow").ok_or("Tao window class unavailable")?;
        let compatible: Bool = msg_send![native, isKindOfClass: tao];
        let supports_tag: Bool =
            msg_send![native, respondsToSelector: sel!(_setPreventsActivation:)];
        if !compatible.as_bool() || !supports_tag.as_bool() {
            return Err("unsupported native receipt window".into());
        }
        if ORIGINAL.get().is_none() {
            let predicate = tao
                .instance_method(sel!(_isNonactivatingPanel))
                .ok_or("activation predicate unavailable")?;
            let ordering = tao
                .instance_method(sel!(makeKeyAndOrderFront:))
                .ok_or("window ordering unavailable")?;
            ORIGINAL
                .set(OriginalMethods {
                    nonactivating: std::mem::transmute::<Imp, Predicate>(
                        predicate.implementation(),
                    ),
                    show: std::mem::transmute::<Imp, Show>(ordering.implementation()),
                })
                .ok()
                .expect("native hooks installed once on the main thread");
            // Add overrides to TaoWindow, never to NSWindow or WebKit's KVO class.
            // Unmarked windows call their original implementations unchanged.
            ffi::class_replaceMethod(
                tao as *const _ as *mut _,
                sel!(_isNonactivatingPanel),
                std::mem::transmute::<Predicate, Imp>(nonactivating),
                ffi::method_getTypeEncoding(predicate),
            );
            ffi::class_replaceMethod(
                tao as *const _ as *mut _,
                sel!(makeKeyAndOrderFront:),
                std::mem::transmute::<Show, Imp>(show),
                ffi::method_getTypeEncoding(ordering),
            );
        }
        let marker: *mut AnyObject = msg_send![class!(NSNull), null];
        ffi::objc_setAssociatedObject(
            pointer.cast(),
            (&RECEIPT_MARKER as *const u8).cast(),
            marker,
            ffi::OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        );
        // AppKit SPI: the predicate is also used by Chromium's native NSWindows.
        // Keep WindowServer's activation tag consistent with the AppKit predicate.
        // https://github.com/chromium/chromium/blob/main/components/remote_cocoa/app_shim/native_widget_mac_nswindow.mm
        let _: () = msg_send![native, _setPreventsActivation: Bool::YES];
        let _: () = msg_send![native, setHidesOnDeactivate: Bool::NO];
    }
    Ok(())
}
