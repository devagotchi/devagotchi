//! Durable interruption floors in the existing machine-local execution journal.
use super::*;
use devagotchi_contracts::{GrantBinding, GrantInterruption};
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
pub(super) fn binding_hash(binding: &GrantBinding) -> BridgeResult<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(binding).map_err(persistence)?,
    )))
}
impl Journal {
    pub(super) fn load_grant(
        &self,
        json: Option<&str>,
        key: &str,
        digest: &str,
    ) -> BridgeResult<Option<GrantBinding>> {
        let Some(json) = json else {
            return Ok(None);
        };
        let binding: GrantBinding = serde_json::from_str(json).map_err(persistence)?;
        binding.validate().map_err(persistence)?;
        if binding.operation_key != key || binding.input_sha256 != digest {
            return Err(persistence("journal grant binding differs from operation"));
        }
        let hash:String=self.connection.query_row("SELECT binding_hash FROM grant_admissions WHERE hub=?1 AND machine=?2 AND generation=?3 AND admission_id=?4 AND operation_key=?5",
            params![binding.origin.hub,binding.origin.machine,binding.origin.generation,binding.admission_id,key],|r|r.get(0)).map_err(persistence)?;
        if hash != binding_hash(&binding)? {
            return Err(persistence("journal grant admission identity differs"));
        }
        Ok(Some(binding))
    }
    pub(super) fn initialize_grants(&self) -> BridgeResult<()> {
        self.connection.execute_batch("CREATE TABLE IF NOT EXISTS grant_floors (
            hub TEXT NOT NULL,machine TEXT NOT NULL,generation TEXT NOT NULL,grant_id TEXT NOT NULL,
            minimum_version INTEGER NOT NULL CHECK(minimum_version>0),
            PRIMARY KEY(hub,machine,generation,grant_id));
            CREATE TABLE IF NOT EXISTS grant_admissions (
                hub TEXT NOT NULL,machine TEXT NOT NULL,generation TEXT NOT NULL,admission_id TEXT NOT NULL,
                operation_key TEXT NOT NULL UNIQUE,binding_hash TEXT NOT NULL,
                PRIMARY KEY(hub,machine,generation,admission_id));
            CREATE TRIGGER IF NOT EXISTS grant_admissions_no_delete BEFORE DELETE ON grant_admissions
                BEGIN SELECT RAISE(ABORT,'grant admission identities are permanent'); END;
            CREATE TRIGGER IF NOT EXISTS grant_admissions_no_update BEFORE UPDATE ON grant_admissions
                BEGIN SELECT RAISE(ABORT,'grant admission identities are immutable'); END;
            CREATE TRIGGER IF NOT EXISTS grant_admissions_no_replace BEFORE INSERT ON grant_admissions
                WHEN EXISTS(SELECT 1 FROM grant_admissions WHERE operation_key=NEW.operation_key OR
                    (hub=NEW.hub AND machine=NEW.machine AND generation=NEW.generation AND admission_id=NEW.admission_id))
                BEGIN SELECT RAISE(ABORT,'grant admission identities cannot be replaced'); END;
            CREATE TRIGGER IF NOT EXISTS grant_floors_no_lower_insert BEFORE INSERT ON grant_floors
                WHEN EXISTS(SELECT 1 FROM grant_floors WHERE hub=NEW.hub AND machine=NEW.machine
                  AND generation=NEW.generation AND grant_id=NEW.grant_id AND minimum_version>=NEW.minimum_version)
                BEGIN SELECT RAISE(IGNORE); END;
            CREATE TRIGGER IF NOT EXISTS grant_floors_no_delete BEFORE DELETE ON grant_floors
                BEGIN SELECT RAISE(ABORT,'grant interruption floors are permanent'); END;
            CREATE TRIGGER IF NOT EXISTS grant_floors_no_lower BEFORE UPDATE ON grant_floors
                WHEN NEW.minimum_version<OLD.minimum_version OR NEW.hub<>OLD.hub OR NEW.machine<>OLD.machine
                  OR NEW.generation<>OLD.generation OR NEW.grant_id<>OLD.grant_id
                BEGIN SELECT RAISE(ABORT,'grant interruption floors cannot be lowered or moved'); END;")
            .map_err(persistence)
    }
    pub(crate) fn check_grant(&self, binding: &GrantBinding) -> BridgeResult<()> {
        binding.validate()?;
        let floor:Option<i64>=self.connection.query_row("SELECT minimum_version FROM grant_floors WHERE hub=?1 AND machine=?2 AND generation=?3 AND grant_id=?4",
            params![binding.origin.hub,binding.origin.machine,binding.origin.generation,binding.grant_id],|r|r.get(0)).optional().map_err(persistence)?;
        if floor.is_some_and(|v| binding.grant_version < v) {
            return Err(BridgeError::new(
                "authority_revoked",
                "this grant version was interrupted",
            ));
        }
        let prior: Option<String> = self
            .connection
            .query_row(
                "SELECT binding_hash FROM grant_admissions WHERE
            (hub=?1 AND machine=?2 AND generation=?3 AND admission_id=?4) OR operation_key=?5",
                params![
                    binding.origin.hub,
                    binding.origin.machine,
                    binding.origin.generation,
                    binding.admission_id,
                    binding.operation_key
                ],
                |r| r.get(0),
            )
            .optional()
            .map_err(persistence)?;
        let expected = binding_hash(binding)?;
        if prior.is_some_and(|hash| hash != expected) {
            return Err(BridgeError::new(
                "idempotency_conflict",
                "the admission identity is already bound to different work",
            ));
        }
        Ok(())
    }
    pub(crate) fn interrupt_grant(&mut self, notice: &GrantInterruption) -> BridgeResult<()> {
        notice.validate()?;
        self.connection.execute("INSERT INTO grant_floors(hub,machine,generation,grant_id,minimum_version) VALUES(?1,?2,?3,?4,?5)
            ON CONFLICT(hub,machine,generation,grant_id) DO UPDATE SET minimum_version=MAX(grant_floors.minimum_version,excluded.minimum_version)",
            params![notice.origin.hub,notice.origin.machine,notice.origin.generation,notice.grant_id,notice.minimum_version]).map_err(persistence)?;
        Ok(())
    }
}
