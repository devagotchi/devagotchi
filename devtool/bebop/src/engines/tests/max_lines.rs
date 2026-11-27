use super::*;

#[test]
fn lines_are_counted_as_wc_counts_them() {
    assert_eq!(line_count(b""), 0);
    assert_eq!(line_count(b"a\nb\n"), 2);
    assert_eq!(line_count(b"a\r\nb\r\n"), 2);
    assert_eq!(line_count(b"a\nb"), 2);
}

#[test]
fn every_file_in_scope_is_measured() {
    let (_dir, tree) = tree(&[
        ("engine/a.rs", "1\n2\n3\n"),
        ("engine/b.rs", "1"),
        ("engine/notes.md", "1\n"),
        ("engine/target/c.rs", "1\n"),
    ]);
    let outcome = run(
        &Check::MaxLines { max: 2 },
        &tree,
        &scope(&["engine/**/*.rs"], &["**/target"]),
    )
    .unwrap();
    let Outcome::Measured(files) = &outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(
        files
            .iter()
            .map(|(p, n)| (p.as_str(), *n))
            .collect::<Vec<_>>(),
        [("engine/a.rs", 3), ("engine/b.rs", 1)]
    );
    assert_eq!(outcome.count(&Check::MaxLines { max: 2 }), 1);
}
