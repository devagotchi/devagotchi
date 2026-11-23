use super::*;
use serde_json::json;

fn read(path: &str, offset: i64, max_bytes: usize) -> FileStep {
    FileStep::Read {
        input: FileReadInput {
            path: path.into(),
            offset,
            max_bytes,
            start_line: None,
            line_count: None,
        },
    }
}
fn invocation(intent: FileIntent) -> FileInvocation {
    FileInvocation {
        key: "one-intent".into(),
        intent,
    }
}

#[test]
fn a_batch_binds_every_path_and_its_first_page_bound() {
    let call = invocation(FileIntent::Batch {
        paths: vec!["/work/a".into(), "/work/b".into()],
        max_bytes: 100,
    });
    call.validate().unwrap();
    assert_eq!(call.max_pages(), 2);
    assert_eq!(call.action(), GrantAction::FileRead);
    for path in ["/work/a", "/work/b"] {
        call.request_for(&read(path, 0, 100)).unwrap();
    }
    for step in [
        read("/secret", 0, 100),
        read("/work/a", 1, 100),
        read("/work/a", 0, 101),
        FileStep::Operation {},
    ] {
        assert_eq!(
            call.request_for(&step).unwrap_err().code,
            "authority_mismatch"
        );
    }
    assert!(
        invocation(FileIntent::Batch {
            paths: vec![],
            max_bytes: 100
        })
        .validate()
        .is_err()
    );
    assert!(
        invocation(FileIntent::Batch {
            paths: vec!["/a".into(); MAX_FILE_BATCH + 1],
            max_bytes: 100
        })
        .validate()
        .is_err()
    );
}

#[test]
fn image_pages_keep_one_path_and_cannot_cross_the_image_byte_limit() {
    let call = invocation(FileIntent::Image {
        path: "/work/image".into(),
    });
    let FileOperation::Read(last) = call
        .request_for(&read(
            "/work/image",
            MAX_IMAGE_BYTES as i64 - 7,
            MAX_FILE_READ_BYTES,
        ))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(last.max_bytes, 7);
    for step in [
        read("/other", 0, MAX_FILE_READ_BYTES),
        read("/work/image", -1, MAX_FILE_READ_BYTES),
        read("/work/image", MAX_IMAGE_BYTES as i64, MAX_FILE_READ_BYTES),
        read("/work/image", 0, MAX_FILE_READ_BYTES + 1),
    ] {
        assert_eq!(
            call.request_for(&step).unwrap_err().code,
            "authority_mismatch"
        );
    }
    let FileStep::Read { mut input } = read("/work/image", 0, MAX_FILE_READ_BYTES) else {
        panic!()
    };
    input.start_line = Some(1);
    assert!(call.request_for(&FileStep::Read { input }).is_err());
}

#[test]
fn account_mutations_need_explicit_content_or_absence_guards() {
    let mut write: FileWriteInput =
        serde_json::from_value(json!({"path":"/work/a","text":"new"})).unwrap();
    assert_eq!(
        FileOperation::Write(write.clone())
            .require_guard()
            .unwrap_err()
            .code,
        "file_guard_required"
    );
    write.mode = WriteMode::Create;
    FileOperation::Write(write.clone()).require_guard().unwrap();
    write.mode = WriteMode::Append;
    write.expected_sha256 = Some("a".repeat(64));
    FileOperation::Write(write.clone()).require_guard().unwrap();
    write.expected_sha256 = Some("page hash".into());
    assert!(FileOperation::Write(write).require_guard().is_err());
    let mut moved: FileMoveInput =
        serde_json::from_value(json!({"source":"/a","destination":"/b"})).unwrap();
    assert!(FileOperation::Move(moved.clone()).require_guard().is_err());
    moved.expected_sha256 = Some("b".repeat(64));
    FileOperation::Move(moved).require_guard().unwrap();
    assert!(
        FileOperation::Delete(FileDeleteInput {
            path: "/a".into(),
            expected_sha256: "".into()
        })
        .require_guard()
        .is_err()
    );
}

#[test]
fn ordinary_intent_cannot_be_replaced_by_a_page_or_another_operation() {
    let call = invocation(FileIntent::Operation {
        operation: FileOperation::Info(FileInfoInput {
            path: "/a".into(),
            sha256: true,
        }),
    });
    let FileOperation::Info(info) = call.request_for(&FileStep::Operation {}).unwrap() else {
        panic!()
    };
    assert!(info.sha256);
    assert!(
        call.request_for(&read("/a", 0, MAX_FILE_READ_BYTES))
            .is_err()
    );
    assert!(FileOperation::from_request(RequestBody::ConfigGet(ConfigGetInput {})).is_err());
}

#[test]
fn invocation_keys_and_wire_shapes_refuse_ambiguous_input() {
    let mut call = invocation(FileIntent::Image { path: "/a".into() });
    for key in [
        String::new(),
        "x".repeat(MAX_OPERATION_KEY_BYTES + 1),
        "line\nbreak".into(),
    ] {
        call.key = key;
        assert!(call.validate().is_err());
    }
    assert!(
        serde_json::from_value::<FileDeleteInput>(
            json!({"path":"/a","expected_sha256":"a".repeat(64),"recursive":true})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<FileIntent>(
            json!({"kind":"image","path":"/a","paths":["/secret"]})
        )
        .is_err()
    );
    let info: FileInfoInput = serde_json::from_value(json!({"path":"/a"})).unwrap();
    assert!(!info.sha256);
}
