//! A recipient's position on a hosted board (`agenthub::inbox`): one row of `board_settings`,
//! read with the addressed notes and written in the same transaction.

use devagotchi_contracts::BridgeResult;
use sqlx::AnyConnection;

use super::{DbResult, HostedBoard, decode};
use crate::agenthub::{
    Note,
    inbox::{self, Cursors, Delivery, InboxNote, KEY, SCAN},
};
use crate::store::transaction;

impl HostedBoard {
    async fn cursors(&self, connection: &mut AnyConnection) -> DbResult<Cursors> {
        let raw: Option<String> = sqlx::query_scalar(
            "SELECT value FROM board_settings WHERE tenant_id=$1 AND board_id=$2 AND key=$3",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(KEY)
        .fetch_optional(&mut *connection)
        .await?;
        Ok(raw
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default())
    }

    async fn save_cursors(
        &self,
        connection: &mut AnyConnection,
        cursors: &Cursors,
    ) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO board_settings (tenant_id,board_id,key,value) VALUES ($1,$2,$3,$4)
             ON CONFLICT (tenant_id,board_id,key) DO UPDATE SET value=excluded.value",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(KEY)
        .bind(serde_json::to_string(cursors).expect("inbox json"))
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn addressed(&self, connection: &mut AnyConnection, after: i64) -> DbResult<Vec<Note>> {
        let texts: Vec<String> = sqlx::query_scalar(
            "SELECT row_to_json(n)::text FROM board_notes n
             WHERE tenant_id=$1 AND board_id=$2 AND seq>$3 AND to_principal IS NOT NULL
             ORDER BY seq LIMIT $4",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(after)
        .bind(SCAN)
        .fetch_all(&mut *connection)
        .await?;
        Ok(texts
            .iter()
            .map(|text| decode(text))
            .collect::<BridgeResult<Vec<Note>>>()?)
    }

    pub(crate) async fn inbox_deliver(
        &self,
        who: &[String],
        me: &str,
        commit: bool,
    ) -> BridgeResult<Delivery> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let mut cursors = self.cursors(connection).await?;
            let notes = self
                .addressed(connection, inbox::floor(&cursors, who))
                .await?;
            let before = cursors.clone();
            let delivery = inbox::plan(&notes, who, me, &mut cursors, now);
            if commit && cursors != before {
                self.save_cursors(connection, &cursors).await?;
            }
            Ok(delivery)
        })
    }

    pub(crate) async fn inbox_mark_delivered(&self, notes: &[InboxNote]) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let mut cursors = self.cursors(connection).await?;
            inbox::mark_delivered(&mut cursors, notes, now);
            self.save_cursors(connection, &cursors).await?;
            Ok(())
        })
    }

    pub(crate) async fn inbox(
        &self,
        who: &[String],
        me: &str,
        ack_through: Option<i64>,
    ) -> BridgeResult<Vec<InboxNote>> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            let mut cursors = self.cursors(connection).await?;
            if let Some(through) = ack_through {
                inbox::acknowledge(&mut cursors, who, through);
                self.save_cursors(connection, &cursors).await?;
            }
            let notes = self
                .addressed(connection, inbox::floor(&cursors, who))
                .await?;
            Ok(inbox::unread(&notes, who, me, &cursors))
        })
    }

    pub(crate) async fn wakes(&self) -> BridgeResult<crate::agenthub::wake::Wakes> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            let raw: Option<String> = sqlx::query_scalar(
                "SELECT value FROM board_settings WHERE tenant_id=$1 AND board_id=$2 AND key=$3",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(crate::agenthub::wake::KEY)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(raw
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or_default())
        })
    }

    pub(crate) async fn save_wakes(
        &self,
        actor: &crate::agenthub::Actor<'_>,
        wakes: &crate::agenthub::wake::Wakes,
        detail: serde_json::Value,
    ) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            sqlx::query(
                "INSERT INTO board_settings (tenant_id,board_id,key,value) VALUES ($1,$2,$3,$4)
                 ON CONFLICT (tenant_id,board_id,key) DO UPDATE SET value=excluded.value",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(crate::agenthub::wake::KEY)
            .bind(serde_json::to_string(wakes).expect("wakes json"))
            .execute(&mut *connection)
            .await?;
            self.event(
                connection,
                actor.id,
                now,
                "notes.wake_set",
                None,
                None,
                detail.clone(),
            )
            .await?;
            Ok(())
        })
    }
}
