//! Канал реального времени (Supabase Realtime, протокол Phoenix поверх WebSocket).
//!
//! Через него идут только зашифрованные правки и положения курсоров.
//! Если канал недоступен, правки всё равно доходят через сервер, просто с задержкой.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const REALTIME_URL: &str = "wss://zjoctahbygocbbzeqvor.supabase.co/realtime/v1/websocket";
pub const ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Inpqb2N0YWhieWdvY2JiemVxdm9yIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTU0NzU5NzgsImV4cCI6MjExMTA1MTk3OH0.0ymwwM5uTSW0TFSBUB0QvQMqRDWp3bvoJ08e-mtPfoM";

/// Самое большое сообщение, которое отправляем в канал; крупнее идут только через сервер.
pub const MAX_BROADCAST: usize = 200 * 1024;

pub enum Cmd {
    Join(String),
    Leave(String),
    Send { topic: String, event: String, payload: Value },
}

pub struct Realtime {
    tx: Sender<Cmd>,
    online: Arc<AtomicBool>,
}

pub enum Event {
    Message { topic: String, event: String, payload: Value },
    Online(bool),
}

impl Realtime {
    pub fn start(on_event: impl Fn(Event) + Send + 'static) -> Realtime {
        let (tx, rx) = mpsc::channel();
        let online = Arc::new(AtomicBool::new(false));
        let flag = online.clone();
        std::thread::Builder::new()
            .name("realtime".into())
            .spawn(move || run(rx, flag, on_event))
            .ok();
        Realtime { tx, online }
    }

    pub fn join(&self, topic: &str) {
        let _ = self.tx.send(Cmd::Join(topic.to_string()));
    }

    pub fn leave(&self, topic: &str) {
        let _ = self.tx.send(Cmd::Leave(topic.to_string()));
    }

    pub fn send(&self, topic: &str, event: &str, payload: Value) {
        let _ = self.tx.send(Cmd::Send { topic: topic.to_string(), event: event.to_string(), payload });
    }

    pub fn online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }
}

type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

fn connect() -> Option<Socket> {
    let url = format!("{REALTIME_URL}?apikey={ANON_KEY}&vsn=1.0.0");
    let (sock, _) = tungstenite::connect(url).ok()?;
    // После рукопожатия переводим сокет в неблокирующий режим: один поток и читает, и пишет.
    match sock.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => {
            let _ = s.set_nonblocking(true);
        }
        tungstenite::stream::MaybeTlsStream::NativeTls(s) => {
            let _ = s.get_ref().set_nonblocking(true);
        }
        _ => {}
    }
    Some(sock)
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if io.kind() == std::io::ErrorKind::WouldBlock)
}

/// Отправка в неблокирующем режиме: если сокет занят, сообщение остаётся в очереди и уйдёт при flush.
fn send_text(sock: &mut Socket, text: String) -> bool {
    match sock.send(tungstenite::Message::text(text)) {
        Ok(()) => true,
        Err(e) => would_block(&e),
    }
}

fn run(rx: Receiver<Cmd>, online: Arc<AtomicBool>, on_event: impl Fn(Event)) {
    // Темы, на которые мы подписаны, и номер подписки для каждой.
    let mut topics: HashMap<String, String> = HashMap::new();
    let mut backoff = Duration::from_secs(1);
    let mut seq: u64 = 0;
    let mut next_ref = move || {
        seq += 1;
        seq.to_string()
    };

    loop {
        // Пока подписок нет, соединение не держим.
        if topics.is_empty() {
            match rx.recv() {
                Ok(Cmd::Join(t)) => {
                    topics.insert(t, String::new());
                }
                Ok(_) => continue,
                Err(_) => return,
            }
        }

        let Some(mut sock) = connect() else {
            if online.swap(false, Ordering::Relaxed) {
                on_event(Event::Online(false));
            }
            // Ждём, но продолжаем принимать команды, чтобы не потерять подписки.
            let until = Instant::now() + backoff;
            while Instant::now() < until {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(Cmd::Join(t)) => {
                        topics.insert(t, String::new());
                    }
                    Ok(Cmd::Leave(t)) => {
                        topics.remove(&t);
                    }
                    Ok(Cmd::Send { .. }) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        };
        backoff = Duration::from_secs(1);

        let join = |sock: &mut Socket, topic: &str, r: &str| {
            let msg = json!({
                "topic": format!("realtime:{topic}"),
                "event": "phx_join",
                "payload": { "config": { "broadcast": { "self": false, "ack": false }, "presence": { "key": "" }, "private": false } },
                "ref": r,
                "join_ref": r,
            });
            send_text(sock, msg.to_string())
        };

        let mut ok = true;
        let names: Vec<String> = topics.keys().cloned().collect();
        for t in names {
            let r = next_ref();
            ok &= join(&mut sock, &t, &r);
            topics.insert(t, r);
        }
        if ok && !online.swap(true, Ordering::Relaxed) {
            on_event(Event::Online(true));
        }

        let mut last_beat = Instant::now();
        while ok {
            // Исходящие команды.
            loop {
                match rx.try_recv() {
                    Ok(Cmd::Join(t)) => {
                        if !topics.contains_key(&t) {
                            let r = next_ref();
                            ok &= join(&mut sock, &t, &r);
                            topics.insert(t, r);
                        }
                    }
                    Ok(Cmd::Leave(t)) => {
                        if let Some(r) = topics.remove(&t) {
                            let msg = json!({ "topic": format!("realtime:{t}"), "event": "phx_leave", "payload": {}, "ref": next_ref(), "join_ref": r });
                            let _ = send_text(&mut sock, msg.to_string());
                        }
                    }
                    Ok(Cmd::Send { topic, event, payload }) => {
                        if let Some(r) = topics.get(&topic) {
                            let msg = json!({
                                "topic": format!("realtime:{topic}"),
                                "event": "broadcast",
                                "payload": { "type": "broadcast", "event": event, "payload": payload },
                                "ref": next_ref(),
                                "join_ref": r,
                            });
                            ok &= send_text(&mut sock, msg.to_string());
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if !ok {
                break;
            }
            if topics.is_empty() {
                let _ = sock.close(None);
                break;
            }

            if last_beat.elapsed() > Duration::from_secs(25) {
                let msg = json!({ "topic": "phoenix", "event": "heartbeat", "payload": {}, "ref": next_ref() });
                ok &= send_text(&mut sock, msg.to_string());
                last_beat = Instant::now();
            }

            if let Err(e) = sock.flush() {
                if !would_block(&e) {
                    break;
                }
            }

            // Входящие сообщения.
            match sock.read() {
                Ok(tungstenite::Message::Text(t)) => {
                    if let Ok(v) = serde_json::from_str::<Value>(t.as_str()) {
                        let topic = v.get("topic").and_then(Value::as_str).unwrap_or_default();
                        if v.get("event").and_then(Value::as_str) == Some("broadcast") {
                            if let Some(inner) = v.get("payload") {
                                let ev = inner.get("event").and_then(Value::as_str).unwrap_or_default().to_string();
                                let payload = inner.get("payload").cloned().unwrap_or(Value::Null);
                                let topic = topic.strip_prefix("realtime:").unwrap_or(topic).to_string();
                                on_event(Event::Message { topic, event: ev, payload });
                            }
                        }
                    }
                }
                Ok(tungstenite::Message::Close(_)) => ok = false,
                Ok(_) => {}
                Err(e) if would_block(&e) => std::thread::sleep(Duration::from_millis(15)),
                Err(_) => ok = false,
            }
        }

        if online.swap(false, Ordering::Relaxed) {
            on_event(Event::Online(false));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
