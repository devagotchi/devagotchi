"""Original AgentHub live-board behavior through actual browser, MCP and owner forms."""
import re


def live_board(original, ai, url, email, password):
    from playwright.sync_api import expect
    def tool(name, **args):
        answer = ai.tool(name, {"project": "alpha", **args})
        assert not answer.get("isError"), answer
        return answer["structuredContent"]

    page = original.context.new_page()
    page.set_viewport_size({"width": 1280, "height": 600})
    page.goto(url + "/app/boards/alpha")
    expect(page.locator("#board-live-status")).to_have_text("Live updates connected.")
    page.evaluate("window.liveBoardMarker = 'same document'")
    page.locator("#q").fill("Unsubmitted search")
    positions = page.evaluate("""() => {
        const main = document.querySelector('main'), columns = document.querySelector('.board-columns');
        main.scrollTop = 60; columns.scrollLeft = 120;
        return [main.scrollTop, columns.scrollLeft];
    }""")
    assert positions[0] > 0 and positions[1] > 0, positions
    task = tool("board_create", title="Live board arrival")
    card = page.locator("#ticket-" + task["id"])
    expect(card).to_contain_text("Live board arrival")
    expect(page.locator("#q")).to_have_value("Unsubmitted search")
    expect(page.locator("#q")).to_be_focused()
    assert page.evaluate("[document.querySelector('main').scrollTop, document.querySelector('.board-columns').scrollLeft]") == positions
    tool("board_update", id=task["id"], version=task["version"],
                   title="Live board revised <script>untrusted</script>")
    expect(card).to_contain_text("Live board revised <script>untrusted</script>")
    assert card.locator("script").count() == 0
    assert page.evaluate("window.liveBoardMarker") == "same document"

    owner = original.context.new_page()
    owner.goto(url + "/app/tickets/alpha/" + task["id"])
    owner.locator("#close").click()
    owner.wait_for_url(re.compile(r".*\?done=close&from=ready$"))
    expect(page.locator('.board-column[data-state="cancelled"] #ticket-' + task["id"])).to_have_count(1)
    expect(page.locator("#q")).to_have_value("Unsubmitted search")
    owner.close()

    # Headless Chromium marks every tab visible. Emulate the browser visibility event;
    # the real server still supplies the change and the browser performs/cancels its fetches.
    reads = []
    page.on("request", lambda request: reads.append(request.url) if request.resource_type == "fetch" else None)
    page.evaluate("""() => {
        Object.defineProperty(document, 'hidden', {configurable: true, value: true});
        document.dispatchEvent(new Event('visibilitychange'));
    }""")
    paused = len(reads)
    hidden = tool("board_create", title="Arrived while hidden")
    page.wait_for_timeout(300)
    assert len(reads) == paused, reads[paused:]
    expect(page.locator("#ticket-" + hidden["id"])).to_have_count(0)
    page.evaluate("""() => {
        delete document.hidden;
        document.dispatchEvent(new Event('visibilitychange'));
    }""")
    expect(page.locator("#ticket-" + hidden["id"])).to_have_count(1)
    expect(page.locator("#q")).to_have_value("Unsubmitted search")

    filtered = url + "/app/boards/alpha?state=cancelled&q=Live"
    page.goto(filtered)
    expect(page.locator("#board-live-status")).to_have_text("Live updates connected.")
    page.locator("#q").fill("Keep this draft")
    tool("board_create", title="Excluded unrelated work")
    # A real event must be observed, while the saved filters continue selecting only the closed task.
    before = page.locator('[data-page="board"]').get_attribute("data-board-seq")
    tool("board_note", id=task["id"], body="Wake the filtered board")
    page.wait_for_function("old => document.querySelector('[data-page=board]').dataset.boardSeq !== old", arg=before)
    assert page.url == filtered
    expect(page.locator(".task-card")).to_have_count(1)
    expect(page.locator("#q")).to_have_value("Keep this draft")
    expect(page.locator("#q")).to_be_focused()
    assert page.locator(".board-snapshot a").get_attribute("href") == "/app/boards/alpha?state=cancelled&q=Live"

    blocked = "**/app/boards/alpha**"
    page.route(blocked, lambda route: route.abort())
    tool("board_note", id=task["id"], body="Wake the disconnected board")
    expect(page.locator("#board-live-status")).to_contain_text("Updates paused", timeout=10000)
    expect(page.locator(".task-card")).to_have_count(1)
    page.unroute(blocked)
    expect(page.locator("#board-live-status")).to_have_text("Live updates connected.", timeout=10000)
    expect(page.locator("#q")).to_have_value("Keep this draft")
    page.close()

    plain_context = original.context.browser.new_context(
        storage_state=original.context.storage_state(), java_script_enabled=False)
    plain = plain_context.new_page()
    plain.goto(filtered)
    assert plain.locator(".task-card").count() == 1
    assert "Showing saved board" in plain.locator("#board-live-status").inner_text()
    assert plain.locator(".board-snapshot a").get_attribute("href") == "/app/boards/alpha?state=cancelled&q=Live"
    plain_context.close()

    # A server-issued lease counts down without another event or an authority mutation.
    leased = tool("board_create", title="Live lease clock")
    lease = tool("board_claim", id=leased["id"], ttl_ms=125000)["lease"]
    timer_context = original.context.browser.new_context(storage_state=original.context.storage_state())
    timer = timer_context.new_page()
    timer.clock.install()
    # Select the actual configured active state, never assume a workflow label/key.
    timer.goto(url + "/app/boards/alpha?state=" + lease["ticket"]["state"])
    expect(timer.locator("#board-live-status")).to_have_text("Live updates connected.")
    label = timer.locator("#ticket-" + leased["id"] + " .lease")
    initial = label.inner_text()
    minutes = int(re.search(r"(\d+) min", initial).group(1))
    assert minutes >= 2, initial
    timer.route(blocked, lambda route: route.abort())
    timer.clock.fast_forward(60000)
    expect(label).to_have_text(f"Lease: {minutes - 1} min remaining")
    timer_context.close()
    tool("board_release", id=leased["id"], fence=lease["fence"], reason="Browser countdown verified")

    # Separate real password session: revoking it must not revoke the original test session.
    context = original.context.browser.new_context()
    signed = context.new_page()
    signed.goto(url + "/app")
    signed.locator("form[action='/auth/password'] input[name=email]").fill(email)
    signed.locator("form[action='/auth/password'] input[name=password]").fill(password)
    signed.locator("form[action='/auth/password'] button[type=submit]").click()
    signed.wait_for_url(url + "/app")
    signed.goto(url + "/app/boards/alpha")
    expect(signed.locator("#board-live-status")).to_have_text("Live updates connected.")
    logout = context.request.post(url + "/auth/logout", headers={"Origin": url}, max_redirects=0)
    assert logout.status == 303, logout.status
    tool("board_create", title="Must not arrive after session ended")
    expect(signed.locator("#board-live-status")).to_contain_text("Board access ended", timeout=10000)
    expect(signed.locator(".task-card")).to_have_count(0)
    assert "Must not arrive after session ended" not in signed.locator("main").inner_text()
    context.close()
    original.bring_to_front()
    print("PASS live board create/edit/close, escaped titles, draft/focus/filters, reconnect, lease countdown and ended session")
