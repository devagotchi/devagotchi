//! What a hosted edition adds, and what this core does when nothing is added.
//!
//! The cloud edition ([the plan](../../../.ai/dev/plans/cloud-edition.md), C2) starts this
//! hub and replaces these pieces. Each default is today's behaviour, so `devagotchi hub
//! serve` changes nothing. Nothing here names a billing provider, a plan catalogue, or a
//! hosted deployment.
//!
//! | Piece | Default here | A hosted edition |
//! |---|---|---|
//! | Plan limits (`projects`, `machines`, `clients`) | the plan the operator set | its own numbers |
//! | Tenant lifecycle | create a tenant on first connect | provisioning, suspend, delete |
//! | In-process events | record nothing | billing usage, chat |
//! | Extra routes | none | its own pages, beside the hub's |
//!
//! Board and account events already leave through signal sinks and, when configured,
//! ClickHouse. Those stay. `EventHook` is an in-process listener for the same process,
//! told about tenant lifecycle. A sink URL is still how events leave the machine.

use std::sync::Arc;

use axum::Router;

/// How many of one kind a tenant may hold. `Unset` leaves the hub's own config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Unset,
    Unlimited,
    AtMost(u32),
}

/// A tenant's plan limits. Return `None` for a key to keep the limit the operator set
/// on the site plan (`/operator`, `site.json`).
pub trait PlanPolicy: Send + Sync {
    fn limit(&self, tenant: &str, key: &str) -> Option<Limit> {
        let _ = (tenant, key);
        None
    }
}

/// What happened to a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantChange {
    /// The hub made the tenant (first connect, or a sign-in that made an account).
    Created,
    /// An edition suspended it. This core has no suspend of its own.
    Suspended,
    /// An edition deleted it. This core has no delete of its own.
    Deleted,
}

/// One tenant lifecycle notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantEvent {
    pub tenant: String,
    pub change: TenantChange,
    /// Why, in the words the caller has (`first connect`, a plan name). Empty when none.
    pub how: String,
}

/// Told when a tenant is created, and when an edition suspends or deletes one.
/// The default does nothing: creating a tenant on first connect is unchanged.
pub trait TenantHooks: Send + Sync {
    fn on_change(&self, event: &TenantEvent) {
        let _ = event;
    }
}

/// An in-process listener beside signal sinks and ClickHouse. The default records nothing.
pub trait EventHook: Send + Sync {
    fn on_event(&self, source: &str, tenant: Option<&str>, kind: &str) {
        let _ = (source, tenant, kind);
    }
}

/// Routes merged onto the hub's after its own are in place. They do not see the hub's
/// state; an edition keeps its own. The default adds none.
pub trait RouteMount: Send + Sync {
    fn mount(&self, router: Router) -> Router {
        router
    }
}

struct CorePlan;
impl PlanPolicy for CorePlan {}
struct CoreTenants;
impl TenantHooks for CoreTenants {}
struct CoreEvents;
impl EventHook for CoreEvents {}
struct CoreRoutes;
impl RouteMount for CoreRoutes {}

/// The pieces a hosted edition replaces. `Extensions::core` is what `devagotchi hub serve` uses.
#[derive(Clone)]
pub struct Extensions {
    pub plan: Arc<dyn PlanPolicy>,
    pub tenants: Arc<dyn TenantHooks>,
    pub events: Arc<dyn EventHook>,
    pub routes: Arc<dyn RouteMount>,
}

impl std::fmt::Debug for Extensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Extensions")
    }
}

impl Default for Extensions {
    fn default() -> Self {
        Self::core()
    }
}

impl Extensions {
    /// Today's behaviour: the operator's plan limits, a tenant on first connect, no extra routes.
    pub fn core() -> Self {
        Self {
            plan: Arc::new(CorePlan),
            tenants: Arc::new(CoreTenants),
            events: Arc::new(CoreEvents),
            routes: Arc::new(CoreRoutes),
        }
    }

    pub fn with_plan(mut self, plan: Arc<dyn PlanPolicy>) -> Self {
        self.plan = plan;
        self
    }

    pub fn with_tenants(mut self, tenants: Arc<dyn TenantHooks>) -> Self {
        self.tenants = tenants;
        self
    }

    pub fn with_events(mut self, events: Arc<dyn EventHook>) -> Self {
        self.events = events;
        self
    }

    pub fn with_routes(mut self, routes: Arc<dyn RouteMount>) -> Self {
        self.routes = routes;
        self
    }

    /// A tenant was created. First connect and a sign-in that makes an account call this.
    pub fn tenant_created(&self, tenant: &str, how: &str) {
        self.notify(tenant, TenantChange::Created, how);
    }

    /// An edition suspended a tenant. This core never suspends one itself.
    pub fn tenant_suspended(&self, tenant: &str, how: &str) {
        self.notify(tenant, TenantChange::Suspended, how);
    }

    /// An edition deleted a tenant. This core never deletes one itself.
    pub fn tenant_deleted(&self, tenant: &str) {
        self.notify(tenant, TenantChange::Deleted, "");
    }

    fn notify(&self, tenant: &str, change: TenantChange, how: &str) {
        let kind = match change {
            TenantChange::Created => "tenant.created",
            TenantChange::Suspended => "tenant.suspended",
            TenantChange::Deleted => "tenant.deleted",
        };
        self.tenants.on_change(&TenantEvent {
            tenant: tenant.to_string(),
            change,
            how: how.to_string(),
        });
        self.events.on_event("tenancy", Some(tenant), kind);
    }

    /// The hub's router plus any routes this edition added.
    pub fn mount(&self, router: Router) -> Router {
        self.routes.mount(router)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Router, routing::get};

    use super::*;

    struct CapMachines;
    impl PlanPolicy for CapMachines {
        fn limit(&self, tenant: &str, key: &str) -> Option<Limit> {
            assert_eq!(tenant, "t-1");
            (key == "machines").then_some(Limit::AtMost(1))
        }
    }

    #[test]
    fn the_core_plan_leaves_the_operators_limit() {
        assert_eq!(CorePlan.limit("t-1", "machines"), None);
        assert_eq!(CorePlan.limit("t-1", "projects"), None);
        assert_eq!(CorePlan.limit("t-1", "clients"), None);
    }

    #[test]
    fn a_plan_policy_replaces_only_the_limit_it_names() {
        let policy = CapMachines;
        assert_eq!(policy.limit("t-1", "machines"), Some(Limit::AtMost(1)));
        assert_eq!(policy.limit("t-1", "projects"), None);
    }

    #[derive(Default)]
    struct Seen(Mutex<Vec<TenantEvent>>);
    impl TenantHooks for Seen {
        fn on_change(&self, event: &TenantEvent) {
            self.0.lock().expect("seen").push(event.clone());
        }
    }

    struct Counted(AtomicUsize);
    impl EventHook for Counted {
        fn on_event(&self, source: &str, tenant: Option<&str>, kind: &str) {
            assert_eq!(source, "tenancy");
            assert_eq!(tenant, Some("t-1"));
            assert!(kind.starts_with("tenant."));
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn tenant_hooks_hear_create_suspend_and_delete_and_the_core_hooks_do_not() {
        let seen = Arc::new(Seen::default());
        let events = Arc::new(Counted(AtomicUsize::new(0)));
        let ext = Extensions::core()
            .with_tenants(seen.clone())
            .with_events(events.clone());
        ext.tenant_created("t-1", "first connect");
        ext.tenant_suspended("t-1", "unpaid");
        ext.tenant_deleted("t-1");
        let got = seen.0.lock().expect("seen").clone();
        assert_eq!(
            got.iter().map(|event| event.change).collect::<Vec<_>>(),
            vec![
                TenantChange::Created,
                TenantChange::Suspended,
                TenantChange::Deleted
            ]
        );
        assert_eq!(got[0].how, "first connect");
        assert_eq!(got[2].how, "");
        assert_eq!(events.0.load(Ordering::SeqCst), 3);
        // The core hooks accept the same calls and record nothing of their own.
        Extensions::core().tenant_created("t-1", "first connect");
    }

    struct Billing;
    impl RouteMount for Billing {
        fn mount(&self, router: Router) -> Router {
            router.route("/billing/health", get(|| async { "ok" }))
        }
    }

    #[tokio::test]
    async fn extra_routes_are_served_beside_the_hubs() {
        let app = Extensions::core()
            .with_routes(Arc::new(Billing))
            .mount(Router::new().route("/healthz", get(|| async { "hub" })));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = reqwest::Client::new();
        let health = http
            .get(format!("http://{addr}/healthz"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let billing = http
            .get(format!("http://{addr}/billing/health"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!((health.as_str(), billing.as_str()), ("hub", "ok"));
    }

    #[tokio::test]
    async fn the_core_adds_no_route() {
        let app = Extensions::core().mount(Router::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let status = reqwest::get(format!("http://{addr}/billing/health"))
            .await
            .unwrap()
            .status();
        assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
    }
}
