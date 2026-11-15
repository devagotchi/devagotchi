//! The migrations are embedded at compile time (`sqlx::migrate!`); a new or changed file there
//! rebuilds this crate.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
