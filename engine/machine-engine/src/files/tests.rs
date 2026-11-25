use super::*;

fn scope(workspace: &Path, policy: ExecPolicy) -> Scope<'_> {
    Scope {
        workspace,
        policy,
        authority: None,
    }
}

fn read(scope: &Scope<'_>, path: &Path, offset: i64, max_bytes: usize) -> FileContent {
    scope
        .read(FileReadInput {
            path: path.display().to_string(),
            offset,
            max_bytes,
            start_line: None,
            line_count: None,
        })
        .unwrap()
}

#[test]
fn text_reads_in_pages_without_splitting_a_character() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap();
    let file = workspace.join("notes.txt");
    std::fs::write(&file, "héllo wörld\nline two\n").unwrap();
    let scope = scope(&workspace, ExecPolicy::Granted);
    // "hé" is three bytes; a two-byte page stops before the cut "é".
    let first = read(&scope, &file, 0, 2);
    assert_eq!(first.text.as_deref(), Some("h"));
    assert_eq!(first.next_offset, 1);
    let rest = read(&scope, &file, first.next_offset as i64, 1024);
    assert_eq!(rest.text.as_deref(), Some("éllo wörld\nline two\n"));
    assert!(rest.eof);
    // A tail: the last 9 bytes.
    let tail = read(&scope, &file, -9, 1024);
    assert_eq!(tail.text.as_deref(), Some("line two\n"));
    assert_eq!(tail.offset, tail.size - 9);
}

#[test]
fn binary_comes_back_as_base64() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap();
    let file = workspace.join("blob.bin");
    std::fs::write(&file, [0u8, 159, 146, 150, 1, 2]).unwrap();
    let content = read(&scope(&workspace, ExecPolicy::Granted), &file, 0, 1024);
    assert!(content.text.is_none());
    assert_eq!(
        BASE64.decode(content.bytes_base64.unwrap()).unwrap(),
        [0u8, 159, 146, 150, 1, 2]
    );
}

#[test]
fn the_granted_policy_stays_inside_the_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    let outside = workspace.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "secret").unwrap();
    let input = FileReadInput {
        path: outside.display().to_string(),
        offset: 0,
        max_bytes: 1024,
        start_line: None,
        line_count: None,
    };
    let refused = scope(&workspace, ExecPolicy::Granted)
        .read(input.clone())
        .unwrap_err();
    assert_eq!(refused.code, "path_outside_workspace");
    let read = scope(&workspace, ExecPolicy::Account).read(input).unwrap();
    assert_eq!(read.text.as_deref(), Some("secret"));
    let missing = scope(&workspace, ExecPolicy::Account)
        .read(FileReadInput {
            path: workspace.join("nope").display().to_string(),
            offset: 0,
            max_bytes: 1024,
            start_line: None,
            line_count: None,
        })
        .unwrap_err();
    assert_eq!(missing.code, "path_not_found");
}

#[test]
fn listings_put_directories_first_and_respect_depth_and_limits() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(directory.path()).unwrap();
    std::fs::create_dir_all(workspace.join("src/deep")).unwrap();
    std::fs::write(workspace.join("README.md"), "hi").unwrap();
    std::fs::write(workspace.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(workspace.join("src/deep/x.rs"), "").unwrap();
    let scope = scope(&workspace, ExecPolicy::Granted);
    let list = |depth, max_entries| {
        scope
            .list(FileListInput {
                path: workspace.display().to_string(),
                depth,
                max_entries,
                cursor: None,
            })
            .unwrap()
    };
    let top = list(0, 200);
    let names: Vec<_> = top
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.kind.as_str()))
        .collect();
    assert_eq!(names, [("src", "dir"), ("README.md", "file")]);
    assert_eq!(top.entries[1].size, Some(2));
    let all = list(2, 200);
    let paths: Vec<_> = all.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "src",
            "src/deep",
            "src/deep/x.rs",
            "src/main.rs",
            "README.md"
        ]
    );
    let cut = list(2, 2);
    assert!(cut.truncated);
    assert_eq!(cut.entries.len(), 2);
}

#[test]
fn a_cursor_goes_on_where_the_last_page_stopped_and_every_entry_comes_once() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(dir.path()).unwrap();
    for name in ["a", "b", "c"] {
        std::fs::create_dir_all(workspace.join(name)).unwrap();
        for file in ["1.txt", "2.txt"] {
            std::fs::write(workspace.join(name).join(file), "x").unwrap();
        }
    }
    std::fs::write(workspace.join("top.txt"), "x").unwrap();
    let scope = scope(&workspace, ExecPolicy::Granted);
    let page = |cursor: Option<String>| {
        scope.list(FileListInput {
            path: workspace.display().to_string(),
            depth: 1,
            max_entries: 4,
            cursor,
        })
    };
    let everything = scope
        .list(FileListInput {
            path: workspace.display().to_string(),
            depth: 1,
            max_entries: 1000,
            cursor: None,
        })
        .unwrap();
    assert!(!everything.truncated && everything.next_cursor.is_none());
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let listing = page(cursor.take()).unwrap();
        assert!(listing.entries.len() <= 4);
        seen.extend(listing.entries.iter().map(|e| e.path.clone()));
        assert_eq!(listing.truncated, listing.next_cursor.is_some());
        match listing.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    let all: Vec<_> = everything.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        seen, all,
        "the pages, in turn, are the whole listing in its order"
    );
    assert_eq!(all.len(), 10);
    let gone = page(Some("no-such-entry".into())).unwrap_err();
    assert_eq!(gone.code, "cursor_not_found");
}

#[test]
fn a_read_says_which_file_object_it_read() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(dir.path()).unwrap();
    let file = workspace.join("notes.txt");
    std::fs::write(&file, "first").unwrap();
    let scope = scope(&workspace, ExecPolicy::Granted);
    let read = |path: &Path| {
        scope
            .read(FileReadInput {
                path: path.display().to_string(),
                offset: 0,
                max_bytes: 1024,
                start_line: None,
                line_count: None,
            })
            .unwrap()
            .identity
            .expect("an identity")
    };
    let first = read(&file);
    assert_eq!(read(&file), first, "the same file, read again");
    let renamed = workspace.join("renamed.txt");
    std::fs::rename(&file, &renamed).unwrap();
    assert_eq!(read(&renamed), first, "a rename keeps it");
    // Another file put in its place: same name, same size, another object.
    let other = workspace.join("other.txt");
    std::fs::write(&other, "other").unwrap();
    std::fs::rename(&other, &renamed).unwrap();
    assert_ne!(read(&renamed), first, "a replacement is another file");
}

#[cfg(target_os = "linux")]
#[test]
fn a_name_that_is_not_unicode_is_marked_as_shown_not_reachable() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let workspace = dunce::canonicalize(dir.path()).unwrap();
    let bad = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    std::fs::write(workspace.join(bad), "x").unwrap();
    std::fs::write(workspace.join("plain.txt"), "x").unwrap();
    let scope = scope(&workspace, ExecPolicy::Granted);
    let listing = scope
        .list(FileListInput {
            path: workspace.display().to_string(),
            depth: 0,
            max_entries: 10,
            cursor: None,
        })
        .unwrap();
    let lossy: Vec<_> = listing
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.name_lossy))
        .collect();
    assert!(lossy.contains(&("caf\u{FFFD}.txt", true)), "{lossy:?}");
    assert!(lossy.contains(&("plain.txt", false)), "{lossy:?}");
}
