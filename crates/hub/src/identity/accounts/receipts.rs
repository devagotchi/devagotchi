//! Nonsecret account mutation results, committed with their audit records.
//! Callers must establish live account authority and environment scope before begin.
use super::*;
use devagotchi_store::receipts::{self, Outcome, Request, Reservation};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug)]
pub struct MutationResult<T> {
    pub value: T,
    pub replayed: bool,
}

impl<T> MutationResult<T> {
    pub(super) fn replay(value: T) -> Self {
        Self {
            value,
            replayed: true,
        }
    }
}

pub(super) enum Receipt<T> {
    Fresh(Option<Reservation>),
    Replay(T),
}

impl<T: DeserializeOwned + Serialize> Receipt<T> {
    pub(super) async fn begin(
        connection: &mut AnyConnection,
        tenant: &str,
        actor: &str,
        key: Option<&str>,
        operation: &str,
        input: &impl Serialize,
    ) -> Result<Self, Failed> {
        let Some(key) = key else {
            return Ok(Self::Fresh(None));
        };
        let request = Request::new(tenant, actor, key, operation, input)?;
        match receipts::begin_in(connection, &request).await? {
            Outcome::Fresh(reservation) => Ok(Self::Fresh(Some(reservation))),
            Outcome::Replay(value) => Ok(Self::Replay(serde_json::from_value(value).map_err(
                |_| {
                    BridgeError::new(
                        "receipt_damaged",
                        "stored account response has an invalid shape",
                    )
                },
            )?)),
        }
    }

    pub(super) async fn complete(
        self,
        connection: &mut AnyConnection,
        value: T,
    ) -> Result<MutationResult<T>, Failed> {
        match self {
            Self::Fresh(reservation) => {
                if let Some(reservation) = reservation {
                    reservation.complete_in(connection, &value).await?;
                }
                Ok(MutationResult {
                    value,
                    replayed: false,
                })
            }
            Self::Replay(value) => Ok(MutationResult::replay(value)),
        }
    }
}
