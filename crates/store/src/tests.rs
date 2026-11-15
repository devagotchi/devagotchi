use super::*;

#[test]
fn a_stores_place_never_shows_its_user_password_or_query() {
    for (url, place) in [
        (
            "postgres://hub:s3cret@db.internal:5432/devagotchi",
            "db.internal:5432/devagotchi",
        ),
        (
            "postgresql://hub@10.40.0.30/devagotchi?sslmode=require&password=x",
            "10.40.0.30/devagotchi",
        ),
        (
            "postgres:///devagotchi?host=/run/postgresql",
            "local socket/devagotchi",
        ),
        ("postgres://a:b@c@host/db", "host/db"),
    ] {
        assert_eq!(place_of(url), place, "{url}");
        assert!(!place_of(url).contains("s3cret"));
    }
}
