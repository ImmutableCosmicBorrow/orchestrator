/// Given a formatted `LogEvent` string, clean it up by removing internal
/// timestamp, and `sender`/`receiver` fields when they are `none`.
pub(super) fn format_log_event_from_string(mut msg: String) -> String {
    // Remove internal timestamp fields if present
    if let Some(start) = msg.find("ts: ").or_else(|| msg.find("timestamp_unix: ")) {
        let rel_comma = msg[start..].find(',');
        let rel_brace = msg[start..].find('}');
        let rel_end = match (rel_comma, rel_brace) {
            (Some(c), Some(b)) => Some(std::cmp::min(c, b)),
            (Some(c), None) => Some(c),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        if let Some(rel) = rel_end {
            let mut end = start + rel + 1; // include comma or brace
            if msg.as_bytes().get(end) == Some(&b' ') {
                end += 1;
            }
            msg.replace_range(start..end, "");
        }
    }

    // Remove `sender: none` / `receiver: none` patterns
    let patterns_with_comma = [
        "sender: none, ",
        "receiver: none, ",
        "sender: None, ",
        "receiver: None, ",
    ];
    for pat in &patterns_with_comma {
        while let Some(pos) = msg.find(pat) {
            msg.replace_range(pos..pos + pat.len(), "");
        }
    }
    let patterns = [
        "sender: none",
        "receiver: none",
        "sender: None",
        "receiver: None",
    ];
    for pat in &patterns {
        while let Some(pos) = msg.find(pat) {
            let mut start = pos;
            if start >= 2 && &msg[start - 2..start] == ", " {
                start -= 2;
            } else if start >= 1 && &msg[start - 1..start] == "," {
                start -= 1;
            }
            let mut end = pos + pat.len();
            if msg.as_bytes().get(end) == Some(&b' ') {
                end += 1;
            }
            msg.replace_range(start..end, "");
        }
    }

    // Cleanup: remove a trailing comma before a closing brace and collapse double spaces
    msg = msg.replace(", }", " }");
    while msg.contains("  ") {
        msg = msg.replacen("  ", " ", 1);
    }

    msg
}

/// If a message contains a `LogEvent { ... }` payload, normalize that payload
/// by stripping duplicated timestamp fields and empty sender/receiver fields.
pub(super) fn sanitize_log_message(message: &str) -> String {
    if let Some(start) = message.find("LogEvent {") {
        let (prefix, event_part) = message.split_at(start);
        let cleaned_event = format_log_event_from_string(event_part.to_string());
        format!("{prefix}{cleaned_event}")
    } else {
        message.to_string()
    }
}
