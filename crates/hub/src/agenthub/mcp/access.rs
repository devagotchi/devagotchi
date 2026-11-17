//! Who reaches which board: configured boards by grant, org boards by role, account boards by binding, and the names a caller may use.

use std::sync::Arc;

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::{Boards, Bound, Kind, SHARED_OWNER};
use crate::{
    agenthub::Board,
    config::valid_name,
    identity::Subject,
    mcp::Hub,
    tenancy::{orgs, orgs::Role},
};

impl Hub {
    /// The board `project` names for this account and what the account is on it: its own
    /// board first, then an organisation's project (`<slug>/<project>`), then a shared one by
    /// grant or by a role the store holds. An account board and a shared board never share
    /// a name (`bind` refuses). The one resolver: the board tools, the web pages
    /// (`/approvals`, the app), the issue routes and the organisations API all open boards
    /// through it, and it reads roles from the store on every call, so a change of role
    /// holds from the caller's next call.
    pub(crate) fn board_access(
        &self,
        subject: &Subject,
        project: &str,
    ) -> BridgeResult<(Arc<Board>, Kind, Role)> {
        if let Some((slug, name)) = project.split_once('/') {
            return self.workspace_board(subject, slug, name);
        }
        if let Some(board) = self
            .boards
            .open_account(&Boards::owner_of(subject), project)?
        {
            return Ok(if subject.org.is_some() {
                (board, Kind::Org, Role::Admin)
            } else {
                (board, Kind::Account, Role::Owner)
            });
        }
        let configured = self.config.board(project).is_some();
        let granted = self.config_role(&subject.id, project);
        let stored = if configured {
            self.shared_role(&subject.id, project)?
        } else {
            None
        };
        let role = match (granted, stored) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let Some(role) = role else {
            return Err(if !configured {
                BridgeError::new(
                    "unknown_board",
                    if self.boards.hosts_accounts() {
                        format!(
                            "no board for project {project:?} here: board_bind makes yours, or ask for a grant on a shared one"
                        )
                    } else {
                        format!("no board for project {project:?} is configured on this hub")
                    },
                )
            } else {
                BridgeError::new(
                    "not_granted",
                    format!("{} holds no grant for board {project:?}", subject.id),
                )
            });
        };
        Ok((self.boards.open(project)?, Kind::Shared, role))
    }

    /// `board_access` without the role, for callers that only open the board.
    pub(crate) fn board_for(
        &self,
        subject: &Subject,
        project: &str,
    ) -> BridgeResult<(Arc<Board>, Kind)> {
        self.board_access(subject, project)
            .map(|(board, kind, _)| (board, kind))
    }

    /// What the hub config makes `subject` on the configured board `project`: its owner
    /// owns every board; a grant marked human is an admin there, any other grant an agent.
    pub(super) fn config_role(&self, subject: &str, project: &str) -> Option<Role> {
        if self.config.is_owner(subject) {
            return self.config.board(project).map(|_| Role::Owner);
        }
        let granted = self
            .config
            .boards_for(subject)
            .iter()
            .any(|b| b.project == project);
        granted.then(|| {
            if self.config.is_human(subject) {
                Role::Admin
            } else {
                Role::Agent
            }
        })
    }

    /// Where a configured board keeps the members the store gives it: the home tenant,
    /// under the owner every configured board has (`@shared`).
    pub(super) fn shared_ref(&self, project: &str) -> BridgeResult<Option<orgs::ProjectRef>> {
        let Some(orgs) = self.boards.orgs() else {
            return Ok(None);
        };
        Ok(self
            .boards
            .block(orgs.home_tenant())?
            .map(|tenant| orgs::ProjectRef {
                tenant,
                owner: SHARED_OWNER.to_string(),
                project: project.to_string(),
            }))
    }

    /// `subject`'s role on the configured board `project` from the store, if any.
    pub(super) fn shared_role(&self, subject: &str, project: &str) -> BridgeResult<Option<Role>> {
        let (Some(orgs), Some(at)) = (self.boards.orgs(), self.shared_ref(project)?) else {
            return Ok(None);
        };
        self.boards.block(orgs.project_role(&at, subject))
    }

    /// An organisation's project, `<slug>/<name>`, and the caller's role on it. A caller
    /// who is not in the organisation learns no more than that there is no such board.
    pub(super) fn workspace_board(
        &self,
        subject: &Subject,
        slug: &str,
        name: &str,
    ) -> BridgeResult<(Arc<Board>, Kind, Role)> {
        let unknown = || {
            BridgeError::new(
                "unknown_board",
                format!("no board {slug}/{name} that you can reach on this hub"),
            )
        };
        let (Some(orgs), true) = (self.boards.orgs(), valid_name(name)) else {
            return Err(unknown());
        };
        let org = self.boards.block(orgs.by_slug(slug))?.ok_or_else(unknown)?;
        let at = orgs::ProjectRef {
            tenant: org.id.clone(),
            owner: orgs::owner_of(&org.id),
            project: name.to_string(),
        };
        let Some(role) = self.boards.block(orgs.project_role(&at, &subject.id))? else {
            let member = self
                .boards
                .block(orgs.role_in(&org, &subject.id))?
                .is_some();
            return Err(if member {
                BridgeError::new(
                    "not_granted",
                    format!("{} holds no role on {slug}/{name}", subject.id),
                )
            } else {
                unknown()
            });
        };
        let board = self
            .boards
            .open_account(&at.owner, name)?
            .ok_or_else(unknown)?;
        Ok((board, Kind::Workspace, role))
    }

    /// Where the store keeps who works `project`: an organisation's project, or a
    /// configured board. None for an account's own board, which is its account's alone.
    pub(crate) fn project_ref(&self, project: &str) -> BridgeResult<Option<orgs::ProjectRef>> {
        let Some(orgs) = self.boards.orgs() else {
            return Ok(None);
        };
        if let Some((slug, name)) = project.split_once('/') {
            return Ok(self
                .boards
                .block(orgs.by_slug(slug))?
                .map(|org| orgs::ProjectRef {
                    owner: orgs::owner_of(&org.id),
                    tenant: org.id,
                    project: name.to_string(),
                }));
        }
        if self.config.board(project).is_some() {
            return self.shared_ref(project);
        }
        Ok(None)
    }

    /// The boards `subject` reaches, by project, as `whoami` lists them: the account's own
    /// (or its org's) first, then the shared ones it holds a grant or a stored role for,
    /// then its organisations' projects and those it was added to, as `<slug>/<project>`.
    pub(crate) fn board_names(&self, subject: &Subject) -> BridgeResult<Vec<String>> {
        let mut names = self.boards.account_boards(&Boards::owner_of(subject))?;
        let add = |names: &mut Vec<String>, name: String| {
            if !names.contains(&name) {
                names.push(name);
            }
        };
        for entry in self.config.boards_for(&subject.id) {
            add(&mut names, entry.project.clone());
        }
        let Some(orgs) = self.boards.orgs() else {
            return Ok(names);
        };
        let given = self.boards.block(orgs.projects_of(&subject.id))?;
        let home = self.boards.block(orgs.home_tenant())?;
        for (at, _) in &given {
            if Some(&at.tenant) == home.as_ref()
                && at.owner == SHARED_OWNER
                && self.config.board(&at.project).is_some()
            {
                add(&mut names, at.project.clone());
            }
        }
        for (org, _) in self.boards.block(orgs.of_subject(&subject.id))? {
            let projects = self.boards.account_boards(&orgs::owner_of(&org.id))?;
            for (project, _) in self
                .boards
                .block(orgs.visible(&org, &projects, &subject.id))?
            {
                add(&mut names, format!("{}/{project}", org.slug));
            }
        }
        for (at, _) in given {
            if orgs::tenant_of_owner(&at.owner) == Some(at.tenant.as_str())
                && let Some(org) = self.boards.block(orgs.by_id(&at.tenant))?
            {
                add(&mut names, format!("{}/{}", org.slug, at.project));
            }
        }
        Ok(names)
    }

    /// `board_bind` for `<slug>/<project>`: make (or find) the organisation's project. The
    /// account must be a person in the organisation (member or above); its AI may ask for
    /// it. A new project is visible to the whole organisation.
    pub(crate) async fn bind_org(
        &self,
        subject: &Subject,
        slug: &str,
        name: &str,
    ) -> BridgeResult<Bound> {
        let Some(orgs) = self.boards.orgs().cloned() else {
            return Err(BridgeError::new(
                "not_offered",
                "this hub keeps no organisations",
            ));
        };
        let org = orgs.by_slug(slug).await?.ok_or_else(|| {
            BridgeError::new(
                "unknown_board",
                format!("no organisation {slug:?} that you belong to on this hub"),
            )
        })?;
        match orgs.role_in(&org, &subject.id).await? {
            Some(role) if role.is_person() => {}
            Some(role) => {
                return Err(BridgeError::new(
                    "not_permitted",
                    format!(
                        "you are {} of {slug}; a member or above makes its projects",
                        role.as_str()
                    ),
                ));
            }
            None => {
                return Err(BridgeError::new(
                    "unknown_board",
                    format!("no organisation {slug:?} that you belong to on this hub"),
                ));
            }
        }
        let owner = orgs::owner_of(&org.id);
        let boards = self.boards.clone();
        let project = name.to_string();
        let created = tokio::task::spawn_blocking(move || boards.make_for_org(&owner, &project))
            .await
            .unwrap_or_else(|join| {
                Err(BridgeError::new(
                    "board_store",
                    format!("board call did not complete: {join}"),
                ))
            })?;
        orgs.register_project(&org, &subject.id, name, orgs::Visibility::Org)
            .await?;
        let hub = self.clone();
        let who = subject.clone();
        let boards = tokio::task::spawn_blocking(move || hub.board_names(&who))
            .await
            .unwrap_or_else(|join| {
                Err(BridgeError::new(
                    "board_store",
                    format!("board call did not complete: {join}"),
                ))
            })?;
        Ok(Bound {
            project: format!("{slug}/{name}"),
            created,
            boards,
            limit: None,
        })
    }
}
