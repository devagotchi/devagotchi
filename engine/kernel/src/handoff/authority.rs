//! Who holds a person's lexicon: one writer at a time, fenced by an epoch.
//!
//! ```text
//! devagotchi    --begin_handover (epoch + 1)-->  handing_over
//! handing_over  --complete_handover----------->  clyffy
//! handing_over  --abort_handover (epoch + 1)-->  devagotchi
//! clyffy        --begin_restore (epoch + 1)--->  restoring
//! restoring     --complete_restore------------>  devagotchi
//! ```
//!
//! Every move but a completion raises the epoch, so a bundle or a write from before it is
//! refused: the side that let go cannot write over the side that took over. While Clyffy holds
//! the lexicon Devagotchi keeps a read-only mirror, which `restore` turns back into its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::bundle::{LexiconBundleV1, Side};
use crate::lexicon::{LexiconError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Holder {
    /// Devagotchi keeps and writes the lexicon.
    Devagotchi,
    /// Devagotchi has stopped writing; Clyffy is taking the bundle in.
    HandingOver,
    /// Clyffy keeps and writes it; Devagotchi holds a read-only mirror.
    Clyffy,
    /// Clyffy has stopped writing; Devagotchi is taking its bundle back.
    Restoring,
}

impl Holder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Devagotchi => "devagotchi",
            Self::HandingOver => "handing_over",
            Self::Clyffy => "clyffy",
            Self::Restoring => "restoring",
        }
    }
}

/// Where one person's lexicon stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Authority {
    pub holder: Holder,
    pub epoch: u64,
}

impl Default for Authority {
    /// Devagotchi holds a lexicon nobody handed over, at epoch 1.
    fn default() -> Self {
        Self {
            holder: Holder::Devagotchi,
            epoch: 1,
        }
    }
}

fn refuse<T>(why: String) -> Result<T> {
    Err(LexiconError::Conflict(why))
}

impl Authority {
    /// Whether Devagotchi may change entries now.
    pub fn devagotchi_writes(&self) -> bool {
        self.holder == Holder::Devagotchi
    }

    /// A write that read the authority at `epoch` may go ahead: Devagotchi holds it, and
    /// nothing moved it since.
    pub fn fence(&self, epoch: u64) -> Result<()> {
        if epoch != self.epoch {
            return refuse(format!(
                "the lexicon moved to epoch {} since this write read epoch {epoch}",
                self.epoch
            ));
        }
        if !self.devagotchi_writes() {
            return refuse(format!(
                "the lexicon is {}: Devagotchi does not write it now",
                self.holder.as_str()
            ));
        }
        Ok(())
    }

    fn step(&self, from: Holder, to: Holder, raise: bool, what: &str) -> Result<Self> {
        if self.holder != from {
            return refuse(format!(
                "{what} starts from {}, and the lexicon is {}",
                from.as_str(),
                self.holder.as_str()
            ));
        }
        let epoch = if raise {
            self.epoch
                .checked_add(1)
                .ok_or_else(|| LexiconError::Invalid("epoch overflow".into()))?
        } else {
            self.epoch
        };
        Ok(Self { holder: to, epoch })
    }

    /// Devagotchi stops writing and hands its bundle to Clyffy at the new epoch.
    pub fn begin_handover(&self) -> Result<Self> {
        self.step(Holder::Devagotchi, Holder::HandingOver, true, "a handover")
    }

    /// Clyffy took the bundle in: it holds the lexicon now.
    pub fn complete_handover(&self, epoch: u64) -> Result<Self> {
        self.at(epoch)?;
        self.step(
            Holder::HandingOver,
            Holder::Clyffy,
            false,
            "completing a handover",
        )
    }

    /// The handover stops before Clyffy took it: Devagotchi writes again, at a new epoch.
    pub fn abort_handover(&self) -> Result<Self> {
        self.step(
            Holder::HandingOver,
            Holder::Devagotchi,
            true,
            "aborting a handover",
        )
    }

    /// The person leaves Clyffy: Clyffy stops writing and Devagotchi takes the lexicon back
    /// from its last bundle, at the new epoch.
    pub fn begin_restore(&self) -> Result<Self> {
        self.step(Holder::Clyffy, Holder::Restoring, true, "a restore")
    }

    /// Devagotchi has its entries back and writes them.
    pub fn complete_restore(&self, epoch: u64) -> Result<Self> {
        self.at(epoch)?;
        self.step(
            Holder::Restoring,
            Holder::Devagotchi,
            false,
            "completing a restore",
        )
    }

    fn at(&self, epoch: u64) -> Result<()> {
        if epoch == self.epoch {
            Ok(())
        } else {
            refuse(format!(
                "this step was for epoch {epoch}; the lexicon is at epoch {}",
                self.epoch
            ))
        }
    }

    /// Whether `bundle` may be taken in now: only Clyffy's, into the mirror while Clyffy holds
    /// the lexicon (at the current epoch) or back while it is restored (at the current epoch
    /// or the one before the restore began). Any other bundle is refused.
    pub fn accepts(&self, bundle: &LexiconBundleV1) -> Result<()> {
        if bundle.from != Side::Clyffy {
            return refuse("only Clyffy's bundle is taken in".into());
        }
        match self.holder {
            Holder::Clyffy | Holder::Restoring => {}
            other => {
                return refuse(format!(
                    "the lexicon is {}: there is no Clyffy bundle to take",
                    other.as_str()
                ));
            }
        }
        // Restoring, Clyffy's last bundle was written at the epoch before the restore began.
        let newest = self.epoch;
        let allowed = match self.holder {
            Holder::Restoring => [newest.saturating_sub(1), newest],
            _ => [newest, newest],
        };
        if !allowed.contains(&bundle.epoch) {
            return refuse(format!(
                "the bundle is from epoch {}; the lexicon is at epoch {newest}",
                bundle.epoch
            ));
        }
        Ok(())
    }
}
