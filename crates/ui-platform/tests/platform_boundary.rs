//! Enforces the platform boundary as a check that can fail.
//!
//! This is a repository lint, not a unit test: it reads `Cargo.toml` and source
//! files, so it is the one place in this workspace that deliberately does I/O.
//! `AGENTS.md` §7.3 forbids I/O in *unit* tests; this file asserts a
//! cross-crate structural contract, which §7.4 places in `tests/`.

use std::path::{Path, PathBuf};

/// Crates that must build and run on any platform.
const PORTABLE: &[&str] = &[
    "ui-core",
    "ui-color-hct",
    "ui-design",
    "ui-gallery",
    "ui-gpu-scene",
    "ui-highlight",
    "ui-i18n",
    "ui-math",
    "ui-text",
    "ui-paint",
    "ui-paint-gpu",
    "ui-platform",
    "ui-text-fontique",
    "ui-richtext",
    "ui-style",
    "ui-widgets",
    "ui-widgets-fluent",
    "ui-widgets-glass",
    "ui-widgets-md3",
];

/// Crates allowed to touch OS-specific APIs, and why.
const PLATFORM_BOUND: &[(&str, &str)] = &[
    (
        "ui-platform-wayland",
        "Wayland window backend: wayland-client, libc, xkbcommon",
    ),
    (
        "ui-demo",
        "Showcase bootstrap: opens a window through the host facade and exits the process",
    ),
    (
        "ui-gpu-egl",
        "EGL device + gbm + dma-buf: unix fds, libc, libgbm",
    ),
    (
        "ui-gpu-d3d11",
        "D3D11 device: COM, DXGI and d3dcompiler are Win32 APIs, and the crate is empty off Windows",
    ),
    (
        "ui-host",
        "Host facade: re-exports this target's window backend, so it depends on a platform crate by design",
    ),
    (
        "ui-platform-win32",
        "Win32 window + DXGI swap chain: user32 and dxgi are Windows APIs, and the crate is empty off Windows",
    ),
];

/// APIs that pin a crate to one OS.
const FORBIDDEN_IN_PORTABLE: &[&str] = &[
    "std::os::unix",
    "std::os::windows",
    "cfg(target_os",
    "cfg(unix",
    "cfg(windows",
    "libc::",
    "OwnedFd",
    "FromRawFd",
    "std::process",
];

fn workspace_root() -> PathBuf {
    // This file lives at <root>/crates/ui-platform/tests/.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn portable_crates_use_no_platform_specific_apis() {
    let root = workspace_root();
    let mut violations: Vec<String> = Vec::new();

    for krate in PORTABLE {
        let src = root.join("crates").join(krate).join("src");
        let mut files = Vec::new();
        rust_sources(&src, &mut files);
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for (line_no, line) in text.lines().enumerate() {
                // Comments may discuss other platforms; only code matters.
                let code = line.split("//").next().unwrap_or("");
                for needle in FORBIDDEN_IN_PORTABLE {
                    if code.contains(needle) {
                        violations.push(format!(
                            "{}:{} uses `{needle}`",
                            file.strip_prefix(&root).unwrap_or(&file).display(),
                            line_no + 1
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "portable crates must not pin themselves to an OS. Either move the code into a \
         platform crate or add the crate to PLATFORM_BOUND with a reason.\n{}",
        violations.join("\n")
    );
}

#[test]
fn portable_crates_do_not_depend_on_platform_crates() {
    let root = workspace_root();
    let mut violations: Vec<String> = Vec::new();

    for krate in PORTABLE {
        let manifest = root.join("crates").join(krate).join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        // Only `[dependencies]` counts: a dev-dependency is unavailable to the library
        // build, so an example may open a window through a backend without coupling the
        // library. Treating them alike would either forbid every example or weaken the rule.
        let mut section = String::new();
        let mut in_deps = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_deps = trimmed == "[dependencies]";
                continue;
            }
            if in_deps {
                section.push_str(trimmed);
                section.push('\n');
            }
        }
        for (bound, _why) in PLATFORM_BOUND {
            for line in section.lines() {
                if line.starts_with('#') {
                    continue;
                }
                if line.starts_with(*bound) {
                    violations.push(format!("{krate}/Cargo.toml depends on {bound}"));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "a portable crate must never depend on a platform crate; the dependency goes the \
         other way.\n{}",
        violations.join("\n")
    );
}

#[test]
fn every_abstraction_crate_is_classified() {
    // A new crate that is neither portable nor declared platform-bound would sit
    // outside the boundary and rot silently.
    let root = workspace_root();
    let crates_dir = root.join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        panic!("no crates directory at {}", crates_dir.display());
    };
    let mut unclassified = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().join("Cargo.toml").exists() {
            continue;
        }
        let known = PORTABLE.contains(&name.as_str())
            || PLATFORM_BOUND.iter().any(|(bound, _)| *bound == name);
        if !known {
            unclassified.push(name);
        }
    }
    assert!(
        unclassified.is_empty(),
        "these crates are neither in PORTABLE nor PLATFORM_BOUND: {unclassified:?}"
    );
}
