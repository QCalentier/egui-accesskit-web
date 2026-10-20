//! The repository's tasks, in Rust so they need nothing installed:
//!
//! - `cargo xtask web`: the demo built for a browser in `dist/` (its page,
//!   the `.wasm` and its JavaScript glue, as the `wasm-bindgen` CLI makes
//!   them). Serve the folder (`python -m http.server --directory dist`)
//!   and open it.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The page loading the demo.
const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>egui, read aloud</title>
<style>
  html, body { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: #1b1b1b; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
</style>
</head>
<body>
<canvas id="demo"></canvas>
<script type="module">
  import start from "./demo.js";
  start().catch((error) => console.error(error));
</script>
</body>
</html>
"#;

fn main() -> ExitCode {
    let task: Vec<String> = std::env::args().skip(1).collect();
    let done = match task
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["web"] => web(),
        _ => Err("tasks: `cargo xtask web` (the demo for a browser, in dist/)".to_owned()),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// The demo for a browser, in `dist/`.
fn web() -> Result<(), String> {
    let root = root();
    let built = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .current_dir(&root)
        .args([
            "build",
            "-p",
            "demo",
            "--target",
            "wasm32-unknown-unknown",
            "--profile",
            "web",
        ])
        .status()
        .map_err(|e| format!("cargo didn't run: {e}"))?;
    if !built.success() {
        return Err("the demo didn't build for the web".to_owned());
    }
    let wasm = target_dir(&root)?.join("wasm32-unknown-unknown/web/demo.wasm");
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| format!("{}: {e}", dist.display()))?;
    wasm_bindgen_cli_support::Bindgen::new()
        .input_path(&wasm)
        .web(true)
        .and_then(|bindgen| {
            bindgen
                .typescript(false)
                .omit_default_module_path(false)
                .out_name("demo")
                .generate(&dist)
        })
        .map_err(|e| format!("wasm-bindgen: {e:#}"))?;
    let page = dist.join("index.html");
    std::fs::write(&page, PAGE).map_err(|e| format!("{}: {e}", page.display()))?;
    println!("the demo is in {}: serve it, then open it", dist.display());
    Ok(())
}

/// Where cargo builds (`target/`, unless a configuration says otherwise),
/// as `cargo metadata` says.
fn target_dir(root: &Path) -> Result<PathBuf, String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .current_dir(root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| format!("cargo metadata didn't run: {e}"))?;
    let json = String::from_utf8_lossy(&out.stdout);
    let key = "\"target_directory\":\"";
    let start = json
        .find(key)
        .map(|at| at + key.len())
        .ok_or("cargo metadata didn't say where it builds")?;
    let end = json[start..]
        .find('"')
        .map(|at| start + at)
        .ok_or("cargo metadata's answer was cut short")?;
    Ok(PathBuf::from(json[start..end].replace("\\\\", "\\")))
}

/// The repository's root (the workspace's).
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is in the repository")
        .to_path_buf()
}
