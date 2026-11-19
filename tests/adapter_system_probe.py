"""Account-policy capabilities exercised against owned, disposable fixtures only."""
import http.server
import os
import subprocess
import sys
import threading
import uuid


def check_system(call, rpc, cwd, git):
    page = call("search_files", {"path": cwd, "pattern": "lines.txt"})
    assert any(row["path"] == "lines.txt" for row in page["matches"]), page
    config = call("get_config", {})
    changed = call("set_config_value", {"key": "max_concurrent", "value": config["max_concurrent"]})
    assert changed["max_concurrent"] == config["max_concurrent"], changed

    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"],
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        processes = call("list_processes", {"filter": "python", "max_results": 1000})
        assert any(p["pid"] == child.pid for p in processes["processes"]), processes
        killed = call("kill_process", {"pid": child.pid, "force": True})
        assert killed["pid"] == child.pid and killed["exited"], killed
        child.wait(timeout=5)
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)

    class Page(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            body = b"probe-local-web"
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Page)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        page = call("read_url", {"url": f"http://127.0.0.1:{server.server_port}/page"})
        assert page.get("text") == "probe-local-web" and page["status"] == 200, page
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    command = "Start-Sleep -Seconds 60" if os.name == "nt" else "sleep 60"
    started = call("operation_start", {"idempotency_key": str(uuid.uuid4()), "command": command,
                                      "deadline_ms": 90000, "interactive": {"mode": "terminal"}})
    operation = started["operation"]["operation_id"]
    try:
        resized = call("operation_resize", {"operation_id": operation, "cols": 100, "rows": 30})
        assert resized["operation_id"] == operation, resized
    finally:
        stopped = call("operation_cancel", {"operation_id": operation})
        settled = call("operation_wait", {"operation_id": operation, "timeout_ms": 30000})
        assert settled["state"] == "cancelled", (stopped, settled)
