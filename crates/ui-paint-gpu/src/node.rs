//! Pick the GPU that drives a connected output.

use std::path::{Path, PathBuf};

/// The GPU behind a connected output, e.g. `/dev/dri/renderD129`.
///
/// Zero-copy only works when the GPU that renders is the one the compositor
/// scans out from: rendering on the other device yields a buffer the compositor
/// imports successfully but shows as black.
pub fn output_render_node() -> Option<String> {
    let mut renders: Vec<(String, PathBuf)> = Vec::new();
    let mut connected: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(rest) = name.strip_prefix("renderD") {
            if rest.chars().all(|c| c.is_ascii_digit())
                && let Ok(dev) = std::fs::canonicalize(path.join("device"))
            {
                renders.push((format!("/dev/dri/{name}"), dev));
            }
        } else if name.starts_with("card") && !name.contains('-') {
            let has_port = std::fs::read_dir(&path).ok().is_some_and(|entries| {
                entries.flatten().any(|connector| {
                    let connector = connector.path();
                    let is_port = connector
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.contains('-'));
                    is_port
                        && std::fs::read_to_string(connector.join("status"))
                            .is_ok_and(|s| s.trim() == "connected")
                })
            });
            if has_port && let Ok(dev) = std::fs::canonicalize(path.join("device")) {
                connected.push(dev);
            }
        }
    }
    renders
        .into_iter()
        .find(|(_, dev)| connected.contains(dev))
        .map(|(node, _)| node)
}

/// Candidate render nodes, most likely first.
pub fn candidate_nodes() -> Vec<String> {
    let mut nodes = Vec::new();
    if let Some(node) = output_render_node() {
        nodes.push(node);
    }
    for fallback in ["/dev/dri/renderD128", "/dev/dri/renderD129"] {
        if Path::new(fallback).exists() && !nodes.iter().any(|n| n == fallback) {
            nodes.push(fallback.to_string());
        }
    }
    nodes
}
