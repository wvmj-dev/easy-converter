//! Reading and writing SRT, WebVTT and ASS/SSA subtitles.
//!
//! - Shifting keeps everything but the times: cue numbers, WebVTT settings and comments,
//!   and every ASS line (styles, positions, effects).
//! - Converting goes through plain cues: start, end and text with only bold, italic and
//!   underline kept (`<b>`, `<i>`, `<u>`), the styling every format understands.

use std::fmt::Write as _;

use crate::error::{AppError, AppResult};

/// Most cues accepted in one file.
pub const MAX_CUES: usize = 500_000;
/// Times are kept below 100 hours, the most the SRT and ASS time formats can show.
const MAX_TIME_MS: i64 = 100 * 3_600_000 - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Srt,
    Vtt,
    /// ASS and its older form SSA.
    Ass,
}

/// A cue as written in an SRT or WebVTT file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    /// WebVTT cue identifier (SRT numbers are rewritten).
    id: Option<String>,
    start: i64,
    end: i64,
    /// WebVTT cue settings after the end time (`align:start`...).
    settings: String,
    text: Vec<String>,
}

/// A cue in neutral form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start: i64,
    pub end: i64,
    /// Text with `\n` between lines and only `<b>`, `<i>` and `<u>` tags.
    pub text: String,
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidFile(message.to_owned())
}

// ---------------------------------------------------------------------------------------
// Times
// ---------------------------------------------------------------------------------------

/// `[h:]mm:ss[.,]fff` (SRT, WebVTT) or `h:mm:ss.cc` (ASS), in milliseconds.
fn parse_time(text: &str) -> Option<i64> {
    let text = text.trim();
    let parts: Vec<&str> = text.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => ("0", *m, *s),
        _ => return None,
    };
    let (whole, fraction) = seconds
        .split_once(['.', ','])
        .map_or((seconds, ""), |(w, f)| (w, f));
    let digits = |s: &str| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit());
    if !digits(hours) || !digits(minutes) || !digits(whole) {
        return None;
    }
    if !fraction.chars().all(|c| c.is_ascii_digit()) || fraction.len() > 3 {
        return None;
    }
    let (h, m, s): (i64, i64, i64) = (
        hours.parse().ok()?,
        minutes.parse().ok()?,
        whole.parse().ok()?,
    );
    if m >= 60 || s >= 60 {
        return None;
    }
    let ms = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()? * 100,
        2 => fraction.parse::<i64>().ok()? * 10,
        _ => fraction.parse::<i64>().ok()?,
    };
    Some(((h * 60 + m) * 60 + s) * 1000 + ms)
}

fn split_ms(ms: i64) -> (i64, i64, i64, i64) {
    let ms = ms.clamp(0, MAX_TIME_MS);
    (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

fn srt_time(ms: i64) -> String {
    let (h, m, s, f) = split_ms(ms);
    format!("{h:02}:{m:02}:{s:02},{f:03}")
}

fn vtt_time(ms: i64) -> String {
    let (h, m, s, f) = split_ms(ms);
    format!("{h:02}:{m:02}:{s:02}.{f:03}")
}

fn ass_time(ms: i64) -> String {
    let (h, m, s, f) = split_ms(ms);
    format!("{h}:{m:02}:{s:02}.{:02}", f / 10)
}

/// `start --> end [settings]`.
fn parse_timing(line: &str) -> Option<(i64, i64, String)> {
    let (start, rest) = line.split_once("-->")?;
    let rest = rest.trim_start();
    let (end, settings) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(e, s)| (e, s.trim()));
    Some((parse_time(start)?, parse_time(end)?, settings.to_owned()))
}

/// Moves a cue by `offset`. `None` when it ends before the start of the video.
fn shifted(start: i64, end: i64, offset: i64) -> Option<(i64, i64)> {
    let end = (end + offset).min(MAX_TIME_MS);
    if end <= 0 {
        return None;
    }
    Some(((start + offset).clamp(0, end), end))
}

// ---------------------------------------------------------------------------------------
// SRT and WebVTT
// ---------------------------------------------------------------------------------------

fn lines(text: &str) -> Vec<&str> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .collect()
}

/// Groups lines separated by blank lines.
fn paragraphs<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            if !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
        } else {
            current.push(*line);
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// The cues of an SRT or WebVTT file, and for WebVTT the blocks before the first cue
/// (header, styles, comments), kept as they are.
fn parse_blocks(kind: Kind, text: &str) -> AppResult<(Vec<String>, Vec<Block>)> {
    let lines = lines(text);
    let mut groups = paragraphs(&lines).into_iter();
    let mut preamble = Vec::new();
    if kind == Kind::Vtt {
        let header = groups.next().unwrap_or_default();
        if !header.first().is_some_and(|l| l.starts_with("WEBVTT")) {
            return Err(invalid("not a WebVTT file (no WEBVTT header)"));
        }
        preamble.push(header.join("\n"));
    }
    let mut blocks = Vec::new();
    for group in groups {
        let Some(timing_at) = group.iter().position(|l| l.contains("-->")) else {
            // WebVTT comments, styles and regions; stray text in SRT.
            if kind == Kind::Vtt && blocks.is_empty() {
                preamble.push(group.join("\n"));
            }
            continue;
        };
        let Some((start, end, settings)) = parse_timing(group[timing_at]) else {
            continue;
        };
        let id = (kind == Kind::Vtt && timing_at > 0).then(|| group[..timing_at].join(" "));
        blocks.push(Block {
            id,
            start,
            end,
            settings: if kind == Kind::Vtt {
                settings
            } else {
                String::new()
            },
            text: group[timing_at + 1..]
                .iter()
                .map(|l| (*l).to_owned())
                .collect(),
        });
        if blocks.len() > MAX_CUES {
            return Err(AppError::TooLarge(format!(
                "more than {MAX_CUES} subtitles"
            )));
        }
    }
    if blocks.is_empty() {
        return Err(invalid("no subtitles found"));
    }
    Ok((preamble, blocks))
}

fn write_blocks(kind: Kind, preamble: &[String], blocks: &[Block]) -> String {
    let mut out = String::new();
    for part in preamble {
        out.push_str(part);
        out.push_str("\n\n");
    }
    for (index, block) in blocks.iter().enumerate() {
        match kind {
            Kind::Srt => {
                let _ = writeln!(out, "{}", index + 1);
                let _ = writeln!(out, "{} --> {}", srt_time(block.start), srt_time(block.end));
            }
            _ => {
                if let Some(id) = &block.id {
                    let _ = writeln!(out, "{id}");
                }
                let _ = write!(out, "{} --> {}", vtt_time(block.start), vtt_time(block.end));
                if !block.settings.is_empty() {
                    let _ = write!(out, " {}", block.settings);
                }
                out.push('\n');
            }
        }
        for line in &block.text {
            let _ = writeln!(out, "{line}");
        }
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------------------
// ASS
// ---------------------------------------------------------------------------------------

/// Field positions of an ASS `[Events]` section, from its `Format:` line.
#[derive(Debug, Clone, Copy)]
struct EventFormat {
    count: usize,
    start: usize,
    end: usize,
    text: usize,
}

impl EventFormat {
    fn parse(line: &str) -> Option<Self> {
        let fields: Vec<String> = line
            .split_once(':')?
            .1
            .split(',')
            .map(|f| f.trim().to_ascii_lowercase())
            .collect();
        let find = |name: &str| fields.iter().position(|f| f == name);
        let format = Self {
            count: fields.len(),
            start: find("start")?,
            end: find("end")?,
            text: find("text")?,
        };
        // Text may contain commas, so it must be the last field.
        (format.text == format.count - 1).then_some(format)
    }

    /// The fields of an event line (after `Dialogue:`), the last one holding the rest.
    fn split<'a>(&self, values: &'a str) -> Option<Vec<&'a str>> {
        let fields: Vec<&str> = values.splitn(self.count, ',').collect();
        (fields.len() == self.count).then_some(fields)
    }
}

/// A line of an ASS file: an event with its fields, or anything else.
enum AssLine<'a> {
    Dialogue(Vec<&'a str>, EventFormat),
    Other(&'a str),
}

/// Calls `on_line` for every line, with `Dialogue:` lines split into fields.
fn walk_ass<'a>(
    text: &'a str,
    mut on_line: impl FnMut(AssLine<'a>) -> AppResult<()>,
) -> AppResult<usize> {
    let mut in_events = false;
    let mut format: Option<EventFormat> = None;
    let mut events = 0usize;
    for line in lines(text) {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_events = trimmed.eq_ignore_ascii_case("[events]");
            on_line(AssLine::Other(line))?;
            continue;
        }
        if in_events && trimmed.to_ascii_lowercase().starts_with("format:") {
            format = EventFormat::parse(trimmed);
            on_line(AssLine::Other(line))?;
            continue;
        }
        let dialogue = in_events
            .then(|| trimmed.strip_prefix("Dialogue:"))
            .flatten()
            .zip(format);
        match dialogue.and_then(|(values, f)| f.split(values.trim_start()).map(|v| (v, f))) {
            Some((fields, f)) => {
                events += 1;
                if events > MAX_CUES {
                    return Err(AppError::TooLarge(format!(
                        "more than {MAX_CUES} subtitles"
                    )));
                }
                on_line(AssLine::Dialogue(fields, f))?;
            }
            None => on_line(AssLine::Other(line))?,
        }
    }
    if events == 0 {
        return Err(invalid(
            "no subtitles found (no Dialogue lines in [Events])",
        ));
    }
    Ok(events)
}

fn shift_ass(text: &str, offset: i64) -> AppResult<String> {
    let mut out = String::with_capacity(text.len());
    let mut kept = 0usize;
    walk_ass(text, |line| {
        match line {
            AssLine::Dialogue(fields, format) => {
                let (Some(start), Some(end)) = (
                    parse_time(fields[format.start]),
                    parse_time(fields[format.end]),
                ) else {
                    return Err(invalid("an ASS line has an unreadable time"));
                };
                if let Some((start, end)) = shifted(start, end, offset) {
                    let mut fields: Vec<String> = fields.iter().map(|f| (*f).to_owned()).collect();
                    fields[format.start] = ass_time(start);
                    fields[format.end] = ass_time(end);
                    out.push_str("Dialogue: ");
                    out.push_str(&fields.join(","));
                    out.push('\n');
                    kept += 1;
                }
            }
            AssLine::Other(line) => {
                out.push_str(line);
                out.push('\n');
            }
        }
        Ok(())
    })?;
    if kept == 0 {
        return Err(AppError::InvalidParameter(
            "every subtitle would end before the start of the video".into(),
        ));
    }
    Ok(out)
}

fn ass_cues(text: &str) -> AppResult<Vec<Cue>> {
    let mut cues = Vec::new();
    walk_ass(text, |line| {
        if let AssLine::Dialogue(fields, format) = line {
            if let (Some(start), Some(end)) = (
                parse_time(fields[format.start]),
                parse_time(fields[format.end]),
            ) {
                cues.push(Cue {
                    start,
                    end,
                    text: from_ass_text(fields[format.text]),
                });
            }
        }
        Ok(())
    })?;
    // ASS events may be in any order; the other formats expect them by time.
    cues.sort_by_key(|c| (c.start, c.end));
    Ok(cues)
}

const ASS_HEADER: &str = "[Script Info]
; Written by Easy Converter
ScriptType: v4.00+
PlayResX: 384
PlayResY: 288
WrapStyle: 0
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,16,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
";

fn write_ass(cues: &[Cue]) -> String {
    let mut out = String::from(ASS_HEADER);
    for cue in cues {
        let _ = writeln!(
            out,
            "Dialogue: 0,{},{},Default,,0,0,0,,{}",
            ass_time(cue.start),
            ass_time(cue.end),
            to_ass_text(&cue.text)
        );
    }
    out
}

// ---------------------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------------------

/// Keeps `<b>`, `<i>` and `<u>` (normalised) and drops every other tag, keeping its
/// content. A `<` that does not open a tag is kept as text.
fn keep_basic_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let tag_end = after.find('>');
        let is_tag =
            tag_end.is_some() && after.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/');
        match (is_tag, tag_end) {
            (true, Some(end)) => {
                let inner = &after[..end];
                let closing = inner.starts_with('/');
                let name: String = inner
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if matches!(name.as_str(), "b" | "i" | "u") {
                    out.push('<');
                    if closing {
                        out.push('/');
                    }
                    out.push_str(&name);
                    out.push('>');
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes ASS override blocks (`{\an8}`) that some SRT files carry.
fn drop_override_blocks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("{\\") {
        out.push_str(&rest[..at]);
        match rest[at..].find('}') {
            Some(end) => rest = &rest[at + end + 1..],
            None => {
                rest = &rest[at..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{a0}")
        .replace("&lrm;", "\u{200e}")
        .replace("&rlm;", "\u{200f}")
        .replace("&amp;", "&")
}

/// Escapes text for WebVTT, keeping the basic tags.
fn escape_vtt(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while !rest.is_empty() {
        for tag in ["<b>", "</b>", "<i>", "</i>", "<u>", "</u>"] {
            if let Some(after) = rest.strip_prefix(tag) {
                out.push_str(tag);
                rest = after;
                continue 'outer;
            }
        }
        let mut chars = rest.chars();
        let c = chars.next().unwrap_or_default();
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
        rest = chars.as_str();
    }
    out
}

fn from_ass_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('}') else {
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        let block = &rest[at + 1..at + end];
        for tag in block.split('\\').skip(1) {
            let html = match tag.trim() {
                "i1" => "<i>",
                "i0" => "</i>",
                "b1" => "<b>",
                "b0" => "</b>",
                "u1" => "<u>",
                "u0" => "</u>",
                _ => "",
            };
            out.push_str(html);
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out.replace("\\N", "\n")
        .replace("\\n", "\n")
        .replace("\\h", " ")
        .trim()
        .to_owned()
}

fn to_ass_text(text: &str) -> String {
    text.replace(['{', '}'], "")
        .replace("<i>", "{\\i1}")
        .replace("</i>", "{\\i0}")
        .replace("<b>", "{\\b1}")
        .replace("</b>", "{\\b0}")
        .replace("<u>", "{\\u1}")
        .replace("</u>", "{\\u0}")
        .replace('\n', "\\N")
}

// ---------------------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------------------

fn cues(kind: Kind, text: &str) -> AppResult<Vec<Cue>> {
    if kind == Kind::Ass {
        return ass_cues(text);
    }
    let (_, blocks) = parse_blocks(kind, text)?;
    Ok(blocks
        .into_iter()
        .map(|b| {
            let joined = b.text.join("\n");
            let text = match kind {
                Kind::Vtt => decode_entities(&keep_basic_tags(&joined)),
                _ => keep_basic_tags(&drop_override_blocks(&joined)),
            };
            Cue {
                start: b.start,
                end: b.end,
                text,
            }
        })
        .collect())
}

/// Rewrites subtitles of one kind as another.
pub fn convert(from: Kind, to: Kind, text: &str) -> AppResult<String> {
    let cues = cues(from, text)?;
    Ok(match to {
        Kind::Ass => write_ass(&cues),
        Kind::Srt | Kind::Vtt => {
            let blocks: Vec<Block> = cues
                .into_iter()
                .map(|c| Block {
                    id: None,
                    start: c.start,
                    end: c.end,
                    settings: String::new(),
                    text: if to == Kind::Vtt {
                        escape_vtt(&c.text).lines().map(str::to_owned).collect()
                    } else {
                        c.text.lines().map(str::to_owned).collect()
                    },
                })
                .collect();
            let preamble = if to == Kind::Vtt {
                vec!["WEBVTT".to_owned()]
            } else {
                Vec::new()
            };
            write_blocks(to, &preamble, &blocks)
        }
    })
}

/// Moves every subtitle by `offset_ms` (negative: earlier). Subtitles that would end
/// before the start are dropped; ones that would start before it start at zero.
pub fn shift(kind: Kind, text: &str, offset_ms: i64) -> AppResult<String> {
    if kind == Kind::Ass {
        return shift_ass(text, offset_ms);
    }
    let (preamble, blocks) = parse_blocks(kind, text)?;
    let moved: Vec<Block> = blocks
        .into_iter()
        .filter_map(|b| {
            let (start, end) = shifted(b.start, b.end, offset_ms)?;
            Some(Block { start, end, ..b })
        })
        .collect();
    if moved.is_empty() {
        return Err(AppError::InvalidParameter(
            "every subtitle would end before the start of the video".into(),
        ));
    }
    Ok(write_blocks(kind, &preamble, &moved))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "\u{feff}1\r\n00:00:01,500 --> 00:00:03,000\r\n<font color=\"red\">Olá</font> & <i>mundo</i>\r\n{\\an8}segunda linha\r\n\r\n2\r\n00:00:04,000 --> 00:00:05,250\r\nfim < meio\r\n";

    #[test]
    fn parses_times() {
        assert_eq!(parse_time("00:00:01,500"), Some(1500));
        assert_eq!(parse_time("01:02.5"), Some(62_500));
        assert_eq!(parse_time("1:00:00.07"), Some(3_600_070));
        for bad in ["", "1:60:00.000", "a:00:00", "00:00:01,5000", "1"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
        assert_eq!(ass_time(3_600_079), "1:00:00.07");
        assert_eq!(srt_time(-5), "00:00:00,000");
    }

    #[test]
    fn srt_becomes_vtt_and_ass() {
        let vtt = convert(Kind::Srt, Kind::Vtt, SRT).unwrap();
        assert_eq!(
            vtt,
            "WEBVTT\n\n00:00:01.500 --> 00:00:03.000\nOlá &amp; <i>mundo</i>\nsegunda linha\n\n00:00:04.000 --> 00:00:05.250\nfim &lt; meio\n\n"
        );
        let ass = convert(Kind::Srt, Kind::Ass, SRT).unwrap();
        assert!(ass.contains("Dialogue: 0,0:00:01.50,0:00:03.00,Default,,0,0,0,,Olá & {\\i1}mundo{\\i0}\\Nsegunda linha\n"));
        // And back again.
        let srt = convert(Kind::Ass, Kind::Srt, &ass).unwrap();
        assert!(srt.starts_with(
            "1\n00:00:01,500 --> 00:00:03,000\nOlá & <i>mundo</i>\nsegunda linha\n\n2\n"
        ));
    }

    #[test]
    fn vtt_comments_settings_and_entities() {
        let vtt = "WEBVTT - legenda\n\nNOTE gerado à mão\n\nintro\n00:01.000 --> 00:02.000 align:start line:0\n<v Ana>Oi &amp; <b>tchau</b> &lt;3\n\n00:03.000 --> 00:04.000\nok\n";
        let shifted = shift(Kind::Vtt, vtt, 1500).unwrap();
        assert_eq!(
            shifted,
            "WEBVTT - legenda\n\nNOTE gerado à mão\n\nintro\n00:00:02.500 --> 00:00:03.500 align:start line:0\n<v Ana>Oi &amp; <b>tchau</b> &lt;3\n\n00:00:04.500 --> 00:00:05.500\nok\n\n"
        );
        let srt = convert(Kind::Vtt, Kind::Srt, vtt).unwrap();
        assert!(srt.contains("Oi & <b>tchau</b> <3\n"), "{srt}");
    }

    #[test]
    fn shifting_drops_and_clamps_cues() {
        let shifted = shift(Kind::Srt, SRT, -2000).unwrap();
        assert!(
            shifted.starts_with("1\n00:00:00,000 --> 00:00:01,000\n<font color=\"red\">Olá</font>")
        );
        let dropped = shift(Kind::Srt, SRT, -3500).unwrap();
        assert!(dropped.starts_with("1\n00:00:00,500 --> 00:00:01,750\nfim < meio\n"));
        assert!(matches!(
            shift(Kind::Srt, SRT, -6000),
            Err(AppError::InvalidParameter(_))
        ));
    }

    #[test]
    fn ass_shift_keeps_everything_else() {
        let ass = "[Script Info]\nTitle: x\n\n[V4+ Styles]\nStyle: Default,Arial,20\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nComment: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,nota\nDialogue: 1,0:00:02.00,0:00:03.50,Top,Ana,0,0,0,,{\\pos(10,20)}Oi, tudo bem?\nDialogue: 0,0:00:00.10,0:00:00.20,Default,,0,0,0,,cedo\n";
        let shifted = shift(Kind::Ass, ass, -500).unwrap();
        assert!(shifted.contains(
            "Dialogue: 1,0:00:01.50,0:00:03.00,Top,Ana,0,0,0,,{\\pos(10,20)}Oi, tudo bem?\n"
        ));
        assert!(!shifted.contains("cedo"));
        assert!(
            shifted.contains("Style: Default,Arial,20\n")
                && shifted.contains("Comment: 0,0:00:00.00")
        );
        let cues = ass_cues(ass).unwrap();
        assert_eq!(cues[0].text, "cedo");
        assert_eq!(cues[1].text, "Oi, tudo bem?");
    }

    #[test]
    fn refuses_files_without_subtitles() {
        assert!(matches!(
            convert(Kind::Srt, Kind::Vtt, "hello\n"),
            Err(AppError::InvalidFile(_))
        ));
        assert!(matches!(
            convert(Kind::Vtt, Kind::Srt, "00:01.000 --> 00:02.000\nx\n"),
            Err(AppError::InvalidFile(_))
        ));
        assert!(matches!(
            convert(Kind::Ass, Kind::Srt, "[Events]\nDialogue: x\n"),
            Err(AppError::InvalidFile(_))
        ));
    }

    #[test]
    fn tag_handling() {
        assert_eq!(
            keep_basic_tags("<I>a</I> <font x>b</font> 1 < 2 <3"),
            "<i>a</i> b 1 < 2 <3"
        );
        assert_eq!(escape_vtt("<i>a</i> & <c>"), "<i>a</i> &amp; &lt;c&gt;");
        assert_eq!(to_ass_text("{x}<b>a</b>\nb"), "x{\\b1}a{\\b0}\\Nb");
    }
}
