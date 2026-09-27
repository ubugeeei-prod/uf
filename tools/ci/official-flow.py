#!/usr/bin/env python3
"""Check real official releases and their editor services through uf."""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading


def send(process, message):
    body = json.dumps(message).encode()
    process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
    process.stdin.flush()


def read_messages(process, messages):
    try:
        while True:
            length = None
            while True:
                line = process.stdout.readline()
                if not line:
                    return
                if line == b"\r\n":
                    break
                if line.lower().startswith(b"content-length:"):
                    length = int(line.split(b":", 1)[1])
            messages.put(json.loads(process.stdout.read(length)))
    except Exception as error:
        messages.put(error)


def response(messages, identifier):
    while True:
        message = messages.get(timeout=90)
        if isinstance(message, Exception):
            raise message
        if message.get("id") == identifier:
            assert "error" not in message, message
            return message["result"]


def verify(uf, directory, version, env):
    directory.mkdir()
    (directory / "uf.config.js").write_text(f"export default {{flow:{{version:'{version}'}}}};\n")
    (directory / "value.js").write_text('"use flow";\nexport const value: number = "bad";\n')
    (directory / "ignored.js").write_text('"use js";\nexport const ignored: number = "bad";\n')
    result = subprocess.run([uf, "check", "--no-lint", "--json"], cwd=directory, env=env, capture_output=True, text=True, timeout=180)
    assert result.returncode == 1, (result.returncode, result.stdout, result.stderr)
    assert result.stdout.strip().startswith("{"), (version, result.stdout, result.stderr)
    checked = json.loads(result.stdout)["typeCheck"]
    assert checked["backend"] == "official-flow" and checked["version"] == version, checked
    diagnostics = checked["diagnostics"]
    assert any(item["primary"]["path"] == "value.js" for item in diagnostics), checked
    assert not any(item["primary"]["path"] == "ignored.js" for item in diagnostics), checked
    assert not (directory / ".flowconfig").exists(), "the author's config was replaced"

    text = '"use flow";\nexport const value: number = 42;\n'
    (directory / "value.js").write_text(text)
    process = subprocess.Popen([uf, "lsp"], cwd=directory, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    messages = queue.Queue()
    threading.Thread(target=read_messages, args=(process, messages), daemon=True).start()
    uri = (directory / "value.js").as_uri()
    try:
        send(process, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"rootUri": directory.as_uri(), "capabilities": {}}})
        assert response(messages, 1)["capabilities"]["hoverProvider"]
        send(process, {"jsonrpc": "2.0", "method": "initialized", "params": {}})
        send(process, {"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "javascript", "version": 1, "text": text}}})
        send(process, {"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri}, "position": {"line": 1, "character": 14}}})
        hover = response(messages, 2)
        assert hover and "number" in json.dumps(hover), hover
        # uf retains its own formatting while official Flow supplies types.
        send(process, {"jsonrpc": "2.0", "id": 3, "method": "textDocument/formatting", "params": {"textDocument": {"uri": uri}, "options": {"tabSize": 2, "insertSpaces": True}}})
        response(messages, 3)
        send(process, {"jsonrpc": "2.0", "id": 4, "method": "shutdown", "params": None})
        response(messages, 4)
        send(process, {"jsonrpc": "2.0", "method": "exit"})
        process.stdin.close()
        assert process.wait(timeout=15) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
    print(f"official Flow {version}: check, source modes, hover and uf formatting passed", flush=True)


def main():
    uf = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="uf-official-flow-") as temporary:
        root = Path(temporary).resolve()
        env = dict(os.environ, UF_STORE=str(root / "store"), UF_ROOTS=str(root / "roots"), CI="1", NO_COLOR="1")
        for version in ["0.330.0", "0.333.0"]:
            verify(uf, root / version, version, env)


if __name__ == "__main__":
    main()
