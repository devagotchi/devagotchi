use super::*;

#[tokio::test]
async fn role_deletion_racing_profile_assignment_never_leaves_a_dangling_role() {
    let (accounts, owner) = or_skip!(super::tests::setup());
    for i in 0..8 {
        let id = format!("race-{i}");
        let role = accounts
            .put_principal_role(
                &owner.id,
                &catalog::Role {
                    id: id.clone(),
                    description: "Concurrent assignment".into(),
                    version: 0,
                },
            )
            .await
            .unwrap();
        let edit = AgentEdit {
            id: id.clone(),
            name: id.clone(),
            kind: "codex".into(),
            roles: vec![id.clone()],
            enabled: true,
            wip_limit: None,
            version: 0,
        };
        let (assigned, deleted) = tokio::join!(
            accounts.put_agent(&owner.id, &owner.id, &edit),
            accounts.delete_principal_role(&owner.id, &id, role.version),
        );
        assert_ne!(
            assigned.is_ok(),
            deleted.is_ok(),
            "exactly one conflicting write must commit: {assigned:?}, {deleted:?}"
        );
        let config = accounts.principal_configuration(&owner.id).await.unwrap();
        let exists = config.roles.iter().any(|role| role.id == id);
        match assigned {
            Ok(agent) => {
                assert!(exists);
                assert_eq!(agent.roles, [id]);
                assert_eq!(deleted.unwrap_err().code, "in_use");
            }
            Err(error) => {
                assert!(!exists);
                assert_eq!(error.code, "invalid");
                assert_eq!(
                    accounts
                        .agent(&owner.id, &owner.id, &id)
                        .await
                        .unwrap_err()
                        .code,
                    "not_found"
                );
            }
        }
    }
}
