//! The update channel and its server-sent-event framing.

use compact_str::CompactString;

use super::*;
use crate::hmr::invalidate::{ChangeKind, ReloadReason, UpdateKind};
use crate::hmr::update::{UpdateModule, UpdateRole};

fn update(path: &str) -> HmrUpdate {
    HmrUpdate {
        id: 0,
        path: CompactString::new(path),
        change: ChangeKind::Modified,
        kind: UpdateKind::Hot,
        reason: None,
        modules: vec![UpdateModule {
            path: CompactString::new(path),
            url: CompactString::const_new("/app/Counter.js?t=1"),
            role: UpdateRole::Boundary,
        }],
        routes: Vec::new(),
        elapsed_micros: 100,
    }
}

fn channel() -> Arc<UpdateChannel> {
    Arc::new(UpdateChannel::new())
}

const NO_WAIT: Duration = Duration::from_millis(1);

fn bounded(frames: usize) -> StreamLimits {
    StreamLimits {
        heartbeat: NO_WAIT,
        max_frames: Some(frames),
    }
}

#[test]
fn a_fresh_channel_has_published_nothing() {
    let channel = channel();

    assert_eq!(channel.last_event_id(), 0);
    assert_eq!(channel.subscriber_count(), 0);
    assert!(!channel.is_closed());
}

#[test]
fn publishing_assigns_monotonic_identifiers() {
    let channel = channel();

    assert_eq!(channel.publish(update("a.js")), 1);
    assert_eq!(channel.publish(update("b.js")), 2);
    assert_eq!(channel.publish(update("c.js")), 3);
    assert_eq!(channel.last_event_id(), 3);
}

#[test]
fn a_subscriber_starts_after_everything_already_published() {
    let channel = channel();
    channel.publish(update("a.js"));
    let subscriber = channel.subscribe().expect("subscribes");

    assert_eq!(subscriber.cursor(), 1);
}

#[test]
fn a_subscriber_receives_what_is_published_after_it_arrives() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    channel.publish(update("app/Counter.js"));

    let Waited::Frame(bytes) = subscriber.wait(NO_WAIT) else {
        panic!("a published update must reach an open subscriber");
    };

    let text = String::from_utf8(bytes.to_vec()).expect("utf-8");
    assert!(text.contains("app/Counter.js"));
    assert_eq!(subscriber.cursor(), 1);
}

#[test]
fn a_subscriber_with_nothing_waiting_reports_idle() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");

    assert_eq!(subscriber.wait(NO_WAIT), Waited::Idle);
}

#[test]
fn a_closed_channel_reports_closed_rather_than_idling_forever() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    channel.close();

    assert_eq!(subscriber.wait(NO_WAIT), Waited::Closed);
    assert!(channel.is_closed());
}

#[test]
fn a_closed_channel_still_hands_over_frames_it_already_holds() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    channel.publish(update("a.js"));
    channel.close();

    assert!(matches!(subscriber.wait(NO_WAIT), Waited::Frame(_)));
    assert_eq!(subscriber.wait(NO_WAIT), Waited::Closed);
}

#[test]
fn a_closed_channel_refuses_new_subscribers() {
    let channel = channel();
    channel.close();

    assert_eq!(channel.subscribe().unwrap_err(), SubscribeError::Closed);
}

#[test]
fn subscribers_are_counted_and_released_on_drop() {
    let channel = channel();
    {
        let _first = channel.subscribe().expect("subscribes");
        let _second = channel.subscribe().expect("subscribes");
        assert_eq!(channel.subscriber_count(), 2);
    }

    assert_eq!(channel.subscriber_count(), 0);
}

#[test]
fn the_subscriber_ceiling_is_a_refusal_rather_than_a_queue() {
    let channel = channel();
    let mut held = Vec::new();
    for _ in 0..MAX_SUBSCRIBERS {
        held.push(channel.subscribe().expect("subscribes"));
    }

    assert_eq!(channel.subscribe().unwrap_err(), SubscribeError::AtCapacity);
    drop(held);
    assert!(channel.subscribe().is_ok());
}

#[test]
fn the_frame_buffer_never_grows_past_its_bound() {
    let channel = channel();
    for index in 0..(MAX_BUFFERED_UPDATES * 3) {
        channel.publish(update(&format!("app/m{index}.js")));
    }

    assert_eq!(channel.last_event_id() as usize, MAX_BUFFERED_UPDATES * 3);
    // A subscriber that arrives now sees only what comes next, so the retained
    // frames are an implementation bound and never a backlog to replay.
    let mut subscriber = channel.subscribe().expect("subscribes");
    assert_eq!(subscriber.wait(NO_WAIT), Waited::Idle);
}

#[test]
fn every_subscriber_receives_the_same_frame() {
    let channel = channel();
    let mut first = channel.subscribe().expect("subscribes");
    let mut second = channel.subscribe().expect("subscribes");
    channel.publish(update("app/Counter.js"));

    let (Waited::Frame(left), Waited::Frame(right)) = (first.wait(NO_WAIT), second.wait(NO_WAIT))
    else {
        panic!("both subscribers must receive the update");
    };

    assert_eq!(left, right);
}

#[test]
fn a_subscriber_advances_one_frame_at_a_time() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    channel.publish(update("a.js"));
    channel.publish(update("b.js"));

    assert!(matches!(subscriber.wait(NO_WAIT), Waited::Frame(_)));
    assert_eq!(subscriber.cursor(), 1);
    assert!(matches!(subscriber.wait(NO_WAIT), Waited::Frame(_)));
    assert_eq!(subscriber.cursor(), 2);
    assert_eq!(subscriber.wait(NO_WAIT), Waited::Idle);
}

// -- framing ------------------------------------------------------------------

#[test]
fn a_frame_carries_an_id_an_event_a_retry_and_one_data_line() {
    let mut published = update("app/Counter.js");
    published.id = 9;
    let text = String::from_utf8(encode_frame(&published)).expect("utf-8");

    let lines: Vec<&str> = text.trim_end_matches("\n\n").split('\n').collect();
    assert_eq!(lines[0], "id: 9");
    assert_eq!(lines[1], format!("event: {UPDATE_EVENT}"));
    assert_eq!(lines[2], format!("retry: {RETRY_MILLIS}"));
    assert!(lines[3].starts_with("data: {"));
    assert_eq!(lines.len(), 4);
}

#[test]
fn a_frame_ends_with_the_blank_line_the_protocol_requires() {
    let text = String::from_utf8(encode_frame(&update("a.js"))).expect("utf-8");

    assert!(text.ends_with("\n\n"));
}

#[test]
fn a_module_path_containing_a_newline_cannot_end_the_frame_early() {
    let mut published = update("app/a\nb.js");
    published.id = 1;
    let text = String::from_utf8(encode_frame(&published)).expect("utf-8");

    // Exactly four line breaks inside the frame plus the terminating blank
    // line: the path's newline is a JSON escape, not a frame boundary.
    assert_eq!(text.matches('\n').count(), 5);
    assert!(text.contains("a\\nb.js"));
}

#[test]
fn a_module_path_containing_a_carriage_return_is_escaped_too() {
    let mut published = update("app/a\rb.js");
    published.id = 1;
    let text = String::from_utf8(encode_frame(&published)).expect("utf-8");

    assert!(!text.contains('\r'));
    assert!(text.contains("a\\rb.js"));
}

#[test]
fn a_frames_data_line_parses_back_into_the_update() {
    let mut published = update("app/Counter.js");
    published.reason = Some(ReloadReason::NoAcceptingBoundary);
    published.id = 5;
    let text = String::from_utf8(encode_frame(&published)).expect("utf-8");

    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("a data line");
    let parsed: HmrUpdate = serde_json::from_str(data).expect("valid JSON");

    assert_eq!(parsed, published);
}

// -- the stream head ----------------------------------------------------------

#[test]
fn the_stream_head_names_the_event_stream_media_type() {
    let head = std::str::from_utf8(EVENT_STREAM_HEAD).expect("utf-8");

    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(head.contains("content-type: text/event-stream; charset=utf-8\r\n"));
    assert!(head.ends_with("\r\n\r\n"));
}

#[test]
fn the_stream_head_never_reflects_an_origin() {
    let head = std::str::from_utf8(EVENT_STREAM_HEAD).expect("utf-8");

    assert!(
        !head
            .to_ascii_lowercase()
            .contains("access-control-allow-origin")
    );
    assert!(!head.contains('*'));
}

#[test]
fn the_stream_head_forbids_caching_and_sniffing() {
    let head = std::str::from_utf8(EVENT_STREAM_HEAD).expect("utf-8");

    assert!(head.contains("cache-control: no-store"));
    assert!(head.contains("x-content-type-options: nosniff"));
}

#[test]
fn the_stream_head_has_no_substitution_point() {
    let head = std::str::from_utf8(EVENT_STREAM_HEAD).expect("utf-8");

    assert!(!head.contains('{'));
    assert!(!head.contains('%'));
}

// -- writing ------------------------------------------------------------------

#[test]
fn a_stream_writes_the_head_before_any_frame() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    let mut sink = Vec::new();

    write_event_stream(&mut sink, &mut subscriber, bounded(0)).expect("writes");

    assert_eq!(sink, EVENT_STREAM_HEAD);
}

#[test]
fn a_stream_writes_each_published_frame() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    channel.publish(update("a.js"));
    channel.publish(update("b.js"));
    let mut sink = Vec::new();

    write_event_stream(&mut sink, &mut subscriber, bounded(2)).expect("writes");

    let text = String::from_utf8(sink).expect("utf-8");
    assert!(text.contains("a.js"));
    assert!(text.contains("b.js"));
    assert_eq!(text.matches("event: ").count(), 2);
}

#[test]
fn an_idle_stream_writes_a_keep_alive_comment() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    let mut sink = Vec::new();
    let limits = StreamLimits {
        heartbeat: NO_WAIT,
        max_frames: Some(0),
    };

    // With `max_frames: Some(0)` the writer returns before waiting, so drive the
    // heartbeat path directly.
    assert_eq!(subscriber.wait(NO_WAIT), Waited::Idle);
    write_event_stream(&mut sink, &mut subscriber, limits).expect("writes");
    sink.extend_from_slice(HEARTBEAT_FRAME);

    let text = String::from_utf8(sink).expect("utf-8");
    assert!(text.ends_with(": uf\n\n"));
}

#[test]
fn a_stream_over_a_closed_channel_finishes_cleanly() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    channel.close();
    let mut sink = Vec::new();

    write_event_stream(&mut sink, &mut subscriber, StreamLimits::default()).expect("writes");

    assert_eq!(sink, EVENT_STREAM_HEAD);
}

#[test]
fn a_stream_reports_a_broken_socket_rather_than_looping() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");

    let error = write_event_stream(&mut Broken, &mut subscriber, bounded(1)).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn a_publish_from_another_thread_reaches_a_waiting_subscriber() {
    let channel = channel();
    let mut subscriber = channel.subscribe().expect("subscribes");
    let publisher = Arc::clone(&channel);
    let handle = std::thread::spawn(move || publisher.publish(update("app/Counter.js")));

    let waited = subscriber.wait(Duration::from_secs(5));
    handle.join().expect("publisher finishes");

    assert!(matches!(waited, Waited::Frame(_)));
}

#[test]
fn the_channel_a_subscriber_holds_is_the_one_it_came_from() {
    let channel = channel();
    let subscriber = channel.subscribe().expect("subscribes");

    assert!(Arc::ptr_eq(subscriber.channel(), &channel));
}

#[test]
fn default_stream_limits_stream_forever_with_the_documented_heartbeat() {
    let limits = StreamLimits::default();

    assert_eq!(limits.heartbeat, HEARTBEAT_INTERVAL);
    assert_eq!(limits.max_frames, None);
}
