//! Restart recovery: the journal read back when the broker opens. A job that had not settled
//! is marked `unknown` (its effects can't be known, and it is never run again or signalled),
//! nobody holds an interactive job's keyboard any more, and each job keeps no more output than
//! the broker now keeps.

use super::*;

pub(super) fn recover(
    state_dir: &Path,
    output_limit: usize,
) -> BridgeResult<(Journal, HashMap<String, Operation>, HashMap<String, String>)> {
    let mut journal = Journal::open(state_dir)?;
    let recovered = journal.load()?;
    let mut operations = HashMap::new();
    let mut keys = HashMap::new();
    for mut record in recovered {
        if let Some(interactive) = &mut record.view.interactive {
            interactive.input_open = false;
            interactive.writer_until_ms = None;
        }
        if !record.view.state.terminal() {
            record.view.state = OperationState::Unknown;
            record.view.updated_at_ms = now_ms();
            record.view.output_complete = false;
            record.view.detail = Some("Broker restarted before settlement. Execution effects and process/resource status are unknown; processes may still be running. Admission does not prove whether spawning occurred. This operation will never be replayed automatically, and no recovered PID will be signalled.".into());
            journal.update(&record.view)?;
        }
        let mut retained_bytes: usize = record.frames.iter().map(|frame| frame.bytes.len()).sum();
        while retained_bytes > output_limit || record.frames.len() > MAX_FRAMES {
            if let Some(frame) = record.frames.pop_front() {
                retained_bytes -= frame.bytes.len();
            }
        }
        let earliest = record
            .frames
            .front()
            .map_or(record.latest_sequence + 1, |frame| frame.sequence);
        journal.prune(&record.view.operation_id, earliest)?;
        keys.insert(record.key.clone(), record.view.operation_id.clone());
        operations.insert(
            record.view.operation_id.clone(),
            Operation {
                record,
                retained_bytes,
                active: false,
                control: None,
                persistence_fault: None,
                input: None,
                writer: None,
                launch: None,
                pid: None,
            },
        );
    }
    Ok((journal, operations, keys))
}
