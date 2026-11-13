mod common;

use parvu_config::{RecentsManager, Settings};

#[test]
fn add_moves_to_front_and_dedups() {
    let (_tmp, paths, mut mgr, _) = common::recents_manager();
    assert!(mgr.recents().is_empty());
    mgr.add("/a.csv");
    mgr.add("/b.parquet");
    mgr.add("/a.csv");
    assert_eq!(mgr.recents(), ["/a.csv", "/b.parquet"]);

    let reloaded = RecentsManager::with_paths(&paths);
    assert_eq!(reloaded.recents(), ["/a.csv", "/b.parquet"]);
    let raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(paths.recents_file()).unwrap()).unwrap();
    assert_eq!(raw, serde_json::json!({"recents": ["/a.csv", "/b.parquet"]}));
}

#[test]
fn remove_and_clear() {
    let (_tmp, paths, mut mgr, _) = common::recents_manager();
    mgr.add("/a");
    mgr.add("/b");
    mgr.remove("/a");
    mgr.remove("/missing");
    assert_eq!(mgr.recents(), ["/b"]);
    mgr.clear();
    assert!(mgr.recents().is_empty());
    assert!(RecentsManager::with_paths(&paths).recents().is_empty());
}

#[test]
fn respects_save_file_history() {
    let (_tmp, _paths, mut mgr, settings) = common::recents_manager();
    assert!(mgr.add_if_enabled("/a", &settings));
    let off = Settings { save_file_history: "False".into(), ..Settings::default() };
    assert!(!RecentsManager::is_enabled(&off));
    assert!(!mgr.add_if_enabled("/b", &off));
    assert_eq!(mgr.recents(), ["/a"]);
}

#[test]
fn corrupt_or_missing_file_gives_empty_list() {
    let (_tmp, paths) = common::temp_app_dir();
    assert!(RecentsManager::with_paths(&paths).recents().is_empty());
    std::fs::create_dir_all(paths.history_dir()).unwrap();
    std::fs::write(paths.recents_file(), "{{{").unwrap();
    let mut mgr = RecentsManager::with_paths(&paths);
    assert!(mgr.recents().is_empty());
    mgr.add("/x");
    assert_eq!(RecentsManager::with_paths(&paths).recents(), ["/x"]);
}

#[test]
fn reads_bundled_recents() {
    let data: parvu_config::RecentsData =
        serde_json::from_str(parvu_config::settings::BUNDLED_RECENTS_JSON).unwrap();
    assert!(data.recents.is_empty());
}
