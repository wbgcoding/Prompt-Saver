use super::*;

// ---------- Import / export ----------

pub(crate) fn csv_cell(s: &str) -> String {
    // CSV formula injection: a cell starting with =, +, -, @ or a tab is read as a
    // formula by Excel/LibreOffice once reopened: CSV-quoting only protects the
    // CSV syntax, not spreadsheet evaluation. A leading single quote forces text.
    let needs_prefix = matches!(s.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r'));
    let s = if needs_prefix { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

// One CSV row: every cell guarded and joined with ';'.
pub(crate) fn csv_row(cells: &[&str]) -> String {
    cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(";")
}

// Joins/splits a rotating prompt's entries for every text-based export format (CSV cell,
// TXT body, Markdown-sync body) with one marker. Not a bare "---": to_txt joins different
// prompts on "\n\n---\n\n" and prompt_to_md's header is bounded by bare "---" lines, so one
// inside a body would be ambiguous. An HTML-comment marker collides with neither and reads
// as generated to someone editing the Markdown-sync file by hand.
pub(crate) const VARIANT_SEP: &str = "\n<!-- next -->\n";
// Escaped form of the marker, so an entry containing the literal separator survives the
// round trip instead of being split: the inserted backslash means it never equals
// VARIANT_SEP, so .split(VARIANT_SEP) cannot match inside it.
pub(crate) const VARIANT_SEP_ESCAPED: &str = "\n<!-- \\next -->\n";

pub(crate) fn join_variants(text: &str, variants: &[String]) -> String {
    // Escaped even with no further entries: a single text that contains the marker
    // verbatim must not come back split in two.
    let escape = |s: &str| s.replace(VARIANT_SEP, VARIANT_SEP_ESCAPED);
    if variants.is_empty() {
        return escape(text);
    }
    std::iter::once(escape(text))
        .chain(variants.iter().map(|s| escape(s)))
        .collect::<Vec<_>>()
        .join(VARIANT_SEP)
}

pub(crate) fn split_variants(joined: &str) -> (String, Vec<String>) {
    let unescape = |s: &str| s.replace(VARIANT_SEP_ESCAPED, VARIANT_SEP);
    let mut parts = joined.split(VARIANT_SEP);
    let text = unescape(parts.next().unwrap_or(""));
    (text, parts.map(unescape).collect())
}

// ---- Static placeholder resolution (export, Markdown sync) ----
// A background write (export, Markdown sync folder) cannot show the fill-in dialog, so it
// resolves the dialog's priority minus the interactive step: a remembered value, then a
// fixed value (ui_texts.varFixed), then the text's own {?name=default}, then blank. The raw
// {?...} markup never leaks into an export or someone else's Markdown folder. A dynamic
// name ({?@date}, {?@uuid}, ...) stays as written: its value only exists at the moment of
// copy.
pub(crate) fn parse_ui_text_map(settings: &Settings, key: &str) -> HashMap<String, String> {
    settings
        .ui_texts
        .get(key)
        .and_then(|s| serde_json::from_str::<HashMap<String, String>>(s).ok())
        .unwrap_or_default()
}

// Same token shape as the frontend's VAR_RE: `{?` + a body that starts with a
// letter, digit, `_` or `@`, holds no `}` or newline and is at most 200
// characters, + `}`. Returns (start, end, trimmed body) triples in source
// order, char-indexed and non-overlapping.
pub(crate) fn find_var_tokens(text: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 3 <= n {
        if chars[i] == '{'
            && chars.get(i + 1) == Some(&'?')
            && chars.get(i + 2).is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '@')
        {
            let body_start = i + 2;
            let mut j = body_start;
            let mut found_end = None;
            while j < n && j - body_start <= 200 {
                if chars[j] == '}' {
                    if j > body_start && j - body_start <= 200 {
                        found_end = Some(j);
                    }
                    break;
                }
                if chars[j] == '\n' {
                    break;
                }
                j += 1;
            }
            if let Some(end) = found_end {
                let body: String = chars[body_start..end].iter().collect();
                out.push((i, end + 1, body.trim().to_string()));
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

// The retired spelling `{#{` + body + `}#}`, kept only so the one-time upgrade (store.rs
// `upgrade_placeholders`) can find what to rewrite. Same return shape as find_var_tokens.
pub(crate) fn find_old_var_tokens(text: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 3 <= n {
        if chars[i] == '{' && chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'{') {
            let body_start = i + 3;
            let mut j = body_start;
            let mut found_end = None;
            while j < n && j - body_start <= 200 {
                if chars.get(j) == Some(&'}') && chars.get(j + 1) == Some(&'#') && chars.get(j + 2) == Some(&'}') {
                    found_end = Some(j);
                    break;
                }
                if chars[j] == '}' || chars[j] == '\n' {
                    break;
                }
                j += 1;
            }
            if let Some(end) = found_end {
                let body: String = chars[body_start..end].iter().collect();
                out.push((i, end + 3, body.trim().to_string()));
                i = end + 3;
                continue;
            }
        }
        i += 1;
    }
    out
}

// name, options (the choice list, empty for a plain variable) and default, split like the
// frontend's parseVar: a bar starts the option list, an equals sign before it (or before
// the whole token with no bar) sets the default. A bar with no usable options falls back to
// a plain variable.
pub(crate) fn parse_var_token(token: &str) -> (String, Vec<String>, String) {
    let raw = token.trim();
    let (name_part, options): (&str, Vec<String>) = match raw.find('|') {
        Some(bar) => {
            let opts: Vec<String> = raw[bar + 1..]
                .split(',')
                .map(|o| o.trim().to_string())
                .filter(|o| !o.is_empty())
                .collect();
            (&raw[..bar], opts)
        }
        None => (raw, Vec::new()),
    };
    let (name, default) = match name_part.find('=') {
        Some(eq) => (name_part[..eq].trim().to_string(), name_part[eq + 1..].trim().to_string()),
        None => (name_part.trim().to_string(), String::new()),
    };
    if name.is_empty() || (options.is_empty() && raw.contains('|')) {
        (raw.to_string(), Vec::new(), String::new())
    } else {
        (name, options, default)
    }
}

// A name written as @name, with a name from DYN_NAMES (the list the frontend keeps
// in step), is a reserved dynamic variable (date, uuid, the clipboard, ...),
// meaningful only at the moment of copy. Any other @word is an ordinary variable.
pub(crate) fn is_dyn_var_name(name: &str) -> bool {
    name.strip_prefix('@').is_some_and(|rest| DYN_NAMES.contains(&rest))
}

pub(crate) fn resolve_static_vars(text: &str, remembered: &HashMap<String, String>, fixed: &HashMap<String, String>) -> String {
    let tokens = find_var_tokens(text);
    if tokens.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (start, end, body) in tokens {
        out.extend(chars[cursor..start].iter().copied());
        let (name, options, default) = parse_var_token(&body);
        if name.is_empty() || is_dyn_var_name(&name) {
            out.extend(chars[start..end].iter().copied()); // left exactly as written
        } else if options.is_empty() {
            let value = remembered.get(&name).cloned().or_else(|| fixed.get(&name).cloned()).unwrap_or(default);
            out.push_str(&value);
        } else {
            let remembered_pick = remembered.get(&name).cloned().filter(|v| options.contains(v));
            let fixed_pick = fixed.get(&name).cloned().filter(|v| options.contains(v));
            let default_pick = if options.contains(&default) { Some(default.clone()) } else { None };
            let pick = remembered_pick.or(fixed_pick).or(default_pick).unwrap_or_else(|| options[0].clone());
            out.push_str(&pick);
        }
        cursor = end;
    }
    out.extend(chars[cursor..].iter().copied());
    out
}

// Resolves a prompt's text and every rotating-entry variant the same way, for a caller
// about to export or sync it (never the live prompt).
pub(crate) fn resolved_for_export(p: &Prompt, remembered: &HashMap<String, String>, fixed: &HashMap<String, String>) -> Prompt {
    let mut rp = p.clone();
    rp.text = resolve_static_vars(&rp.text, remembered, fixed);
    rp.variants = rp.variants.iter().map(|v| resolve_static_vars(v, remembered, fixed)).collect();
    rp
}

// Export the prompt library only (incl. images/files): never app settings,
// views or grid layout, so importing elsewhere can't clobber the user's setup.
pub(crate) fn to_csv(prompts: &[Prompt]) -> String {
    // pdfpage (like rotatemode) is appended at the END, not inserted among the columns: an
    // older export re-imported here has no such column, and row.get() on a short row returns
    // None (default 0) without shifting any other column's index.
    let head = [
        "name", "text", "positions", "color", "font", "size", "file", "icon", "show", "copy",
        "image", "caption", "capsize", "tags", "notes", "pdfpage", "rotatemode",
    ];
    let mut rows = vec![csv_row(&head)];
    for p in prompts {
        // Entries 2..n ride along inside the text cell: the CSV writer quotes every cell, embedded
        // newlines included, so the join marker needs no escaping.
        let text = join_variants(&p.text, &p.variants);
        rows.push(csv_row(&[
            &p.name,
            &text,
            "", // positions omitted (prompts-only export)
            &p.color,
            &p.font,
            &p.font_size.to_string(),
            &p.file_path,
            &p.icon_path,
            if p.show_image { "1" } else { "0" },
            if p.copy_image { "1" } else { "0" },
            &p.image,
            &p.caption,
            &p.caption_size.to_string(),
            &p.tags.join(","),
            // The CSV writer quotes every cell, so a multi-line note travels verbatim.
            &p.notes,
            &p.pdf_page.to_string(),
            &p.rotate_mode,
        ]));
    }
    rows.join("\r\n")
}

pub(crate) fn to_txt(prompts: &[Prompt]) -> String {
    use std::fmt::Write;
    let blocks: Vec<String> = prompts.iter().map(|p| {
        // Entries 2..n ride along inside the body, joined by the same marker as CSV/Markdown-sync,
        // never the bare "---" that separates blocks.
        let mut block = format!("### {}\n{}", p.name, txt_escape_body(&join_variants(&p.text, &p.variants)));
        if !p.color.is_empty() {
            let _ = write!(block, "\n@color {}", p.color);
        }
        if !p.notes.is_empty() {
            let _ = write!(block, "\n@notes {}", notes_escape(&p.notes));
        }
        if !p.tags.is_empty() {
            let _ = write!(block, "\n@tags {}", p.tags.join(","));
        }
        // Per-tile style: "@style <font-key|-> <size>" ("-" = default font).
        if !p.font.is_empty() || p.font_size > 0 {
            let font = if p.font.is_empty() { "-" } else { &p.font };
            let _ = write!(block, "\n@style {} {}", font, p.font_size);
        }
        if !p.file_path.is_empty() {
            let _ = write!(block, "\n@file {}", p.file_path);
        }
        // Only a PDF attachment reads this and 0 (the first page) is the common case, so the line
        // is skipped then like other optional fields.
        if p.pdf_page > 0 {
            let _ = write!(block, "\n@pdfpage {}", p.pdf_page);
        }
        if !p.icon_path.is_empty() {
            let _ = write!(block, "\n@icon {}", p.icon_path);
        }
        if !p.caption.is_empty() {
            let _ = write!(block, "\n@caption {}", p.caption);
        }
        if p.caption_size > 0 {
            let _ = write!(block, "\n@capsize {}", p.caption_size);
        }
        if p.show_image || p.copy_image {
            let _ = write!(
                block,
                "\n@flags {} {}",
                p.show_image as u8, p.copy_image as u8
            );
        }
        if !p.image.is_empty() {
            let _ = write!(block, "\n@imagedata {}", p.image);
        }
        // Order/random preference: written sparsely like every other optional field.
        if !p.rotate_mode.is_empty() {
            let _ = write!(block, "\n@rotatemode {}", p.rotate_mode);
        }
        block
    }).collect();
    blocks.join("\n\n---\n\n")
}

// The JSON export format: unlike CSV/TXT (prompt content only, see import_prompts) it is
// the lossless round trip, so it also keeps `favorite`. `id`, `hotkey`, `hotkey_paste`,
// `trashed_at` and `rotate_auto` stay out: they are machine-local or per-session state, and
// a re-import grabbing an existing hotkey binding or resurrecting a trashed_at would be a
// footgun. Rotating entries ride as a real JSON array, so no VARIANT_SEP is needed.
#[derive(Serialize, Deserialize, Default)]
pub(crate) struct ExportPrompt {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) color: String,
    #[serde(default)]
    pub(crate) image: String,
    #[serde(default)]
    pub(crate) show_image: bool,
    #[serde(default)]
    pub(crate) copy_image: bool,
    #[serde(default)]
    pub(crate) file_path: String,
    #[serde(default)]
    pub(crate) pdf_page: u32,
    #[serde(default)]
    pub(crate) icon_path: String,
    #[serde(default)]
    pub(crate) caption: String,
    #[serde(default)]
    pub(crate) caption_size: u32,
    #[serde(default)]
    pub(crate) font: String,
    #[serde(default)]
    pub(crate) font_size: u32,
    #[serde(default)]
    pub(crate) favorite: bool,
    // Written only for a confidential entry, which then carries its sealed
    // text and variants as they are.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) secret: bool,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) notes: String,
    #[serde(default)]
    pub(crate) variants: Vec<String>,
    #[serde(default)]
    pub(crate) rotate_mode: String,
}

pub(crate) fn to_json(prompts: &[Prompt]) -> String {
    let items: Vec<ExportPrompt> = prompts
        .iter()
        .map(|p| ExportPrompt {
            name: p.name.clone(),
            text: p.text.clone(),
            color: p.color.clone(),
            image: p.image.clone(),
            show_image: p.show_image,
            copy_image: p.copy_image,
            file_path: p.file_path.clone(),
            pdf_page: p.pdf_page,
            icon_path: p.icon_path.clone(),
            caption: p.caption.clone(),
            caption_size: p.caption_size,
            font: p.font.clone(),
            font_size: p.font_size,
            favorite: p.favorite,
            secret: p.secret,
            tags: p.tags.clone(),
            notes: p.notes.clone(),
            variants: p.variants.clone(),
            rotate_mode: p.rotate_mode.clone(),
        })
        .collect();
    // Every field is a String/bool/u32/Vec<String>, so serialization cannot fail and the
    // default on the unreachable error path is safe.
    serde_json::to_string_pretty(&items).unwrap_or_default()
}

pub(crate) fn parse_json_data(content: &str) -> Result<ImportData, String> {
    let items: Vec<ExportPrompt> = serde_json::from_str(content)
        .map_err(|_| "the file is not valid JSON".to_string())?;
    let mut data = ImportData::default();
    for item in items {
        // Mirrors parse_csv_data's `row[0].trim().is_empty()` skip: an
        // entry with neither a name nor any text is not a real prompt.
        if item.name.trim().is_empty() && item.text.trim().is_empty() {
            continue;
        }
        data.prompts.push(ImportedPrompt {
            name: item.name,
            text: item.text,
            color: item.color,
            font: item.font,
            font_size: item.font_size,
            file_path: item.file_path,
            pdf_page: item.pdf_page,
            icon_path: item.icon_path,
            caption: item.caption,
            caption_size: item.caption_size,
            show_image: item.show_image,
            copy_image: item.copy_image,
            image: item.image,
            tags: item.tags,
            notes: item.notes,
            variants: item.variants,
            rotate_mode: item.rotate_mode,
            favorite: item.favorite,
            secret: item.secret,
        });
    }
    Ok(data)
}

// A confidential entry's fate in a one-shot export: `true` keeps it, ciphertext and all
// (the receiving end needs the same vault password), `false` (the default) drops it rather
// than silently shipping ciphertext nobody asked for. Only the JSON export carries the
// confidential flag, so CSV and TXT never ship sealed text (an import would read it back as
// a plain entry). Pure, apart from export_prompts' file-dialog and format side effects.
pub(crate) fn secrets_exportable(format: &str, include_secrets: bool) -> bool {
    include_secrets && format == "json"
}

pub(crate) fn filter_export_secrets(prompts: Vec<Prompt>, include_secrets: bool) -> Vec<Prompt> {
    if include_secrets {
        prompts
    } else {
        prompts.into_iter().filter(|p| !p.secret).collect()
    }
}

// Async: building the export (base64 images) + writing it stays off the UI thread.
#[tauri::command]
pub(crate) async fn export_prompts(
    app: AppHandle,
    state: State<'_, Db>,
    format: String,
    ids: Option<Vec<String>>,
    include_secrets: Option<bool>,
) -> Result<usize, String> {
    let (content, count) = {
        let store = lock(&state);
        // An explicit id list exports only the selection; otherwise everything.
        let selected: Vec<Prompt> = match &ids {
            Some(list) if !list.is_empty() => {
                let set: std::collections::HashSet<&str> = list.iter().map(|s| s.as_str()).collect();
                store.prompts.iter().filter(|p| set.contains(p.id.as_str())).cloned().collect()
            }
            _ => store.prompts.clone(),
        };
        let selected = filter_export_secrets(selected, secrets_exportable(&format, include_secrets.unwrap_or(false)));
        // A placeholder is filled at copy time and an export has no such moment, so it is resolved
        // here instead of leaving raw {?...} syntax in the file. Each source is consulted only
        // while its own flag is on, as in the fill-in dialog (varDefaults ships on, varFixed off).
        let remembered = if store.settings.ui_flags.get("varDefaults") != Some(&false) {
            parse_ui_text_map(&store.settings, "varDefaults")
        } else {
            HashMap::new()
        };
        let fixed = if store.settings.ui_flags.get("varFixed") == Some(&true) {
            parse_ui_text_map(&store.settings, "varFixed")
        } else {
            HashMap::new()
        };
        let resolved: Vec<Prompt> = selected.iter().map(|p| resolved_for_export(p, &remembered, &fixed)).collect();
        // An exported file must never carry an absolute demo-dir path (see strip_data_dir_prefix).
        let export_dir = data_dir(&app);
        let resolved = strip_data_dir_prefix(&export_dir, data_dir_is_demo(&app, &export_dir), &resolved).into_owned();
        let content = match format.as_str() {
            "csv" => to_csv(&resolved),
            "txt" => to_txt(&resolved),
            "json" => to_json(&resolved),
            _ => return Err("unsupported-export-format".to_string()),
        };
        (content, resolved.len())
    };
    let file = file_dialog(&app)
        .set_file_name(format!("{}.{}", brand::ITEM_PLURAL, format))
        .add_filter(format.to_uppercase(), &[format.as_str()])
        .save_file();
    match file {
        Some(path) => {
            write_bytes_atomic(&path, content.as_bytes()).map_err(|e| e.to_string())?;
            Ok(count)
        }
        None => Err("canceled".to_string()),
    }
}

// Reverses csv_cell's formula-injection guard: a field re-quoted with a leading `'` in
// front of =, +, -, @, tab or CR gets that quote stripped, so a round trip never gains a
// stray character.
pub(crate) fn strip_formula_guard(s: String) -> String {
    let mut chars = s.chars();
    if chars.next() == Some('\'') && matches!(chars.next(), Some('=' | '+' | '-' | '@' | '\t' | '\r')) {
        return s[1..].to_string();
    }
    s
}

// Minimal CSV reader for our own export format (quoted fields, ';' delimiter).
pub(crate) fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ';' => row.push(strip_formula_guard(std::mem::take(&mut field))),
                '\r' => {}
                '\n' => {
                    row.push(strip_formula_guard(std::mem::take(&mut field)));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(strip_formula_guard(field));
        rows.push(row);
    }
    rows
}

pub(crate) struct ImportedPrompt {
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) color: String,
    pub(crate) font: String,
    pub(crate) font_size: u32,
    pub(crate) file_path: String,
    pub(crate) pdf_page: u32,
    pub(crate) icon_path: String,
    pub(crate) caption: String,
    pub(crate) caption_size: u32,
    pub(crate) show_image: bool,
    pub(crate) copy_image: bool,
    pub(crate) image: String,
    pub(crate) tags: Vec<String>,
    pub(crate) notes: String,
    pub(crate) variants: Vec<String>,
    pub(crate) rotate_mode: String,
    // True only from a JSON file (the one format that exports it).
    pub(crate) favorite: bool,
    // Only ever true coming from a JSON file; honoured for sealed text only.
    pub(crate) secret: bool,
}

// "a,b , c" -> ["a", "b", "c"]. Used by both import parsers; empty parts drop out.
pub(crate) fn split_tag_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

// 0 = follow settings, 1 = auto-fit, otherwise a fixed pixel size.
pub(crate) fn clamp_font_size(size: u32) -> u32 {
    if size <= 1 { size } else { size.clamp(10, 40) }
}

pub(crate) fn clamp_caption_size(size: u32) -> u32 {
    clamp_font_size(size)
}

#[derive(Default)]
pub(crate) struct ImportData {
    pub(crate) language: Option<String>,
    pub(crate) theme: Option<String>,
    pub(crate) tile_font: Option<String>,
    pub(crate) tile_size: Option<u32>,
    pub(crate) minimize_to_tray: Option<bool>,
    pub(crate) auto_update: Option<bool>,
    pub(crate) show_header: Option<bool>,
    pub(crate) show_composer: Option<bool>,
    pub(crate) view_defs: Vec<String>,
    pub(crate) prompts: Vec<ImportedPrompt>,
}

// "key=value" lines from an @settings block.
pub(crate) fn parse_settings_lines(lines: &str, data: &mut ImportData) {
    let flag = |v: &str| Some(v.trim() == "1");
    for line in lines.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("language=") {
            data.language = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("theme=") {
            data.theme = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("tile_font=") {
            data.tile_font = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("tile_size=") {
            data.tile_size = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("minimize_to_tray=") {
            data.minimize_to_tray = flag(v);
        } else if let Some(v) = line.strip_prefix("auto_update=") {
            data.auto_update = flag(v);
        } else if let Some(v) = line.strip_prefix("show_header=") {
            data.show_header = flag(v);
        } else if let Some(v) = line.strip_prefix("show_composer=") {
            data.show_composer = flag(v);
        }
    }
}

// The metadata keys a TXT block may end with (see to_txt).
pub(crate) const TXT_META_KEYS: [&str; 12] = [
    "rotatemode", "imagedata", "flags", "capsize", "caption", "icon", "pdfpage", "file", "style", "tags", "notes", "color",
];

// True for a body line that would read as metadata or as the block rule once
// written out: "@..." or a bare "---", behind any number of escaping backslashes.
pub(crate) fn txt_line_needs_escape(line: &str) -> bool {
    let bare = line.trim_start_matches('\\');
    bare.starts_with('@') || bare == "---"
}

// A body line that could be mistaken for metadata or for the block separator gets
// one extra leading backslash; every other line, including one that merely starts
// with a backslash, is written as it is.
pub(crate) fn txt_escape_body(body: &str) -> String {
    body.split('\n')
        .map(|l| if txt_line_needs_escape(l) { format!("\\{l}") } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn txt_unescape_body(body: &str) -> String {
    body.split('\n')
        .map(|l| match l.strip_prefix('\\') {
            Some(rest) if txt_line_needs_escape(l) => rest,
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// Splits a block body into its text and the trailing run of "@key value" lines
// (known keys only, each at most once, the run ends at the first other line).
pub(crate) fn split_txt_meta(body: &str) -> (String, HashMap<&'static str, String>) {
    let mut lines: Vec<&str> = body.split('\n').collect();
    let mut meta: HashMap<&'static str, String> = HashMap::new();
    while let Some(last) = lines.last() {
        let Some((key, value)) = last.strip_prefix('@').and_then(|r| r.split_once(' ')) else { break };
        let Some(known) = TXT_META_KEYS.iter().find(|k| **k == key) else { break };
        if meta.contains_key(known) {
            break;
        }
        meta.insert(known, value.to_string());
        lines.pop();
    }
    (lines.join("\n"), meta)
}

pub(crate) fn parse_txt(content: &str) -> ImportData {
    let mut data = ImportData::default();
    for block in content.replace("\r\n", "\n").split("\n\n---\n\n") {
        let block = block.trim();
        if let Some(s) = block.strip_prefix("@settings") {
            parse_settings_lines(s, &mut data);
            continue;
        }
        if let Some(defs) = block.strip_prefix("@views") {
            data.view_defs.extend(
                defs.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()),
            );
            continue;
        }
        let Some(rest) = block.strip_prefix("### ") else { continue };
        let (name, body) = rest.split_once('\n').unwrap_or((rest, ""));
        // A legacy export may still carry an @positions block; the import is
        // prompts-only, so it is dropped here rather than parsed.
        let body = match body.split_once("\n@positions\n") {
            Some((t, _)) => t,
            None => body,
        };
        // The metadata is the trailing run of "@key value" lines in any order; everything above is
        // the body, whose own lines were escaped on export when they could pass for one.
        let (body, mut meta) = split_txt_meta(body);
        let body = txt_unescape_body(&body);
        let rotate_mode = meta.remove("rotatemode").map(|v| v.trim().to_string()).unwrap_or_default();
        let image = meta.remove("imagedata").map(|v| v.trim().to_string()).unwrap_or_default();
        let (show_image, copy_image) = match meta.remove("flags") {
            Some(v) => {
                let mut parts = v.split_whitespace();
                let show = parts.next() == Some("1");
                let copy = parts.next() == Some("1");
                (show, copy)
            }
            None => (false, false),
        };
        let caption_size = meta.remove("capsize").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let caption = meta.remove("caption").map(|v| v.trim().to_string()).unwrap_or_default();
        let icon_path = meta.remove("icon").map(|v| v.trim().to_string()).unwrap_or_default();
        let pdf_page = meta.remove("pdfpage").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let file_path = meta.remove("file").map(|v| v.trim().to_string()).unwrap_or_default();
        let (font, font_size) = match meta.remove("style") {
            Some(v) => {
                let mut parts = v.split_whitespace();
                let font = parts.next().filter(|f| *f != "-").unwrap_or("").to_string();
                let size = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                (font, size)
            }
            None => (String::new(), 0),
        };
        let tags = meta.remove("tags").map(|v| split_tag_list(&v)).unwrap_or_default();
        let notes = meta.remove("notes").map(|v| notes_unescape(v.trim())).unwrap_or_default();
        let color = meta.remove("color").map(|v| v.trim().to_string()).unwrap_or_default();
        let text = body;
        // Entries 2..n still ride inside `text`: split them out now that the trailing @key lines
        // are gone.
        let (text, variants) = split_variants(&text);
        data.prompts.push(ImportedPrompt {
            name: name.trim().to_string(),
            text,
            color,
            font,
            font_size,
            file_path,
            pdf_page,
            icon_path,
            caption,
            caption_size,
            show_image,
            copy_image,
            image,
            tags,
            notes,
            variants,
            rotate_mode,
            favorite: false,
            secret: false,
        });
    }
    data
}

// Our own CSV export format back into import data.
pub(crate) fn parse_csv_data(content: &str) -> ImportData {
    let mut data = ImportData::default();
    for row in parse_csv(content).into_iter().skip(1) {
            if row.is_empty() || row[0].trim().is_empty() {
                continue;
            }
            match row[0].trim() {
                "@settings" => {
                    if let Some(s) = row.get(1) {
                        parse_settings_lines(s, &mut data);
                    }
                    continue;
                }
                "@views" => {
                    if let Some(d) = row.get(1) {
                        data.view_defs.extend(
                            d.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()),
                        );
                    }
                    continue;
                }
                _ => {}
            }
            if row.len() < 2 {
                continue;
            }
            // Entries 2..n ride along inside the text cell (column 1).
            let (text, variants) = split_variants(&row[1]);
            data.prompts.push(ImportedPrompt {
                name: row[0].trim().to_string(),
                text,
                color: row.get(3).map(|c| c.trim().to_string()).unwrap_or_default(),
                font: row.get(4).map(|f| f.trim().to_string()).unwrap_or_default(),
                font_size: row
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0),
                file_path: row.get(6).map(|f| f.trim().to_string()).unwrap_or_default(),
                pdf_page: row.get(15).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                icon_path: row.get(7).map(|f| f.trim().to_string()).unwrap_or_default(),
                show_image: row.get(8).map(|v| v.trim() == "1").unwrap_or(false),
                copy_image: row.get(9).map(|v| v.trim() == "1").unwrap_or(false),
                image: row.get(10).map(|v| v.trim().to_string()).unwrap_or_default(),
                caption: row.get(11).map(|v| v.trim().to_string()).unwrap_or_default(),
                caption_size: row.get(12).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                tags: row.get(13).map(|v| split_tag_list(v)).unwrap_or_default(),
                notes: row.get(14).cloned().unwrap_or_default(),
                variants,
                rotate_mode: row.get(16).map(|v| v.trim().to_string()).unwrap_or_default(),
                favorite: false,
                secret: false,
            });
    }
    data
}

// The signature a prompt is de-duplicated by, on preview and real import alike; the two
// must agree or the preview's "new"/"duplicate" label could lie.
pub(crate) fn import_sig(name: &str, text: &str, file: &str, image: &str, copy: bool) -> String {
    format!("{name}\u{1}{text}\u{1}{file}\u{1}{image}\u{1}{copy}")
}

// Parses already-read bytes: the caller reads once and hashes those same bytes for the
// changed-since-preview check, so the file is never read twice.
pub(crate) fn parse_import_bytes(bytes: &[u8], path: &std::path::Path) -> Result<ImportData, String> {
    let content = String::from_utf8(bytes.to_vec()).map_err(|_| "the file is not valid UTF-8 text".to_string())?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut data = match ext.as_str() {
        "csv" => parse_csv_data(&content),
        "json" => parse_json_data(&content)?,
        _ => parse_txt(&content),
    };
    // A file from an older version still spells placeholders the old way.
    for p in &mut data.prompts {
        p.text = upgrade_placeholders(&p.text);
        for v in &mut p.variants {
            *v = upgrade_placeholders(v);
        }
    }
    if data.prompts.is_empty() {
        return Err(format!("no {} found", brand::ITEM_PLURAL));
    }
    Ok(data)
}

pub(crate) fn import_file_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

// What an import would do, without touching the store. The file is picked here and its
// path travels back so the confirm step (import_prompts) re-reads the same file without a
// second dialog. `hash` lets the confirm step detect the file changing under the preview
// instead of importing something the user never saw.
#[derive(Serialize)]
pub(crate) struct ImportPreviewItem {
    pub(crate) name: String,
    pub(crate) duplicate: bool,
}
#[derive(Serialize)]
pub(crate) struct ImportPreview {
    pub(crate) path: String,
    pub(crate) hash: String,
    pub(crate) items: Vec<ImportPreviewItem>,
}
#[tauri::command]
pub(crate) async fn preview_import(app: AppHandle, state: State<'_, Db>) -> Result<ImportPreview, String> {
    let file = file_dialog(&app)
        .add_filter(title_case(brand::ITEM_PLURAL), &["csv", "txt", "json"])
        .pick_file();
    let Some(path) = file else {
        return Err("canceled".to_string());
    };
    let bytes = read_file_capped(&path, MAX_IMPORT_BYTES)?;
    let hash = import_file_hash(&bytes);
    let data = parse_import_bytes(&bytes, &path)?;
    let store = lock(&state);
    let mut seen: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .map(|p| import_sig(&p.name, &p.text, &p.file_path, &p.image, p.copy_image))
        .collect();
    let items = data
        .prompts
        .into_iter()
        .map(|item| {
            let dup = !seen.insert(import_sig(&item.name, &item.text, &item.file_path, &item.image, item.copy_image));
            ImportPreviewItem { name: item.name, duplicate: dup }
        })
        .collect();
    Ok(ImportPreview { path: path.to_string_lossy().to_string(), hash, items })
}

// A foreign file is an entry point like any other, but one bad tag must not cost the entry
// its other tags: invalid names are dropped one by one.
pub(crate) fn lenient_import_tags(raw: Vec<String>, known: &HashMap<String, String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let valid: Vec<String> = raw
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().count() <= MAX_TAG_LEN && tag_chars_ok(s))
        .filter(|s| seen.insert(tag_key(s)))
        .take(MAX_TAGS_PER_PROMPT)
        .collect();
    sanitize_tags(valid, known).unwrap_or_default()
}

// An over-long note is cut at the limit (on a character boundary) rather than
// dropped whole.
pub(crate) fn lenient_import_note(raw: &str) -> String {
    sanitize_notes(raw).unwrap_or_else(|_| {
        let text = raw.replace("\r\n", "\n").replace('\r', "\n");
        text.trim().chars().take(MAX_NOTES_LEN).collect()
    })
}

// A path that reaches another machine: UNC (`\\host\share`, `//host/share`, `\\?\UNC\...`)
// or a drive letter that maps to a remote share. Opening one makes Windows send the user's
// credentials to that host, so a file from outside never gets to name one.
pub(crate) fn is_network_path(raw: &str) -> bool {
    let t = raw.trim();
    if t.starts_with("\\\\?\\") || t.starts_with("//?/") {
        let rest = &t[4..];
        if rest.get(..4).is_some_and(|h| h.eq_ignore_ascii_case("unc\\") || h.eq_ignore_ascii_case("unc/")) {
            return true;
        }
        return drive_is_remote(rest);
    }
    if t.starts_with("\\\\") || t.starts_with("//") {
        return true;
    }
    drive_is_remote(t)
}

fn drive_is_remote(p: &str) -> bool {
    let b = p.as_bytes();
    if b.len() < 2 || !b[0].is_ascii_alphabetic() || b[1] != b':' {
        return false;
    }
    let root: Vec<u16> = format!("{}:\\", b[0] as char).encode_utf16().chain(std::iter::once(0)).collect();
    const DRIVE_REMOTE: u32 = 4;
    unsafe { windows::Win32::Storage::FileSystem::GetDriveTypeW(windows::core::PCWSTR(root.as_ptr())) == DRIVE_REMOTE }
}

// Clears every network path an imported row carries; the row itself stays.
pub(crate) fn clear_network_paths(p: &mut Prompt) {
    for f in [&mut p.file_path, &mut p.icon_path, &mut p.image] {
        if is_network_path(f) {
            f.clear();
        }
    }
}

// Shared by import_prompts and import_folder_files: builds a real Prompt for each parsed
// item and appends it, skipping an exact duplicate of one already present so the original
// is kept. A malformed tag or note on one item never aborts the whole batch.
pub(crate) fn append_imported_prompts(store: &mut Store, items: Vec<ImportedPrompt>) -> usize {
    let mut seen: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .map(|p| import_sig(&p.name, &p.text, &p.file_path, &p.image, p.copy_image))
        .collect();
    let mut count = 0usize;
    // Grows while importing so tags introduced by the file itself also settle on one casing.
    let mut known = canonical_tags(&store.prompts);
    for item in items {
        if !seen.insert(import_sig(&item.name, &item.text, &item.file_path, &item.image, item.copy_image)) {
            continue; // duplicate of an existing (or earlier imported) prompt
        }
        let tags = lenient_import_tags(item.tags, &known);
        for t in &tags {
            known.entry(tag_key(t)).or_insert_with(|| t.clone());
        }
        // The same soft-editor-limit / hard-backend-floor split every other
        // entry point into a rotating prompt already goes through.
        let variants = sanitize_variants(item.variants);
        let rotate_mode = sanitize_rotate_mode(item.rotate_mode, &variants);
        // A JSON file keeps an entry confidential when its text and every
        // variant are still sealed; anything else arrives as a plain entry.
        let secret = item.secret && looks_sealed(&item.text) && variants.iter().all(|v| looks_sealed(v));
        let prompt = Prompt {
            id: gen_id(),
            trashed_at: 0,
            name: item.name,
            text: item.text,
            color: item.color,
            image: item.image,
            show_image: item.show_image,
            copy_image: item.copy_image,
            file_path: item.file_path,
            pdf_page: item.pdf_page,
            icon_path: item.icon_path,
            caption: item.caption,
            caption_size: clamp_caption_size(item.caption_size),
            font: item.font,
            font_size: clamp_font_size(item.font_size),
            // Only ever true for a JSON import: CSV/TXT never carry it.
            favorite: item.favorite,
            tags,
            // A note from a foreign file must not abort the import either.
            notes: lenient_import_note(&item.notes),
            // A CSV/TXT file carries prompt content, never machine-local bindings (hotkeys, or the
            // paste choice that belongs to one).
            hotkey: String::new(),
            hotkey_paste: String::new(),
            variants,
            rotate_mode,
            // A CSV/TXT file never carries the auto-session state either: it is a
            // per-machine live behaviour, not portable prompt content.
            rotate_auto: false,
            // Not measured here: an imported attachment path is often not reachable yet (different
            // machine). The library backfill measures it when the row is first shown.
            media_duration_ms: 0,
            media_width: 0,
            media_height: 0,
            media_size_bytes: 0,
            // Never exported, so never imported; the OCR backfill picks up any picture this entry
            // carries.
            image_text: String::new(),
            secret,
        };
        let mut prompt = prompt;
        clear_network_paths(&mut prompt);
        store.prompts.push(prompt);
        count += 1;
    }
    count
}

// Async: parsing + persisting stays off the UI thread. `path` comes from a prior
// preview_import call, never a fresh dialog, so confirming cannot import a different file.
// `hash` is that preview's digest of the file; a mismatch means it changed after the
// preview and the import is refused.
#[tauri::command]
pub(crate) async fn import_prompts(app: AppHandle, state: State<'_, Db>, path: String, hash: String) -> Result<usize, String> {
    let bytes = read_file_capped(std::path::Path::new(&path), MAX_IMPORT_BYTES)?;
    if import_file_hash(&bytes) != hash {
        return Err("import-changed".to_string());
    }
    let data = parse_import_bytes(&bytes, std::path::Path::new(&path))?;

    // Prompts-only import: settings, views and layout are never touched, even if an older
    // export still carries those blocks (parsed but ignored).
    let mut store = lock(&state);
    let before_len = store.prompts.len(); // everything imported is appended after this
    let count = append_imported_prompts(&mut store, data.prompts);
    // A failed save must not report N prompts imported for a write that never
    // reached disk: an appended tail is trivial to roll back exactly.
    if !save_prompts(&app, &store) {
        store.prompts.truncate(before_len);
        return Err("save-failed".into());
    }
    let settings = store.settings.clone();
    drop(store);
    push_theme(&app, &settings);
    rebuild_tray(&app);
    Ok(count)
}

// ---- Import from a folder of .md/.txt files ----
// Read-only: nothing here moves or deletes a source file. `importFolderDepth` (ui_values,
// 1-5, default 3) counts the picked folder as depth 1, so 1 means this folder only; a
// subfolder past the limit is left out entirely, never partially. A file past
// `importFolderMax` (ui_values, default 500) stops the scan rather than filtering
// afterwards, so nothing skipped this way could later be picked by name and still get in.
pub(crate) const IMPORT_FOLDER_EXTS: [&str; 2] = ["md", "txt"];
// A file bigger than this is almost certainly not a hand-written note; treated
// the same as an unreadable one (grayed out with a reason, never imported).
pub(crate) const MAX_IMPORT_FOLDER_FILE_BYTES: u64 = 2 * 1024 * 1024;
// The first line becomes the standalone note only while it stays this short --
// past this length it clearly already IS the body, not a one-line summary.
pub(crate) const IMPORT_FOLDER_NOTE_MAX_CHARS: usize = 120;

pub(crate) fn import_folder_depth_setting(store: &Store) -> u32 {
    store.settings.ui_values.get("importFolderDepth").copied().unwrap_or(3.0).clamp(1.0, 5.0) as u32
}

pub(crate) fn import_folder_max_setting(store: &Store) -> usize {
    store.settings.ui_values.get("importFolderMax").copied().unwrap_or(500.0).max(1.0) as usize
}

pub(crate) fn has_import_folder_ext(path: &Path) -> bool {
    path.extension()
        .map(|e| IMPORT_FOLDER_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

pub(crate) fn is_import_folder_file(path: &Path) -> bool {
    path.is_file() && has_import_folder_ext(path)
}

// One file the scan found, already saying whether it can be imported and why not; the same
// read measures its size, so preview and confirm cannot disagree.
#[derive(Serialize, Clone)]
pub(crate) struct FolderImportEntry {
    pub(crate) rel_path: String,
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) size: u64,
    pub(crate) readable: bool,
    pub(crate) skip_reason: String,
}

#[derive(Serialize)]
pub(crate) struct FolderImportPreview {
    pub(crate) dir: String,
    pub(crate) items: Vec<FolderImportEntry>,
    pub(crate) truncated: bool,
}

// Depth-first and alphabetical so a folder always previews in the same order. `depth`
// counts `dir` itself as 0; recursion into a child happens only while the child's own depth
// fits inside `max_depth`, so a folder past the limit contributes no file at all.
pub(crate) fn walk_import_folder(dir: &Path, depth: u32, max_depth: u32, max_files: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= max_files {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= max_files {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if depth + 1 >= max_depth {
                continue; // too deep -- left out whole, not just truncated
            }
            walk_import_folder(&path, depth + 1, max_depth, max_files, out);
        } else if is_import_folder_file(&path) {
            out.push(path);
        }
    }
}

// One tag per subfolder level between `base` and the file, in order --
// `base` itself never becomes a tag.
pub(crate) fn import_folder_tags(base: &Path, path: &Path) -> Vec<String> {
    path.parent()
        .and_then(|p| p.strip_prefix(base).ok())
        .map(|rel| rel.components().filter_map(|c| c.as_os_str().to_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

pub(crate) fn import_folder_rel_path(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

// The first line becomes the standalone note when it is short and something follows; the
// rest is the prompt text. A long first line, or a file with nothing after it, stays the
// whole text with no note.
pub(crate) fn split_folder_note(content: &str) -> (String, String) {
    let content = content.replace("\r\n", "\n");
    let mut lines = content.splitn(2, '\n');
    let first = lines.next().unwrap_or("");
    let rest = lines.next();
    match rest {
        Some(rest) if !rest.trim().is_empty() && first.chars().count() <= IMPORT_FOLDER_NOTE_MAX_CHARS => {
            (rest.to_string(), first.trim().to_string())
        }
        _ => (content, String::new()),
    }
}

// Checks readability the way the confirm step will read the file (size cap, then UTF-8), so
// a file shown as importable can never turn out unreadable at confirm time on an unchanged
// disk.
pub(crate) fn read_import_folder_file(path: &Path) -> Result<String, &'static str> {
    let meta = fs::metadata(path).map_err(|_| "read-error")?;
    if meta.len() > MAX_IMPORT_FOLDER_FILE_BYTES {
        return Err("too-large");
    }
    let bytes = fs::read(path).map_err(|_| "read-error")?;
    String::from_utf8(bytes).map_err(|_| "bad-encoding")
}

pub(crate) fn scan_import_folder(base: &Path, max_depth: u32, max_files: usize) -> (Vec<FolderImportEntry>, bool) {
    let mut paths = Vec::new();
    // One extra slot so the scan can tell "exactly max_files" and "more than
    // max_files" apart; the extra entry is dropped again right below.
    walk_import_folder(base, 0, max_depth, max_files.saturating_add(1), &mut paths);
    let truncated = paths.len() > max_files;
    paths.truncate(max_files);
    let items = paths
        .into_iter()
        .map(|path| {
            let rel_path = import_folder_rel_path(base, &path);
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| rel_path.clone());
            let tags = import_folder_tags(base, &path);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let (readable, skip_reason) = match read_import_folder_file(&path) {
                Ok(_) => (true, String::new()),
                Err(reason) => (false, reason.to_string()),
            };
            FolderImportEntry { rel_path, name, tags, size, readable, skip_reason }
        })
        .collect();
    (items, truncated)
}

// Opens the folder picker itself (like preview_import's file picker) and scans it
// read-only with the current importFolderDepth/importFolderMax. Never touches the store;
// what gets imported is decided by which checkboxes import_folder_files receives.
#[tauri::command]
pub(crate) async fn preview_import_folder(app: AppHandle, state: State<'_, Db>, title: String) -> Result<FolderImportPreview, String> {
    let Some(dir) = file_dialog(&app).set_title(&title).pick_folder() else {
        return Err("canceled".to_string());
    };
    let (max_depth, max_files) = {
        let store = lock(&state);
        (import_folder_depth_setting(&store), import_folder_max_setting(&store))
    };
    // The pick is the user's consent to read this folder: the confirm step below
    // spends it, so it cannot be pointed at a folder nobody picked.
    grant_picked_dir(&dir, "importFolder");
    let (items, truncated) = scan_import_folder(&dir, max_depth, max_files);
    Ok(FolderImportPreview { dir: dir.to_string_lossy().to_string(), items, truncated })
}

// Confirm step: re-reads exactly the checked files, by the preview's relPath resolved
// against `dir`, and imports each as its own prompt. A file gone or unreadable since the
// preview is skipped silently instead of failing the batch; with no single hash for a
// folder of files, "reviewed, then acted on" is enforced per file.
#[tauri::command]
pub(crate) async fn import_folder_files(app: AppHandle, state: State<'_, Db>, dir: String, rel_paths: Vec<String>) -> Result<usize, String> {
    let base = Path::new(&dir);
    if !consume_granted_dir(base, "importFolder") {
        return Err("dir-not-picked".to_string());
    }
    let mut items = Vec::new();
    for rel in rel_paths {
        if !folder_rel_path_is_safe(&rel) {
            continue;
        }
        let path = base.join(&rel);
        // Only what the scan itself lists: a renderer-made path to some other kind of
        // file never becomes a prompt.
        if !has_import_folder_ext(&path) {
            continue;
        }
        let Ok(content) = read_import_folder_file(&path) else { continue };
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let tags = import_folder_tags(base, &path);
        items.push(folder_file_item(name, tags, &content));
    }
    let mut store = lock(&state);
    let before_len = store.prompts.len();
    let count = append_imported_prompts(&mut store, items);
    if !save_prompts(&app, &store) {
        store.prompts.truncate(before_len);
        return Err("save-failed".into());
    }
    let settings = store.settings.clone();
    drop(store);
    push_theme(&app, &settings);
    rebuild_tray(&app);
    Ok(count)
}

// A path from the frontend is only ever joined onto the chosen folder when it stays
// inside it: no absolute path, drive prefix, root or `..` step.
pub(crate) fn folder_rel_path_is_safe(rel: &str) -> bool {
    let path = Path::new(rel);
    path.components().next().is_some() && path.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

// One note file of an imported folder as a prompt; placeholders in the old spelling are
// upgraded like on every other inbound path.
pub(crate) fn folder_file_item(name: String, tags: Vec<String>, content: &str) -> ImportedPrompt {
    // The rotation split runs FIRST, on the whole file: the note is only
    // ever pulled off entry 0 (the plain text), never off a later variant,
    // so a short line right after the separator is never mistaken for it.
    let (raw_text, variants) = split_variants(content);
    let (text, note) = split_folder_note(&raw_text);
    ImportedPrompt {
        name,
        text: upgrade_placeholders(&text),
        color: String::new(),
        font: String::new(),
        font_size: 0,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        show_image: false,
        copy_image: false,
        image: String::new(),
        tags,
        notes: note,
        variants: variants.iter().map(|v| upgrade_placeholders(v)).collect(),
        rotate_mode: String::new(),
        favorite: false,
        secret: false,
    }
}

// Full backup: prompts + settings as one JSON file ("export/import everything").
#[derive(Serialize, Deserialize)]
pub(crate) struct Backup {
    pub(crate) prompts: Vec<Prompt>,
    pub(crate) settings: Settings,
}
