//! Compile la feuille de style Tailwind (styles/app.css) vers `$OUT_DIR/app.css`, embarquée ensuite
//! dans le binaire. Exécutable requis : `tailwindcss` (autonome, sans Node), voir docs/tech/development.md.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=styles");
    println!("cargo:rerun-if-changed=templates");
    println!("cargo:rerun-if-env-changed=TAILWINDCSS");

    let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join("app.css");
    let binary = tailwind_binary();
    let status = Command::new(&binary)
        .args(["--input", "styles/app.css", "--output"])
        .arg(&output)
        .arg("--minify")
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "Impossible de lancer `{}` ({e}). Installe l'exécutable autonome de Tailwind CSS dans ~/.local/bin \
                 (voir docs/tech/development.md) ou indique son chemin dans la variable TAILWINDCSS.",
                binary.display()
            )
        });
    assert!(
        status.success(),
        "La compilation de styles/app.css par Tailwind a échoué."
    );
}

/// `$TAILWINDCSS`, sinon `~/.local/bin/tailwindcss` s'il existe, sinon `tailwindcss` dans le PATH.
fn tailwind_binary() -> PathBuf {
    if let Some(path) = env::var_os("TAILWINDCSS") {
        return path.into();
    }
    let local = env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin/tailwindcss"));
    match local {
        Some(path) if path.exists() => path,
        _ => "tailwindcss".into(),
    }
}
