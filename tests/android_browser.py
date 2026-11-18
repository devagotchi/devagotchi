#!/usr/bin/env python3
"""The one-user hub on an Android phone, in a real browser (one-user edition, phase 4).

Starts `devagotchi hub serve` as the self-hosted, one-owner hub (`auth.mode hub`, the default
single tenancy) on Postgres (`DEVAGOTCHI_HUB_STORAGE_URL`) at http://localhost, a secure
context, so the service worker and passkeys run without TLS. A Pixel 7 (emulated, 412 px wide)
claims the hub from the link its log printed, with a virtual passkey, and is its one owner.

This is the home of the Android checks, so the phone's increments add theirs here instead of
colliding in tests/app_browser.py (the hosted hub's phone app, iPhone and Pixel):

1. The owner claims the hub on the Pixel 7.
2. `/app`, `/approvals` and `/app/me` open as the owner and none scrolls sideways at 412 px.

Needs Python Playwright with its Chromium (tests/requirements-browser.txt). CI runs it on
ubuntu-24.04 (`e2e`, beside tests/app_browser.py); locally, with a throwaway Postgres
(tests/throwaway_postgres.sh):

    DEVAGOTCHI_HUB_STORAGE_URL=postgres://postgres@127.0.0.1:55432/android \\
        python3 tests/android_browser.py --channel chrome --devagotchi "$PWD/target/debug/devagotchi"
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
import time
import urllib.parse
from pathlib import Path

from claim_browser import free_port, serve, settled, stop, wait_ready

PHONE = "Pixel 7"
WIDTH = 412


def device(p) -> dict:
    """Playwright's descriptor of the phone, for Chromium."""
    described = dict(p.devices[PHONE])
    described.pop("default_browser_type", None)
    assert described["viewport"]["width"] == WIDTH, described["viewport"]
    return described


def passkey(page) -> None:
    """A phone's own passkey: the Chrome DevTools virtual authenticator, always verified."""
    cdp = page.context.new_cdp_session(page)
    cdp.send("WebAuthn.enable")
    cdp.send("WebAuthn.addVirtualAuthenticator", {"options": {
        "protocol": "ctap2", "transport": "internal", "hasResidentKey": True,
        "hasUserVerification": True, "isUserVerified": True,
        "automaticPresenceSimulation": True,
    }})


def sideways(page) -> list:
    """What scrolls sideways at the phone's width: the page, and the app's frame if it has one."""
    found = []
    for what in ("document.documentElement.scrollWidth",
                 "(document.querySelector('body > main') || document.body).scrollWidth"):
        seen = page.evaluate(what)
        if seen > WIDTH:
            found.append(f"{what} is {seen} px")
    return found


def open_as_owner(page, url: str, path: str) -> None:
    """`path` opens for the signed-in owner (no sign-in in between) and fits the phone."""
    page.goto(url + path)
    landed = urllib.parse.urlsplit(page.url).path
    assert landed == path, f"{path} went to {landed}"
    wide = sideways(page)
    assert not wide, f"{path} scrolls sideways at {WIDTH} px: {wide}"


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

    work = Path(tempfile.mkdtemp(prefix="android-browser-"))
    state, config = work / "state", work / "hub.json"
    port = free_port()
    url = f"http://localhost:{port}"
    # The self-hosted hub as install.sh makes it: its own sign-in, one owner (no tenancy).
    config.write_text(json.dumps({
        "listen": f"127.0.0.1:{port}",
        "public_url": url,
        "auth": {"mode": "hub"},
        "machines": {"auto_update": False},
        "brokers": [],
        "grants": [],
    }))
    hub = serve(args.devagotchi, state, config, work / "hub.log")
    try:
        wait_ready(url, hub, work / "hub.log")
        printed = re.findall(rf"{re.escape(url)}/claim#[0-9A-Z-]+", (work / "hub.log").read_text())
        assert len(printed) == 1, "the hub prints one claim link when it starts"

        with sync_playwright() as p:
            browser = p.chromium.launch(channel=args.channel)
            phone = browser.new_context(**device(p))
            page = phone.new_page()
            passkey(page)

            # 1. The one owner claims the hub on the phone.
            page.goto(printed[0])
            deadline = time.time() + 5
            while page.evaluate("location.hash") and time.time() < deadline:
                time.sleep(0.05)
            page.fill("#claim-name", "Jesse")
            page.click("#passkey-claim")
            page.wait_for_url(f"{url}/account", timeout=15_000)
            assert "You own this hub" in page.inner_text("main"), settled(page)
            print(f"PASS the owner claimed the one-user hub on a {PHONE}")

            # 2. The app's pages as the owner, at the phone's width.
            for path in ("/app", "/approvals", "/app/me"):
                open_as_owner(page, url, path)
                print(f"PASS {path} opens as the owner on a {PHONE}; no sideways scroll at {WIDTH} px")
            browser.close()
    finally:
        stop(hub)
    print("ANDROID BROWSER PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
