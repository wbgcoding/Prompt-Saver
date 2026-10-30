use super::*;

// The video editor hands the backend its filter graph as text. The graph can name
// files (movie=, amovie=, drawtext textfile=, subtitles=, sendcmd ...), so a page
// that was taken over could make ffmpeg read any local file into an export that the
// page may then play back. The graph is therefore parsed here the way ffmpeg parses
// it and refused unless it only uses the filters the editor itself builds, reads no
// file but the source and the media the request names, and names no text file.

// Every filter the video editor (ui/videoGraph.js) builds, and nothing else.
const EXPORT_GRAPH_FILTERS: [&str; 33] = [
    "fps", "split", "trim", "setpts", "concat", "movie", "amovie", "scale", "format", "colorchannelmixer", "overlay", "drawbox",
    "drawtext", "crop", "fade", "palettegen", "paletteuse", "null", "anullsrc", "atrim", "asetpts", "aresample", "aformat", "apad",
    "asplit", "volume", "asetrate", "atempo", "afade", "adelay", "amix", "anull", "pad",
];

// Extensions a media file named by the request may have. Text-like extensions are
// left out on purpose: ffmpeg decodes several of them (.txt, .nfo, .asc ...) into
// pictures, which would turn a text file into video.
const EXPORT_MEDIA_EXTS: [&str; 46] = [
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "apng", "avif", "mp4", "webm", "avi", "mov", "mkv", "m4v", "wmv", "flv",
    "mpg", "mpeg", "ts", "m2ts", "3gp", "ogv", "mp3", "wav", "m4a", "aac", "ogg", "oga", "opus", "flac", "wma", "mka", "aif", "aiff",
    "mp2", "ac3", "ico", "jfif", "mts", "mxf", "vob", "m2v", "heic", "jxl",
];

const WS: [char; 4] = [' ', '\n', '\t', '\r'];

// ffmpeg's own token reader (av_get_token): leading blanks skipped, a backslash
// escapes the next character, single quotes keep everything inside literally, and
// the token ends at the first unquoted, unescaped terminator.
fn av_token(c: &[char], i: &mut usize, term: &[char]) -> String {
    while *i < c.len() && WS.contains(&c[*i]) {
        *i += 1;
    }
    let mut out: Vec<char> = Vec::new();
    let mut keep = 0usize; // length that may not be trimmed (quoted or escaped text)
    while *i < c.len() && !term.contains(&c[*i]) {
        let ch = c[*i];
        *i += 1;
        if ch == '\\' && *i < c.len() {
            out.push(c[*i]);
            *i += 1;
            keep = out.len();
        } else if ch == '\'' {
            while *i < c.len() && c[*i] != '\'' {
                out.push(c[*i]);
                *i += 1;
            }
            if *i < c.len() {
                *i += 1;
                keep = out.len();
            }
        } else {
            out.push(ch);
        }
    }
    while out.len() > keep && out.last().is_some_and(|ch| WS.contains(ch)) {
        out.pop();
    }
    out.into_iter().collect()
}

// One filter of the graph: its name and its argument string (both unescaped once,
// the way ffmpeg's graph parser hands them to the filter).
struct GraphFilter {
    name: String,
    args: Option<String>,
}

fn skip_ws(c: &[char], i: &mut usize) {
    while *i < c.len() && WS.contains(&c[*i]) {
        *i += 1;
    }
}

fn skip_labels(c: &[char], i: &mut usize) -> Result<(), String> {
    loop {
        skip_ws(c, i);
        if *i < c.len() && c[*i] == '[' {
            *i += 1;
            let _ = av_token(c, i, &[']']);
            if *i >= c.len() || c[*i] != ']' {
                return Err("unterminated link label".to_string());
            }
            *i += 1;
        } else {
            return Ok(());
        }
    }
}

fn parse_graph(graph: &str) -> Result<Vec<GraphFilter>, String> {
    let c: Vec<char> = graph.chars().collect();
    let mut i = 0usize;
    let mut out = Vec::new();
    loop {
        skip_labels(&c, &mut i)?;
        let name = av_token(&c, &mut i, &['=', ',', ';', '[']);
        let args = if i < c.len() && c[i] == '=' {
            i += 1;
            Some(av_token(&c, &mut i, &['[', ']', ',', ';']))
        } else {
            None
        };
        out.push(GraphFilter { name, args });
        skip_labels(&c, &mut i)?;
        skip_ws(&c, &mut i);
        if i >= c.len() {
            return Ok(out);
        }
        if c[i] == ',' || c[i] == ';' {
            i += 1;
        } else {
            return Err("unexpected character in the graph".to_string());
        }
    }
}

// ffmpeg's own key reader: an option key is a run of plain characters, optionally framed by
// blanks, that ends right at the `=`. Anything else in front of the `=` (an escaped drive colon,
// a blank inside a word) makes the whole token a positional value, so the path
// `C\:/clips/a=b.mp4` is a file name and not a key.
fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.')
}

fn take_key(c: &[char], i: &mut usize) -> Option<String> {
    let mut j = *i;
    skip_ws(c, &mut j);
    let start = j;
    while j < c.len() && is_key_char(c[j]) {
        j += 1;
    }
    let end = j;
    skip_ws(c, &mut j);
    if end == start || j >= c.len() || c[j] != '=' {
        return None;
    }
    *i = j + 1;
    Some(c[start..end].iter().collect())
}

// The filter's option string split the way ffmpeg splits it: `key=value` pairs and
// shorthand values, separated by an unescaped colon.
fn parse_filter_args(args: &str) -> Vec<(Option<String>, String)> {
    let c: Vec<char> = args.chars().collect();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < c.len() {
        if let Some(key) = take_key(&c, &mut i) {
            let val = av_token(&c, &mut i, &[':']);
            out.push((Some(key), val));
        } else {
            let val = av_token(&c, &mut i, &[':']);
            out.push((None, val));
        }
        if i < c.len() && c[i] == ':' {
            i += 1;
        }
    }
    out
}

// The file a movie=/amovie= source reads, when its options are only that file and
// an optional loop count; None for any other shape (format_name=, a second
// positional value that would be one, unknown keys).
fn movie_source_path(args: Option<&str>) -> Option<String> {
    let pairs = parse_filter_args(args?);
    let mut path: Option<String> = None;
    for (n, (key, val)) in pairs.into_iter().enumerate() {
        match key.as_deref() {
            None if n == 0 => path = Some(val),
            Some("filename") if path.is_none() => path = Some(val),
            Some("loop") if !val.is_empty() && val.chars().all(|ch| ch.is_ascii_digit()) => {}
            _ => return None,
        }
    }
    path.filter(|p| !p.is_empty())
}

pub(crate) fn has_export_media_ext(path: &Path) -> bool {
    path.extension().is_some_and(|e| EXPORT_MEDIA_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn canonical(path: &str) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

// A path spelled the way a graph spells it, for comparing texts: forward slashes, no case.
fn graph_text_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

// Checks one graph against the allowed file set (canonical paths). `allowed_texts` are the
// request's own path spellings (the source and the media files it names, already vetted by the
// caller): a network-looking source in a graph is canonicalized only when its text is one of
// them, so a path the request does not name never makes this machine contact a remote host.
pub(crate) fn validate_export_graph(graph: &str, allowed: &[PathBuf], allowed_texts: &[String]) -> Result<(), String> {
    let refuse = |why: &str| Err(format!("export graph not allowed: {why}"));
    let filters = match parse_graph(graph) {
        Ok(f) => f,
        Err(e) => return refuse(&e),
    };
    for f in &filters {
        // "name@id" is ffmpeg's way of labelling an instance.
        let base = f.name.split('@').next().unwrap_or("");
        if !EXPORT_GRAPH_FILTERS.contains(&base) {
            return refuse(&format!("filter {base}"));
        }
        if base == "movie" || base == "amovie" {
            let Some(path) = movie_source_path(f.args.as_deref()) else {
                return refuse("media source options");
            };
            if crate::paths::is_network_path(&path) {
                let key = graph_text_key(&path);
                if !allowed_texts.iter().any(|t| graph_text_key(t) == key) {
                    return refuse("media file not named by the request");
                }
            }
            let ok = canonical(&path).is_some_and(|p| allowed.contains(&p));
            if !ok {
                return refuse("media file not named by the request");
            }
        } else if let Some(args) = &f.args {
            let pairs = parse_filter_args(args);
            // ffmpeg loads any option's value from a file when its key starts with `/`.
            if pairs.iter().any(|(k, _)| k.as_deref().is_some_and(|k| k.trim_start().starts_with('/'))) {
                return refuse("file-loaded option");
            }
            // drawtext can read its text from a file, by key or as the third positional value.
            if pairs.iter().any(|(k, _)| k.as_deref().is_some_and(|k| k.eq_ignore_ascii_case("textfile"))) {
                return refuse("text file");
            }
            if base == "drawtext" && pairs.iter().enumerate().any(|(n, (k, _))| k.is_none() && n >= 2) {
                return refuse("text file");
            }
        }
    }
    Ok(())
}

// The whole export request: the source plus the media files it names (each must
// exist and carry a media extension) are the only files a graph may read.
pub(crate) fn validate_export_request_graphs(src: &Path, media_paths: &[String], graphs: &[&str]) -> Result<(), String> {
    let mut allowed: Vec<PathBuf> = Vec::new();
    let mut allowed_texts: Vec<String> = vec![src.to_string_lossy().to_string()];
    allowed_texts.extend(media_paths.iter().cloned());
    if let Ok(p) = fs::canonicalize(src) {
        allowed.push(p);
    }
    for m in media_paths {
        let path = Path::new(m);
        if !has_export_media_ext(path) {
            return Err("export graph not allowed: media file type".to_string());
        }
        if let Some(p) = canonical(m) {
            if p.is_file() {
                allowed.push(p);
            }
        }
    }
    for g in graphs {
        validate_export_graph(g, &allowed, &allowed_texts)?;
    }
    Ok(())
}
