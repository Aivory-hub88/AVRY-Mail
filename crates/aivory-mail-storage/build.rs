// `sqlx::migrate!` embeds the migration files at compile time, but Cargo
// can't see them as inputs. Without this, adding a migration leaves a stale
// binary that silently doesn't know about it.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
