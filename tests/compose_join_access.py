"""Public human and agent setup for the disposable image-join acceptance journey."""

import json
import http.cookies
import secrets
import urllib.parse

from operator_browser import call


def owner_cookie(hub, link):
    """Redeem the real host-issued setup link; no database seeding or public signup."""
    token = urllib.parse.unquote(urllib.parse.urlsplit(link).fragment)
    assert token, "the owner command returned no setup token"
    status, headers, body = call(hub + "/owner/password", urllib.parse.urlencode({
        "link": token, "email": "image-owner@example.test", "password": secrets.token_urlsafe(24),
    }).encode(), {"Origin": hub, "Content-Type": "application/x-www-form-urlencoded"})
    assert status == 303, (status, body)
    jar = http.cookies.SimpleCookie()
    for value in headers.get_all("Set-Cookie") or []:
        jar.load(value)
    return f"dg_session={jar['dg_session'].value}"


def human(person, path, value=None, *, method=None, key=None):
    headers = {"Cookie": person.cookie, "Origin": person.hub}
    if key:
        headers["Idempotency-Key"] = key
    data = None
    if value is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(value).encode()
    status, _, body = call(person.hub + path, data, headers, method)
    assert status == 200, (path, status, body)
    return json.loads(body)


def form(person, path, fields):
    status, _, body = call(person.hub + path, urllib.parse.urlencode(fields).encode(), {
        "Cookie": person.cookie, "Origin": person.hub,
        "Content-Type": "application/x-www-form-urlencoded",
    })
    assert status == 303, (path, status, body)


def refused(person, machine, code):
    result = person.tool("run", {"machine": machine, "command": "echo must-not-run"})
    assert result.get("isError") and result["structuredContent"]["code"] == code, result


def register(person, machine):
    """The signed-in person binds its actual OAuth client; registration alone grants nothing."""
    refused(person, machine, "not_granted")
    subject = person.tool("whoami")["structuredContent"]["subject"]
    form(person, "/app/agents", {
        "account": subject, "id": "image-join", "name": "Image join agent",
        "kind": "service", "roles": "worker", "wip": "", "version": 0,
        "enabled": "true", "request_id": "image-join-register",
    })
    form(person, "/app/agents/image-join/oauth", {
        "account": subject, "client": person.client, "version": 0,
        "enabled": "true", "request_id": "image-join-bind",
    })
    identity = person.tool("whoami")["structuredContent"]
    assert identity["agent"] == f"@agent:{subject}:image-join", identity
    refused(person, machine, "grant_required")
    settings = human(person, "/api/v1/access-settings")
    risks = {row["id"]: row for row in human(person, "/api/v1/risks")["rows"]}
    human(person, "/api/v1/sandbox-policies", {
        "version": 0, "allowed": True, "expires_at": None,
        "acknowledged": [{"id": rid, "version": risks[rid]["version"]}
                         for rid in settings["unsandboxed_risk_ids"]],
    }, method="PUT", key="image-join-policy")
    print("PASS unregistered and unapproved execution refused; human bound the OAuth agent", flush=True)


def approve(person, machine):
    """The agent requests; its signed-in human independently approves the current request."""
    refused(person, machine, "grant_required")
    requested = person.tool("request_access", {
        "request_id": f"image-join-{machine}", "machine": machine,
        "selection": {"kind": "items", "items": [
            {"capability": "exec", "sandbox_level": "unsandboxed", "selector": {}}]},
        "duration": "1h", "reason": "Run echo on this disposable image-install acceptance machine.",
    })
    assert not requested.get("isError"), requested
    group = requested["structuredContent"]["result"]
    assert len(group["request_ids"]) == 1, group
    found = human(person, f"/api/v1/access-requests/{group['group_id']}")
    assert found["status"] == "pending" and len(found["items"]) == 1, found
    pending = human(person, "/api/v1/access-requests?status=pending")
    matches = [row for row in pending["rows"] if row["id"] == group["request_ids"][0]]
    assert len(matches) == 1, pending
    item = matches[0]
    path = f"/api/v1/access-requests/{group['request_ids'][0]}/approve"
    decision = {"version": item["version"], "note": "Approve the disposable image join check."}
    # A bearer remains an agent, even though its human owns this environment.
    status, _, body = call(person.hub + path, json.dumps(decision).encode(), {
        "Authorization": f"Bearer {person.token}", "Content-Type": "application/json",
        "Idempotency-Key": f"image-join-self-approve-{machine}",
    })
    assert status == 403, (status, body)
    approved = human(person, path, decision, key=f"image-join-approve-{machine}")
    assert approved["result"]["status"] == "approved", approved
    ready = person.tool("wait_for_access", {"request_id": group["group_id"], "timeout_s": 0})
    assert not ready.get("isError") and ready["structuredContent"]["status"] == "approved", ready
    print(f"PASS agent self-approval refused; human approved command access on {machine}", flush=True)
