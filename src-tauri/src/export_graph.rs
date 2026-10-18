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
const EXPORT_MEDIA_EXTS: [&str; 40] = [
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "apng", "avif", "mp4", "webm", "avi", "mov", "mkv", "m4v", "wmv", "flv",
    "mpg", "mpeg", "ts", "m2ts", "3gp", "ogv", "mp3", "wav", "m4a", "aac", "ogg", "oga", "opus", "flac", "wma", "mka", "aif", "aiff",
    "mp2", "ac3", "ico", "jfif",
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

// The filter's option string split the way ffmpeg splits it: `key=value` pairs and
// shorthand values, separated by an unescaped colon (a key ends at the colon too).
fn parse_filter_args(args: &str) -> Vec<(Option<String>, String)> {
    let c: Vec<char> = args.chars().collect();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < c.len() {
        let start = i;
        let key = av_token(&c, &mut i, &['=', ':']);
        if !key.is_empty() && i < c.len() && c[i] == '=' {
            i += 1;
            let val = av_token(&c, &mut i, &[':']);
            out.push((Some(key), val));
        } else {
            i = start;
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

// Checks one graph against the allowed file set (canonical paths).
pub(crate) fn validate_export_graph(graph: &str, allowed: &[PathBuf]) -> Result<(), String> {
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
            let ok = canonical(&path).is_some_and(|p| allowed.contains(&p));
            if !ok {
                return refuse("media file not named by the request");
            }
        } else if let Some(args) = &f.args {
            // drawtext can read its text from a file.
            if parse_filter_args(args).iter().any(|(k, _)| k.as_deref().is_some_and(|k| k.eq_ignore_ascii_case("textfile"))) {
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
        validate_export_graph(g, &allowed)?;
    }
    Ok(())
}
