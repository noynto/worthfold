// Recompile la crate quand une migration change : `sqlx::migrate!` les embarque dans le binaire.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
