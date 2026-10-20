//! Clipboard receipts survive a late/reloaded WebView. The backend retains
//! each one until React confirms it was rendered, with bounded memory/age.
use super::{ensure_receive_window, now_ms, AppHandle, Backend, RECEIVE_WINDOW_LABEL};
use serde::Serialize;
use std::collections::VecDeque;
use tauri::{Emitter, Manager, State};

#[derive(Clone, Serialize)]
pub struct Receipt {
    id: u64,
    #[serde(flatten)]
    content: serde_json::Value,
}

struct Pending {
    receipt: Receipt,
    queued_at: u64,
}

#[derive(Default)]
pub struct Receipts {
    next_id: u64,
    pending: VecDeque<Pending>,
    last_recovery: Option<u64>,
    view_requested_at: Option<u64>,
    recovery_attempts: u32,
}

impl Receipts {
    fn expire(&mut self, now: u64) {
        self.pending
            .retain(|item| now.saturating_sub(item.queued_at) < 120_000);
    }

    fn push(&mut self, content: serde_json::Value, now: u64) -> Receipt {
        self.expire(now);
        self.next_id += 1;
        let receipt = Receipt {
            id: self.next_id,
            content,
        };
        self.pending.push_back(Pending {
            receipt: receipt.clone(),
            queued_at: now,
        });
        while self.pending.len() > 4 {
            self.pending.pop_front();
        }
        receipt
    }

    fn acknowledge(&mut self, ids: &[u64]) {
        self.pending.retain(|item| !ids.contains(&item.receipt.id));
    }

    fn needs_recovery(&mut self, now: u64) -> bool {
        self.expire(now);
        let receipt_waiting = self
            .pending
            .front()
            .is_some_and(|item| now.saturating_sub(item.queued_at) >= 2500);
        let view_waiting = self
            .view_requested_at
            .is_some_and(|at| now.saturating_sub(at) >= 2500);
        if (receipt_waiting || view_waiting)
            && self
                .last_recovery
                .is_none_or(|at| now.saturating_sub(at) >= 30_000)
        {
            self.last_recovery = Some(now);
            self.recovery_attempts += 1;
            return true;
        }
        false
    }
}

pub fn request_view(app: &AppHandle) {
    if let Some(backend) = app.try_state::<Backend>() {
        backend
            .receipts
            .lock()
            .unwrap()
            .view_requested_at
            .get_or_insert(now_ms());
    }
}

pub fn show(app: &AppHandle, content: serde_json::Value) {
    let Some(backend) = app.try_state::<Backend>() else {
        return;
    };
    let receipt = backend.receipts.lock().unwrap().push(content, now_ms());
    ensure_receive_window(app);
    let _ = app.emit_to(RECEIVE_WINDOW_LABEL, "clipboard-popup", receipt);
}

#[tauri::command]
pub fn get_clipboard_receipts(backend: State<'_, Backend>) -> Vec<Receipt> {
    let mut receipts = backend.receipts.lock().unwrap();
    receipts.expire(now_ms());
    receipts
        .pending
        .iter()
        .map(|item| item.receipt.clone())
        .collect()
}

#[tauri::command]
pub fn acknowledge_clipboard_receipts(backend: State<'_, Backend>, ids: Vec<u64>) {
    backend.receipts.lock().unwrap().acknowledge(&ids);
}

#[tauri::command]
pub fn acknowledge_receive_view(window: tauri::WebviewWindow, backend: State<'_, Backend>) {
    if window.label() == RECEIVE_WINDOW_LABEL {
        let mut receipts = backend.receipts.lock().unwrap();
        receipts.view_requested_at = None;
        receipts.recovery_attempts = 0;
    }
}

pub fn recover_if_needed(app: &AppHandle) {
    let Some(backend) = app.try_state::<Backend>() else {
        return;
    };
    let mut receipts = backend.receipts.lock().unwrap();
    receipts.expire(now_ms());
    if app.get_webview_window(RECEIVE_WINDOW_LABEL).is_none()
        && (!receipts.pending.is_empty() || receipts.view_requested_at.is_some())
    {
        drop(receipts);
        ensure_receive_window(app);
        return;
    }
    if !receipts.needs_recovery(now_ms()) {
        return;
    }
    let recreate = receipts.recovery_attempts > 1;
    drop(receipts);
    backend.log("Receipt view did not acknowledge display; restoring it");
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(backend) = app.try_state::<Backend>() {
            let receipts = backend.receipts.lock().unwrap();
            // An acknowledgement may arrive while this UI task is queued.
            if receipts.pending.is_empty() && receipts.view_requested_at.is_none() {
                return;
            }
        }
        if let Some(window) = app.get_webview_window(RECEIVE_WINDOW_LABEL) {
            super::show_receive_window(&window);
            // reload() recovers a failed renderer and re-runs subscription +
            // pending-state hydration. It doesn't clear backend file offers.
            if recreate || window.reload().is_err() {
                // A dead WebView2 browser process cannot be fixed by Reload.
                // Destroy is asynchronous; the next watchdog tick recreates
                // the missing window after Tauri has released its label.
                let _ = window.destroy();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_events_survive_reload_and_repeated_content_is_distinct() {
        let mut queue = Receipts::default();
        let a = queue.push(serde_json::json!({"preview": "same"}), 100);
        let b = queue.push(serde_json::json!({"preview": "same"}), 200);
        assert_ne!(a.id, b.id);
        assert!(!queue.needs_recovery(2000));
        assert!(queue.needs_recovery(2700));
        assert!(!queue.needs_recovery(2800));
        assert_eq!(queue.recovery_attempts, 1);
        assert!(queue.needs_recovery(33_000));
        assert_eq!(queue.recovery_attempts, 2);
        queue.acknowledge(&[a.id]);
        assert_eq!(queue.pending.front().unwrap().receipt.id, b.id);
        queue.acknowledge(&[b.id]);
        assert!(!queue.needs_recovery(50_000));
    }

    #[test]
    fn backlog_and_recovery_are_bounded_across_overnight_sleep() {
        let mut queue = Receipts::default();
        for now in 1..100 {
            queue.push(serde_json::json!({"kind": "image"}), now);
        }
        assert_eq!(queue.pending.len(), 4);
        assert!(!queue.needs_recovery(12 * 60 * 60 * 1000));
        assert!(queue.pending.is_empty());
        queue.push(
            serde_json::json!({"kind": "text"}),
            17 * 24 * 60 * 60 * 1000,
        );
        assert!(queue.needs_recovery(17 * 24 * 60 * 60 * 1000 + 3000));
    }

    #[test]
    fn pending_file_confirmations_also_recover_an_unresponsive_view() {
        let mut queue = Receipts::default();
        queue.view_requested_at = Some(1000);
        assert!(queue.needs_recovery(4000));
        queue.view_requested_at = None;
        assert!(!queue.needs_recovery(40_000));
    }
}
