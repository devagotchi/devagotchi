//! Private local development evidence. Never a replacement for the shared board.

use devagotchi_contracts::{AiCaptureEvent, AiEventKind, AiHarness};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LIMIT: usize = 1024 * 1024;
const IMPORT_LIMIT: u64 = 64 * 1024 * 1024;
const HELP: &str = "\
Usage: devagotchi ai <command>
  policy show
  policy set --operator NAME [--mode managed|advanced]
      [--push validated|approval|disabled] [--merge validated|approval|disabled]
      [--capture off|metadata|content] [--acknowledge advanced,content-risk]
      [--transcript-root ABSOLUTE_DIR | --clear-transcript-roots]
  policy check --action push|merge [--validated] [--approved]
  hook claude-code|codex|copilot NATIVE_EVENT       native JSON on stdin
  capture append                                canonical event JSON on stdin
  capture import --harness HARNESS --session ID --file ABSOLUTE_JSONL
  capture status | verify
  capture export [--after SEQUENCE] [--limit 1..1000]
  delivery status                               local Git facts; no remote claims
  schema                                        canonical capture JSON schema
  check                                         repository contract/schema drift
Private state is scoped to the canonical checkout path under DEVAGOTCHI_STATE_DIR.
Capture defaults OFF; content requires explicit risk acknowledgment. No network upload.
Export writes sensitive JSONL to stdout; redirect only to private storage, never Git.
Policy checks are cooperative attestation gates, not a Git sandbox.
Exit: 0 success, 1 error/denied. No board initialization is required.";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    mode: Mode,
    push: Permission,
    merge: Permission,
    capture: Capture,
    operator: Option<String>,
    advanced_acknowledged: bool,
    content_acknowledged: bool,
    transcript_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Managed,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Permission {
    Validated,
    Approval,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Capture {
    Off,
    Metadata,
    Content,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            mode: Mode::Managed,
            push: Permission::Validated,
            merge: Permission::Approval,
            capture: Capture::Off,
            operator: None,
            advanced_acknowledged: false,
            content_acknowledged: false,
            transcript_roots: Vec::new(),
        }
    }
}

impl Policy {
    fn validate(&self) -> Result<()> {
        if self.mode == Mode::Advanced && !self.advanced_acknowledged {
            return Err("advanced mode requires --acknowledge advanced".into());
        }
        if self.merge == Permission::Validated && self.mode != Mode::Advanced {
            return Err("merge without approval requires acknowledged advanced mode".into());
        }
        if self.capture == Capture::Content && !self.content_acknowledged {
            return Err(
                "content may contain secrets/private material: --acknowledge content-risk required"
                    .into(),
            );
        }
        Ok(())
    }

    fn permits(&self, action: &str, validated: bool, approved: bool) -> Result<()> {
        self.validate()?;
        let permission = match action {
            "push" => self.push,
            "merge" => self.merge,
            _ => return Err("action must be push or merge".into()),
        };
        if permission == Permission::Disabled {
            return Err(format!("{action} disabled by operator policy").into());
        }
        if !validated {
            return Err(
                "applicable validation must be completed and attested with --validated".into(),
            );
        }
        if permission == Permission::Approval && !approved {
            return Err(
                "explicit operator approval required (--approved attests actual approval)".into(),
            );
        }
        Ok(())
    }
}

pub fn cli(args: Vec<OsString>, state: Option<PathBuf>) -> ExitCode {
    match run(args, state) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("devagotchi ai: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<OsString>, state: Option<PathBuf>) -> Result<()> {
    let args: Vec<String> = args
        .into_iter()
        .map(|s| {
            s.into_string()
                .map_err(|_| "arguments must be UTF-8".into())
        })
        .collect::<Result<_>>()?;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    if words.is_empty() || words == ["--help"] || words == ["-h"] {
        println!("{HELP}");
        return Ok(());
    }
    if words == ["schema"] {
        println!("{}", schema()?);
        return Ok(());
    }
    let repo = repo_root(&std::env::current_dir()?)?;
    if words == ["check"] {
        check_contracts(&repo)?;
        println!(
            "DG-LIFECYCLE-1 / DG-CAPTURE-1: repository contracts and generated capture schema agree."
        );
        return Ok(());
    }
    if words == ["delivery", "status"] {
        delivery(&repo)?;
        return Ok(());
    }
    let state = state.ok_or("no state directory; set absolute DEVAGOTCHI_STATE_DIR")?;
    let mut store = Journal::open(&repo, &state)?;
    match words.as_slice() {
        ["policy", "show"] => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "contract":"DG-DELIVERY-1", "policy":store.policy()?,
                "storage":store.path, "scope":"this canonical checkout path",
                "enforcement":"cooperative local gate; not branch protection"
            }))?
        ),
        ["policy", "set", rest @ ..] => {
            let options = options(
                rest,
                &[
                    "operator",
                    "mode",
                    "push",
                    "merge",
                    "capture",
                    "acknowledge",
                    "transcript-root",
                ],
                &["clear-transcript-roots"],
            )?;
            let mut policy = store.policy()?;
            let previous = policy.clone();
            let operator = required(&options, "operator")?;
            bounded(operator, 128, "operator")?;
            policy.operator = Some(operator.into());
            let ack = options.get("acknowledge").copied().unwrap_or("");
            for name in ack.split(',').filter(|s| !s.is_empty()) {
                match name {
                    "advanced" => policy.advanced_acknowledged = true,
                    "content-risk" => policy.content_acknowledged = true,
                    _ => return Err(format!("unknown acknowledgment: {name}").into()),
                }
            }
            if let Some(value) = options.get("mode") {
                policy.mode = parse_enum(value)?;
                if policy.mode == Mode::Managed {
                    policy.advanced_acknowledged = false;
                    if policy.merge == Permission::Validated {
                        policy.merge = Permission::Approval;
                    }
                }
            }
            if let Some(value) = options.get("push") {
                policy.push = parse_enum(value)?;
            }
            if let Some(value) = options.get("merge") {
                policy.merge = parse_enum(value)?;
            }
            if let Some(value) = options.get("capture") {
                policy.capture = parse_enum(value)?;
                if policy.capture != Capture::Content {
                    policy.content_acknowledged = false;
                }
            }
            if options.contains_key("clear-transcript-roots") {
                policy.transcript_roots.clear();
            }
            if let Some(root) = options.get("transcript-root") {
                if options.contains_key("clear-transcript-roots") {
                    return Err(
                        "choose --transcript-root or --clear-transcript-roots, not both".into(),
                    );
                }
                let path = Path::new(root);
                if !path.is_absolute() || !path.is_dir() {
                    return Err("transcript root must be an existing absolute directory".into());
                }
                if policy.capture != Capture::Content {
                    return Err(
                        "automatic transcript access requires acknowledged content capture".into(),
                    );
                }
                let canonical = path.canonicalize()?;
                if !policy.transcript_roots.contains(&canonical) {
                    policy.transcript_roots.push(canonical);
                }
            }
            policy.validate()?;
            store.set_policy(&previous, &policy)?;
            println!("{}", serde_json::to_string_pretty(&policy)?);
        }
        ["policy", "check", rest @ ..] => {
            let options = options(rest, &["action"], &["validated", "approved"])?;
            store.policy()?.permits(
                required(&options, "action")?,
                options.contains_key("validated"),
                options.contains_key("approved"),
            )?;
            println!(
                "Allowed by local policy on the supplied attestations; no Git operation performed."
            );
        }
        ["hook", harness, native] => {
            let harness = parse_enum(harness)?;
            let raw = read_json(io::stdin().lock())?;
            let policy = store.policy()?;
            let transcript = if harness != AiHarness::Copilot && policy.capture == Capture::Content
            {
                match raw.get("transcript_path") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(path)) => Some(PathBuf::from(path)),
                    _ => return Err("native transcript_path must be a string or null".into()),
                }
            } else {
                None
            };
            let event = normalize(harness, native, raw, &repo, policy.capture)?;
            if policy.capture != Capture::Off {
                store.append(&event)?;
                if policy.capture == Capture::Content
                    && matches!(event.kind, AiEventKind::TurnEnd | AiEventKind::SessionEnd)
                {
                    if let Some(path) = transcript {
                        if policy.transcript_roots.is_empty() {
                            eprintln!(
                                "DG-CAPTURE-1: transcript not followed; no authorized transcript roots. Hook coverage remains partial."
                            );
                        } else {
                            authorize_transcript(&path, &policy.transcript_roots)?;
                            store.import(harness, &event.session_id, &path, true)?;
                        }
                    }
                }
            } else if event.kind == AiEventKind::SessionStart {
                eprintln!(
                    "DG-CAPTURE-1: capture OFF; no session archive. Enable with ai policy set."
                );
            }
            println!("{{}}");
        }
        ["capture", "append"] => {
            let event: AiCaptureEvent = serde_json::from_value(read_json(io::stdin().lock())?)?;
            println!("{}", serde_json::to_string(&store.append(&event)?)?);
        }
        ["capture", "import", rest @ ..] => {
            let options = options(rest, &["harness", "session", "file"], &[])?;
            let count = store.import(
                parse_enum(required(&options, "harness")?)?,
                required(&options, "session")?,
                Path::new(required(&options, "file")?),
                false,
            )?;
            println!(
                "{}",
                json!({"records":count, "coverage":"partial", "training_eligible":false})
            );
        }
        ["capture", "status"] => {
            let count: i64 = store
                .db
                .query_row("SELECT count(*) FROM events", [], |r| r.get(0))?;
            println!(
                "{}",
                json!({
                    "policy":store.policy()?, "events":count, "storage":store.path,
                    "coverage":"partial", "automatic_transcript_following":false,
                    "training_eligible":false
                })
            );
        }
        ["capture", "verify"] => {
            let count = store.verify()?;
            println!(
                "{}",
                json!({"verified_events":count, "integrity":"local hash-chain; not external attestation"})
            );
        }
        ["capture", "export", rest @ ..] => {
            let options = options(rest, &["after", "limit"], &[])?;
            let after = options
                .get("after")
                .copied()
                .unwrap_or("0")
                .parse::<i64>()?;
            let limit = options
                .get("limit")
                .copied()
                .unwrap_or("100")
                .parse::<i64>()?;
            if after < 0 || !(1..=1000).contains(&limit) {
                return Err("after must be >=0; limit must be 1..1000".into());
            }
            let mut stdout = io::stdout().lock();
            for row in store.page(after, limit)? {
                serde_json::to_writer(&mut stdout, &row)?;
                writeln!(stdout)?;
            }
        }
        _ => return Err(format!("unknown command/options\n{HELP}").into()),
    }
    Ok(())
}

fn options<'a>(
    words: &'a [&str],
    valued: &[&str],
    flags: &[&str],
) -> Result<std::collections::BTreeMap<&'a str, &'a str>> {
    let mut result = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < words.len() {
        let key = words[i].strip_prefix("--").ok_or("expected --option")?;
        let value = if flags.contains(&key) {
            ""
        } else if valued.contains(&key) {
            i += 1;
            *words
                .get(i)
                .filter(|v| !v.starts_with("--"))
                .ok_or("missing option value")?
        } else {
            return Err(format!("unknown option: --{key}").into());
        };
        if result.insert(key, value).is_some() {
            return Err(format!("duplicate option: --{key}").into());
        }
        i += 1;
    }
    Ok(result)
}

fn required<'a>(map: &std::collections::BTreeMap<&str, &'a str>, key: &str) -> Result<&'a str> {
    map.get(key)
        .copied()
        .ok_or_else(|| format!("--{key} required").into())
}

fn parse_enum<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_value(Value::String(value.into()))?)
}

fn bounded(text: &str, max: usize, field: &str) -> Result<()> {
    if text.trim().is_empty() || text.len() > max || text.chars().any(char::is_control) {
        return Err(format!(
            "{field} must be nonempty, <= {max} bytes, with no control characters"
        )
        .into());
    }
    Ok(())
}

fn read_json(reader: impl Read) -> Result<Value> {
    let mut bytes = Vec::new();
    reader.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > LIMIT {
        return Err("JSON input exceeds 1 MiB".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn hash(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

fn validate_event(event: &AiCaptureEvent) -> Result<()> {
    if event.schema_version != 1 {
        return Err("unsupported capture schema_version".into());
    }
    bounded(&event.event_id, 256, "event_id")?;
    bounded(&event.session_id, 256, "session_id")?;
    for (name, value) in [("model", &event.model), ("issue", &event.issue)] {
        if let Some(value) = value {
            bounded(value, 512, name)?;
        }
    }
    if serde_json::to_vec(event)?.len() > LIMIT {
        return Err("event exceeds 1 MiB".into());
    }
    Ok(())
}

fn normalize(
    harness: AiHarness,
    native: &str,
    raw: Value,
    repo: &Path,
    capture: Capture,
) -> Result<AiCaptureEvent> {
    let kind = match (harness, native) {
        (AiHarness::ClaudeCode | AiHarness::Codex, "SessionStart")
        | (AiHarness::Copilot, "sessionStart") => AiEventKind::SessionStart,
        (AiHarness::ClaudeCode | AiHarness::Codex, "SessionEnd")
        | (AiHarness::Copilot, "sessionEnd") => AiEventKind::SessionEnd,
        (AiHarness::ClaudeCode | AiHarness::Codex, "Stop") | (AiHarness::Copilot, "agentStop") => {
            AiEventKind::TurnEnd
        }
        (AiHarness::ClaudeCode | AiHarness::Codex, "UserPromptSubmit")
        | (AiHarness::Copilot, "userPromptSubmitted") => AiEventKind::UserPrompt,
        (AiHarness::ClaudeCode | AiHarness::Codex, "PreToolUse")
        | (AiHarness::Copilot, "preToolUse") => AiEventKind::ToolBefore,
        (AiHarness::ClaudeCode | AiHarness::Codex, "PostToolUse")
        | (AiHarness::Copilot, "postToolUse") => AiEventKind::ToolAfter,
        (AiHarness::Copilot, "errorOccurred") => AiEventKind::Error,
        _ => return Err("unsupported harness/native event combination".into()),
    };
    if let Some(event) = raw.get("hook_event_name") {
        if event.as_str() != Some(native) {
            return Err("native event does not match hook_event_name".into());
        }
    }
    let session_key = if harness == AiHarness::Copilot {
        "sessionId"
    } else {
        "session_id"
    };
    let session = raw
        .get(session_key)
        .and_then(Value::as_str)
        .ok_or("missing native session identifier")?;
    if let Some(cwd) = raw.get("cwd") {
        let cwd = Path::new(cwd.as_str().ok_or("cwd must be a string")?);
        if !cwd.is_absolute() || !cwd.canonicalize()?.starts_with(repo) {
            return Err("native cwd must be within this canonical checkout".into());
        }
    }
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|e| format!("random event identity: {e}"))?;
    let model = match raw.get("model") {
        None | Some(Value::Null) => None,
        Some(Value::String(model)) => Some(model.clone()),
        _ => return Err("native model must be a string or null".into()),
    };
    let event = AiCaptureEvent {
        schema_version: 1,
        event_id: hex::encode(random),
        session_id: session.into(),
        harness,
        kind,
        model,
        issue: None,
        content: (capture == Capture::Content).then_some(raw),
    };
    validate_event(&event)?;
    Ok(event)
}

struct Journal {
    db: Connection,
    path: PathBuf,
}

impl Journal {
    fn open(repo: &Path, state: &Path) -> Result<Self> {
        if !state.is_absolute() {
            return Err("private state directory must be absolute".into());
        }
        fs::create_dir_all(state)?;
        let state = state.canonicalize()?;
        if state.starts_with(repo) || state.ancestors().any(|parent| parent.join(".git").exists()) {
            return Err("private AI state cannot be inside a source checkout".into());
        }
        let dir = state.join("ai").join(hash(
            repo.to_str()
                .ok_or("canonical checkout path must be UTF-8")?
                .as_bytes(),
        ));
        fs::create_dir_all(&dir)?;
        if dir.canonicalize()? != dir || dir.canonicalize()?.starts_with(repo) {
            return Err("private archive directory must not redirect through a link".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        let path = dir.join("journal.sqlite3");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err("archive database must be a regular file, not a link".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut open = OpenOptions::new();
        open.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        open.open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        let db = Connection::open(&path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS policy(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS policy_audit(seq INTEGER PRIMARY KEY, recorded_ms INTEGER NOT NULL, body TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS events(
                seq INTEGER PRIMARY KEY AUTOINCREMENT, harness TEXT NOT NULL,
                session TEXT NOT NULL, event_id TEXT NOT NULL, recorded_ms INTEGER NOT NULL,
                body TEXT NOT NULL, previous_hash TEXT NOT NULL, entry_hash TEXT NOT NULL,
                UNIQUE(harness,session,event_id));",
        )?;
        Ok(Self { db, path })
    }

    fn policy(&self) -> Result<Policy> {
        let body: Option<String> = self
            .db
            .query_row("SELECT body FROM policy WHERE id=1", [], |r| r.get(0))
            .optional()?;
        let policy: Policy = match body {
            Some(body) => serde_json::from_str(&body)?,
            None => Policy::default(),
        };
        policy.validate()?;
        Ok(policy)
    }

    fn set_policy(&mut self, previous: &Policy, policy: &Policy) -> Result<()> {
        policy.validate()?;
        let body = serde_json::to_string(policy)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<String> = tx
            .query_row("SELECT body FROM policy WHERE id=1", [], |r| r.get(0))
            .optional()?;
        let current: Policy = current
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        if current != *previous {
            return Err(
                "operator policy changed concurrently; inspect policy show and retry".into(),
            );
        }
        tx.execute("INSERT INTO policy(id,body) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET body=excluded.body", [&body])?;
        tx.execute(
            "INSERT INTO policy_audit(recorded_ms,body) VALUES(?1,?2)",
            params![now_ms()?, body],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn append(&mut self, event: &AiCaptureEvent) -> Result<Value> {
        self.append_authorized(event, None)
    }

    fn append_authorized(
        &mut self,
        event: &AiCaptureEvent,
        transcript: Option<&Path>,
    ) -> Result<Value> {
        validate_event(event)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Consent is read inside the write transaction so revocation serializes with capture.
        let body: Option<String> = tx
            .query_row("SELECT body FROM policy WHERE id=1", [], |r| r.get(0))
            .optional()?;
        let policy: Policy = body
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        policy.validate()?;
        if policy.capture == Capture::Off {
            return Err("capture is OFF; explicit operator opt-in required".into());
        }
        if event.content.is_some() && policy.capture != Capture::Content {
            return Err("content capture is not authorized; event was not stored".into());
        }
        if let Some(path) = transcript {
            authorize_transcript(path, &policy.transcript_roots)?;
        }
        let body = serde_json::to_string(event)?;
        let harness = serde_json::to_string(&event.harness)?;
        let old: Option<(i64, String)> = tx
            .query_row(
                "SELECT seq,body FROM events WHERE harness=?1 AND session=?2 AND event_id=?3",
                params![harness, event.session_id, event.event_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((seq, old)) = old {
            if old != body {
                return Err("event identity replayed with different content; no mutation".into());
            }
            return Ok(json!({"sequence":seq,"replayed":true}));
        }
        let previous: String = tx
            .query_row(
                "SELECT entry_hash FROM events ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default();
        let recorded = now_ms()?;
        let entry = entry_hash(&previous, recorded, &body);
        tx.execute(
            "INSERT INTO events(harness,session,event_id,recorded_ms,body,previous_hash,entry_hash) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![harness,event.session_id,event.event_id,recorded,body,previous,entry],
        )?;
        let seq = tx.last_insert_rowid();
        tx.commit()?;
        Ok(json!({"sequence":seq,"replayed":false}))
    }

    fn page(&self, after: i64, limit: i64) -> Result<Vec<Value>> {
        let mut stmt = self.db.prepare("SELECT seq,recorded_ms,body,previous_hash,entry_hash FROM events WHERE seq>?1 ORDER BY seq LIMIT ?2")?;
        let rows = stmt.query_map(params![after, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut output = Vec::new();
        for row in rows {
            let (seq, recorded, body, previous, entry) = row?;
            output.push(json!({
                "sequence":seq,"recorded_ms":recorded,"event":serde_json::from_str::<Value>(&body)?,
                "previous_hash":previous,"entry_hash":entry,"coverage":"partial",
                "training_eligible":false,"redaction":"not_performed"
            }));
        }
        Ok(output)
    }

    fn verify(&self) -> Result<usize> {
        let mut stmt = self
            .db
            .prepare("SELECT recorded_ms,body,previous_hash,entry_hash,seq,harness,session,event_id FROM events ORDER BY seq")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        let mut previous = String::new();
        let mut count = 0;
        for row in rows {
            let (recorded, body, prev, entry, seq, harness, session, event_id) = row?;
            let event: AiCaptureEvent = serde_json::from_str(&body)?;
            validate_event(&event)?;
            if previous != prev
                || entry_hash(&prev, recorded, &body) != entry
                || seq != (count + 1) as i64
                || harness != serde_json::to_string(&event.harness)?
                || session != event.session_id
                || event_id != event.event_id
            {
                return Err(format!("archive integrity failure at event {}", count + 1).into());
            }
            previous = entry;
            count += 1;
        }
        Ok(count)
    }

    fn import(
        &mut self,
        harness: AiHarness,
        session: &str,
        file: &Path,
        automatic: bool,
    ) -> Result<usize> {
        let policy = self.policy()?;
        if policy.capture != Capture::Content {
            return Err("transcript import requires acknowledged content capture".into());
        }
        if automatic {
            authorize_transcript(file, &policy.transcript_roots)?;
        }
        bounded(session, 256, "session")?;
        if !file.is_absolute() {
            return Err(
                "import requires an explicit absolute regular JSONL file, not a link".into(),
            );
        }
        let before_open = fs::symlink_metadata(file)?;
        if !before_open.is_file() || before_open.file_type().is_symlink() {
            return Err("transcript must be a regular file, not a directory, pipe or link".into());
        }
        let input = fs::File::open(file)?;
        let metadata = input.metadata()?;
        if !metadata.is_file() || metadata.len() > IMPORT_LIMIT {
            return Err("transcript must be a regular file <=64 MiB per import".into());
        }
        let canonical = file.canonicalize()?;
        let source = hash(
            canonical
                .to_str()
                .ok_or("canonical transcript path must be UTF-8")?
                .as_bytes(),
        );
        let mut reader = io::BufReader::new(input).take(IMPORT_LIMIT + 1);
        let mut offset = 0u64;
        let mut count = 0;
        loop {
            let mut line = Vec::new();
            // A live provider may still be writing its final line; only commit complete records.
            let size = reader
                .by_ref()
                .take((LIMIT + 1) as u64)
                .read_until(b'\n', &mut line)?;
            if size == 0 {
                break;
            }
            if offset + size as u64 > IMPORT_LIMIT || size > LIMIT {
                return Err(
                    "transcript/record limit exceeded; prior records remain safely retryable"
                        .into(),
                );
            }
            if !line.ends_with(b"\n") {
                eprintln!(
                    "DG-CAPTURE-1: incomplete JSONL tail at byte {offset}; not stored, retry after the provider finishes the record."
                );
                break;
            }
            let content: Value = serde_json::from_slice(&line).map_err(|e| {
                format!("invalid JSONL at byte {offset}; prior records retained: {e}")
            })?;
            self.append_authorized(
                &AiCaptureEvent {
                    schema_version: 1,
                    event_id: format!("import:{source}:{offset}"),
                    session_id: session.into(),
                    harness,
                    kind: AiEventKind::TranscriptRecord,
                    model: None,
                    issue: None,
                    content: Some(json!({
                        "record":content, "source_sha256":hash(&line), "source_offset":offset
                    })),
                },
                automatic.then_some(file),
            )?;
            offset += size as u64;
            count += 1;
        }
        Ok(count)
    }
}

fn entry_hash(previous: &str, recorded: u64, body: &str) -> String {
    hash(format!("{previous}\n{recorded}\n{body}"))
}

fn authorize_transcript(path: &Path, roots: &[PathBuf]) -> Result<()> {
    if !path.is_absolute() {
        return Err("hook transcript must be an absolute regular file, not a link".into());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("hook transcript must be a regular file, not a directory, pipe or link".into());
    }
    let canonical = path.canonicalize()?;
    if !roots.iter().any(|root| canonical.starts_with(root)) {
        return Err(
            "hook transcript is outside explicitly authorized roots; refusing access".into(),
        );
    }
    Ok(())
}

fn schema() -> Result<String> {
    Ok(serde_json::to_string_pretty(&schemars::schema_for!(
        AiCaptureEvent
    ))?)
}

fn repo_root(cwd: &Path) -> Result<PathBuf> {
    for dir in cwd.ancestors() {
        if dir.join(".ai/project.json").is_file() {
            return Ok(dir.canonicalize()?);
        }
    }
    Err("no .ai/project.json in this checkout; run from the configured repository".into())
}

fn check_contracts(repo: &Path) -> Result<()> {
    let project: Value = serde_json::from_slice(&fs::read(repo.join(".ai/project.json"))?)?;
    if project["contract"] != "DG-LIFECYCLE-1"
        || project["delivery_contract"] != "DG-DELIVERY-1"
        || project["capture_contract"] != "DG-CAPTURE-1"
    {
        return Err("unknown project contract identifiers".into());
    }
    match project["stage"].as_str() {
        Some("alpha") if project["production_promotion"].is_null() => {}
        Some("production") => {
            for field in ["approval", "candidate", "evidence", "date"] {
                bounded(
                    project["production_promotion"][field]
                        .as_str()
                        .ok_or("production promotion record incomplete")?,
                    2048,
                    field,
                )?;
            }
        }
        _ => {
            return Err(
                "stage must be alpha (no promotion) or explicitly promoted production".into(),
            );
        }
    }
    let defaults = Policy::default();
    for (key, value) in [
        ("mode", serde_json::to_value(defaults.mode)?),
        ("push", serde_json::to_value(defaults.push)?),
        ("merge", serde_json::to_value(defaults.merge)?),
        ("capture", serde_json::to_value(defaults.capture)?),
    ] {
        if project["policy_defaults"][key] != value {
            return Err(format!("policy default drift: {key}").into());
        }
    }
    let committed: Value =
        serde_json::from_slice(&fs::read(repo.join(".ai/docs/capture-event.schema.json"))?)?;
    if committed != serde_json::from_str::<Value>(&schema()?)? {
        return Err("capture schema drift; regenerate with devagotchi ai schema".into());
    }
    for path in [
        "AGENTS.md",
        "CLAUDE.md",
        ".github/copilot-instructions.md",
        ".ai/docs/lifecycle.md",
        ".ai/docs/capture.md",
        ".ai/docs/adapters.md",
        ".ai/docs/evaluations.json",
        ".ai/dev/workflow.md",
        ".ai/dev/plans/TEMPLATE.md",
        ".ai/patterns/README.md",
    ] {
        if !repo.join(path).is_file() {
            return Err(format!("missing canonical AI surface: {path}").into());
        }
    }
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("--no-pager")
        .args(args)
        .current_dir(repo)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn delivery(repo: &Path) -> Result<()> {
    let branch = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let head = git(repo, &["rev-parse", "HEAD"])?;
    let dirty = !git(repo, &["status", "--porcelain"])?.is_empty();
    let upstream = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{branch}"),
        ],
    )?;
    let counts = if upstream.is_empty() {
        None
    } else {
        Some(git(
            repo,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("HEAD...{upstream}"),
            ],
        )?)
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "branch":branch, "head":head, "uncommitted":dirty, "upstream":upstream,
            "local_ahead_behind":counts,"remote_checked":false,"pr":"not_verified",
            "main":"not_verified","delivery":"INCOMPLETE: PUSH / PR / MAIN REQUIRE CONFIRMATION"
        }))?
    );
    eprintln!(
        "DELIVERY INCOMPLETE: local Git facts cannot confirm a remote push or merge to main."
    );
    Ok(())
}

#[cfg(test)]
mod tests;
