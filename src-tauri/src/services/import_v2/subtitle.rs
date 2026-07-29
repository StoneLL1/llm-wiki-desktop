use std::{borrow::Cow, io::Read};

use flate2::read::GzDecoder;
use serde::Serialize;
use serde_json::Value;

const MAX_RENDERED_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: Option<u64>,
    pub text: String,
}

/// Render the subtitle formats emitted by the supported platform providers.
///
/// A subtitle URL is not evidence that a usable transcript exists. Callers
/// use `None` to continue with the next subtitle candidate or local ASR.
pub fn render_subtitle_markdown(bytes: &[u8], extension: &str) -> Option<String> {
    let segments = parse_subtitle_segments(bytes, extension)?;
    let mut output = String::new();
    let mut last_line = None::<String>;
    for segment in segments {
        append_segment(&mut output, &mut last_line, segment);
        if output.len() >= MAX_RENDERED_BYTES {
            break;
        }
    }
    (!output.trim().is_empty()).then_some(output)
}

pub fn parse_subtitle_segments(bytes: &[u8], extension: &str) -> Option<Vec<TranscriptSegment>> {
    let bytes = decode_transport(bytes)?;
    let segments = match extension.to_ascii_lowercase().as_str() {
        "vtt" | "srt" => parse_timed_text(&bytes)?,
        "ass" | "ssa" => parse_ass(&bytes)?,
        "lrc" => parse_lrc(&bytes)?,
        "json" => parse_json(&bytes)?,
        _ => return None,
    };
    (!segments.is_empty()).then_some(segments)
}

fn decode_transport(bytes: &[u8]) -> Option<Cow<'_, [u8]>> {
    if !bytes.starts_with(&[0x1f, 0x8b]) {
        return Some(Cow::Borrowed(bytes));
    }
    let mut decoder = GzDecoder::new(bytes);
    let mut output = Vec::new();
    decoder
        .by_ref()
        .take((MAX_RENDERED_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .ok()?;
    (output.len() <= MAX_RENDERED_BYTES).then_some(Cow::Owned(output))
}

fn parse_timed_text(bytes: &[u8]) -> Option<Vec<TranscriptSegment>> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut output = Vec::new();
    let mut start_ms = None;
    let mut cue_lines = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            flush_timed_cue(&mut output, &mut start_ms, &mut cue_lines);
            continue;
        }
        if line.contains("-->") {
            flush_timed_cue(&mut output, &mut start_ms, &mut cue_lines);
            start_ms = line.split("-->").next().and_then(parse_clock_ms);
            continue;
        }
        if start_ms.is_none()
            && (line.eq_ignore_ascii_case("WEBVTT")
                || line.chars().all(|value| value.is_ascii_digit())
                || line.starts_with("NOTE")
                || line.starts_with("STYLE")
                || line.starts_with("REGION"))
        {
            continue;
        }
        if start_ms.is_some() {
            cue_lines.push(line.to_string());
        }
        if output.len() >= 100_000 {
            break;
        }
    }
    flush_timed_cue(&mut output, &mut start_ms, &mut cue_lines);
    Some(output)
}

fn flush_timed_cue(
    output: &mut Vec<TranscriptSegment>,
    start_ms: &mut Option<u64>,
    cue_lines: &mut Vec<String>,
) {
    let Some(start_ms) = start_ms.take() else {
        cue_lines.clear();
        return;
    };
    let text = cue_lines.join("\n");
    cue_lines.clear();
    if text.trim().is_empty() {
        return;
    }
    output.push(TranscriptSegment {
        start_ms: Some(start_ms),
        text,
    });
}

fn parse_ass(bytes: &[u8]) -> Option<Vec<TranscriptSegment>> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut output = Vec::new();
    for line in text.lines().map(str::trim) {
        let Some(dialogue) = line.strip_prefix("Dialogue:") else {
            continue;
        };
        let fields = dialogue.trim_start().splitn(10, ',').collect::<Vec<_>>();
        if fields.len() != 10 {
            continue;
        }
        let Some(start_ms) = parse_ass_clock_ms(fields[1]) else {
            continue;
        };
        let text = strip_ass_tags(fields[9])
            .replace("\\N", "\n")
            .replace("\\n", "\n");
        if text.trim().is_empty() {
            continue;
        }
        output.push(TranscriptSegment {
            start_ms: Some(start_ms),
            text,
        });
        if output.len() >= 100_000 {
            break;
        }
    }
    Some(output)
}

fn parse_lrc(bytes: &[u8]) -> Option<Vec<TranscriptSegment>> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut output = Vec::new();
    for line in text.lines().map(str::trim) {
        let mut rest = line;
        let mut timestamps = Vec::new();
        while let Some(value) = rest.strip_prefix('[') {
            let Some((timestamp, suffix)) = value.split_once(']') else {
                break;
            };
            let Some(start_ms) = parse_lrc_clock_ms(timestamp) else {
                break;
            };
            timestamps.push(start_ms);
            rest = suffix;
        }
        let lyric = rest.trim();
        if lyric.is_empty() {
            continue;
        }
        output.extend(timestamps.into_iter().map(|start_ms| TranscriptSegment {
            start_ms: Some(start_ms),
            text: lyric.to_string(),
        }));
        if output.len() >= 100_000 {
            break;
        }
    }
    output.sort_by_key(|segment| segment.start_ms);
    Some(output)
}

fn parse_lrc_clock_ms(value: &str) -> Option<u64> {
    let (minutes, seconds) = value.trim().split_once(':')?;
    let minutes = minutes.parse::<u64>().ok()?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, "0"));
    let seconds = seconds.parse::<u64>().ok()?;
    let fraction_text = fraction.chars().take(3).collect::<String>();
    let fraction = fraction_text.parse::<u64>().unwrap_or(0);
    let millis = match fraction_text.len() {
        1 => fraction * 100,
        2 => fraction * 10,
        _ => fraction,
    };
    Some(
        minutes
            .saturating_mul(60_000)
            .saturating_add(seconds.saturating_mul(1_000))
            .saturating_add(millis),
    )
}

fn parse_json(bytes: &[u8]) -> Option<Vec<TranscriptSegment>> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let mut segments = Vec::new();
    collect_json_segments(&value, &mut segments);
    if !segments.iter().any(|segment| segment.start_ms.is_some()) {
        return None;
    }
    Some(segments)
}

fn collect_json_segments(value: &Value, output: &mut Vec<TranscriptSegment>) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_json_segments(value, output)),
        Value::Object(object) => {
            let text = object
                .get("segs")
                .and_then(Value::as_array)
                .map(|segs| {
                    segs.iter()
                        .filter_map(|segment| segment.get("utf8").and_then(Value::as_str))
                        .collect::<String>()
                })
                .filter(|text| !text.trim().is_empty())
                .or_else(|| {
                    ["content", "text", "caption", "utf8"]
                        .iter()
                        .find_map(|key| object.get(*key).and_then(Value::as_str))
                        .filter(|text| !text.trim().is_empty())
                        .map(str::to_string)
                });
            if let Some(text) = text {
                let start_ms = ["tStartMs", "startMs", "start_ms"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(number_as_u64))
                    .or_else(|| {
                        ["from", "start", "startTime"]
                            .iter()
                            .find_map(|key| object.get(*key).and_then(number_as_seconds_ms))
                    });
                output.push(TranscriptSegment { start_ms, text });
                return;
            }
            object
                .values()
                .for_each(|value| collect_json_segments(value, output));
        }
        _ => {}
    }
}

fn append_segment(output: &mut String, last_line: &mut Option<String>, segment: TranscriptSegment) {
    let clean = escape_subtitle_text(&segment.text);
    if clean.trim().is_empty() || last_line.as_deref() == Some(clean.as_str()) {
        return;
    }
    let needs_anchor = segment.start_ms.is_some_and(|start_ms| {
        let last_anchor = output
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix("### ["))
            .and_then(|line| line.strip_suffix(']'))
            .and_then(parse_clock_ms);
        last_anchor.is_none_or(|anchor| start_ms.saturating_sub(anchor) >= 45_000)
    });
    if needs_anchor {
        output.push_str(&format!(
            "\n### [{}]\n\n",
            format_timestamp(segment.start_ms.unwrap_or_default())
        ));
    }
    output.push_str(&clean.replace('\n', " "));
    output.push('\n');
    *last_line = Some(clean);
}

fn escape_subtitle_text(value: &str) -> String {
    value
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "")
        .trim()
        .to_string()
}

fn strip_ass_tags(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '{' => in_tag = true,
            '}' if in_tag => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

fn parse_clock_ms(value: &str) -> Option<u64> {
    let value = value.trim().replace(',', ".");
    let mut parts = value.split(':');
    let first = parts.next()?;
    let (hours, minutes, seconds) = match (parts.next(), parts.next()) {
        (Some(minutes), Some(seconds)) => (first.parse::<u64>().ok()?, minutes, seconds),
        (Some(seconds), None) => (0, first, seconds),
        _ => return None,
    };
    let (seconds, fraction) = seconds
        .split_once('.')
        .map(|(seconds, fraction)| (seconds, fraction))
        .unwrap_or((seconds, "0"));
    let fraction_text = fraction.chars().take(3).collect::<String>();
    let fraction_digits = fraction_text.len();
    let fraction = fraction_text.parse::<u64>().unwrap_or(0);
    let millis = match fraction_digits {
        1 => fraction * 100,
        2 => fraction * 10,
        _ => fraction,
    };
    Some(
        hours
            .saturating_mul(3_600_000)
            .saturating_add(minutes.parse::<u64>().ok()?.saturating_mul(60_000))
            .saturating_add(seconds.parse::<u64>().ok()?.saturating_mul(1_000))
            .saturating_add(millis),
    )
}

fn parse_ass_clock_ms(value: &str) -> Option<u64> {
    let value = value.trim();
    let (hours, minutes, seconds) = value.split_once(':').and_then(|(hours, rest)| {
        let (minutes, seconds) = rest.split_once(':')?;
        Some((hours.parse::<u64>().ok()?, minutes, seconds))
    })?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, "0"));
    let fraction_text = fraction.chars().take(3).collect::<String>();
    let fraction_digits = fraction_text.len();
    let fraction = fraction_text.parse::<u64>().unwrap_or(0);
    let millis = match fraction_digits {
        1 => fraction * 100,
        2 => fraction * 10,
        _ => fraction,
    };
    Some(
        hours
            .saturating_mul(3_600_000)
            .saturating_add(minutes.parse::<u64>().ok()?.saturating_mul(60_000))
            .saturating_add(seconds.parse::<u64>().ok()?.saturating_mul(1_000))
            .saturating_add(millis),
    )
}

fn number_as_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value as u64)
    })
}

fn number_as_seconds_ms(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| (value * 1_000.0) as u64)
        .or_else(|| value.as_u64().map(|value| value.saturating_mul(1_000)))
}

fn format_timestamp(milliseconds: u64) -> String {
    let hours = milliseconds / 3_600_000;
    let minutes = milliseconds / 60_000 % 60;
    let seconds = milliseconds / 1_000 % 60;
    let millis = milliseconds % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::render_subtitle_markdown;
    use std::io::Write;

    #[test]
    fn renders_gzip_compressed_bilibili_json_subtitles() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(br#"{"body":[{"from":1.25,"content":"hello"}]}"#)
            .unwrap();
        let compressed = encoder.finish().unwrap();
        let markdown = render_subtitle_markdown(&compressed, "json").unwrap();
        assert!(markdown.contains("### [00:00:01.250]\n\nhello"));
    }

    #[test]
    fn renders_lrc_with_sparse_time_anchors() {
        let markdown =
            render_subtitle_markdown(b"[00:01.00]first\n[00:12.50]second\n[00:48.00]third", "lrc")
                .unwrap();
        assert!(markdown.contains("### [00:00:01.000]"));
        assert!(markdown.contains("### [00:00:48.000]"));
        assert_eq!(markdown.matches("### [").count(), 2);
    }

    #[test]
    fn renders_vtt_without_html_and_keeps_timestamp() {
        let markdown = render_subtitle_markdown(
            b"WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello <script>\n",
            "vtt",
        )
        .unwrap();
        assert!(markdown.contains("### [00:00:01.000]\n\nHello &lt;script&gt;"));
        assert!(!markdown.contains("<script>"));
    }

    #[test]
    fn renders_every_line_of_a_multiline_vtt_cue() {
        let markdown = render_subtitle_markdown(
            b"WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nFirst line\nSecond line\n\n",
            "vtt",
        )
        .unwrap();
        assert!(markdown.contains("First line Second line"));
    }

    #[test]
    fn renders_ass_dialogue_and_removes_override_tags() {
        let markdown = render_subtitle_markdown(
            b"[Events]\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\i1}Hello\\Nworld\n",
            "ass",
        )
        .unwrap();
        assert!(markdown.contains("### [00:00:01.000]\n\nHello world"));
        assert!(!markdown.contains("\\i1"));
    }

    #[test]
    fn renders_bilibili_json_body_segments() {
        let markdown = render_subtitle_markdown(
            br#"{"body":[{"from":1.25,"to":2.5,"content":"hello"}]}"#,
            "json",
        )
        .unwrap();
        assert!(markdown.contains("### [00:00:01.250]\n\nhello"));
    }

    #[test]
    fn rejects_json_without_text_segments() {
        assert!(render_subtitle_markdown(br#"{"body":[]}"#, "json").is_none());
    }

    #[test]
    fn rejects_plain_error_text_without_a_valid_timeline() {
        assert!(render_subtitle_markdown(b"login required\ntry again\n", "srt").is_none());
    }
}
