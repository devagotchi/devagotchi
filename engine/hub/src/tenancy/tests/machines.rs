//! Machines on a hosted hub's tests: joining one without its socket.

use super::tenants::Hosted;

impl Hosted {
    /// Join a machine `name` with an account's `code`, as a link's enroll hello does without
    /// the socket: the early-access gate first, then the code binds `public_key`.
    pub(crate) async fn enroll(
        &self,
        name: &str,
        code: &str,
        public_key: &str,
    ) -> devagotchi_contracts::BridgeResult<()> {
        self.hub.links.may_enroll(code).await?;
        self.hub
            .links
            .enroll_owned(name, code, public_key)
            .await
            .map(drop)
    }
}
