"""Real owner UI and public API setup for the self-hosted command journey.

No database seeding or test-only authorization: the owner binds the OAuth client,
acknowledges the current native-execution risks, then approves the agent's request.
"""

import re


def grant_command_access(page, ai, machine: str) -> str:
    url = ai.hub
    denied = ai.tool("run", {"machine": machine, "command": "echo not-authorized"})
    assert denied.get("isError") and denied["structuredContent"]["code"] == "not_granted", denied

    page.goto(f"{url}/app/agents/new")
    page.locator("input[name=id]").fill("phone-agent")
    page.locator("input[name=name]").fill("Phone agent")
    page.locator("form[action='/app/agents'] button[type=submit]").click()
    page.wait_for_url(re.compile(r".*/app/agents/phone-agent\?.*"))
    binding = page.locator("form[action='/app/agents/phone-agent/oauth']").filter(
        has=page.locator(f"input[name=client][value='{ai.client}']"))
    binding.locator("input[name=enabled]").check()
    binding.locator("button[type=submit]").click()
    assert "Currently assigned to: phone-agent (Enabled)" in page.inner_text("main")

    def get(path):
        response = page.request.get(url + path)
        assert response.ok, (path, response.status, response.text())
        return response.json()

    settings = get("/api/v1/access-settings")
    risks = {row["id"]: row for row in get("/api/v1/risks")["rows"]}
    policy = page.request.put(
        f"{url}/api/v1/sandbox-policies",
        headers={"Origin": url, "Idempotency-Key": "phone-native-policy"},
        data={"version": 0, "allowed": True, "expires_at": None,
              "acknowledged": [{"id": rid, "version": risks[rid]["version"]}
                               for rid in settings["unsandboxed_risk_ids"]]},
    )
    assert policy.ok, (policy.status, policy.text())

    requested = ai.tool("request_access", {
        "request_id": "phone-command-access", "machine": machine,
        "selection": {"kind": "items", "items": [
            {"capability": "exec", "sandbox_level": "unsandboxed", "selector": {}}]},
        "duration": "1h", "reason": "Run the owner's phone acceptance command on this test machine.",
    })
    assert not requested.get("isError"), requested
    group = requested["structuredContent"]["result"]
    assert len(group["request_ids"]) == 1, group
    page.goto(f"{url}/app/access/requests/{group['request_ids'][0]}")
    assert "Phone agent" in page.inner_text("main"), page.inner_text("main")
    page.locator("button[name=action][value=approve]").click()
    approved = ai.tool("wait_for_access", {"request_id": group["group_id"], "timeout_s": 0})
    assert not approved.get("isError"), approved
    assert approved["structuredContent"]["status"] == "approved", approved
    print("PASS unbound execution refused; owner registered, bound and approved the real agent")
    identity = ai.tool("whoami")["structuredContent"]
    assert identity["agent"] == f"@agent:{identity['subject']}:phone-agent", identity
    return identity["agent"]
