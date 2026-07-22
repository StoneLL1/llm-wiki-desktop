use serde_json::Value;

const MAX_RENDERED_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
struct Segment {
    start_ms: Option<u64>,
    text: String,
}

/// Render the subtitle formats emitted by the supported platform providers.
///
/// A subtitle URL is not evidence that a usable transcript exists. Callers
/// use `None` to continue with the next subtitle candidate or local ASR.
pub fn render_subtitle_markdown(bytes: &[u8], extension: &str) -> Option<String> {
    match extension.to_ascii_lowercase().as_str() {
        "vtt" | "srt" => render_timed_text(bytes),
        "ass" | "ssa" => render_ass(bytes),
        "json" => render_json(bytes),
        _ => None,
    }
}

fn render_timed_text(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut output = String::new();
    let mut last_line = None::<String>;
    let mut start_ms = None;
    let mut cue_lines = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            flush_timed_cue(&mut output, &mut last_line, &mut start_ms, &mut cue_lines);
            continue;
        }
        if line.contains("-->") {
            flush_timed_cue(&mut output, &mut last_line, &mut start_ms, &mut cue_lines);
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
        if output.len() >= MAX_RENDERED_BYTES {
            break;
        }
    }
    flush_timed_cue(&mut output, &mut last_line, &mut start_ms, &mut cue_lines);
    (!output.trim().is_empty()).then_some(output)
}

fn flush_timed_cue(
    output: &mut String,
    last_line: &mut Option<String>,
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
    append_segment(
        output,
        last_line,
        Segment {
            start_ms: Some(start_ms),
            text,
        },
    );
}

fn render_ass(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut output = String::new();
    let mut last_line = None::<String>;
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
        append_segment(
            &mut output,
            &mut last_line,
            Segment {
                start_ms: Some(start_ms),
                text,
            },
        );
        if output.len() >= MAX_RENDERED_BYTES {
            break;
        }
    }
    (!output.trim().is_empty()).then_some(output)
}

fn render_json(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let mut segments = Vec::new();
    collect_json_segments(&value, &mut segments);
    if !segments.iter().any(|segment| segment.start_ms.is_some()) {
        return None;
    }
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

fn collect_json_segments(value: &Value, output: &mut Vec<Segment>) {
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
                output.push(Segment { start_ms, text });
                return;
            }
            object
                .values()
                .for_each(|value| collect_json_segments(value, output));
        }
        _ => {}
    }
}

fn append_segment(output: &mut String, last_line: &mut Option<String>, segment: Segment) {
    let clean = escape_subtitle_text(&segment.text);
    if clean.trim().is_empty() || last_line.as_deref() == Some(clean.as_str()) {
        return;
    }
    if let Some(start_ms) = segment.start_ms {
        output.push_str(&format!("- [{}] {}\n", format_timestamp(start_ms), clean));
    } else {
        output.push_str(&format!("- {}\n", clean));
    }
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

    #[test]
    fn renders_vtt_without_html_and_keeps_timestamp() {
        let markdown = render_subtitle_markdown(
            b"WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello <script>\n",
            "vtt",
        )
        .unwrap();
        assert!(markdown.contains("[00:00:01.000] Hello &lt;script&gt;"));
        assert!(!markdown.contains("<script>"));
    }

    #[test]
    fn renders_every_line_of_a_multiline_vtt_cue() {
        let markdown = render_subtitle_markdown(
            b"WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nFirst line\nSecond line\n\n",
            "vtt",
        )
        .unwrap();
        assert!(markdown.contains("First line\nSecond line"));
    }

    #[test]
    fn renders_ass_dialogue_and_removes_override_tags() {
        let markdown = render_subtitle_markdown(
            b"[Events]\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\i1}Hello\\Nworld\n",
            "ass",
        )
        .unwrap();
        assert!(markdown.contains("[00:00:01.000] Hello"));
        assert!(markdown.contains("world"));
        assert!(!markdown.contains("\\i1"));
    }

    #[test]
    fn renders_bilibili_json_body_segments() {
        let markdown = render_subtitle_markdown(
            br#"{"body":[{"from":1.25,"to":2.5,"content":"hello"}]}"#,
            "json",
        )
        .unwrap();
        assert!(markdown.contains("[00:00:01.250] hello"));
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
