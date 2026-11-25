use super::*;

fn seed() -> &'static Dictionary {
    Dictionary::seed()
}

fn ids(found: Vec<(&Concept, LabelKind)>) -> Vec<(String, LabelKind)> {
    found.into_iter().map(|(c, k)| (c.id.clone(), k)).collect()
}

#[test]
fn the_seed_holds_the_four_boundaries_and_the_stack_registry() {
    let d = seed();
    let boundaries: Vec<&str> = d
        .concepts()
        .iter()
        .filter(|c| c.scheme == Scheme::Boundary)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        boundaries,
        [
            "boundary/iam",
            "boundary/admin",
            "boundary/messaging",
            "boundary/llm"
        ]
    );
    assert_eq!(
        d.concepts()
            .iter()
            .filter(|c| c.scheme == Scheme::Stack)
            .count(),
        17
    );
    assert!(d.concepts().iter().all(|c| !c.sources.is_empty()));
    assert_eq!(
        d.concept("boundary/admin").unwrap().depends_on,
        ["boundary/iam"]
    );
}

#[test]
fn names_resolve_by_strength_and_shared_terms_stay_shared() {
    let d = seed();
    assert_eq!(
        ids(d.lookup("Sign  In")),
        [("boundary/iam".to_string(), LabelKind::Alias)]
    );
    assert_eq!(
        ids(d.lookup("identity and access management")),
        [("boundary/iam".to_string(), LabelKind::Canonical)]
    );
    assert_eq!(
        ids(d.lookup("god mode")),
        [("boundary/admin".to_string(), LabelKind::Informal)]
    );
    let role: Vec<String> = ids(d.lookup("role"))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(role, ["boundary/iam", "boundary/admin"]);
    assert!(d.lookup("billing").is_empty());
}

#[test]
fn a_name_kept_to_one_spelling_matches_only_that_spelling() {
    let d = seed();
    let go = d.concept("stack/go").unwrap();
    assert!(d.spelled_right(go, "Go"));
    assert!(!d.spelled_right(go, "go"));
    let rust = d.concept("stack/rust").unwrap();
    assert!(d.spelled_right(rust, "rust"));
}

#[test]
fn stacks_come_from_markers_and_packages_and_yield_to_their_frameworks() {
    let d = seed();
    let stacks = |files: &[&str], packages: &[&str]| -> Vec<String> {
        d.stacks_for(files, packages)
            .into_iter()
            .map(|c| c.id.clone())
            .collect()
    };
    assert_eq!(
        stacks(&["Cargo.toml", "crates/x/src/lib.rs"], &[]),
        ["stack/rust"]
    );
    assert_eq!(
        stacks(&["package.json", "next.config.mjs"], &["next", "react"]),
        ["stack/nextjs"]
    );
    assert_eq!(
        stacks(&["package.json", "vite.config.ts"], &["react"]),
        ["stack/react"]
    );
    assert_eq!(
        stacks(&["package.json", "tsconfig.json"], &[]),
        ["stack/typescript"]
    );
    assert_eq!(stacks(&["package.json"], &[]), ["stack/vanilla-js"]);
    assert_eq!(
        stacks(&["db\\migrations\\0001.sql", "go.mod"], &[]),
        ["stack/go", "stack/sql"]
    );
}

fn concept(id: &str, label: &str) -> Concept {
    Concept {
        id: id.into(),
        scheme: if id.starts_with("stack/") {
            Scheme::Stack
        } else {
            Scheme::Boundary
        },
        label: label.into(),
        definition: "d".into(),
        aliases: vec![],
        informal: vec![],
        retired: vec![],
        terms: vec![],
        exact_case: vec![],
        sources: vec![],
        markers: vec![],
        packages: vec![],
        yields_to: vec![],
        depends_on: vec![],
    }
}

#[test]
fn a_dictionary_with_a_broken_promise_is_refused() {
    let data = |concepts: Vec<Concept>, rules: Vec<Rule>| DictionaryData {
        source: "test".into(),
        concepts,
        rules,
    };
    let refused = |d: DictionaryData| Dictionary::new(d).unwrap_err().0;
    assert!(Dictionary::new(data(vec![concept("boundary/a", "A")], vec![])).is_ok());
    assert!(refused(data(vec![concept("a", "A")], vec![])).contains("<scheme>/<key>"));
    let mut wrong_scheme = concept("stack/a", "A");
    wrong_scheme.scheme = Scheme::Boundary;
    assert!(refused(data(vec![wrong_scheme], vec![])).contains("<scheme>/<key>"));
    let twice = vec![concept("boundary/a", "A"), concept("boundary/a", "B")];
    assert!(refused(data(twice, vec![])).contains("used twice"));
    let same = vec![
        concept("boundary/a", "Same"),
        concept("boundary/b", " same "),
    ];
    assert!(refused(data(same, vec![])).contains("label"));
    let mut marked = concept("boundary/a", "A");
    marked.markers = vec!["Cargo.toml".into()];
    assert!(refused(data(vec![marked], vec![])).contains("only a stack"));
    let mut yielding = concept("stack/a", "A");
    yielding.yields_to = vec!["stack/missing".into()];
    assert!(refused(data(vec![yielding], vec![])).contains("stack/missing"));
    let routed = |route: Route| {
        let rule = Rule {
            phrase: "x".into(),
            cues: vec![],
            route,
            note: None,
        };
        refused(data(vec![concept("boundary/a", "A")], vec![rule]))
    };
    let missing = "boundary/b".to_string();
    assert!(routed(Route::Concept { concept: missing }).contains("not a concept"));
    let one = vec!["boundary/a".to_string()];
    assert!(routed(Route::Split { concepts: one }).contains("two or more"));
    let blank = " ".to_string();
    assert!(routed(Route::Ask { question: blank }).contains("its words"));
}

#[test]
fn the_seed_round_trips_through_its_own_shape() {
    let d = seed();
    let again: DictionaryData =
        serde_json::from_value(serde_json::to_value(d.data()).unwrap()).unwrap();
    assert_eq!(&again, d.data());
    let json = serde_json::to_string(&schemars::schema_for!(DictionaryData)).unwrap();
    assert!(json.contains("yields_to"));
}
