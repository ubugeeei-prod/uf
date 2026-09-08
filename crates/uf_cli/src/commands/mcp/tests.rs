use camino::Utf8PathBuf;
use serde_json::{Value, json};

use super::*;

/// Drive the server with a script of messages and read back what it wrote.
///
/// One line per message, which is the transport: a test that passed a slice of
/// `Value`s would be testing a function this server does not have.
fn exchange(cwd: &Utf8Path, messages: &[Value]) -> Vec<Value> {
    let script = messages
        .iter()
        .map(|message| serde_json::to_string(message).expect("a message serialises"))
        .collect::<Vec<_>>()
        .join("\n");
    replies(cwd, &script)
}

/// The same, for input a well-formed client would never send.
fn replies(cwd: &Utf8Path, script: &str) -> Vec<Value> {
    let mut input = std::io::Cursor::new(script.as_bytes().to_vec());
    let mut output = Vec::new();
    serve(&mut input, &mut output, cwd).expect("the server reads to the end");
    let text = String::from_utf8(output).expect("replies are UTF-8");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("every reply is one JSON line"))
        .collect()
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

fn path_of(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 temporary path")
}

fn request(id: u32, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

#[test]
fn initialize_answers_with_the_version_this_server_speaks() {
    let dir = scratch();
    let out = exchange(
        &path_of(&dir),
        &[request(
            1,
            "initialize",
            json!({ "protocolVersion": PROTOCOL_VERSION }),
        )],
    );

    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], 1);
    assert_eq!(out[0]["jsonrpc"], "2.0");
    assert_eq!(out[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(out[0]["result"]["serverInfo"]["name"], "uf");
    // Tools are the whole capability set: no resources, no prompts, and
    // saying so is how a client knows not to ask.
    assert!(out[0]["result"]["capabilities"]["tools"].is_object());
    assert!(out[0]["result"]["capabilities"].get("resources").is_none());
}

/// A client asking for a version this server does not speak is told which one
/// it does, rather than having its own echoed back at it.
#[test]
fn initialize_does_not_claim_a_version_it_was_merely_asked_for() {
    let dir = scratch();
    let out = exchange(
        &path_of(&dir),
        &[request(
            1,
            "initialize",
            json!({ "protocolVersion": "3000-01-01" }),
        )],
    );

    assert_eq!(out[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
}

#[test]
fn a_notification_is_not_answered() {
    let dir = scratch();
    let out = exchange(
        &path_of(&dir),
        &[
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            request(7, "ping", json!({})),
        ],
    );

    // Only the ping. A reply to the notification would have no `id` to carry
    // and would desynchronise a client counting responses.
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0]["id"], 7);
}

#[test]
fn an_unknown_method_is_a_protocol_error() {
    let dir = scratch();
    let out = exchange(&path_of(&dir), &[request(2, "resources/list", json!({}))]);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["error"]["code"], METHOD_NOT_FOUND);
    assert!(out[0].get("result").is_none());
}

/// A bad line is answered and the session continues. The alternative — closing
/// the stream — loses every request behind it over one client's bad frame.
#[test]
fn a_line_that_is_not_json_does_not_end_the_session() {
    let dir = scratch();
    let out = replies(
        &path_of(&dir),
        "{ not json\n\n{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n",
    );

    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!(out[0]["error"]["code"], PARSE_ERROR);
    assert_eq!(out[0]["id"], Value::Null);
    // And the blank line between them was skipped rather than answered.
    assert_eq!(out[1]["id"], 9);
}

#[test]
fn tools_list_names_the_commands_and_says_which_ones_write() {
    let dir = scratch();
    let out = exchange(&path_of(&dir), &[request(3, "tools/list", json!({}))]);

    let listed = out[0]["result"]["tools"]
        .as_array()
        .expect("a list of tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name").to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        listed,
        [
            "uf_check",
            "uf_lint",
            "uf_info",
            "uf_routes",
            "uf_explain",
            "uf_test",
            "uf_fmt_write",
            "uf_lint_fix",
        ]
    );
}

/// The names carry the warning as well as the prose: an agent picking from a
/// list should not have to read a description to learn that a tool edits the
/// checkout.
#[test]
fn the_tools_that_write_are_named_for_it() {
    for tool in tools() {
        let name = tool["name"].as_str().expect("a name");
        let description = tool["description"].as_str().expect("a description");
        let named_as_writing = name.ends_with("_write") || name.ends_with("_fix");
        assert_eq!(
            named_as_writing,
            description.contains(Effect::Writes.note()),
            "{name} and its description disagree about writing"
        );
    }
}

/// Every description ends with exactly one of the three sentences, so that a
/// tool claiming "Reads only" is one that does.
#[test]
fn every_description_states_one_effect() {
    let notes = [Effect::Reads, Effect::Writes, Effect::Runs].map(Effect::note);
    for tool in tools() {
        let name = tool["name"].as_str().expect("a name");
        let description = tool["description"].as_str().expect("a description");
        let stated = notes
            .iter()
            .filter(|note| description.contains(*note))
            .count();
        assert_eq!(stated, 1, "{name}: {description}");
        assert!(description.ends_with('.'), "{name}");
    }
}

/// The one tool that runs project code says so rather than passing for a
/// reader.
#[test]
fn running_the_suite_is_not_described_as_reading() {
    let test_tool = tools()
        .into_iter()
        .find(|tool| tool["name"] == "uf_test")
        .expect("uf_test is exposed");
    let description = test_tool["description"].as_str().expect("a description");

    assert!(description.contains(Effect::Runs.note()), "{description}");
    assert!(!description.contains(Effect::Reads.note()), "{description}");
}

#[test]
fn every_tool_declares_an_object_schema() {
    for tool in tools() {
        let name = tool["name"].as_str().expect("a name").to_owned();
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        assert!(schema["properties"].is_object(), "{name}");
        // Closed, so that a client sending a misspelled argument is told
        // rather than silently ignored.
        assert_eq!(schema["additionalProperties"], false, "{name}");
    }
}

/// The contract that keeps [`tools`] and [`call`] from drifting apart: a tool
/// this server advertises is a tool it dispatches. Nothing but a test can hold
/// this — the list is data and the dispatch is a `match`, and they are only
/// the same by hand.
#[test]
fn every_advertised_tool_is_one_the_server_dispatches() {
    let dir = scratch();
    let cwd = path_of(&dir);

    for tool in tools() {
        let name = tool["name"].as_str().expect("a name").to_owned();
        let result = call(&cwd, &name, &json!({ "command": "build" }));
        let text = result["content"][0]["text"]
            .as_str()
            .expect("a text block")
            .to_owned();
        assert!(
            !text.contains("no tool named"),
            "{name} is listed but not dispatched"
        );
    }
}

#[test]
fn an_unknown_tool_is_a_tool_error_not_a_protocol_error() {
    let dir = scratch();
    let out = exchange(
        &path_of(&dir),
        &[request(
            4,
            "tools/call",
            json!({ "name": "uf_rm_rf", "arguments": {} }),
        )],
    );

    // The request was well-formed and this server answered it. What it says is
    // that there is no such tool — which is a result, not a JSON-RPC failure.
    assert!(out[0].get("error").is_none(), "{:?}", out[0]);
    assert_eq!(out[0]["result"]["isError"], true);
    assert!(
        out[0]["result"]["content"][0]["text"]
            .as_str()
            .expect("a text block")
            .contains("uf_rm_rf")
    );
}

/// The point of the whole exercise: what comes back is the command's own
/// output, not something this module formatted.
#[test]
fn a_tool_returns_what_the_command_itself_printed() {
    let dir = scratch();
    let out = exchange(
        &path_of(&dir),
        &[request(
            5,
            "tools/call",
            json!({ "name": "uf_check", "arguments": {} }),
        )],
    );

    assert_eq!(out[0]["result"]["isError"], false, "{:?}", out[0]);
    let text = out[0]["result"]["content"][0]["text"]
        .as_str()
        .expect("a text block");
    let report: Value = serde_json::from_str(text).expect("uf check's own JSON, unaltered");
    assert_eq!(report["command"], "uf check");
    assert_eq!(report["filesChecked"], 0);
}

/// A command that fails is still a tool call that succeeded, and the reason
/// comes back as text rather than as a protocol failure.
#[test]
fn a_command_that_fails_is_a_tool_error_carrying_its_reason() {
    let dir = scratch();
    let out = exchange(
        &path_of(&dir),
        &[request(
            6,
            "tools/call",
            json!({ "name": "uf_explain", "arguments": { "command": "not-a-command" } }),
        )],
    );

    assert!(out[0].get("error").is_none(), "{:?}", out[0]);
    assert_eq!(out[0]["result"]["isError"], true);
    // `uf explain` bails before printing anything, so the reason is the only
    // block — an empty one ahead of it would be noise.
    let blocks = out[0]["result"]["content"]
        .as_array()
        .expect("content blocks");
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert!(
        blocks[0]["text"]
            .as_str()
            .expect("a text block")
            .contains("not-a-command")
    );
}

/// A project whose sources do not parse: `uf lint` reports and exits non-zero.
fn broken_project() -> tempfile::TempDir {
    let dir = scratch();
    let root = dir.path();
    std::fs::write(root.join("uf.config.js"), "export default {};\n").expect("a config");
    std::fs::write(
        root.join("package.json"),
        "{\"name\":\"broken\",\"private\":true}\n",
    )
    .expect("a manifest");
    std::fs::create_dir_all(root.join("src")).expect("a source directory");
    std::fs::write(root.join("src/bad.js"), "// @flow\nexport function f( {\n")
        .expect("a source file");
    dir
}

/// The regression the first real client found: a failing command's report and
/// the reason it failed are two blocks, so the report is still the JSON
/// document it was. One block carrying both parses as neither.
#[test]
fn a_failing_commands_report_is_still_parseable() {
    let dir = broken_project();
    let out = exchange(
        &path_of(&dir),
        &[request(
            8,
            "tools/call",
            json!({ "name": "uf_lint", "arguments": {} }),
        )],
    );

    assert_eq!(out[0]["result"]["isError"], true, "{:?}", out[0]);
    let blocks = out[0]["result"]["content"]
        .as_array()
        .expect("content blocks");
    assert_eq!(blocks.len(), 2, "{blocks:?}");

    let report: Value = serde_json::from_str(blocks[0]["text"].as_str().expect("a report"))
        .expect("the report is JSON on its own");
    assert_eq!(report["command"], "uf lint");
    assert!(
        report["errors"].as_u64().expect("a count") > 0,
        "{report:?}"
    );
    // And the reason is prose, in its own block, where it cannot corrupt it.
    assert!(serde_json::from_str::<Value>(blocks[1]["text"].as_str().expect("a reason")).is_err());
}

/// The bug the same client found first: a command that renders for a reader
/// must not be run in JSON mode, where `Ui::render` writes nothing at all.
#[test]
fn a_command_that_renders_for_a_reader_returns_its_text() {
    let dir = broken_project();
    let out = exchange(
        &path_of(&dir),
        &[request(
            9,
            "tools/call",
            json!({ "name": "uf_info", "arguments": {} }),
        )],
    );

    let text = out[0]["result"]["content"][0]["text"]
        .as_str()
        .expect("a text block");
    assert!(!text.is_empty(), "uf_info returned nothing");
    assert!(text.contains("uf"), "{text}");
}

/// A JSON value that is not an object — an array, which is how JSON-RPC spells
/// a batch, or a bare scalar. Neither is a message this server can act on, and
/// both must be *answered*: a client waiting on a reply that never comes is a
/// worse failure than one told its request was invalid.
#[test]
fn a_message_that_is_not_an_object_is_refused_rather_than_ignored() {
    let dir = scratch();
    let out = replies(
        &path_of(&dir),
        "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]\n42\n{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}\n",
    );

    assert_eq!(out.len(), 3, "{out:?}");
    for refused in &out[..2] {
        assert_eq!(refused["error"]["code"], INVALID_REQUEST);
        assert_eq!(refused["id"], Value::Null);
    }
    // And the well-formed request behind them was still answered.
    assert_eq!(out[2]["id"], 3);
}

/// `serve` returns when the input does, rather than treating a closed stdin as
/// an error: a client that exits is a session that ended.
#[test]
fn the_session_ends_when_the_input_does() {
    let dir = scratch();
    assert!(replies(&path_of(&dir), "").is_empty());
}
