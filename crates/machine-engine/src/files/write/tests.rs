use super::*;

fn input(path: &Path, text: &str, mode: WriteMode) -> FileWriteInput {
    FileWriteInput {
        path: path.display().to_string(),
        text: Some(text.into()),
        bytes_base64: None,
        mode,
        expected_sha256: None,
        create_dirs: false,
    }
}

#[test]
fn writes_create_overwrite_and_append_with_a_checksum() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap();
    let scope = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    let file = workspace.join("a.txt");
    let made = scope
        .write(input(&file, "one\n", WriteMode::Create))
        .unwrap();
    assert!(made.created);
    let again = scope
        .write(input(&file, "x", WriteMode::Create))
        .unwrap_err();
    assert_eq!(again.code, "already_exists");
    let mut guarded = input(&file, "two\n", WriteMode::Overwrite);
    guarded.expected_sha256 = Some("0".repeat(64));
    assert_eq!(
        scope.write(guarded.clone()).unwrap_err().code,
        "changed_since_read"
    );
    guarded.expected_sha256 = Some(made.sha256);
    let replaced = scope.write(guarded).unwrap();
    assert!(!replaced.created);
    scope
        .write(input(&file, "three\n", WriteMode::Append))
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "two\nthree\n");
    // Nothing is left beside it.
    assert_eq!(fs::read_dir(&workspace).unwrap().count(), 1);
}

#[test]
fn edits_replace_exact_text_only_at_the_expected_count() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap();
    let scope = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    let file = workspace.join("main.rs");
    fs::write(&file, "let a = 1;\nlet b = 1;\n").unwrap();
    let edit = |old: &str, new: &str, count| {
        scope.edit(FileEditInput {
            path: file.display().to_string(),
            old_text: old.into(),
            new_text: new.into(),
            expected_replacements: count,
        })
    };
    let ambiguous = edit("= 1;", "= 2;", 1).unwrap_err();
    assert_eq!(ambiguous.code, "edit_mismatch");
    assert!(ambiguous.message.contains("occurs 2 times"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "let a = 1;\nlet b = 1;\n"
    );
    let done = edit("let b = 1;", "let b = 2;", 1).unwrap();
    assert_eq!(done.replacements, Some(1));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "let a = 1;\nlet b = 2;\n"
    );
    edit("= 1;", "= 3;", 1).unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "let a = 3;\nlet b = 2;\n"
    );
}

#[test]
fn an_edit_matches_and_keeps_the_files_own_line_endings() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap();
    let scope = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    let edit = |file: &Path, old: &str, new: &str| {
        scope.edit(FileEditInput {
            path: file.display().to_string(),
            old_text: old.into(),
            new_text: new.into(),
            expected_replacements: 1,
        })
    };
    let windows = workspace.join("crlf.txt");
    fs::write(&windows, "one\r\ntwo\r\nthree\r\n").unwrap();
    edit(&windows, "one\ntwo", "one\nTWO\nmore").unwrap();
    assert_eq!(
        fs::read_to_string(&windows).unwrap(),
        "one\r\nTWO\r\nmore\r\nthree\r\n"
    );
    let unix = workspace.join("lf.txt");
    fs::write(&unix, "one\ntwo\n").unwrap();
    edit(&unix, "one\r\ntwo", "1\r\n2").unwrap();
    assert_eq!(fs::read_to_string(&unix).unwrap(), "1\n2\n");
    // A file with both endings is matched only exactly.
    let mixed = workspace.join("mixed.txt");
    fs::write(&mixed, "a\r\nb\nc\n").unwrap();
    assert_eq!(edit(&mixed, "a\nb", "x").unwrap_err().code, "edit_mismatch");
}

#[test]
fn the_granted_policy_writes_only_inside_the_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let workspace = root.join("ws");
    fs::create_dir(&workspace).unwrap();
    let outside = root.join("new/dir/file.txt");
    let mut write = input(&outside, "x", WriteMode::Overwrite);
    write.create_dirs = true;
    let granted = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    assert_eq!(
        granted.write(write.clone()).unwrap_err().code,
        "path_outside_workspace"
    );
    assert!(
        !root.join("new").exists(),
        "nothing created outside the workspace"
    );
    let mut inside = input(&workspace.join("made/here.txt"), "y", WriteMode::Overwrite);
    inside.create_dirs = true;
    assert!(granted.write(inside).unwrap().created);
    let account = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Account,
        authority: None,
    };
    let written = account.write(write).unwrap();
    assert!(written.created);
    assert_eq!(fs::read_to_string(&outside).unwrap(), "x");
}

/// A path whose missing part climbs back out (`nope/../..`) makes nothing outside.
#[test]
fn made_parents_never_climb_out_of_the_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let workspace = root.join("ws");
    fs::create_dir(&workspace).unwrap();
    let granted = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    let climbing = format!("{}/nope/../../made/file.txt", workspace.display());
    let mut write = input(Path::new(&climbing), "x", WriteMode::Overwrite);
    write.create_dirs = true;
    assert!(granted.write(write).is_err());
    assert!(
        !root.join("made").exists(),
        "nothing made outside the workspace"
    );
}

/// A link inside the workspace whose target is missing and outside: appending through it
/// would create the file outside.
#[cfg(unix)]
#[test]
fn a_dangling_link_out_of_the_workspace_is_not_written_through() {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let workspace = root.join("ws");
    fs::create_dir(&workspace).unwrap();
    let link = workspace.join("door");
    std::os::unix::fs::symlink(root.join("loot.txt"), &link).unwrap();
    let granted = Scope {
        workspace: &workspace,
        policy: ExecPolicy::Granted,
        authority: None,
    };
    let refused = granted
        .write(input(&link, "x", WriteMode::Append))
        .unwrap_err();
    assert_eq!(refused.code, "path_outside_workspace");
    assert!(!root.join("loot.txt").exists(), "nothing written outside");
}
