//! Organisation APIs are hosted-only; local project membership is available in both editions.

use super::*;
use axum::routing::{delete, get, post, put};

pub(crate) fn routes(multi_tenant: bool) -> Router<AppState> {
    let projects = Router::new()
        .route("/api/v1/projects/{project}/members", get(shared_members))
        .route(
            "/api/v1/projects/{project}/members/{kind}/{principal}",
            put(set_shared_member).delete(remove_shared_member),
        );
    if !multi_tenant {
        return projects;
    }
    projects
        .route("/api/v1/orgs", get(list).post(create))
        .route("/api/v1/orgs/{org}", get(show))
        .route(
            "/api/v1/orgs/{org}/members/{subject}",
            put(set_member).delete(remove_member),
        )
        .route("/api/v1/orgs/{org}/teams", post(create_team))
        .route("/api/v1/orgs/{org}/teams/{team}", delete(delete_team))
        .route(
            "/api/v1/orgs/{org}/teams/{team}/members/{subject}",
            put(add_team_member).delete(remove_team_member),
        )
        .route("/api/v1/orgs/{org}/projects", post(create_project))
        .route("/api/v1/orgs/{org}/projects/{project}", put(set_visibility))
        .route(
            "/api/v1/orgs/{org}/projects/{project}/members/{kind}/{principal}",
            put(set_org_project_member).delete(remove_org_project_member),
        )
        .route(
            "/api/v1/orgs/{org}/invites",
            get(invites).post(create_invite),
        )
        .route("/api/v1/orgs/{org}/invites/{id}", delete(revoke_invite))
        .route("/api/v1/invites/accept", post(accept))
}
