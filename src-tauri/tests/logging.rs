//! M0 acceptance: a log file is created under the data folder. This lives in its own test
//! binary because a process can install the global subscriber only once.

use kept::config::DataPaths;

#[test]
fn log_file_is_created_under_the_data_folder() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(dir.path().join("kept"));
    let guard = kept::logging::init(&paths.logs).unwrap();
    assert!(
        guard.is_some(),
        "first init in this process installs the subscriber"
    );
    tracing::info!(rows = 3, "test event with a count, never an amount");
    drop(guard);

    let files: Vec<_> = std::fs::read_dir(&paths.logs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "exactly one daily log file: {files:?}");
    let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("kept.") && name.ends_with(".log"),
        "{name}"
    );
    let text = std::fs::read_to_string(&files[0]).unwrap();
    assert!(text.contains("kept logging started"), "{text}");
    assert!(text.contains("test event with a count"), "{text}");

    let second = kept::logging::init(&paths.logs).unwrap();
    assert!(
        second.is_none(),
        "a second init does not install another subscriber"
    );
}
