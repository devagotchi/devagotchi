#!/usr/bin/env python3
"""The phone control app in a real browser (docs/spec-phone-app.md, Tests), at phone size.

Starts `devagotchi hub serve` as a hosted hub (`tenancy.mode multi`) on SQLite at
http://localhost (a secure context, so the service worker runs without TLS). Then:

1. A person signs up from their AI client's sign-in page on an iPhone 13 (emulated); the AI
   adds a machine (a broker and `devagotchi link`, online) and a board with a ticket.
2. `/app` on the iPhone shows the machine online and the board, in one frame that does not
   scroll sideways at 390 px; the install banner says Share, then Add to Home Screen.
   Its machine page: online, its build, last seen now, the AI's command among the recent
   jobs, and Update now saying it is already on the hub's build (the hub hands out this
   binary's own build).
   Then the board, from Home: open the ticket, comment, close it (Undo shows), undo.
3. The manifest starts the installed app at `/app`, standalone.
4. The same on a Pixel 7 (emulated), signed in there with the email and password.
5. With the hub gone, reloading the app shows its offline page, not a browser error.

Between 2 and 3, the phone as one person's: Home greets them by name, the bar has their mark,
the Approvals tab counts what waits; a request reads at a glance with thumb-sized buttons, and
"approve and allow" makes the AI's next one go through. On their own page they set a name, dark
and a shorter "allow", and every screen follows.

Needs Python Playwright with its Chromium (tests/requirements-browser.txt). CI runs it on
ubuntu-24.04 (`e2e`, beside tests/workflow_browser.py); locally:

    python3 tests/app_browser.py --devagotchi "$PWD/target/debug/devagotchi"
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path

from analytics_browser import start
from operator_browser import call, free_port, wait_ready
from workflow_browser import Connector, downloads

EMAIL, PASSWORD = "phone@example.com", "a good password"


def device(p, name: str) -> dict:
    """Playwright's descriptor of a phone, for Chromium."""
    described = dict(p.devices[name])
    described.pop("default_browser_type", None)
    return described


def link_machine(binary: str, url: str, line: str, work: Path, processes: list) -> None:
    """A machine without root: a broker and `devagotchi link enroll` with the line's code."""
    code = re.search(r"dvg-[0-9a-f]{32}", line).group(0)
    script = urllib.request.urlopen(f"{url}/join/{code}", timeout=10).read().decode()
    name = re.search(r"^device='([a-z0-9-]+)'$", script, re.M).group(1)
    workspace = work / "workspace"
    workspace.mkdir()
    (work / "broker.json").write_text(json.dumps({
        "workspace": str(workspace), "exec_policy": "account",
        "output_limit_bytes": 65536, "max_operations": 16, "max_concurrent": 2,
    }))
    broker, ready = start([binary, "broker", "run", "--state-dir", str(work / "broker-state"),
                           "--config", str(work / "broker.json")], "broker", work)
    processes.append(broker)
    enrolled = subprocess.run(
        [binary, "link", "enroll", "--hub", url, "--device", name,
         "--broker", ready.split("ready:", 1)[1].strip(), "--state-dir", str(work / "link-state")],
        capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL,
        env={**os.environ, "DEVAGOTCHI_ENROLL_CODE": code},
    )
    assert enrolled.returncode == 0, enrolled.stderr
    linked, _ = start([binary, "link", "run", "--state-dir", str(work / "link-state")], "link", work)
    processes.append(linked)


def swipe(page, x0: float, x1: float, y: float) -> None:
    """A finger across the screen from x0 to x1 at height y, as the phone sends it."""
    cdp = page.context.new_cdp_session(page)
    point = lambda x: [{"x": x, "y": y}]
    cdp.send("Input.dispatchTouchEvent", {"type": "touchStart", "touchPoints": point(x0)})
    for step in range(1, 9):
        cdp.send("Input.dispatchTouchEvent", {"type": "touchMove", "touchPoints": point(x0 + (x1 - x0) * step / 8)})
    cdp.send("Input.dispatchTouchEvent", {"type": "touchEnd", "touchPoints": []})
    cdp.detach()


def no_sideways_scroll(page, width: int) -> None:
    for what in ("document.documentElement.scrollWidth", "document.querySelector('body.app > main').scrollWidth"):
        seen = page.evaluate(what)
        assert seen <= width, f"{what} is {seen} at {width} px: the page scrolls sideways"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--devagotchi", required=True, help="absolute path of the binary")
    parser.add_argument("--allow-skip", action="store_true",
                        help="exit 0 when Playwright is missing instead of failing")
    parser.add_argument("--channel", default=None,
                        help="a browser already installed (e.g. chrome) instead of Playwright's")
    args = parser.parse_args()
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("NOT RUN: Python Playwright is not installed")
        return 0 if args.allow_skip else 2

    work = Path(tempfile.mkdtemp(prefix="app-browser-"))
    state, config = work / "state", work / "hub.json"
    port = free_port()
    url = f"http://localhost:{port}"
    # The hub hands out this binary's own build, so Update now finds the machine on it.
    build = re.search(r"\(([^)]+)\)", subprocess.run([args.devagotchi, "--version"], capture_output=True,
                                                     text=True, check=True).stdout).group(1)
    dist = downloads(work, args.devagotchi)
    (dist / "VERSION").write_text(build + "\n")
    config.write_text(json.dumps({
        "listen": f"127.0.0.1:{port}",
        "public_url": url,
        "auth": {"mode": "hub"},
        "tenancy": {"mode": "multi", "starts_per_hour": 100},
        "account_boards": {"dir": str(work / "boards"), "free_limit": 5},
        "downloads": str(dist),
        "machines": {"auto_update": False},
        "brokers": [],
        "grants": [],
    }))
    processes = []
    hub = subprocess.Popen(
        [args.devagotchi, "hub", "serve", "--state-dir", str(state), "--config", str(config)],
        stdout=subprocess.DEVNULL,
        stderr=open(work / "hub.log", "w"),
    )
    processes.append(hub)
    try:
        wait_ready(url, hub, work / "hub.log")
        ai = Connector(url, "Claude")
        with sync_playwright() as p:
            browser = p.chromium.launch(channel=args.channel)
            iphone = browser.new_context(**device(p, "iPhone 13"))
            page = iphone.new_page()

            # 0. The landing page fits the phone.
            page.goto(url + "/")
            seen = page.evaluate("document.documentElement.scrollWidth")
            assert seen <= 390, f"the landing page is {seen} px wide at 390 px: it scrolls sideways"
            print("PASS the landing page fits an iPhone 13 (no sideways scroll at 390 px)")

            # 1. Sign up from the AI's sign-in page; the AI adds a machine and a board.
            page.goto(ai.authorize_url)
            page.locator("a[href*='tab=up']").click()
            assert page.locator("a[href*='tab=up'][aria-current=page]").count() == 1
            for field in ("email", "password"):
                box = page.locator(f"input[name={field}]").bounding_box()
                size = page.locator(f"input[name={field}]").evaluate("e => parseFloat(getComputedStyle(e).fontSize)")
                assert box["height"] >= 44 and box["width"] >= 300 and size >= 16, (field, box, size)
            page.locator("input[name=email]").fill(EMAIL)
            page.locator("input[name=password]").fill(PASSWORD)
            page.locator("form[action='/auth/password/new'] button[type=submit]").click()
            page.wait_for_url(re.compile(re.escape(ai.redirect) + r"\?.*code="), timeout=15_000)
            ai.finish()
            added = ai.tool("machine_add", {"name": "desk"})
            assert not added.get("isError"), added
            link_machine(args.devagotchi, url, added["structuredContent"]["linux"], work, processes)
            for _ in range(60):
                listed = ai.tool("machines")["structuredContent"]["machines"]
                if any(m["name"] == "desk" and m["status"] == "online" for m in listed):
                    break
                time.sleep(0.5)
            else:
                raise AssertionError(f"desk never came online: {listed}")
            bound = ai.tool("board_bind", {"project": "alpha"})
            assert not bound.get("isError"), bound
            made = ai.tool("board_create", {"project": "alpha", "title": "Check the backups"})
            assert not made.get("isError"), made
            ran = ai.tool("run", {"machine": "desk", "command": "echo from-the-phone"})
            assert not ran.get("isError"), ran
            print("PASS signed up on an iPhone; the AI added desk (online) and the board alpha")

            # 2. Home on the iPhone: the machine and the board, one frame, no sideways scroll.
            page.goto(f"{url}/app")
            assert urllib.parse.urlsplit(page.url).path == "/app", page.url
            machine = page.locator("#machines #machine-desk")
            assert "Online" in machine.inner_text(), machine.inner_text()
            assert "alpha" in page.locator("#boards").inner_text()
            assert page.locator("nav.tabs a[aria-current=page]").inner_text() == "Overview"
            no_sideways_scroll(page, 390)
            banner = page.locator("#install [data-os=iphone]")
            banner.wait_for(state="visible", timeout=5_000)
            assert "Add to Home Screen" in banner.inner_text()
            print("PASS /app on an iPhone 13 shows desk online and alpha; no sideways scroll at 390 px")

            # Swipe left from Overview: AgentHub. Swipe right: back to Overview.
            swipe(page, 300, 60, 430)
            page.wait_for_url(re.compile(r".*/app/boards$"), timeout=10_000)
            assert page.locator("nav.tabs a[aria-current=page]").inner_text() == "AgentHub"
            swipe(page, 60, 300, 430)
            page.wait_for_url(re.compile(r".*/app$"), timeout=10_000)
            # A swipe from the screen's edge is the phone's (Back), not the app's.
            swipe(page, 4, 250, 430)
            page.wait_for_timeout(600)
            assert urllib.parse.urlsplit(page.url).path == "/app", page.url
            print("PASS a swipe moves between Overview and AgentHub; one from the edge does not")

            # The Go to sheet: the board and the machine, sections that swipe, Escape closes it.
            page.locator("#focus-open").click()
            sheet = page.locator("#focus")
            sheet.wait_for(state="visible")
            page.locator("#focus [data-fill=boards] a", has_text="alpha").wait_for(timeout=5_000)
            page.locator("#focus [data-fill=machines] a", has_text="desk").wait_for(timeout=5_000)
            page.locator("#focus .sheet-chips a[href='#focus-machines']").click()
            page.locator("#focus .sheet-chips a[href='#focus-machines'][aria-current=true]").wait_for(timeout=5_000)
            no_sideways_scroll(page, 390)
            page.keyboard.press("Escape")
            sheet.wait_for(state="hidden")
            page.locator("#focus-open").click()
            sheet.wait_for(state="visible")
            page.locator("#focus [data-fill=boards] a", has_text="alpha").click()
            page.wait_for_url(re.compile(r".*/app/boards/alpha$"), timeout=10_000)
            print("PASS the Go to sheet lists alpha and desk, its chips move between sections, Escape closes it, a board opens from it")
            page.goto(f"{url}/app")

            # Its machine page: status, build, last seen, recent jobs, Update now.
            page.locator("#machine-desk a").click()
            page.wait_for_url(re.compile(r".*/app/machines/desk$"))
            assert page.locator("h1").inner_text() == "desk"
            assert "Online" in page.locator("#machine").inner_text()
            assert page.locator("#build").inner_text() == build
            assert page.locator("#last-seen").inner_text() == "Now"
            jobs = page.locator("table.jobs").inner_text()
            assert "echo from-the-phone" in jobs and "succeeded" in jobs, jobs
            no_sideways_scroll(page, 390)
            page.locator("#update-now").click()
            page.wait_for_url(re.compile(r".*/app/machines/desk\?state=done.*"))
            result = page.locator("#update-result").inner_text()
            assert "Update: done" in result and f"already on {build}" in result, result
            print("PASS desk's page: online, its build, last seen now, the AI's job, Update now answered")

            # The board from Home; a ticket: comment, close, undo.
            page.goto(f"{url}/app")
            page.locator("#board-alpha a").click()
            page.wait_for_url(re.compile(r".*/app/boards/alpha$"))
            no_sideways_scroll(page, 390)
            page.locator("#tickets a", has_text="Check the backups").click()
            page.wait_for_url(re.compile(r".*/app/tickets/alpha/[^/?]+$"))
            assert page.locator("h1").inner_text() == "Check the backups"
            no_sideways_scroll(page, 390)
            page.locator("#comment").fill("Looked at it from the phone")
            page.locator("button", has_text="Comment").click()
            page.wait_for_url(re.compile(r".*\?done=comment$"))
            assert "Looked at it from the phone" in page.locator("#notes").inner_text()
            page.locator("#close").click()
            page.wait_for_url(re.compile(r".*\?done=close&from=ready$"))
            assert page.locator("#state").inner_text() == "cancelled"
            assert page.locator("#undo button").is_visible()
            page.locator("#undo button").click()
            page.wait_for_url(re.compile(r".*\?done=undo$"))
            assert page.locator("#state").inner_text() == "ready"
            assert page.locator("#flash").inner_text() == "Reopened."
            ticket = ai.tool("board_get", {"project": "alpha", "id": made["structuredContent"]["id"]})
            content = ticket["structuredContent"]
            assert content["ticket"]["state"] == "ready", content
            assert [n["body"] for n in content["notes"]] == ["Looked at it from the phone"], content
            print("PASS a ticket from the board: commented, closed, undone; the AI sees it")

            # A question left for the AI from the phone: its section, and it reaches the AI.
            page.locator("#comment").fill("Which region did you pick?")
            page.locator("#note-kind").select_option("question")
            page.locator("#note-to").fill("claude")
            page.locator("button", has_text="Comment").click()
            page.wait_for_url(re.compile(r".*\?done=comment$"))
            section = page.locator("#notes-question").inner_text()
            assert "Which region did you pick?" in section and "for claude" in section, section
            no_sideways_scroll(page, 390)
            # (Inboxes are looked at no more than every 10 seconds per caller.)
            for _ in range(15):
                heard = ai.tool("board_list", {"project": "alpha"})
                texts = " ".join(block.get("text", "") for block in heard["content"])
                if "Which region did you pick?" in texts:
                    break
                time.sleep(1)
            else:
                raise AssertionError(f"the note never reached the AI: {texts[-400:]}")
            assert "LEFT FOR YOU" in texts, texts[-400:]
            print("PASS a note left from the phone, in its section, reached the AI on its next call")

            # The phone as one person's: greeted by name, their mark, what waits on the tab.
            restart = {"project": "alpha", "title": "Restart the api", "what": "systemctl restart api",
                       "why": "a new build", "undo": "start the old build", "kind": "restart",
                       "target": "desk"}
            asked = ai.tool("approval_request", restart)
            assert not asked.get("isError"), asked
            page.goto(f"{url}/app")
            assert page.locator("#hello h1").inner_text() == "Hi Phone"
            assert page.locator("header.bar #me-link .avatar").inner_text() == "P"
            assert page.locator("nav.tabs a").count() == 5
            badge = page.locator("#approvals-badge")
            badge.wait_for(state="visible", timeout=5_000)
            assert badge.inner_text() == "1", badge.inner_text()
            no_sideways_scroll(page, 390)

            # A request at a glance, the buttons big enough for a thumb; allowing it ahead of
            # time lets the AI's next one through.
            page.locator("#review").click()
            page.wait_for_url(re.compile(r".*/approvals$"))
            card = page.locator("section.ask")
            assert card.locator(".tag.kind").inner_text() == "restart on desk"
            assert card.locator("dd.cmd").inner_text() == "systemctl restart api"
            assert "just now" in card.locator(".meta").inner_text()
            assert card.locator("button.approve").first.bounding_box()["height"] >= 48
            no_sideways_scroll(page, 390)
            card.locator("form.allow button").click()
            page.wait_for_url(re.compile(r".*/approvals\?done="))
            again = ai.tool("approval_request", restart)["structuredContent"]
            assert again["status"] == "approved", again
            print("PASS greeted by name; the count on the tab; a request at a glance; allowed ahead of time")

            # Their own page: who they are, and what they set is what every screen does.
            page.locator("nav.tabs a", has_text="You").click()
            page.wait_for_url(re.compile(r".*/app/me$"))
            assert page.locator("nav.tabs a[aria-current=page]").inner_text() == "You"
            assert page.locator("#login").inner_text() == EMAIL
            no_sideways_scroll(page, 390)
            light = page.evaluate("getComputedStyle(document.body).backgroundColor")
            page.locator("#prefs input[name=name]").fill("Grace Hopper")
            page.locator("#prefs select[name=theme]").select_option("dark")
            page.locator("#prefs select[name=allow_days]").select_option("7")
            page.locator("#prefs button[type=submit]").click()
            page.wait_for_url(re.compile(r".*/app/me\?saved=1.*"))
            assert page.locator("html").get_attribute("data-theme") == "dark"
            assert page.evaluate("getComputedStyle(document.body).backgroundColor") != light
            assert page.locator("#me-link .avatar").inner_text() == "GH"
            no_sideways_scroll(page, 390)
            ai.tool("approval_request", {**restart, "kind": "deploy", "title": "Deploy main"})
            page.goto(f"{url}/approvals")
            assert "for 7 days" in page.locator("form.allow button").inner_text()
            page.goto(f"{url}/app")
            assert page.locator("#hello h1").inner_text() == "Hi Grace"
            print("PASS their own page: a name, dark and a shorter allow, and every screen follows")

            # 3. The installed app starts at /app, standalone.
            status, _, body = call(f"{url}/manifest.webmanifest")
            manifest = json.loads(body)
            assert status == 200 and manifest["start_url"] == "/app", manifest
            assert manifest["display"] == "standalone", manifest
            print("PASS the manifest has start_url /app and display standalone")

            # 4. A Pixel 7, signed in with the email and password.
            pixel = browser.new_context(**device(p, "Pixel 7"))
            android = pixel.new_page()
            android.goto(f"{url}/app")
            assert urllib.parse.urlsplit(android.url).path == "/auth/login", android.url
            android.locator("form[action='/auth/password'] input[name=email]").fill(EMAIL)
            android.locator("form[action='/auth/password'] input[name=password]").fill(PASSWORD)
            android.locator("form[action='/auth/password'] button[type=submit]").click()
            android.wait_for_url(re.compile(r".*/app$"), timeout=15_000)
            assert "Online" in android.locator("#machine-desk").inner_text()
            assert "alpha" in android.locator("#boards").inner_text()
            no_sideways_scroll(android, android.viewport_size["width"])
            print("PASS /app on a Pixel 7 after signing in there")

            # 5. Offline: the service worker keeps the frame; with the hub gone a reload
            # shows the app's offline page instead of a browser error.
            page.goto(f"{url}/app")
            page.wait_for_function("navigator.serviceWorker && navigator.serviceWorker.controller",
                                   timeout=15_000)
            page.wait_for_function(
                "caches.open('devagotchi-app-2').then(c => c.match('/app/offline')).then(Boolean)",
                timeout=15_000)
            hub.terminate()
            hub.wait(timeout=10)
            page.reload()
            assert page.locator("h1").inner_text() == "You're offline", page.content()
            assert page.locator("nav.tabs").count() == 1
            no_sideways_scroll(page, 390)
            print("PASS with the hub gone, reloading the app shows its offline page")
            browser.close()
    finally:
        for process in reversed(processes):
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
    print("APP BROWSER PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
