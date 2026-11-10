#!/usr/bin/env python3
"""Probe a running hub's organisations: the JSON API and the board tools over MCP.

    python3 tests/orgs_probe.py http://127.0.0.1:8787 --shared e2e-board \
        --tokens owner=<bearer> member=<bearer> operator=<bearer>

`owner` and `member` are two people with no grants of their own; `operator` holds a grant
marked human on the configured board `--shared`. The owner makes an organisation and a
project in it, invites the member, and the member's AI works the project over one MCP
session while the owner changes the member's role: each change holds from the AI's next
call on that same session (viewer: reads, changes nothing; members-only project: nothing;
a team made agent: works again; removed: no such board). A seat is refused as not a
person; a stranger manages nothing. Then the configured board: its operator gives the
member a role on it in the store and takes it back, and the member's AI follows each time.
Exit 0 only when every step holds.
"""
import argparse
import json
import sys
import urllib.error
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("base")
parser.add_argument("--shared", required=True, help="a configured board the operator holds a human grant on")
parser.add_argument("--tokens", nargs="+", required=True, help="name=bearer, for owner member operator")
args = parser.parse_args()
base = args.base.rstrip("/")
tokens = dict(pair.split("=", 1) for pair in args.tokens)
for who in ("owner", "member", "operator"):
    if who not in tokens:
        sys.exit(f"--tokens needs {who}=<bearer>")
failures = 0
ORG = "e2e-team"
PROJECT = f"{ORG}/web"


def check(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name} {detail}".rstrip(), flush=True)
    if not ok:
        failures += 1


def request(method, path, who, body=None, headers=None, accept=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(base + path, data=data, method=method)
    req.add_header("Authorization", "Bearer " + tokens[who])
    if accept:
        req.add_header("Accept", accept)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    for name, value in (headers or {}).items():
        req.add_header(name, value)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, resp.headers, resp.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


def api(method, path, who, body=None, headers=None):
    status, _, raw = request(method, path, who, body, headers)
    try:
        payload = json.loads(raw) if raw else {}
    except json.JSONDecodeError:
        payload = {"raw": raw.decode(errors="replace")}
    return status, payload


class Mcp:
    """One MCP session, kept for the whole probe: what proves a change of role holds from
    the next call on a session that is already open."""

    def __init__(self, who):
        self.who = who
        self.session = None
        self.rid = 0
        init = self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                       "clientInfo": {"name": f"orgs-probe-{who}", "version": "0"}})
        assert init["result"]["serverInfo"]["name"] == "devagotchi-hub", init
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, body):
        headers = {"MCP-Protocol-Version": "2025-06-18"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        status, response_headers, raw = request("POST", "/mcp", self.who, body, headers,
                                                accept="application/json, text/event-stream")
        self.session = self.session or response_headers.get("Mcp-Session-Id")
        text = raw.decode()
        if response_headers.get("Content-Type", "").startswith("text/event-stream"):
            data = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
            text = next((d for d in data if d.startswith("{")), "")
        return json.loads(text) if text else {}

    def rpc(self, method, params):
        self.rid += 1
        return self.post({"jsonrpc": "2.0", "id": self.rid, "method": method, "params": params})

    def call(self, tool, arguments):
        """(refused, the error code or None, the structured result)"""
        answer = self.rpc("tools/call", {"name": tool, "arguments": arguments})
        result = answer.get("result", {})
        content = result.get("structuredContent") or {}
        if answer.get("error") or result.get("isError"):
            text = json.dumps(answer)
            code = content.get("code") if isinstance(content, dict) else None
            for known in ("read_only", "unknown_board", "not_granted", "not_permitted"):
                if code is None and known in text:
                    code = known
            return True, code, content
        return False, None, content


# The owner makes the organisation and its project; a stranger manages nothing of it.
status, made = api("POST", "/api/v1/orgs", "owner", {"org": ORG, "name": "E2E Team"})
check("the owner makes an organisation", status == 201 and made.get("role") == "owner", f"{status} {made}")
status, _ = api("POST", f"/api/v1/orgs/{ORG}/invites", "member", {"role": "admin"})
check("someone who is not a member invites nobody", status == 404, str(status))
status, project = api("POST", f"/api/v1/orgs/{ORG}/projects", "owner", {"project": "web"})
check("a project of the organisation", status == 201 and project.get("project") == PROJECT, f"{status} {project}")
status, again = api("POST", f"/api/v1/orgs/{ORG}/projects", "owner", {"project": "web"})
check("making it again changes nothing", status == 200 and again.get("created") is False, f"{status} {again}")
status, seat = api("POST", f"/api/v1/orgs/{ORG}/invites", "owner", {"role": "member"},
                   headers={"X-Devagotchi-Seat": "claude"})
check("a seat is not a person: it manages nobody", status == 403 and seat.get("code") == "people_only", f"{status} {seat}")

# The member takes an invite.
status, invite = api("POST", f"/api/v1/orgs/{ORG}/invites", "owner", {"role": "member", "note": "probe"})
check("an invite", status == 201 and invite.get("token", "").startswith("inv_"), f"{status}")
status, joined = api("POST", "/api/v1/invites/accept", "member", {"token": invite.get("token", "")})
check("the member takes it", status == 200 and joined.get("role") == "member", f"{status} {joined}")
status, spent = api("POST", "/api/v1/invites/accept", "operator", {"token": invite.get("token", "")})
check("an invite works once", status == 409 and spent.get("code") == "spent", f"{status} {spent}")

ai = Mcp("member")
refused, _, me = ai.call("whoami", {})
member_id = me.get("subject", "")
check("the member's AI sees the project", not refused and PROJECT in me.get("boards", []), str(me.get("boards")))
check("and its role there", any(o.get("org") == ORG and o.get("role") == "member" for o in me.get("orgs", [])),
      str(me.get("orgs")))
refused, code, created = ai.call("board_create", {"project": PROJECT, "title": "Ship the web page"})
check("a member's AI works the project", not refused, f"{code} {created}")


def set_role(role):
    status, payload = api("PUT", f"/api/v1/orgs/{ORG}/members/{member_id}", "owner", {"role": role})
    check(f"the owner makes the member {role}", status == 200, f"{status} {payload}")


# Each change holds from the AI's next call, on the session it already has.
set_role("viewer")
refused, code, _ = ai.call("board_list", {"project": PROJECT})
check("a viewer's AI reads the board", not refused, str(code))
refused, code, _ = ai.call("board_create", {"project": PROJECT, "title": "must not land"})
check("and changes nothing on its next call", refused and code == "read_only", str(code))
status, _ = api("POST", f"/api/v1/orgs/{ORG}/invites", "member", {"role": "viewer"})
check("a viewer invites nobody", status == 403, str(status))

status, _ = api("PUT", f"/api/v1/orgs/{ORG}/projects/web", "owner", {"visibility": "members"})
check("the owner makes the project members-only", status == 200, str(status))
refused, code, _ = ai.call("board_list", {"project": PROJECT})
check("the viewer's AI no longer reaches it", refused and code == "not_granted", str(code))

status, _ = api("POST", f"/api/v1/orgs/{ORG}/teams", "owner", {"team": "core", "name": "Core"})
check("a team", status == 201, str(status))
status, _ = api("PUT", f"/api/v1/orgs/{ORG}/teams/core/members/{member_id}", "owner")
check("the member on it", status == 200, str(status))
status, payload = api("PUT", f"/api/v1/orgs/{ORG}/projects/web/members/team/core", "owner", {"role": "agent"})
check("the team made agent on the project", status == 200, f"{status} {payload}")
refused, code, _ = ai.call("board_create", {"project": PROJECT, "title": "the team's work"})
check("the AI works it again, through its team", not refused, str(code))

status, shown = api("GET", f"/api/v1/orgs/{ORG}", "owner")
check("the organisation lists members, teams and projects",
      status == 200 and len(shown.get("members", [])) == 2 and shown.get("teams", [{}])[0].get("team") == "core"
      and shown.get("projects", [{}])[0].get("visibility") == "members", f"{status} {shown}")

status, _ = api("DELETE", f"/api/v1/orgs/{ORG}/members/{member_id}", "owner")
check("the owner removes the member", status == 200, str(status))
refused, code, _ = ai.call("board_list", {"project": PROJECT})
check("the removed member's AI reaches no such board", refused and code == "unknown_board", str(code))
refused, _, me = ai.call("whoami", {})
check("and whoami lists neither", PROJECT not in me.get("boards", []) and not me.get("orgs"), str(me))

# A configured board's members, kept in the store by a person granted on it.
shared = args.shared
status, payload = api("PUT", f"/api/v1/projects/{shared}/members/subject/{member_id}", "owner", {"role": "agent"})
check("a stranger to the configured board gives nobody a role on it", status == 403, f"{status} {payload}")
refused, code, _ = ai.call("board_list", {"project": shared})
check("the member holds no grant on it", refused and code == "not_granted", str(code))
status, payload = api("PUT", f"/api/v1/projects/{shared}/members/subject/{member_id}", "operator", {"role": "agent"})
check("its operator gives the member agent there", status == 200, f"{status} {payload}")
refused, code, _ = ai.call("board_list", {"project": shared})
check("the member's AI works it from its next call", not refused, str(code))
status, payload = api("DELETE", f"/api/v1/projects/{shared}/members/subject/{member_id}", "operator")
check("and takes it back", status == 200, f"{status} {payload}")
refused, code, _ = ai.call("board_list", {"project": shared})
check("the member's AI is refused from its next call", refused and code == "not_granted", str(code))

if failures:
    sys.exit(f"orgs probe: {failures} failed")
print("orgs probe ok", flush=True)
