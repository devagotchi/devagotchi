//! The ratchet's tests, ported from `runtime/devagotchi/tests/layout.rs` with the rule they
//! held, and the listed baselines beside them.

use super::*;

fn files(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
    entries
        .iter()
        .map(|(path, lines)| ((*path).to_owned(), *lines))
        .collect()
}

fn messages(breaches: &[Breach]) -> Vec<String> {
    breaches.iter().map(ToString::to_string).collect()
}

#[test]
fn a_listed_file_may_shrink_and_never_grow() {
    let listed = files(&[("a.rs", 900)]);
    assert!(counted_breaches(500, &listed, &files(&[("a.rs", 900)])).is_empty());
    assert!(counted_breaches(500, &listed, &files(&[("a.rs", 899)])).is_empty());
    assert!(counted_breaches(500, &listed, &files(&[("a.rs", 120)])).is_empty());
    let grown = messages(&counted_breaches(500, &listed, &files(&[("a.rs", 901)])));
    assert_eq!(grown.len(), 1);
    assert!(
        grown[0].starts_with("a.rs: grew from 900 to 901"),
        "{grown:?}"
    );
}

#[test]
fn a_file_that_is_not_listed_stays_within_the_cap() {
    let listed = files(&[("a.rs", 900)]);
    let at_the_cap = files(&[("a.rs", 900), ("new.rs", 500)]);
    assert!(counted_breaches(500, &listed, &at_the_cap).is_empty());
    let over = messages(&counted_breaches(
        500,
        &listed,
        &files(&[("a.rs", 900), ("new.rs", 501)]),
    ));
    assert_eq!(over.len(), 1);
    assert!(over[0].starts_with("new.rs: 501 lines"), "{over:?}");
}

#[test]
fn a_moved_file_takes_its_entry_along() {
    let listed = files(&[("old/a.rs", 900)]);
    let moved = messages(&counted_breaches(
        500,
        &listed,
        &files(&[("new/a.rs", 900)]),
    ));
    assert_eq!(moved.len(), 2, "{moved:?}");
    assert!(moved.iter().any(|m| m.starts_with("new/a.rs: 900 lines")));
    assert!(
        moved
            .iter()
            .any(|m| m.starts_with("old/a.rs: listed but not found"))
    );
}

#[test]
fn tightening_only_lowers_and_removes() {
    let listed = files(&[
        ("a.rs", 900),
        ("b.rs", 800),
        ("c.rs", 700),
        ("gone.rs", 600),
    ]);
    let actual = files(&[
        ("a.rs", 950),
        ("b.rs", 640),
        ("c.rs", 500),
        ("new.rs", 2000),
    ]);
    assert_eq!(
        tighten_counted(500, &listed, &actual),
        files(&[("a.rs", 900), ("b.rs", 640)])
    );
}

#[test]
fn the_list_is_read_strictly() {
    let kind = Kind::Counted { cap: 500 };
    let read = parse("# note\n\n900\ta.rs\n501\tb c.rs\n", kind).unwrap();
    let listed = Entries::Counted(files(&[("a.rs", 900), ("b c.rs", 501)]));
    assert_eq!(read.entries, listed);
    assert_eq!(read.header, ["# note", ""]);
    let again = parse(&render(&seed_header("x", kind), &listed), kind).unwrap();
    assert_eq!(again.entries, listed);
    for bad in [
        "900 a.rs\n",
        "many\ta.rs\n",
        "500\ta.rs\n",
        "900\ta.rs\n901\ta.rs\n",
        "900\tb.rs\n900\ta.rs\n",
        "900\ta.rs\n# late note\n",
        "900\ta.rs\n\n901\tb.rs\n",
        "900\t a.rs\n",
        "900\ta\\b.rs\n",
    ] {
        assert!(parse(bad, kind).is_err(), "{bad:?}");
    }
}

#[test]
fn a_listed_baseline_is_read_as_strictly() {
    let read = parse("# header\nb/x.rs\nc.rs\n", Kind::Listed).unwrap();
    assert_eq!(
        read.entries,
        Entries::Listed(["b/x.rs".to_owned(), "c.rs".to_owned()].into())
    );
    assert_eq!(read.lines["c.rs"], 3);
    for bad in ["c.rs\nb.rs\n", "a.rs\na.rs\n", "a.rs\tdetail\n", "./a.rs\n"] {
        assert!(parse(bad, Kind::Listed).is_err(), "{bad:?}");
    }
    let unsorted = parse("c.rs\nb.rs\n", Kind::Listed).unwrap_err();
    assert!(unsorted.contains("goes above c.rs"), "{unsorted}");
}

#[test]
fn a_crlf_checkout_reads_the_same() {
    let lf = parse("# h\n900\ta.rs\n", Kind::Counted { cap: 500 }).unwrap();
    let crlf = parse("# h\r\n900\ta.rs\r\n", Kind::Counted { cap: 500 }).unwrap();
    assert_eq!(lf, crlf);
}

#[test]
fn tightening_a_list_only_removes() {
    let listed: BTreeSet<String> = ["a.rs".to_owned(), "b.rs".to_owned()].into();
    let found: BTreeSet<String> = ["b.rs".to_owned(), "new.rs".to_owned()].into();
    assert_eq!(tighten_listed(&listed, &found), ["b.rs".to_owned()].into());
}
