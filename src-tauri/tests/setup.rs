//! Runs the composition root's setup chain against Tauri's mock runtime.

use tauri::test::{mock_builder, mock_context, noop_assets};

#[test]
fn setup_chain_succeeds_on_mock_runtime() {
    let mut app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app");
    snipt_lib::setup_specs(&mut app).expect("setup chain failed");
}
