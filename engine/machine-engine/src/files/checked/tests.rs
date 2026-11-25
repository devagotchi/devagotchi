use super::*;
use crate::{broker::ExecPolicy, files::Scope};
use devagotchi_contracts::{BridgeError, FileReadInput};
use std::sync::atomic::{AtomicUsize, Ordering};

fn link_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link)
        .expect("native Windows link test requires symlink permission");
}
fn link_dir(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(target, link)
        .expect("native Windows link test requires symlink permission");
}

#[test]
fn a_leaf_substituted_after_parent_open_is_not_followed() {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let leaf = root.join("leaf");
    let foreign = root.join("foreign");
    std::fs::write(&leaf, b"allowed").unwrap();
    std::fs::write(&foreign, b"private").unwrap();
    let entry = Entry::at(&leaf).unwrap();
    std::fs::remove_file(&leaf).unwrap();
    link_file(&foreign, &leaf);
    assert!(entry.read().is_err());
    assert_eq!(std::fs::read(&foreign).unwrap(), b"private");
}

#[test]
fn a_parent_substitution_is_pinned_or_detected_before_file_open() {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let parent = root.join("parent");
    let foreign = root.join("foreign");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(parent.join("leaf"), b"allowed").unwrap();
    std::fs::write(foreign.join("leaf"), b"private").unwrap();
    let entry = Entry::at(&parent.join("leaf")).unwrap();
    match std::fs::rename(&parent, root.join("moved")) {
        Ok(()) => {
            link_dir(&foreign, &parent);
            assert!(
                entry.read().is_err(),
                "a moved held directory must not silently follow the replacement"
            );
        }
        Err(error) => {
            assert!(
                cfg!(windows),
                "only Windows delete-sharing should pin the directory: {error}"
            );
            assert_eq!(entry.read().unwrap().metadata().unwrap().len(), 7);
        }
    }
    assert_eq!(std::fs::read(foreign.join("leaf")).unwrap(), b"private");
}

#[test]
fn authority_ending_during_a_line_scan_aborts_the_read() {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    std::fs::write(root.join("large"), "line\n".repeat(50_000)).unwrap();
    let checks = AtomicUsize::new(0);
    let authority = |_: Option<&Path>| {
        if checks.fetch_add(1, Ordering::SeqCst) >= 4 {
            Err(BridgeError::new("authority_revoked", "ended during scan"))
        } else {
            Ok(())
        }
    };
    let scope = Scope {
        workspace: &root,
        policy: ExecPolicy::Granted,
        authority: Some(&authority),
    };
    let error = scope
        .read(FileReadInput {
            path: "large".into(),
            offset: 0,
            max_bytes: 100,
            start_line: Some(40_000),
            line_count: Some(1),
        })
        .unwrap_err();
    assert_eq!(error.code, "authority_revoked");
}

#[test]
fn a_child_replaced_mid_walk_cannot_disclose_outside_names() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let foreign = dunce::canonicalize(outside.path()).unwrap();
    std::fs::create_dir(root.join("child")).unwrap();
    std::fs::write(root.join("child/allowed"), b"allowed").unwrap();
    std::fs::write(foreign.join("private-name"), b"private").unwrap();
    let scope = Scope {
        workspace: &root,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    let mut walk = crate::files::search_walk::Walk::new(&scope, &root, true, None).unwrap();
    assert_eq!(walk.next().unwrap().unwrap().name, "child");
    std::fs::rename(root.join("child"), root.join("old")).unwrap();
    link_dir(&foreign, &root.join("child"));
    assert!(
        walk.next().is_err(),
        "must refuse before enumerating the substituted directory"
    );
    assert_eq!(
        std::fs::read(foreign.join("private-name")).unwrap(),
        b"private"
    );
}
