use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn registry_pause_state_doc_is_linked_from_readme() {
    let readme = fs::read_to_string(root().join("README.md")).expect("README should be readable");
    assert!(
        readme.contains("docs/credence-registry-pause-state.md"),
        "README should link the registry pause state machine doc"
    );
}

#[test]
fn registry_pause_state_doc_tracks_real_symbols() {
    let doc = fs::read_to_string(root().join("docs/credence-registry-pause-state.md"))
        .expect("registry pause doc should be readable");
    let pausable = fs::read_to_string(root().join("contracts/credence_registry/src/pausable.rs"))
        .expect("pausable source should be readable");
    let tests = fs::read_to_string(root().join("contracts/credence_registry/src/test_pausable.rs"))
        .expect("pausable tests should be readable");

    for marker in [
        "DataKey::Paused",
        "PauseProposal(id)",
        "ContractError::ContractPaused",
        "get_pause_state()",
        "execute_pause_proposal(id)",
        "register",
        "self_register_bond",
        "deactivate",
    ] {
        assert!(doc.contains(marker), "doc should mention {marker}");
    }

    assert!(pausable.contains("pub fn require_not_paused"));
    assert!(pausable.contains("pub fn execute_pause_proposal"));
    assert!(pausable.contains("ContractError::ContractPaused"));
    assert!(tests.contains("test_get_pause_state_multisig_flow"));
    assert!(tests.contains("invalid_pause_action_symbol_is_rejected"));
}