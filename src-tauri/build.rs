use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Every app command becomes an `allow-<name>` / `deny-<name>` permission, so each
    // window's capability file can grant exactly the commands its page calls.
    let commands: &'static [&'static str] = Box::leak(handler_commands().into_boxed_slice());
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(commands));
    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri build setup failed: {e:#}");
    }
    generate_third_party_notices();
}

// The command names from the `generate_handler!` block of src/lib.rs, in list order. The
// last `::` segment of each path is the command name. A missing block or an
// entry that is not a plain path stops the build instead of leaving a command ungated.
fn handler_commands() -> Vec<&'static str> {
    println!("cargo:rerun-if-changed=src/lib.rs");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let path = Path::new(&manifest_dir).join("src").join("lib.rs");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
    let src: String = raw
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    let start = src
        .find("generate_handler![")
        .expect("src/lib.rs has no generate_handler! block");
    let open = start + "generate_handler!".len();
    let close = open + src[open..].find(']').expect("generate_handler! block is not closed");
    let mut names: Vec<&'static str> = Vec::new();
    for entry in src[open + 1..close].split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let plain = entry
            .split("::")
            .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if !plain {
            panic!("generate_handler! entry `{entry}` is not a plain path");
        }
        let name: &'static str = Box::leak(entry.rsplit("::").next().unwrap_or(entry).to_string().into_boxed_str());
        if names.contains(&name) {
            panic!("command `{name}` is listed twice in generate_handler!");
        }
        names.push(name);
    }
    if names.is_empty() {
        panic!("generate_handler! lists no commands");
    }
    names
}

#[derive(Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    targets: Vec<CargoTarget>,
}

#[derive(Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
}

#[derive(Deserialize)]
struct CargoDepKind {
    // null = a normal dependency, otherwise "dev" or "build".
    kind: Option<String>,
}

#[derive(Deserialize)]
struct CargoNodeDep {
    pkg: String,
    dep_kinds: Vec<CargoDepKind>,
}

#[derive(Deserialize)]
struct CargoNode {
    id: String,
    deps: Vec<CargoNodeDep>,
}

#[derive(Deserialize)]
struct CargoResolve {
    root: Option<String>,
    nodes: Vec<CargoNode>,
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: CargoResolve,
}

// The platform the shipped binary is built for: the notice list covers exactly what is linked
// into it, not what other platforms or the build itself would need.
const NOTICE_TARGET: &str = "x86_64-pc-windows-msvc";

// Ids of the packages that end up in the binary: reachable from the root package through normal
// dependencies only. Build and dev dependencies and proc-macro crates run at compile time and
// are not shipped, so neither they nor anything only they pull in is walked.
fn shipped_package_ids(metadata: &CargoMetadata) -> std::collections::BTreeSet<String> {
    let by_id: std::collections::HashMap<&str, &CargoNode> =
        metadata.resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let packages: std::collections::HashMap<&str, &CargoPackage> =
        metadata.packages.iter().map(|p| (p.id.as_str(), p)).collect();
    let root = metadata
        .resolve
        .root
        .as_deref()
        .expect("cargo metadata reported no root package for the third-party license list");
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut stack: Vec<&str> = vec![root];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.to_string()) {
            continue;
        }
        let Some(node) = by_id.get(id) else { continue };
        for dep in &node.deps {
            if !dep.dep_kinds.iter().any(|k| k.kind.is_none()) {
                continue;
            }
            let is_proc_macro = packages
                .get(dep.pkg.as_str())
                .map(|p| p.targets.iter().any(|t| t.kind.iter().any(|k| k == "proc-macro")))
                .unwrap_or(false);
            if !is_proc_macro {
                stack.push(dep.pkg.as_str());
            }
        }
    }
    seen.remove(root);
    seen
}

// Orders dotted versions numerically part by part ("1.9.0" before "1.10.0"); a part that is not
// a number (a pre-release tag) compares as text after the numbers.
fn version_order(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| -> Vec<(u64, String)> {
        v.split(['.', '-', '+'])
            .map(|p| (p.parse::<u64>().unwrap_or(u64::MAX), p.to_string()))
            .collect()
    };
    parts(a).cmp(&parts(b))
}

#[derive(Deserialize, Clone)]
struct ManualEntry {
    name: String,
    version: String,
    license: String,
    #[serde(rename = "licenseId")]
    license_key: Option<String>,
    note: String,
}

struct Entry {
    name: String,
    // All shipped versions of the name, ascending, joined by ", ".
    version: String,
    // The distinct licenses of those versions, joined by " / ".
    license: String,
    // One text key per distinct license that has a shipped text, in the same order.
    license_keys: Vec<String>,
    note: Option<String>,
}

// One row per crate name: every shipped version and every distinct license of it.
// Input tuples are (name, version, license, license text key).
fn group_by_name(mut found: Vec<(String, String, String, Option<String>)>) -> Vec<Entry> {
    found.sort_by(|a, b| {
        a.0.to_lowercase().cmp(&b.0.to_lowercase()).then(a.0.cmp(&b.0)).then(version_order(&a.1, &b.1))
    });
    let mut out: Vec<Entry> = Vec::new();
    let mut current: Option<Entry> = None;
    let mut versions: Vec<String> = Vec::new();
    let mut licenses: Vec<String> = Vec::new();
    for (name, version, license, key) in found {
        if current.as_ref().map(|e| e.name != name).unwrap_or(false) {
            let mut done = current.take().unwrap();
            done.version = versions.join(", ");
            done.license = licenses.join(" / ");
            versions.clear();
            licenses.clear();
            out.push(done);
        }
        let entry = current.get_or_insert_with(|| Entry {
            name: name.clone(),
            version: String::new(),
            license: String::new(),
            license_keys: Vec::new(),
            note: None,
        });
        if !versions.contains(&version) {
            versions.push(version);
        }
        if !licenses.contains(&license) {
            licenses.push(license);
        }
        if let Some(k) = key {
            if !entry.license_keys.contains(&k) {
                entry.license_keys.push(k);
            }
        }
    }
    if let Some(mut done) = current {
        done.version = versions.join(", ");
        done.license = licenses.join(" / ");
        out.push(done);
    }
    out
}

// Preferred order when a crate offers several licenses as an "A OR B" choice: pick
// the option this app already ships a real license text for, favouring the shortest
// and least restrictive one first. This is also why almost nothing here ends up
// needing the long copyleft texts even though a handful of crates offer them as one
// of several options.
const LICENSE_PREFERENCE: &[&str] = &[
    "MIT",
    "Zlib",
    "ISC",
    "0BSD",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "NCSA",
    "BSL-1.0",
    "Unlicense",
    "CC0-1.0",
    "Unicode-3.0",
    "IJG",
    "CDLA-Permissive-2.0",
    "Apache-2.0",
    "MPL-2.0",
    "LGPL-2.1-or-later",
    "GPL-3.0-or-later",
];

// Splits an SPDX-style expression ("MIT OR Apache-2.0", "MIT/Apache-2.0", a bare
// id, or one with a "WITH ..." exception clause) into the plain ids it offers.
fn split_license_expression(expr: &str) -> Vec<String> {
    expr.split('/')
        .flat_map(|part| part.split(" OR "))
        .flat_map(|part| part.split(" AND "))
        .map(|part| {
            let part = part.trim().trim_start_matches('(').trim_end_matches(')');
            match part.find(" WITH ") {
                Some(i) => part[..i].trim().to_string(),
                None => part.trim().to_string(),
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn resolve_license_key(expr: &str) -> Option<String> {
    let ids = split_license_expression(expr);
    for preferred in LICENSE_PREFERENCE {
        if ids.iter().any(|id| id == preferred) {
            return Some((*preferred).to_string());
        }
    }
    None
}

fn generate_third_party_notices() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=third-party-manual.json");
    println!("cargo:rerun-if-changed=license-texts");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is not set");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let output = Command::new(&cargo)
        .args(["metadata", "--format-version", "1", "--filter-platform", NOTICE_TARGET, "--manifest-path"])
        .arg(format!("{manifest_dir}/Cargo.toml"))
        .output()
        .expect("failed to run `cargo metadata` while generating the third-party license list");
    if !output.status.success() {
        panic!(
            "`cargo metadata` failed while generating the third-party license list, refusing to \
             ship a stale one:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("could not parse `cargo metadata` output for the third-party license list: {e}")
    });

    let mut used_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let shipped = shipped_package_ids(&metadata);
    let mut found: Vec<(String, String, String, Option<String>)> = Vec::new();
    for pkg in &metadata.packages {
        if !shipped.contains(&pkg.id) {
            continue;
        }
        let license = match &pkg.license {
            Some(l) if !l.trim().is_empty() => l.trim().to_string(),
            _ => {
                if pkg.license_file.is_some() {
                    "see the crate's own LICENSE file".to_string()
                } else {
                    panic!(
                        "crate `{} {}` has no `license` field and no `license_file`; add an                          override for it in third-party-manual.json before shipping",
                        pkg.name, pkg.version
                    );
                }
            }
        };
        let key = resolve_license_key(&license);
        if let Some(k) = &key {
            used_keys.insert(k.clone());
        }
        found.push((pkg.name.clone(), pkg.version.clone(), license, key));
    }
    let mut entries: Vec<Entry> = group_by_name(found);

    if entries.is_empty() {
        panic!("third-party license generator resolved zero crates; refusing to ship an empty list");
    }

    let manual_path = Path::new(&manifest_dir).join("third-party-manual.json");
    let manual_raw = fs::read_to_string(&manual_path).unwrap_or_else(|e| {
        panic!("could not read {}: {e}", manual_path.display())
    });
    let manual: Vec<ManualEntry> = serde_json::from_str(&manual_raw)
        .unwrap_or_else(|e| panic!("could not parse {}: {e}", manual_path.display()));
    for m in &manual {
        if let Some(k) = &m.license_key {
            used_keys.insert(k.clone());
        }
        entries.push(Entry {
            name: m.name.clone(),
            version: m.version.clone(),
            license: m.license.clone(),
            license_keys: m.license_key.clone().into_iter().collect(),
            note: Some(m.note.clone()),
        });
    }

    let texts_dir = Path::new(&manifest_dir).join("license-texts");
    let mut license_texts: BTreeMap<String, String> = BTreeMap::new();
    for key in &used_keys {
        let path = texts_dir.join(format!("{key}.txt"));
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "third-party license generator needs the text for `{key}` at {}, but it could \
                 not be read ({e}); add that file before shipping",
                path.display()
            )
        });
        license_texts.insert(key.clone(), text);
    }

    // JSON payload embedded into the binary and served to the frontend as-is.
    let mut json_entries = Vec::with_capacity(entries.len());
    for e in &entries {
        json_entries.push(serde_json::json!({
            "name": e.name,
            "version": e.version,
            "license": e.license,
            "licenseId": e.license_keys.first(),
            "licenseIds": e.license_keys,
            "note": e.note,
        }));
    }
    let payload = serde_json::json!({
        "generatedFromCrateCount": entries.len(),
        "entries": json_entries,
        "licenseTexts": license_texts,
    });
    let json_out = serde_json::to_string(&payload).expect("failed to serialize the third-party license list");
    fs::write(Path::new(&out_dir).join("third_party_licenses.json"), &json_out)
        .expect("failed to write the generated third-party license JSON");

    write_notices_markdown(&manifest_dir, &entries);
}

fn write_notices_markdown(manifest_dir: &str, entries: &[Entry]) {
    let repo_root = Path::new(manifest_dir)
        .parent()
        .expect("src-tauri has no parent directory");
    let mut md = String::new();
    md.push_str("# Third-Party Notices\n\n");
    md.push_str(
        "This file is generated at build time from the real dependency manifests \
         (`cargo metadata` for the Rust crates, plus the hand-maintained non-Rust parts) so it \
         can never drift from what actually ships. Do not edit it by hand; edit \
         `src-tauri/third-party-manual.json` or the crate dependencies instead, then rebuild. \
         The same list is shown in the app itself, in About > Third-party licenses.\n\n",
    );
    md.push_str(&format!(
        "{} components are listed below, one row per name, sorted by name; a name that ships in \
         several versions lists them all.\n\n",
        entries.len()
    ));
    md.push_str("| Name | Version | License |\n|---|---|---|\n");
    for e in entries {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            e.name.replace('|', "\\|"),
            e.version.replace('|', "\\|"),
            e.license.replace('|', "\\|"),
        ));
    }
    md.push('\n');
    let notes: Vec<&Entry> = entries.iter().filter(|e| e.note.is_some()).collect();
    if !notes.is_empty() {
        md.push_str("## Notes\n\n");
        for e in notes {
            md.push_str(&format!(
                "- **{}** ({}): {}\n",
                e.name,
                e.license,
                e.note.as_deref().unwrap_or_default()
            ));
        }
        md.push('\n');
    }
    md.push_str(
        "The full text of every license named above is shown in the app itself (About > \
         Third-party licenses, expand any entry) and is embedded in the executable at build \
         time from `src-tauri/license-texts/`.\n",
    );

    let target = repo_root.join("THIRD-PARTY-NOTICES.md");
    let write_needed = fs::read_to_string(&target).map(|old| old != md).unwrap_or(true);
    if write_needed {
        fs::write(&target, md).unwrap_or_else(|e| {
            panic!("failed to write {}: {e}", target.display())
        });
    }
}
