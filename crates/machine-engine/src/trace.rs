//! The broker's own log: one line for each operation it refused, admitted or settled, and
//! for each file request it refused, with the reason. In the foreground it goes to the
//! terminal; `devagotchi start --background` and `devagotchi mcp` send it to
//! `<state>/broker.log`, where `read_file` reaches it. Never the command's output.

use devagotchi_contracts::{BridgeError, StartInput};
use std::time::{SystemTime, UNIX_EPOCH};

/// One line: `devagotchi broker: 2026-11-16T18:52:01.123Z <subject> <text>`.
pub(crate) fn line(subject: &str, text: &str) {
    let text: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    eprintln!("devagotchi broker: {} {subject} {text}", stamp());
}

/// A refused request, with what was asked.
pub(crate) fn refused(what: &str, error: &BridgeError) {
    line(
        "refused",
        &format!("{what}: {}: {}", error.code, error.message),
    );
}

/// What an operation was asked to run, short enough for one log line.
pub(crate) fn asked(input: &StartInput) -> String {
    let asked = match &input.command {
        Some(command) => format!("command {command:?}"),
        None => format!("executable {:?} args {:?}", input.executable, input.args),
    };
    let cwd = input
        .cwd
        .as_deref()
        .map(|cwd| format!(" in {cwd}"))
        .unwrap_or_default();
    shorten(format!("{asked}{cwd}"), 400)
}

pub(crate) fn shorten(mut text: String, most: usize) -> String {
    if text.len() > most {
        let mut end = most;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

/// UTC, to the millisecond, without a date library.
fn stamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        now.subsec_millis()
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(19_723), (2024, 1, 1));
        assert_eq!(super::civil(20_773), (2026, 11, 16));
    }

    #[test]
    fn shorten_keeps_whole_characters() {
        assert_eq!(super::shorten("ab€cd".into(), 3), "ab…");
    }
}
