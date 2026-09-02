//! The update channel: server-sent events, on the server that already exists.
//!
//! # Threat model
//!
//! [CVE-2025-29927] was an inbound header that *chose* what the server did.
//! The update channel is where that mistake would be easiest to repeat, because
//! the server-sent-events specification hands one over: `Last-Event-ID` is
//! defined as a resume cursor a client supplies. Honouring it would mean an
//! inbound header selecting which bytes leave the process.
//!
//! So it is not honoured, and it cannot be: [`crate::http::RequestHead`] keeps
//! `Host` and `Origin` and forgets every other header while it scans, and the
//! stream head written by [`write_event_stream`] is a `&'static [u8]` with no
//! substitution points at all. A subscriber's cursor is assigned by the server
//! at [`UpdateChannel::subscribe`] and moves only as the server publishes.
//! `an_hmr_stream_ignores_last_event_id` asserts the byte-for-byte equality
//! that makes this checkable rather than merely stated.
//!
//! There is also no `Access-Control-Allow-Origin` on the stream. A page on
//! another origin can open an `EventSource`, but without that header the
//! browser will not hand it a single byte, and the `Host` allowlist in
//! [`crate::network`] has already had its say.
//!
//! # Bounds
//!
//! [`MAX_SUBSCRIBERS`] concurrent streams, [`MAX_BUFFERED_UPDATES`] events
//! retained, and a frame that is encoded once and shared as an `Arc` rather
//! than re-serialized per subscriber. A dev server that allocates per client
//! per event is a dev server one browser tab can wedge.
//!
//! # Why `Arc`
//!
//! `clippy.toml` disallows [`std::sync::Arc`] without an ownership proof, and
//! this is the proof: one channel is jointly owned by the watcher thread that
//! publishes and by up to [`MAX_SUBSCRIBERS`] connection threads that each
//! outlive the accept-loop turn that created them. There is no lexical scope
//! that contains all of those, so a borrow cannot express the lifetime and a
//! scoped thread cannot hold it. The same exemption is taken in
//! [`crate::server`], [`crate::hmr::session`] and `uf dev`, for the same reason.
//!
//! [CVE-2025-29927]: https://nvd.nist.gov/vuln/detail/CVE-2025-29927
#![expect(
    clippy::disallowed_types,
    reason = "the update channel is shared between the publisher and every open stream thread"
)]

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::update::HmrUpdate;

/// Most event-stream connections one dev server will hold open.
pub const MAX_SUBSCRIBERS: usize = 32;

/// How many published updates the channel keeps for a subscriber that is
/// briefly behind.
pub const MAX_BUFFERED_UPDATES: usize = 64;

/// How long a stream waits before writing a keep-alive comment.
///
/// A proxy or a sleeping laptop drops an idle connection silently; the comment
/// turns that into a write error the stream can act on.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// The reconnection delay advertised to the browser, in milliseconds.
pub const RETRY_MILLIS: u32 = 1_000;

/// The event name every update frame carries.
pub const UPDATE_EVENT: &str = "uf:update";

/// The response head of an event stream.
///
/// A `&'static [u8]` on purpose: there is no position in it a request could
/// influence. No `Access-Control-Allow-Origin`, no reflected `Origin`, no
/// reflected path.
pub const EVENT_STREAM_HEAD: &[u8] = b"HTTP/1.1 200 OK\r\n\
content-type: text/event-stream; charset=utf-8\r\n\
cache-control: no-store\r\n\
x-content-type-options: nosniff\r\n\
x-uf-loader: none\r\n\
connection: close\r\n\
\r\n";

/// Why a subscription could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SubscribeError {
    /// [`MAX_SUBSCRIBERS`] streams are already open.
    #[error("the update channel already holds {MAX_SUBSCRIBERS} subscribers")]
    AtCapacity,
    /// The channel has been closed.
    #[error("the update channel is closed")]
    Closed,
}

/// One encoded server-sent event, ready to write to any number of sockets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    id: u64,
    bytes: Arc<[u8]>,
}

#[derive(Debug, Default)]
struct ChannelState {
    next_id: u64,
    frames: VecDeque<Frame>,
    subscribers: usize,
}

/// The published stream of hot-module-replacement updates.
///
/// Shared as an `Arc` between the watcher that publishes and the connection
/// threads that write. Every method is safe to call from any thread.
#[derive(Debug, Default)]
pub struct UpdateChannel {
    state: Mutex<ChannelState>,
    published: Condvar,
    closed: AtomicBool,
}

impl UpdateChannel {
    /// An open channel with no subscribers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish `update`, stamping it with the next event identifier.
    ///
    /// Returns the identifier that was assigned. An inert update is published
    /// like any other: a client that wants to log "nothing changed" should be
    /// able to, and a channel that silently drops events is a channel nobody
    /// can debug.
    pub fn publish(&self, mut update: HmrUpdate) -> u64 {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.next_id += 1;
        let id = state.next_id;
        update.id = id;
        let bytes = encode_frame(&update);
        state.frames.push_back(Frame {
            id,
            bytes: Arc::from(bytes.into_boxed_slice()),
        });
        while state.frames.len() > MAX_BUFFERED_UPDATES {
            state.frames.pop_front();
        }
        drop(state);
        self.published.notify_all();
        id
    }

    /// Open a subscription positioned after every update published so far.
    ///
    /// The cursor is the server's, not the client's. A browser that reconnects
    /// receives what is published from that moment on and re-fetches the
    /// modules it needs; it does not get to ask for history.
    ///
    /// # Errors
    ///
    /// Returns [`SubscribeError`] when the channel is closed or already holds
    /// [`MAX_SUBSCRIBERS`] streams.
    pub fn subscribe(self: &Arc<Self>) -> Result<Subscriber, SubscribeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SubscribeError::Closed);
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.subscribers >= MAX_SUBSCRIBERS {
            return Err(SubscribeError::AtCapacity);
        }
        state.subscribers += 1;
        let cursor = state.next_id;
        drop(state);
        Ok(Subscriber {
            channel: Arc::clone(self),
            cursor,
        })
    }

    /// How many streams are open.
    pub fn subscriber_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .subscribers
    }

    /// The identifier of the most recent published update.
    pub fn last_event_id(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .next_id
    }

    /// Whether the channel has been closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Close the channel and wake every stream so it can finish.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.published.notify_all();
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.subscribers = state.subscribers.saturating_sub(1);
    }
}

/// What a subscriber found when it woke up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Waited {
    /// An encoded frame to write.
    Frame(Arc<[u8]>),
    /// Nothing arrived before the deadline; write a keep-alive comment.
    Idle,
    /// The channel is closed and no frame is left.
    Closed,
}

/// One open event stream's position in the channel.
#[derive(Debug)]
pub struct Subscriber {
    channel: Arc<UpdateChannel>,
    cursor: u64,
}

impl Subscriber {
    /// The last event identifier this subscriber has been handed.
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// The channel this subscriber reads.
    pub fn channel(&self) -> &Arc<UpdateChannel> {
        &self.channel
    }

    /// Take the next frame, waiting at most `timeout` for one to arrive.
    pub fn wait(&mut self, timeout: Duration) -> Waited {
        let mut state = self
            .channel
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(frame) = state
                .frames
                .iter()
                .find(|frame| frame.id > self.cursor)
                .cloned()
            {
                self.cursor = frame.id;
                return Waited::Frame(frame.bytes);
            }
            if self.channel.closed.load(Ordering::Acquire) {
                return Waited::Closed;
            }
            let (next, result) = self
                .channel
                .published
                .wait_timeout(state, timeout)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if result.timed_out() {
                return Waited::Idle;
            }
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.channel.release();
    }
}

/// Encode one update as a server-sent event frame.
///
/// The `data:` field is JSON, which is what makes the frame injection-proof: a
/// module path containing a newline — which a POSIX filesystem permits — is
/// escaped by the serializer rather than ending the frame early. Nothing about
/// the framing depends on the path being well behaved.
pub fn encode_frame(update: &HmrUpdate) -> Vec<u8> {
    let payload = serde_json::to_string(update).unwrap_or_else(|_| String::from("{}"));
    let mut out = String::with_capacity(payload.len() + 64);
    out.push_str("id: ");
    uf_term::push_usize(&mut out, update.id as usize);
    out.push_str("\nevent: ");
    out.push_str(UPDATE_EVENT);
    out.push_str("\nretry: ");
    uf_term::push_u32(&mut out, RETRY_MILLIS);
    out.push_str("\ndata: ");
    out.push_str(&payload);
    out.push_str("\n\n");
    out.into_bytes()
}

/// The keep-alive comment written when nothing has been published.
pub const HEARTBEAT_FRAME: &[u8] = b": uf\n\n";

/// How long an event stream may stay open, and how patiently it waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamLimits {
    /// How long to wait for a frame before writing a keep-alive.
    pub heartbeat: Duration,
    /// How many frames to write before returning. `None` streams until the
    /// socket fails or the channel closes; a bounded count is what lets tests
    /// drive the writer without a socket.
    pub max_frames: Option<usize>,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            heartbeat: HEARTBEAT_INTERVAL,
            max_frames: None,
        }
    }
}

/// Write the event-stream head and then every frame the subscriber receives.
///
/// Returns when the channel closes, when `limits.max_frames` frames have been
/// written, or when the socket refuses a write — which is how a closed browser
/// tab ends its own stream.
///
/// # Errors
///
/// Returns the underlying I/O error from the first failed write.
pub fn write_event_stream<W: Write>(
    sink: &mut W,
    subscriber: &mut Subscriber,
    limits: StreamLimits,
) -> io::Result<()> {
    sink.write_all(EVENT_STREAM_HEAD)?;
    sink.flush()?;

    let mut written = 0usize;
    loop {
        if limits.max_frames.is_some_and(|max| written >= max) {
            return Ok(());
        }
        match subscriber.wait(limits.heartbeat) {
            Waited::Frame(bytes) => {
                sink.write_all(&bytes)?;
                sink.flush()?;
                written += 1;
            }
            Waited::Idle => {
                sink.write_all(HEARTBEAT_FRAME)?;
                sink.flush()?;
            }
            Waited::Closed => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
