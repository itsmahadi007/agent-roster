fn main() {
    // The window's files come from `npm run build` (app/dist). A fresh clone has none yet;
    // a placeholder lets `cargo test --workspace` compile without Node.
    let dist = std::path::Path::new("../dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(dist).expect("create app/dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>Agent Roster</title><p>Run npm run build in app/.</p>\n",
        )
        .expect("write placeholder index.html");
    }
    tauri_build::build()
}
