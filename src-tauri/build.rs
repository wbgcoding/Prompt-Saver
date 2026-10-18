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
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
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
    version: String,
    license: String,
    license_key: Option<String>,
    note: Option<String>,
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
        .args(["metadata", "--format-version", "1", "--manifest-path"])
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
    let mut entries: Vec<Entry> = Vec::new();

    for pkg in &metadata.packages {
        // This crate itself (and any other workspace member) has nothing to
        // attribute to a third party.
        if metadata.workspace_members.iter().any(|m| m == &pkg.id) {
            continue;
        }
        let license = match &pkg.license {
            Some(l) if !l.trim().is_empty() => l.trim().to_string(),
            _ => {
                if pkg.license_file.is_some() {
                    "see the crate's own LICENSE file".to_string()
                } else {
                    panic!(
                        "crate `{} {}` has no `license` field and no `license_file`; add an \
                         override for it in third-party-manual.json before shipping",
                        pkg.name, pkg.version
                    );
                }
            }
        };
        let key = resolve_license_key(&license);
        if let Some(k) = &key {
            used_keys.insert(k.clone());
        }
        entries.push(Entry {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            license,
            license_key: key,
            note: None,
        });
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.version.cmp(&b.version)));

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
            license_key: m.license_key.clone(),
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
            "licenseId": e.license_key,
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
        "{} components are listed below, sorted by name.\n\n",
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
