//! Real-time updates over Server-Sent Events.
//!
//! Every change is announced with `pg_notify` when its transaction commits
//! (see `activity.rs`). One task per server instance `LISTEN`s for those and
//! re-broadcasts them in-process, and each browser holds an SSE stream for
//! its workspace. Because Postgres does the fan-out, several server
//! instances behind a load balancer all see every change.
//!
//! Events only say what changed (an item, a comment, a label…); clients
//! re-fetch what they're showing. That keeps the stream small and means a
//! client can never drift from the server's state. When a client might have
//! missed something (it reconnected, or fell behind), it's told to `resync`.

use std::{sync::Arc, time::Duration};

use actix_web::{HttpResponse, web};
use bytes::Bytes;
use futures_util::stream;
use sqlx::{PgPool, postgres::PgListener};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    AppState, access,
    activity::{CHANNEL, Event},
    auth::AuthUser,
    error::ApiResult,
};

/// What the listener broadcasts to every open stream.
#[derive(Clone, Debug)]
pub enum Signal {
    Event(Arc<Event>, Arc<str>),
    /// Notifications may have been missed (the listener reconnected).
    Resync,
}

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<Signal>,
}

impl Hub {
    /// Starts listening for change notifications.
    pub fn start(db: PgPool) -> Hub {
        let (tx, _) = broadcast::channel(1024);
        let hub = Hub { tx: tx.clone() };
        tokio::spawn(async move {
            let mut first = true;
            loop {
                match listen(&db, &tx, first).await {
                    Ok(()) => {}
                    Err(e) => tracing::warn!(error = %e, "change listener disconnected; retrying"),
                }
                first = false;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
        hub
    }
}

async fn listen(db: &PgPool, tx: &broadcast::Sender<Signal>, first: bool) -> Result<(), sqlx::Error> {
    let mut listener = PgListener::connect_with(db).await?;
    // Don't silently reconnect inside recv(): we want to know, so clients
    // can be told to resync.
    listener.ignore_pool_close_event(true);
    listener.listen(CHANNEL).await?;
    if !first {
        let _ = tx.send(Signal::Resync);
    }
    loop {
        let n = listener.try_recv().await?;
        let Some(n) = n else {
            // The connection dropped.
            let _ = tx.send(Signal::Resync);
            return Ok(());
        };
        match serde_json::from_str::<Event>(n.payload()) {
            Ok(ev) => {
                let _ = tx.send(Signal::Event(Arc::new(ev), Arc::from(n.payload())));
            }
            Err(e) => tracing::warn!(error = %e, "unreadable change notification"),
        }
    }
}

struct Conn {
    rx: broadcast::Receiver<Signal>,
    db: PgPool,
    user: Uuid,
    workspace: Uuid,
    ping: tokio::time::Interval,
    started: bool,
}

fn frame(event: &str, id: Option<i64>, data: &str) -> Bytes {
    let id = id.map(|i| format!("id: {i}\n")).unwrap_or_default();
    Bytes::from(format!("{id}event: {event}\ndata: {data}\n\n"))
}

/// `GET /api/workspaces/{id}/events`: a stream of the workspace's changes.
pub async fn events(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?;
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    ping.reset(); // the first tick would otherwise fire immediately
    let conn = Conn {
        rx: state.hub.tx.subscribe(),
        db: state.db.clone(),
        user: user.id,
        workspace: *path,
        ping,
        started: false,
    };

    let body = stream::unfold(conn, |mut c| async move {
        if !c.started {
            c.started = true;
            // Reconnect quickly, and tell the client it's live.
            return Some((Ok::<_, actix_web::Error>(Bytes::from("retry: 3000\nevent: ready\ndata: {}\n\n")), c));
        }
        loop {
            tokio::select! {
                signal = c.rx.recv() => match signal {
                    Ok(Signal::Event(ev, raw)) => {
                        if ev.workspace_id != c.workspace {
                            continue;
                        }
                        // Membership can change while a stream is open:
                        // check it before sending anything.
                        match access::is_member(&c.db, c.workspace, c.user).await {
                            Ok(true) => return Some((Ok(frame("change", ev.id, &raw)), c)),
                            _ => return None,
                        }
                    }
                    Ok(Signal::Resync) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        return Some((Ok(frame("resync", None, "{}")), c));
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
                _ = c.ping.tick() => return Some((Ok(Bytes::from(": ping\n\n")), c)),
            }
        }
    });

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        // no-transform: compressing proxies (including the Next dev server)
        // must not gzip the stream, or they buffer it.
        .insert_header(("Cache-Control", "no-cache, no-transform"))
        // Ask proxies such as nginx not to buffer the stream.
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(body))
}
