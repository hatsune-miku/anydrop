//! Bounded, non-consuming event history shared by HTTP and MCP waiters.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{watch, Semaphore};
use tokio_util::sync::CancellationToken;

const MAX_EVENTS: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_WAITERS: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    FileOffered,
    FileReceived,
    ClipboardText,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub cursor: String,
    pub kind: Kind,
    pub received_at: u64,
    pub data: Value,
    #[serde(skip)]
    sequence: u64,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaitOptions {
    /// Resume after an event cursor or a cursor returned by get_event_cursor.
    /// When omitted, only events arriving after this call starts are eligible.
    pub after: Option<String>,
    /// Default 60000 ms. Zero waits until an event, cancellation, or app exit.
    /// Nonzero values may be at most 86400000 ms (24 hours).
    pub timeout_ms: Option<u64>,
    /// Optional exact peer string as returned in an event or transfer.
    pub peer: Option<String>,
    /// Optional transfer key; useful for waiting for one accepted file offer.
    pub transfer_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitResult {
    pub timed_out: bool,
    pub cursor: String,
    pub event: Option<Event>,
}

#[derive(Default)]
struct Journal {
    next: u64,
    dropped_through: u64,
    bytes: usize,
    events: VecDeque<(Arc<Event>, usize)>,
}

pub struct Events {
    boot: String,
    journal: Mutex<Journal>,
    changed: watch::Sender<u64>,
    waiters: Semaphore,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            boot: format!("{:032x}", rand::random::<u128>()),
            journal: Mutex::new(Journal::default()),
            changed: watch::channel(0).0,
            waiters: Semaphore::new(MAX_WAITERS),
        }
    }
}

impl Events {
    fn cursor_at(&self, sequence: u64) -> String {
        format!("{}:{sequence}", self.boot)
    }

    pub fn cursor(&self) -> String {
        self.cursor_at(self.journal.lock().unwrap().next)
    }

    fn parse_cursor(&self, cursor: &str) -> Result<u64, String> {
        let (boot, number) = cursor.split_once(':').ok_or("invalid_event_cursor")?;
        if boot != self.boot {
            return Err("event_cursor_from_previous_app_session".into());
        }
        number.parse().map_err(|_| "invalid_event_cursor".into())
    }

    pub fn publish(&self, kind: Kind, data: Value) {
        let size = data.to_string().len();
        let mut journal = self.journal.lock().unwrap();
        journal.next += 1;
        let sequence = journal.next;
        let event = Arc::new(Event {
            cursor: self.cursor_at(sequence),
            kind,
            received_at: crate::now_ms(),
            data,
            sequence,
        });
        journal.bytes += size;
        journal.events.push_back((event, size));
        while journal.events.len() > MAX_EVENTS || journal.bytes > MAX_BYTES {
            if let Some((old, size)) = journal.events.pop_front() {
                journal.bytes -= size;
                journal.dropped_through = old.sequence;
            }
        }
        // Publish under the journal lock so concurrent callbacks cannot send an
        // older watch revision after a newer one. No caller consumes an event.
        self.changed.send_replace(sequence);
    }

    pub async fn wait(
        &self,
        kind: Kind,
        options: WaitOptions,
        cancellation: CancellationToken,
    ) -> Result<WaitResult, String> {
        let timeout = options.timeout_ms.unwrap_or(60_000);
        if timeout > 86_400_000 {
            return Err("timeoutMs must be between 0 and 86400000".into());
        }
        let _permit = self
            .waiters
            .try_acquire()
            .map_err(|_| "too_many_event_waiters")?;
        let mut changed = self.changed.subscribe();
        let mut sequence = match options.after.as_deref() {
            Some(cursor) => self.parse_cursor(cursor)?,
            None => self.journal.lock().unwrap().next,
        };
        let deadline = tokio::time::sleep(Duration::from_millis(timeout));
        tokio::pin!(deadline);
        let mut timed_out = false;
        loop {
            {
                let journal = self.journal.lock().unwrap();
                if sequence < journal.dropped_through {
                    return Err(
                        "event_cursor_expired; get a new cursor before waiting again".into(),
                    );
                }
                if sequence > journal.next {
                    return Err("event_cursor_is_in_the_future".into());
                }
                if let Some((event, _)) = journal.events.iter().find(|(event, _)| {
                    event.sequence > sequence
                        && event.kind == kind
                        && options
                            .peer
                            .as_deref()
                            .is_none_or(|peer| event.data["peer"].as_str() == Some(peer))
                        && options
                            .transfer_key
                            .as_deref()
                            .is_none_or(|key| event.data["transferKey"].as_str() == Some(key))
                }) {
                    return Ok(WaitResult {
                        timed_out: false,
                        cursor: event.cursor.clone(),
                        event: Some((**event).clone()),
                    });
                }
                sequence = journal.next;
            }
            if timed_out {
                return Ok(WaitResult {
                    timed_out: true,
                    cursor: self.cursor_at(sequence),
                    event: None,
                });
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err("request_cancelled".into()),
                _ = &mut deadline, if timeout != 0 => timed_out = true,
                result = changed.changed() => { result.map_err(|_| "event_service_closed")?; },
            }
            // Re-read before returning a timeout, so its resume cursor never
            // skips an event that raced with the deadline.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn waits_for_new_events_and_all_clients_receive_the_same_event() {
        let events = Arc::new(Events::default());
        events.publish(Kind::ClipboardText, json!({"text":"old"}));
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let events = events.clone();
            tasks.push(tokio::spawn(async move {
                events
                    .wait(
                        Kind::ClipboardText,
                        WaitOptions::default(),
                        CancellationToken::new(),
                    )
                    .await
                    .unwrap()
            }));
        }
        while events.waiters.available_permits() != MAX_WAITERS - 2 {
            tokio::task::yield_now().await;
        }
        events.publish(Kind::FileOffered, json!({"transferKey":"1"}));
        events.publish(Kind::ClipboardText, json!({"text":"新内容"}));
        for task in tasks {
            assert_eq!(task.await.unwrap().event.unwrap().data["text"], "新内容");
        }
        assert_eq!(events.waiters.available_permits(), MAX_WAITERS);
    }

    #[tokio::test]
    async fn cursor_replay_timeout_and_identical_text_are_lossless() {
        let events = Events::default();
        let before = events.cursor();
        events.publish(Kind::ClipboardText, json!({"text":"same"}));
        events.publish(Kind::ClipboardText, json!({"text":"same"}));
        let first = events
            .wait(
                Kind::ClipboardText,
                WaitOptions {
                    after: Some(before),
                    ..Default::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let second = events
            .wait(
                Kind::ClipboardText,
                WaitOptions {
                    after: Some(first.cursor.clone()),
                    ..Default::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(first.cursor, second.cursor);
        let timeout = events
            .wait(
                Kind::FileReceived,
                WaitOptions {
                    after: Some(second.cursor),
                    timeout_ms: Some(1),
                    ..Default::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(timeout.timed_out && timeout.event.is_none());
        events.publish(
            Kind::FileReceived,
            json!({"transferKey":"2", "localPath":"/received/file"}),
        );
        let file = events
            .wait(
                Kind::FileReceived,
                WaitOptions {
                    after: Some(timeout.cursor),
                    ..Default::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(file.event.unwrap().data["localPath"], "/received/file");
    }

    #[tokio::test]
    async fn cancellation_releases_waiters_and_stale_cursors_fail_explicitly() {
        let events = Events::default();
        let stale = events.cursor();
        for _ in 0..=MAX_EVENTS {
            events.publish(Kind::ClipboardText, json!({"text":"x"}));
        }
        assert!(events
            .wait(
                Kind::ClipboardText,
                WaitOptions {
                    after: Some(stale),
                    ..Default::default()
                },
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .contains("expired"));
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            events
                .wait(
                    Kind::ClipboardText,
                    WaitOptions {
                        timeout_ms: Some(0),
                        ..Default::default()
                    },
                    token
                )
                .await
                .unwrap_err(),
            "request_cancelled"
        );
        assert_eq!(events.waiters.available_permits(), MAX_WAITERS);
        assert!(events
            .parse_cursor(&Events::default().cursor())
            .unwrap_err()
            .contains("previous"));
    }
}
