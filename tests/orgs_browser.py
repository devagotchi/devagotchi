#!/usr/bin/env python3
"""The organisation pages in a real browser (`web/orgs.rs`, increment 116).

Starts `devagotchi hub serve` as a hosted hub (`tenancy.mode multi`) at http://localhost
on the Postgres DEVAGOTCHI_HUB_STORAGE_URL names (an empty database). Two people sign up
with an email and a password, each in a browser of their own, and then:
1. Alice makes an organisation on `/orgs`, a project and a team on its page, and an invite;
   the invite's link is shown once.
2. Bob opens the link signed out: the page keeps the invite (its token never leaves the
   browser), he signs up, comes back to `/invite` and joins with one press.
3. Bob's organisation page says he is a member; he opens the project's board in the app,
   opens its getting-started ticket and comments on it.
4. Alice makes Bob a viewer from his row; Bob's next comment is refused and says why.
5. On a phone-sized screen the organisation page scrolls down, never sideways.
Needs Python Playwright (tests/requirements-browser.txt). CI runs it on ubuntu-24.04
(`e2e`); locally:
    DEVAGOTCHI_HUB_STORAGE_URL=postgres://... python3 tests/orgs_browser.py --channel chrome \
        --devagotchi "$PWD/target/debug/devagotchi"
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from operator_browser import free_port, wait_ready


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--devagotchi", required=True, help="absolute path of the binary")
    parser.add_argument("--channel", default=None,
                        help="a browser already installed (e.g. chrome) instead of Playwright's")
    args = parser.parse_args()
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("NOT RUN: Python Playwright is not installed")
        return 2
    if not os.environ.get("DEVAGOTCHI_HUB_STORAGE_URL", "").startswith(("postgres://", "postgresql://")):
        sys.exit("set DEVAGOTCHI_HUB_STORAGE_URL to an empty Postgres database for this script's hub")

    work = Path(tempfile.mkdtemp(prefix="orgs-browser-"))
    port = free_port()
    url = f"http://localhost:{port}"
    config = work / "hub.json"
    config.write_text(json.dumps({
        "listen": f"127.0.0.1:{port}",
        "public_url": url,
        "auth": {"mode": "hub"},
        "tenancy": {"mode": "multi", "starts_per_hour": 100, "anonymous_connect": True},
        "account_boards": {},
        "brokers": [],
        "grants": [],
    }))
    log = work / "hub.log"
    hub = subprocess.Popen(
        [args.devagotchi, "hub", "serve", "--state-dir", str(work / "state"), "--config", str(config)],
        stdout=subprocess.DEVNULL, stderr=open(log, "w"),
    )
    try:
        wait_ready(url, hub, log)
        with sync_playwright() as p:
            browser = p.chromium.launch(channel=args.channel)
            alice = browser.new_context().new_page()
            bob = browser.new_context().new_page()

            def sign_up(page, email, back):
                page.goto(f"{url}/auth/login?tab=up&return_to={back}")
                page.fill('form[action="/auth/password/new"] input[name="email"]', email)
                page.fill('form[action="/auth/password/new"] input[name="password"]', "a long password for the browser test")
                page.click('form[action="/auth/password/new"] button[type="submit"]')

            # 1. Alice: an organisation, a project, a team and an invite.
            sign_up(alice, "alice@example.com", "%2Forgs")
            alice.wait_for_url(f"{url}/orgs", timeout=15_000)
            alice.fill('#new-org input[name="org"]', "acme")
            alice.fill('#new-org input[name="name"]', "Acme Corp")
            alice.click('#new-org button[type="submit"]')
            alice.wait_for_url(f"{url}/orgs/acme?done=1")
            assert "Acme Corp" in alice.inner_text("h1")
            assert alice.is_visible("#flash")
            alice.fill('#projects input[name="project"]', "site")
            alice.click('#projects form[action="/orgs/acme/projects"] button[type="submit"]')
            alice.wait_for_url(f"{url}/orgs/acme?done=1")
            assert alice.is_visible("#project-site"), alice.inner_text("#projects")
            alice.fill('#teams input[name="team"]', "web")
            alice.fill('#teams input[name="name"]', "Web")
            alice.click('#teams form[action="/orgs/acme/teams"] button[type="submit"]')
            alice.wait_for_url(f"{url}/orgs/acme?done=1")
            assert alice.is_visible("#team-web")
            alice.select_option('#invites select[name="role"]', "member")
            alice.fill('#invites input[name="note"]', "for Bob")
            alice.click('#invites form[action="/orgs/acme/invites"] button[type="submit"]')
            link = alice.input_value("#invite-link")
            assert re.fullmatch(rf"{re.escape(url)}/invite#inv_[A-Za-z0-9_-]+", link), link
            print("PASS Alice made an organisation, a project, a team and an invite; its link shown once")

            # 2. Bob opens the link signed out, signs up, and joins.
            bob.goto(link)
            assert bob.is_visible("#invite-sign-in")
            assert bob.evaluate("location.hash") == "", "the token left the address bar"
            sign_up(bob, "bob@example.com", "%2Finvite")
            bob.wait_for_url(f"{url}/invite", timeout=15_000)
            assert bob.input_value("#invite-token").startswith("inv_"), "the page kept the invite"
            bob.click("#invite-join")
            bob.wait_for_url(f"{url}/orgs/acme?done=1")
            assert "member" in bob.inner_text("#your-role")
            print("PASS Bob kept the invite through signing up and joined with one press")

            # 3. Bob opens the project's board and comments on its ticket.
            bob.click("#project-site a")
            bob.wait_for_url(re.compile(re.escape(f"{url}/app/boards/acme%2Fsite")))
            bob.click("main a[href^='/app/tickets/']")
            bob.wait_for_url(re.compile(re.escape(f"{url}/app/tickets/acme%2Fsite/")))
            ticket = bob.url.split("?")[0]
            bob.fill('form[action$="/comment"] textarea[name="body"]', "on it")
            bob.click('form[action$="/comment"] button[type="submit"]')
            bob.wait_for_url(re.compile(r".*done=comment.*"))
            print("PASS Bob opened the organisation's board and commented on its ticket")

            # 4. Alice makes Bob a viewer; his next comment is refused, with why.
            alice.goto(f"{url}/orgs/acme")
            row = alice.locator("#members li", has=alice.locator("button", has_text="Remove"))
            row.locator('select[name="role"]').select_option("viewer")
            row.locator('form[action="/orgs/acme/members"] button[type="submit"]').click()
            alice.wait_for_url(f"{url}/orgs/acme?done=1")
            bob.goto(ticket)
            bob.fill('form[action$="/comment"] textarea[name="body"]', "not now")
            bob.click('form[action$="/comment"] button[type="submit"]')
            bob.wait_for_url(re.compile(r".*done=no.*"))
            assert "viewer" in bob.inner_text("main"), bob.inner_text("main")
            print("PASS made a viewer, Bob's next comment was refused and said why")

            # 5. A phone-sized screen: the organisation page scrolls down, never sideways.
            phone = browser.new_context(viewport={"width": 390, "height": 844}, storage_state=alice.context.storage_state()).new_page()
            phone.goto(f"{url}/orgs/acme")
            wide = phone.evaluate("document.documentElement.scrollWidth - document.documentElement.clientWidth")
            assert wide <= 0, f"{wide}px of sideways scroll"
            print("PASS the organisation page fits a phone's width")
            browser.close()
    finally:
        hub.terminate()
        hub.wait(10)
    print("ORGS BROWSER PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
