//! `lexicon_handoff`: the person's lexicon moving between this hub and Clyffy, one writer at a
//! time (kernel K5). The steps and their rules are `devagotchi_kernel::handoff`'s; the owner
//! is the signed-in subject, and each move is atomic in the hub's store (`authority.rs`).
//!
//! - `status`: who holds it, at which epoch, and the owner id a bundle must carry.
//! - `preview` (Clyffy's bundle): what a handover would merge, and the phrases with two
//!   confirmed meanings the person settles first. Moves nothing.
//! - `begin`: this hub stops writing (`handing_over`, epoch + 1).
//! - `export` (Clyffy's own bundle, if it had a lexicon): the merged lexicon, which this hub
//!   keeps and Clyffy takes; refused while a conflict stands.
//! - `complete` (`epoch`): Clyffy holds it; this hub keeps a read-only mirror. `abort` instead
//!   gives it back to this hub at a new epoch.
//! - `mirror` (Clyffy's bundle at the current epoch): the mirror becomes Clyffy's lexicon.
//! - `restore` (Clyffy's last bundle, or none to keep the mirror): this hub holds it again.

use devagotchi_contracts::BridgeError;
use devagotchi_kernel::{
    handoff::{Authority, Conflict, Holder, LexiconBundleV1, export, preview, take_in},
    lexicon::{LexiconError, Result as Answer},
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Stored, mcp::refused, owner_of};
use crate::mcp::Hub;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Status,
    Preview,
    Begin,
    Export,
    Complete,
    Abort,
    Mirror,
    Restore,
}

impl Step {
    fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Preview => "preview",
            Self::Begin => "begin",
            Self::Export => "export",
            Self::Complete => "complete",
            Self::Abort => "abort",
            Self::Mirror => "mirror",
            Self::Restore => "restore",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HandoffInput {
    pub action: Step,
    /// Clyffy's lexicon as a `devagotchi.lexicon.bundle/1` (preview, export, mirror, restore),
    /// with `from: clyffy` and the `owner_id` that `status` gives.
    #[serde(default)]
    pub bundle: Option<LexiconBundleV1>,
    /// The epoch the step is for (`complete`).
    #[serde(default)]
    pub epoch: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct HandoffOutput {
    /// The owner id every bundle of this person's lexicon carries.
    pub owner_id: String,
    pub holder: Holder,
    pub epoch: u64,
    /// The merged lexicon Clyffy takes (`export`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<LexiconBundleV1>,
    /// Phrases with two confirmed meanings (`preview`): only the person says which stands.
    pub conflicts: Vec<Conflict>,
    /// Live entries another entry for the same phrase replaced (`preview`, `export`).
    pub replaced: usize,
    /// Entries the step kept or would keep.
    pub entries: usize,
}

fn need<T>(value: Option<T>, what: &str) -> Answer<T> {
    value.ok_or_else(|| LexiconError::Invalid(format!("{what} is needed for this step")))
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

/// One step for `owner`, against where their lexicon stands now.
async fn step(stored: &Stored, owner: &str, input: HandoffInput) -> Answer<HandoffOutput> {
    let at = stored.authority(owner).await?;
    let mut out = HandoffOutput {
        owner_id: owner.to_string(),
        holder: at.holder,
        epoch: at.epoch,
        bundle: None,
        conflicts: Vec::new(),
        replaced: 0,
        entries: 0,
    };
    let land = |out: &mut HandoffOutput, to: Authority| {
        out.holder = to.holder;
        out.epoch = to.epoch;
    };
    match input.action {
        Step::Status => {}
        Step::Preview => {
            let bundle = need(input.bundle, "bundle")?;
            let merged = preview(owner, &stored.entries_of(owner).await?, &bundle)?;
            out.replaced = merged.superseded.len();
            out.entries = merged.entries.len();
            out.conflicts = merged.conflicts;
        }
        Step::Begin | Step::Abort | Step::Complete => {
            let to = match input.action {
                Step::Begin => at.begin_handover()?,
                Step::Abort => at.abort_handover()?,
                _ => at.complete_handover(need(input.epoch, "epoch")?)?,
            };
            stored.moved(owner, at, to, None).await?;
            land(&mut out, to);
        }
        Step::Export => {
            let ours = stored.entries_of(owner).await?;
            let handover = export(&at, owner, now(), &ours, input.bundle.as_ref())?;
            let entries = handover.bundle.entries();
            stored.moved(owner, at, at, Some(&entries)).await?;
            out.replaced = handover.superseded;
            out.entries = entries.len();
            out.bundle = Some(handover.bundle);
        }
        Step::Mirror => {
            let entries = take_in(&at, owner, &need(input.bundle, "bundle")?)?;
            stored.moved(owner, at, at, Some(&entries)).await?;
            out.entries = entries.len();
        }
        Step::Restore => {
            let restoring = at.begin_restore()?;
            let entries = input
                .bundle
                .map(|bundle| take_in(&restoring, owner, &bundle))
                .transpose()?;
            let home = restoring.complete_restore(restoring.epoch)?;
            stored.moved(owner, at, home, entries.as_deref()).await?;
            out.entries = match entries {
                Some(entries) => entries.len(),
                None => stored.entries_of(owner).await?.len(),
            };
            land(&mut out, home);
        }
    }
    Ok(out)
}

#[tool_router(router = handoff_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "lexicon_handoff",
        description = "Move this person's lexicon between this hub and Clyffy, one writer at a time. `status`: who holds it, its epoch, and the owner_id a bundle carries. `preview` (Clyffy's `bundle`): what a handover would merge and the phrases with two confirmed meanings the person settles first. `begin` stops this hub writing; `export` (with Clyffy's own `bundle` if it had one) returns the merged lexicon for Clyffy; `complete` (`epoch`) hands it to Clyffy, `abort` takes it back. While Clyffy holds it, `mirror` (Clyffy's `bundle`) keeps this hub's read-only copy current. `restore` (Clyffy's last `bundle`, or none to keep the copy) gives it back to this hub.",
        title = "Hand your words to Clyffy or take them back",
        annotations(
            title = "Hand your words to Clyffy or take them back",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn lexicon_handoff(
        &self,
        Parameters(input): Parameters<HandoffInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<HandoffOutput>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let owner = owner_of(&subject);
        let action = input.action;
        let answer = step(&Stored::new(self.audit.store().clone()), &owner, input).await;
        if !matches!(action, Step::Status | Step::Preview) {
            let outcome = answer.as_ref().map_or_else(|e| e.code(), |_| "ok");
            let epoch = answer.as_ref().map(|o| o.epoch).unwrap_or_default();
            let operation = format!("{}@{epoch}", action.as_str());
            self.record(&subject, "lexicon_handoff", "", &operation, outcome)
                .await?;
        }
        answer.map(Json).map_err(refused)
    }
}
