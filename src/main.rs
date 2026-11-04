use serde_json::Value;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const B: &str = "\x1b[1m";
const D: &str = "\x1b[2m";
const C: &str = "\x1b[36m";
const Y: &str = "\x1b[33m";
const G: &str = "\x1b[32m";
const M: &str = "\x1b[35m";
const R: &str = "\x1b[0m";
const HEADER: &str = "enter resume · ctrl-f fuzzy ⇄ semantic";
const USAGE: &str = "clodo: browse recent Claude Code sessions, see what each was about, resume one

usage: clodo [-n N] [QUERY...]
  -n N    how many recent sessions to list (0 = all, default: as many as fit the terminal)
  QUERY   start with a semantic search for QUERY

in the picker:
  type     semantic search over ALL sessions, runs when you pause (claude -p haiku, ~4s)
  ctrl-f   switch to fuzzy filtering of the recent list, and back
  enter    cd to the session's path and run claude --resume <id>";

#[derive(Default)]
struct Session {
    id: String,
    file: PathBuf,
    mtime: u64,
    size: u64,
    cwds: Vec<(String, String)>,
    branch: String,
    title: String,
    name: String,
    away: String,
    prompts: Vec<String>,
    prs: Vec<String>,
    start: String,
    log: Vec<(&'static str, String)>,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn field(line: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":\"");
    let start = line.find(&pat)? + pat.len();
    let bytes = line.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i] != b'"' {
        i += if bytes[i] == b'\\' { 2 } else { 1 };
    }
    serde_json::from_str(&format!("\"{}\"", &line[start..i.min(bytes.len())])).ok()
}

fn json_str(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

fn texts(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn prompt_text(o: &Value) -> Option<String> {
    if o["isMeta"] == true || o["isCompactSummary"] == true {
        return None;
    }
    let t = match &o["message"]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(a) if !a.iter().any(|b| b["type"] == "tool_result") => texts(a),
        _ => return None,
    };
    let t = t.trim();
    (!t.is_empty() && !t.starts_with('<') && !t.starts_with("[Request interrupted"))
        .then(|| t.to_string())
}

fn scan(path: &Path, full: bool) -> io::Result<Session> {
    let meta = fs::metadata(path)?;
    let mut s = Session {
        id: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        file: path.to_path_buf(),
        mtime: meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        size: meta.len(),
        ..Default::default()
    };
    for raw in BufReader::new(File::open(path)?).split(b'\n') {
        let raw = raw?;
        let line = String::from_utf8_lossy(&raw);
        if line.starts_with("{\"type\":\"api-request") {
            continue;
        }
        if let Some(cwd) = field(&line, "cwd") {
            if let Some(b) = field(&line, "gitBranch") {
                s.branch = b;
            }
            match s.cwds.iter_mut().find(|(c, _)| *c == cwd) {
                Some(e) => e.1 = s.branch.clone(),
                None => s.cwds.push((cwd, s.branch.clone())),
            }
            if s.start.is_empty() {
                s.start = field(&line, "timestamp").unwrap_or_default();
            }
        }
        let parse = || serde_json::from_str::<Value>(&line).unwrap_or_default();
        if line.starts_with("{\"type\":\"ai-title\"") {
            s.title = json_str(&parse(), "aiTitle");
        } else if line.starts_with("{\"type\":\"agent-name\"") {
            s.name = json_str(&parse(), "agentName");
        } else if line.starts_with("{\"type\":\"last-prompt\"") {
            let p = json_str(&parse(), "lastPrompt").trim().to_string();
            if !p.is_empty() {
                s.prompts.retain(|x| *x != p);
                s.prompts.push(p);
            }
        } else if line.starts_with("{\"type\":\"pr-link\"") {
            let u = json_str(&parse(), "prUrl");
            if !u.is_empty() && !s.prs.contains(&u) {
                s.prs.push(u);
            }
        } else if line.contains("\"subtype\":\"away_summary\"") {
            let v = parse();
            if v["subtype"] == "away_summary" {
                s.away = json_str(&v, "content");
            }
        } else if full
            && (line.contains("\"type\":\"user\"") || line.contains("\"type\":\"assistant\""))
        {
            let o = parse();
            if o["isSidechain"] == true {
                continue;
            }
            if o["type"] == "user" {
                if let Some(t) = prompt_text(&o) {
                    if !s.log.last().is_some_and(|l| l.0 == "you" && l.1 == t) {
                        s.log.push(("you", t));
                    }
                }
            } else if o["type"] == "assistant" {
                let t = texts(
                    o["message"]["content"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                );
                if !t.trim().is_empty() {
                    s.log.push(("claude", t.trim().to_string()));
                }
            }
        }
    }
    Ok(s)
}

fn one_line(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        s
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

fn short(p: &str, n: usize) -> String {
    let p = match p.strip_prefix(&home()) {
        Some(rest) => format!("~{rest}"),
        None => p.to_string(),
    };
    let len = p.chars().count();
    if len <= n {
        p
    } else {
        format!("…{}", p.chars().skip(len + 1 - n).collect::<String>())
    }
}

fn ago(t: u64) -> String {
    let d = now().saturating_sub(t);
    for (n, u) in [(604_800, "w"), (86_400, "d"), (3_600, "h"), (60, "m")] {
        if d >= n {
            return format!("{}{u} ago", d / n);
        }
    }
    "now".into()
}

fn utc(ts: &str) -> String {
    ts.get(..16)
        .map(|t| t.replace('T', " ") + " UTC")
        .unwrap_or_else(|| ts.to_string())
}

fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn title_of(s: &Session) -> String {
    [&s.name, &s.title]
        .into_iter()
        .find(|t| !t.is_empty())
        .cloned()
        .or_else(|| s.prompts.first().map(|p| one_line(p, 60)))
        .unwrap_or_else(|| "(untitled)".into())
}

fn cwd_of(s: &Session) -> String {
    s.cwds.first().map(|c| c.0.clone()).unwrap_or_else(home)
}

fn files() -> Vec<PathBuf> {
    let mut v: Vec<(SystemTime, PathBuf)> =
        fs::read_dir(Path::new(&home()).join(".claude/projects"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|d| fs::read_dir(d.path()).ok())
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
            .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.0));
    v.into_iter().map(|(_, p)| p).collect()
}

fn sessions(n: usize) -> impl Iterator<Item = Session> {
    files()
        .into_iter()
        .filter_map(|p| scan(&p, false).ok())
        .filter(|s| !s.prompts.is_empty() || !s.title.is_empty())
        .take(if n == 0 { usize::MAX } else { n })
}

fn row(s: &Session) -> String {
    let cwd = cwd_of(s);
    let what = if s.away.is_empty() {
        s.prompts
            .last()
            .map(|p| format!("› {p}"))
            .unwrap_or_default()
    } else {
        s.away.clone()
    };
    format!(
        "{}\t{cwd}\t{:>7}  {C}{:<34}{R}  {Y}{:<24}{R}  {B}{}{R}  {D}{}{R}",
        s.file.display(),
        ago(s.mtime),
        short(&cwd, 34),
        one_line(&s.branch, 24),
        one_line(&title_of(s), 48),
        one_line(&what, 400)
    )
}

fn ranked(answer: &str, n: usize) -> Vec<usize> {
    let mut seen = Vec::new();
    for i in answer
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|t| t.parse::<usize>().ok())
    {
        if i < n && !seen.contains(&i) {
            seen.push(i);
        }
    }
    seen
}

fn semantic(query: &str) -> Vec<Session> {
    let all: Vec<Session> = sessions(0).collect();
    let corpus = all
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut parts = vec![
                title_of(s),
                short(&cwd_of(s), 80),
                s.branch.clone(),
                s.away.clone(),
            ];
            parts.extend(s.prompts.iter().rev().take(12).map(|p| one_line(p, 160)));
            format!("[{i}] {}", one_line(&parts.join(" | "), 1500))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let ask = format!(
        "Past coding sessions, one per line: [index] title | path | git branch | recap | user prompts (latest first).\n\n{corpus}\n\n\
         Query: {query}\n\nThe user half-remembers a session and may use none of its words. \
         Rank sessions by how related they are to the query: same topic, synonyms, paraphrases, related concepts, \
         the tool or system involved. Reply with the 15 most related indices, best first, comma-separated, nothing else."
    );
    let child = Command::new("claude")
        .args([
            "-p",
            "--model",
            "haiku",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--tools",
            "",
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return Vec::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(ask.as_bytes());
    }
    let answer = child
        .wait_with_output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let order = ranked(&answer, all.len());
    let mut all: Vec<Option<Session>> = all.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| all[i].take()).collect()
}

fn preview(file: &str) -> io::Result<String> {
    let s = scan(Path::new(file), true)?;
    let mut out = vec![
        format!("{B}{}{R}", title_of(&s)),
        format!(
            "{D}{} · started {} · active {} · {:.1} MB{R}",
            s.id,
            utc(&s.start),
            ago(s.mtime),
            s.size as f64 / 1e6
        ),
        format!("{G}cd {} && claude --resume {}{R}", sh(&cwd_of(&s)), s.id),
        String::new(),
        format!("{C}Paths{R}"),
    ];
    out.extend(
        s.cwds
            .iter()
            .map(|(p, b)| format!("  {}  {Y}{b}{R}", short(p, 999))),
    );
    if !s.prs.is_empty() {
        out.push(format!("\n{C}PRs{R}"));
        out.extend(s.prs.iter().map(|u| format!("  {u}")));
    }
    if !s.away.is_empty() {
        out.push(format!("\n{C}What's happening{R}\n{}", s.away));
    }
    let mut you: Vec<&String> = s
        .log
        .iter()
        .filter(|l| l.0 == "you")
        .map(|l| &l.1)
        .collect();
    if you.is_empty() {
        you = s.prompts.iter().collect();
    }
    if !you.is_empty() {
        out.push(format!("\n{C}Recent prompts{R}"));
        out.extend(
            you[you.len().saturating_sub(5)..]
                .iter()
                .map(|t| format!("{M}›{R} {}", one_line(t, 400))),
        );
    }
    if let Some(l) = s.log.iter().rev().find(|l| l.0 == "claude") {
        let t: String = l.1.chars().take(1500).collect();
        let more = if t.len() < l.1.len() { "…" } else { "" };
        out.push(format!("\n{C}Last reply{R}\n{t}{more}"));
    }
    Ok(out.join("\n"))
}

fn emit(rows: impl Iterator<Item = String>, out: &mut impl Write) {
    for r in rows {
        if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
            return;
        }
    }
}

fn me() -> String {
    sh(&std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "clodo".into()))
}

fn count(n: &str) -> usize {
    if n != "fit" {
        return n.parse().unwrap_or(30);
    }
    let get = |k: &str| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
    };
    let (lines, top) = (get("FZF_LINES"), get("FZF_PREVIEW_TOP"));
    let rows = if top > 1 {
        top.saturating_sub(4)
    } else {
        lines.saturating_sub(3)
    };
    if rows == 0 {
        30
    } else {
        rows
    }
}

fn fuzzy() -> bool {
    std::env::var("FZF_PROMPT")
        .unwrap_or_default()
        .starts_with("fuzzy")
}

fn toggle(n: &str) -> String {
    let me = me();
    if fuzzy() {
        format!("disable-search+rebind(change)+change-prompt(semantic> )+reload-sync({me} _semantic {n} {{q}})")
    } else {
        format!("enable-search+unbind(change)+change-prompt(fuzzy> )+reload({me} _list {n})")
    }
}

fn resized(n: &str) -> String {
    if fuzzy()
        || std::env::var("FZF_QUERY")
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        format!("reload({} _list {n})", me())
    } else {
        String::new()
    }
}

fn pick(query: &str, n: &str) -> io::Result<String> {
    let me = me();
    let out = Command::new("fzf")
        .args([
            "--ansi",
            "--delimiter",
            "\t",
            "--with-nth",
            "3..",
            "--scheme",
            "history",
            "--layout",
            "reverse",
        ])
        .args([
            "--disabled",
            "--prompt",
            "semantic> ",
            "--query",
            query,
            "--header",
            HEADER,
        ])
        .args(["--ghost", "describe what you remember, then pause"])
        .args([
            "--preview",
            &format!("{me} _preview {{1}}"),
            "--preview-window",
            "right,50%,wrap,<70(down,55%,wrap)",
        ])
        .args([
            "--bind",
            &format!("load:unbind(load)+reload-sync({me} _semantic {n} {{q}})"),
        ])
        .args([
            "--bind",
            &format!("change:reload-sync(sleep 0.8; {me} _semantic {n} {{q}})"),
        ])
        .args(["--bind", &format!("resize:transform({me} _resized {n})")])
        .args(["--bind", &format!("ctrl-f:transform({me} _toggle {n})")])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = &mut io::stdout();
    match args.first().map(String::as_str) {
        Some("_preview") => return writeln!(stdout, "{}", preview(&args[1])?),
        Some("_list") => {
            emit(sessions(count(&args[1])).map(|s| row(&s)), stdout);
            return Ok(());
        }
        Some("_toggle") => return writeln!(stdout, "{}", toggle(&args[1])),
        Some("_resized") => return writeln!(stdout, "{}", resized(&args[1])),
        Some("_semantic") => {
            let q = args[2..].join(" ");
            if q.trim().is_empty() {
                emit(sessions(count(&args[1])).map(|s| row(&s)), stdout);
            } else {
                emit(semantic(&q).iter().map(row), stdout);
            }
            return Ok(());
        }
        _ => {}
    }

    let (mut n, mut query) = ("fit".to_string(), Vec::new());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "-n" => {
                n = it
                    .next()
                    .filter(|v| v.parse::<usize>().is_ok())
                    .unwrap_or(n)
            }
            _ => query.push(a),
        }
    }
    let out = pick(&query.join(" "), &n)?;
    let mut parts = out.trim_end().splitn(3, '\t');
    let (Some(file), Some(cwd)) = (parts.next(), parts.next()) else {
        return Ok(());
    };
    if file.is_empty() {
        return Ok(());
    }
    let sid = Path::new(file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    eprintln!("cd {} && claude --resume {sid}", sh(cwd));
    Err(Command::new("claude")
        .args(["--resume", &sid])
        .current_dir(cwd)
        .exec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_extracts_what_the_picker_shows() {
        let dir = std::env::temp_dir().join(format!("clodo-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc.jsonl");
        let lines = [
            r#"{"type":"mode","mode":"normal"}"#,
            r#"{"parentUuid":null,"type":"user","message":{"role":"user","content":"fix the \"tables\""},"timestamp":"2026-11-03T10:00:00.000Z","cwd":"/w/repo","gitBranch":"main"}"#,
            r#"{"type":"last-prompt","lastPrompt":"fix the tables"}"#,
            r#"{"type":"ai-title","aiTitle":"Table fixes"}"#,
            r#"{"type":"api-request","cwd":"/ignored"}"#,
            r#"{"parentUuid":"x","type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]},"timestamp":"2026-11-03T10:00:02.000Z","cwd":"/w/repo/sub","gitBranch":"feat"}"#,
            r#"{"parentUuid":"y","type":"assistant","message":{"content":[{"type":"thinking"},{"type":"text","text":"Done."}]},"timestamp":"2026-11-03T10:00:03.000Z","cwd":"/w/repo","gitBranch":"feat"}"#,
            r#"{"parentUuid":"z","type":"system","subtype":"away_summary","content":"Fixing tables. Next: ship.","cwd":"/w/repo"}"#,
            r#"{"type":"last-prompt","lastPrompt":"ship it"}"#,
            r#"{"type":"last-prompt","lastPrompt":"fix the tables"}"#,
        ];
        fs::write(&f, lines.join("\n")).unwrap();
        let s = scan(&f, true).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(s.id, "abc");
        assert_eq!(
            s.cwds,
            vec![
                ("/w/repo".into(), "feat".into()),
                ("/w/repo/sub".into(), "feat".into())
            ]
        );
        assert_eq!(s.branch, "feat");
        assert_eq!(s.start, "2026-11-03T10:00:00.000Z");
        assert_eq!(title_of(&s), "Table fixes");
        assert_eq!(s.away, "Fixing tables. Next: ship.");
        assert_eq!(s.prompts, vec!["ship it", "fix the tables"]);
        let roles: Vec<_> = s.log.iter().map(|l| (l.0, l.1.as_str())).collect();
        assert_eq!(
            roles,
            vec![("you", "fix the \"tables\""), ("claude", "Done.")]
        );
        assert_eq!(ranked("3, 1, 3, 99, NONE 0", 5), vec![3, 1, 0]);
        assert_eq!(short("/a/very/long/path/name", 10), "…path/name");
    }
}
