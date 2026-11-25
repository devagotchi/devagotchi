//! `operation_read`: a page of a job's kept output, from a cursor, so a reader that fell
//! behind the output limit is told what it missed (`gap`).

use super::*;

impl Broker {
    pub(super) fn read(&self, input: ReadInput) -> BridgeResult<OutputPage> {
        let id = validate_id(&input.operation_id)?;
        if !(1024..=MAX_OUTPUT_PAGE_BYTES).contains(&input.max_bytes) {
            return Err(invalid("max_bytes must be 1024..=16384"));
        }
        let state = self.shared.lock();
        let operation = reach(&state.operations, &id, input.caller.as_ref())?;
        let record = &operation.record;
        if input.cursor > record.latest_sequence {
            return Err(invalid(
                "cursor is ahead of this operation's latest sequence",
            ));
        }
        let earliest = record
            .frames
            .front()
            .map_or(record.latest_sequence + 1, |frame| frame.sequence);
        let gap = input.cursor.saturating_add(1) < earliest;
        let mut bytes = 0;
        let mut frames = Vec::new();
        let mut next_cursor = input.cursor;
        for frame in record
            .frames
            .iter()
            .filter(|frame| frame.sequence > input.cursor)
        {
            if frames.len() >= MAX_PAGE_FRAMES || bytes + frame.bytes.len() > input.max_bytes {
                break;
            }
            bytes += frame.bytes.len();
            next_cursor = frame.sequence;
            frames.push(OutputFrame {
                sequence: frame.sequence,
                stream: frame.stream.clone(),
                bytes_base64: BASE64.encode(&frame.bytes),
                text: frame.text.clone(),
            });
        }
        Ok(OutputPage {
            operation_id: id,
            frames,
            next_cursor,
            gap,
            earliest_retained_sequence: earliest,
            latest_sequence: record.latest_sequence,
            has_more: next_cursor < record.latest_sequence,
            operation: record.view.clone(),
        })
    }
}
