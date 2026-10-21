use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::error::Error;
use crate::event::Event;

/// Events pushed by mpv, in order. The stream ends when the connection closes.
pub type Events = mpsc::UnboundedReceiver<Event>;

type Reply = Result<Value, Error>;

#[derive(Default)]
struct Pending {
    waiting: HashMap<u64, oneshot::Sender<Reply>>,
    /// Set once the connection is gone, so new requests fail instead of waiting forever.
    closed: bool,
}

/// A connection to one mpv instance. Cheap to share behind an `Arc`; requests may run
/// concurrently and are matched to replies by `request_id`.
pub struct Client {
    lines: mpsc::UnboundedSender<String>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicU64,
}

impl Client {
    /// Takes over a connected stream. Must be called inside a Tokio runtime: it spawns one
    /// task that writes requests and one that reads replies and events.
    pub fn new<S>(stream: S) -> (Client, Events)
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let pending = Arc::new(Mutex::new(Pending::default()));

        tokio::spawn(async move {
            while let Some(line) = line_rx.recv().await {
                if let Err(err) = writer.write_all(line.as_bytes()).await {
                    log::debug!("mpv IPC write failed: {err}");
                    break;
                }
            }
            // The client was dropped. The reader task still holds the other half of the stream,
            // so close our direction explicitly: mpv sees the end of the connection.
            let _ = writer.shutdown().await;
        });

        let reader_pending = Arc::clone(&pending);
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => dispatch(&line, &reader_pending, &event_tx),
                    Ok(None) => break,
                    Err(err) => {
                        log::debug!("mpv IPC read failed: {err}");
                        break;
                    }
                }
            }
            let mut pending = lock(&reader_pending);
            pending.closed = true;
            for (_, reply) in pending.waiting.drain() {
                let _ = reply.send(Err(Error::Disconnected));
            }
            // Dropping `event_tx` here ends the event stream.
        });

        let client = Client {
            lines: line_tx,
            pending,
            next_id: AtomicU64::new(1),
        };
        (client, event_rx)
    }

    /// Runs an mpv command (`["loadfile", "/a.flac"]`, ...) and returns its `data`.
    pub async fn command(&self, args: Vec<Value>) -> Result<Value, Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        {
            let mut pending = lock(&self.pending);
            if pending.closed {
                return Err(Error::Disconnected);
            }
            pending.waiting.insert(id, reply_tx);
        }

        let mut line = serde_json::to_string(&json!({ "command": args, "request_id": id }))?;
        line.push('\n');
        if self.lines.send(line).is_err() {
            lock(&self.pending).waiting.remove(&id);
            return Err(Error::Disconnected);
        }
        reply_rx.await.unwrap_or(Err(Error::Disconnected))
    }

    pub async fn get_property<T: DeserializeOwned>(&self, name: &str) -> Result<T, Error> {
        let value = self
            .command(vec![json!("get_property"), json!(name)])
            .await?;
        Ok(serde_json::from_value(value)?)
    }

    pub async fn set_property(&self, name: &str, value: impl Serialize) -> Result<(), Error> {
        let value = serde_json::to_value(value)?;
        self.command(vec![json!("set_property"), json!(name), value])
            .await
            .map(drop)
    }

    /// Asks mpv to send `property-change` events for `name`, tagged with `id`.
    pub async fn observe_property(&self, id: u64, name: &str) -> Result<(), Error> {
        self.command(vec![json!("observe_property"), json!(id), json!(name)])
            .await
            .map(drop)
    }
}

fn lock(pending: &Mutex<Pending>) -> MutexGuard<'_, Pending> {
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn dispatch(line: &str, pending: &Mutex<Pending>, events: &mpsc::UnboundedSender<Event>) {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(err) => {
            log::warn!("ignoring malformed mpv message ({err}): {line}");
            return;
        }
    };

    // Check for an event first: some mpv versions add `"request_id": 0` to events.
    if message.get("event").is_some() {
        if let Some(event) = Event::from_value(message) {
            let _ = events.send(event);
        }
        return;
    }

    let Some(id) = message.get("request_id").and_then(Value::as_u64) else {
        log::debug!("ignoring mpv message without request id: {line}");
        return;
    };
    let reply = match message.get("error").and_then(Value::as_str) {
        Some("success") => Ok(message.get("data").cloned().unwrap_or(Value::Null)),
        Some(error) => Err(Error::Mpv(error.to_owned())),
        None => Err(Error::Mpv("reply without status".to_owned())),
    };
    if let Some(waiting) = lock(pending).waiting.remove(&id) {
        let _ = waiting.send(reply);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EndReason;
    use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

    /// The mpv side of an in-memory connection.
    struct FakeMpv {
        requests: tokio::io::Lines<BufReader<ReadHalf<DuplexStream>>>,
        writer: WriteHalf<DuplexStream>,
    }

    impl FakeMpv {
        fn connect() -> (Client, Events, FakeMpv) {
            let (client_side, mpv_side) = tokio::io::duplex(64 * 1024);
            let (client, events) = Client::new(client_side);
            let (reader, writer) = tokio::io::split(mpv_side);
            let fake = FakeMpv {
                requests: BufReader::new(reader).lines(),
                writer,
            };
            (client, events, fake)
        }

        async fn next_request(&mut self) -> Value {
            let line = self.requests.next_line().await.unwrap().unwrap();
            serde_json::from_str(&line).unwrap()
        }

        async fn send(&mut self, message: Value) {
            let mut line = message.to_string();
            line.push('\n');
            self.writer.write_all(line.as_bytes()).await.unwrap();
        }

        async fn reply(&mut self, request: &Value, data: Value) {
            let id = request["request_id"].clone();
            self.send(json!({ "request_id": id, "error": "success", "data": data }))
                .await;
        }
    }

    #[tokio::test]
    async fn sends_commands_and_returns_data() {
        let (client, _events, mut mpv) = FakeMpv::connect();
        let call = tokio::spawn(async move { client.get_property::<f64>("volume").await });

        let request = mpv.next_request().await;
        assert_eq!(request["command"], json!(["get_property", "volume"]));
        mpv.reply(&request, json!(70.0)).await;

        assert_eq!(call.await.unwrap().unwrap(), 70.0);
    }

    #[tokio::test]
    async fn matches_out_of_order_replies() {
        let (client, _events, mut mpv) = FakeMpv::connect();
        let client = Arc::new(client);
        let (a, b) = (Arc::clone(&client), Arc::clone(&client));
        let first = tokio::spawn(async move { a.get_property::<String>("path").await });
        let first_request = mpv.next_request().await;
        let second = tokio::spawn(async move { b.get_property::<bool>("pause").await });
        let second_request = mpv.next_request().await;

        mpv.reply(&second_request, json!(true)).await;
        mpv.reply(&first_request, json!("/music/a.flac")).await;

        assert_eq!(first.await.unwrap().unwrap(), "/music/a.flac");
        assert!(second.await.unwrap().unwrap());
    }

    #[tokio::test]
    async fn reports_mpv_errors() {
        let (client, _events, mut mpv) = FakeMpv::connect();
        let call = tokio::spawn(async move { client.get_property::<f64>("duration").await });

        let request = mpv.next_request().await;
        mpv.send(json!({
            "request_id": request["request_id"], "error": "property unavailable"
        }))
        .await;

        let err = call.await.unwrap().unwrap_err();
        assert!(matches!(err, Error::Mpv(ref message) if message == "property unavailable"));
    }

    #[tokio::test]
    async fn delivers_events_in_order() {
        let (_client, mut events, mut mpv) = FakeMpv::connect();
        mpv.send(json!({ "event": "property-change", "id": 1, "name": "pause", "data": false }))
            .await;
        mpv.send(json!({ "event": "end-file", "reason": "eof", "request_id": 0 }))
            .await;

        assert_eq!(
            events.recv().await,
            Some(Event::PropertyChange {
                id: 1,
                name: "pause".into(),
                data: Some(json!(false)),
            })
        );
        assert!(matches!(
            events.recv().await,
            Some(Event::EndFile {
                reason: EndReason::Eof,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn skips_malformed_lines() {
        let (client, _events, mut mpv) = FakeMpv::connect();
        let call = tokio::spawn(async move { client.get_property::<bool>("mute").await });
        let request = mpv.next_request().await;

        mpv.writer.write_all(b"this is not json\n").await.unwrap();
        mpv.reply(&request, json!(false)).await;

        assert!(!call.await.unwrap().unwrap());
    }

    #[tokio::test]
    async fn disconnect_fails_pending_and_later_requests() {
        let (client, mut events, mut mpv) = FakeMpv::connect();
        let client = Arc::new(client);
        let waiting = {
            let client = Arc::clone(&client);
            tokio::spawn(async move { client.get_property::<f64>("time-pos").await })
        };
        mpv.next_request().await;

        drop(mpv);

        assert!(matches!(waiting.await.unwrap(), Err(Error::Disconnected)));
        assert_eq!(events.recv().await, None);
        assert!(matches!(
            client.set_property("pause", true).await,
            Err(Error::Disconnected)
        ));
    }
}
