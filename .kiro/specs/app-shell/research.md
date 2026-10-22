# Research & Design Decisions: app-shell

## Summary
- **Feature**: `app-shell`
- **Discovery Scope**: New Feature(グリーンフィールド、フルディスカバリ。Web 調査は Tauri v2 公式プラグインと Windows 固有の落とし穴に絞った)
- **Key Findings**:
  - Windows 11 の「Print Screen キーを使用して画面キャプチャを開く」が有効だと、`RegisterHotKey` による単独 PrintScreen の登録は**失敗として返る**(ShareX のログで "Hotkey register failed" を確認)。Shift+PrintScreen など修飾付きは同時に登録できる。設定値は `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`(DWORD, 1=有効)で判定できる。
  - Tauri v2 公式プラグイン(global-shortcut / single-instance / autostart / notification / dialog)はいずれも Rust 1.90 以上が必要。single-instance は「最初に登録するプラグイン」であることが求められる。
  - global-shortcut のハンドラは Pressed / Released の両方で呼ばれるため、`ShortcutState::Pressed` のみを処理する必要がある。

## Research Log

### グローバルホットキー(tauri-plugin-global-shortcut v2)
- **Context**: 要件 2.x, 3.3, 3.5, 7.1
- **Sources Consulted**: v2.tauri.app/plugin/global-shortcut、@tauri-apps/plugin-global-shortcut README
- **Findings**:
  - `tauri_plugin_global_shortcut::Builder::new().with_handler(|app, shortcut, event| ..).build()`。`event.state` が `ShortcutState::Pressed` / `Released`。
  - `Shortcut::new(Option<Modifiers>, Code)`、`app.global_shortcut().register(shortcut)` / `unregister` / `unregister_all` / `is_registered`(`GlobalShortcutExt`)。
  - フロントエンド向け permission は既定で無効。本設計では登録を Rust 側だけで行うため、WebView に global-shortcut の permission を与えない。
  - 内部実装(global-hotkey クレート)は Windows で `RegisterHotKey` を使う。
- **Implications**: `ShortcutBackend` トレイトでプラグインを包み、登録の失敗・ロールバックをフェイクで単体テストできるようにする。

### Windows 11 の PrintScreen 予約
- **Context**: 要件 2.1, 2.4(既定の範囲キャプチャが PrintScreen)
- **Sources Consulted**: winaero.com「How to disable Print Screen from opening Snipping Tool」、ShareX issue #6581
- **Findings**:
  - 設定 UI: 設定 > アクセシビリティ > キーボード > 「Print Screen キーを使用して画面キャプチャを開く」(Windows 10 では「PrtScn ボタンを使用して画面切り取りを開く」)。
  - レジストリ: `HKCU\Control Panel\Keyboard` の `PrintScreenKeyForSnippingEnabled`(1=有効、0=無効)。新しい Windows 11 では既定で有効。
  - 有効時、単独 PrintScreen の登録は失敗する。設定変更後はアプリ側で再登録(または再起動)が必要。
- **Implications**: 登録失敗かつ「修飾なし PrintScreen」かつレジストリ値が 0 以外(未設定含む)の場合を `PrintScreenReservedByWindows` に分類し、通知本文で設定の場所を案内する。設定画面に「再登録」操作を用意する。

### 多重起動防止(tauri-plugin-single-instance v2)
- **Context**: 要件 6.1
- **Sources Consulted**: v2.tauri.app/plugin/single-instance
- **Findings**: `tauri_plugin_single_instance::init(|app, args, cwd| ..)`。コールバックは既存プロセス側で呼ばれ、新しいプロセスは自動的に終了する。他プラグインより先に登録する必要がある。
- **Implications**: コールバックで `Notifier` に「既に起動しています」を出す。新プロセス側ではトレイもホットキーも作られない。

### 自動起動(tauri-plugin-autostart v2)
- **Context**: 要件 5.x
- **Sources Consulted**: v2.tauri.app/plugin/autostart
- **Findings**: `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, args)`、`app.autolaunch().enable() / disable() / is_enabled()`(いずれも `Result`)。Windows では auto-launch クレート経由で `HKCU\...\Run` に実行ファイルパスを登録する(既知の挙動)。
- **Implications**: 設定値 `autostart` を正とし、起動時と設定確定時に OS 側の状態を同期する。デバッグビルドでは `target\debug` の exe が登録されるのを避けるため同期を行わない。タスクマネージャーのスタートアップで利用者が無効化した場合(StartupApproved)は Run キーが残っていても起動しないが、これは OS 側の利用者操作として尊重する。

### 通知(tauri-plugin-notification v2)
- **Context**: 要件 2.3, 2.4, 4.3, 6.1
- **Findings**: `app.notification().builder().title(..).body(..).show()`。Windows のトースト。デスクトップではボタン付きアクションの扱いが限定的なため、案内は本文テキストで完結させる。開発実行時は送信元が PowerShell 表示になる(インストール版では identifier に紐づく)。
- **Implications**: 案内文は本文だけで完結させる。クリック時のアクションには依存しない。

### 設定の保存方式(tauri-plugin-store vs 自前 JSON)
- **Context**: 要件 4.1〜4.4
- **Findings**: plugin-store は汎用 KV で、破損ファイルを検知して利用者に通知する(4.3)、スキーマ検証する、といった制御は自前で書く必要がある。設定は 1 ファイル・数項目で、Rust 側だけが読み書きする。
- **Implications**: `serde_json` と一時ファイル + rename による原子的書き込みで自前実装する(採用)。plugin-store は使わない。

### トレイ(Tauri コア tray-icon)
- **Context**: 要件 1.x, 7.x
- **Findings**: `TrayIconBuilder::with_id(..).icon(..).tooltip(..).menu(&menu).show_menu_on_left_click(false).on_menu_event(..).on_tray_icon_event(..)`。`TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }` が Windows で発火する(直前に Click も発火する)。`tray.set_tooltip(Some(..))` で更新可能。
- **Implications**: 左クリックではメニューを出さず、ダブルクリックで履歴を開く。ツールチップは設定変更とホットキー状態変化のたびに更新する。

### ウィンドウが 0 枚の常駐と終了制御
- **Context**: 要件 1.1, 7.1, 7.2
- **Findings**: `tauri.conf.json` の `app.windows` を空にすると起動時にウィンドウは作られない。最後のウィンドウが閉じたときに `RunEvent::ExitRequested { code: None, api }` が来るので `api.prevent_exit()` で常駐を続ける。`app.exit(0)` 経由は `code: Some(0)` になり区別できる。
- **Implications**: 終了の唯一の入口を `Lifecycle::quit` にする。

### WebView でのキー入力取得の落とし穴
- **Context**: 要件 3.3
- **Findings**: WebView2(Chromium)では PrintScreen の `keydown` が届かず `keyup` のみ届く。また登録済みグローバルホットキーは OS に横取りされ、WebView にキーイベントが来ない。
- **Implications**: ホットキー入力欄にフォーカスがある間は全ホットキーを一時解除(`hotkeys_suspend`)し、PrintScreen は `keyup` で確定する。

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 機能モジュール + トレイト注入(採用) | app-shell が `shared` にトレイトを定義し、下流が実装を managed state に登録 | 依存方向が上流→下流にならない。app-shell 単体で実装・テスト可 | 起動順(2 段階 setup)を守る必要 | shared-contracts の方針どおり |
| 各機能を Tauri プラグイン化 | 機能ごとに `tauri::plugin::Builder` | invoke_handler を分割できる | command 名が `plugin:x|y` になり permission 定義が増える | 見送り |
| イベントバスのみ | `capture://request` 等の Tauri event で疎結合 | 型が緩く順序保証が弱い | 型安全性・テスト性が低い | 見送り |

## Design Decisions

### Decision: 設定確定はオールオアナッシング
- **Context**: 3.4, 3.5, 3.7, 3.8, 4.1。部分確定にすると「永続化された設定」と「実際に有効な設定」がずれる。
- **Alternatives Considered**: 1) 失敗したホットキーだけ旧値を残し他を確定 2) 全体を確定しない
- **Selected Approach**: 2。検証 → 保存先チェック → ホットキー適用(失敗時ロールバック)→ 自動起動同期 → 永続化 → メモリ反映・通知、のどこかで失敗したら全て元に戻しエラーを返す。設定画面は下書きを保持したまま失敗理由を表示する。
- **Rationale**: 不変条件「永続化値 = 適用値」を保てる。3.5 の「直前に有効だったホットキーを維持」を満たす。
- **Trade-offs**: ホットキー以外の変更も同時に保留になるが、下書きは残るため再操作は 1 クリック。

### Decision: 一般化 — 「キャプチャ開始要求」の入口を 1 本化
- トレイメニュー(1.3)とホットキー(2.2)はどちらも `CaptureDispatch::request(mode)` に集約し、`CaptureRequester` に転送する。2.5(操作中は無視)は screen-capture が持つ busy フラグで一括判定される。

### Decision: Build vs Adopt
- 採用: Tauri 公式プラグイン(global-shortcut, single-instance, autostart, notification, dialog)、コアのトレイ。
- 自作: 設定ファイル I/O(破損検知と通知のため)、アクセラレータ文字列の正規化(フロントと Rust で同一表記にするため)。
- レジストリ読み取りは `windows` クレートの `RegGetValueW` を platform アダプタに閉じ込める(新規クレートを増やさない)。

### Decision: 単純化
- `SettingsStore` の購読はコールバック登録のみ(チャネルや非同期ランタイム依存を持ち込まない)。
- 履歴ウィンドウは「ラベル `history` のウィンドウを生成 / 前面化する」だけを app-shell が行い、中身は capture-library のフロントエンドが描画する。専用トレイトは設けない。
- ホットキーの「未割り当て」状態は要件に無いため持たない。

## Risks & Mitigations
- PrintScreen が Windows に予約されている(既定構成で初回起動時に高確率で発生)— 2.4 の専用案内 + 設定画面の「再登録」ボタン。
- 開発中の自動起動登録汚染 — デバッグビルドでは同期しない。
- `generate_handler!` が lib.rs に 1 か所のため、4 spec のタスクが同じファイルを編集する — lib.rs の `generate_handler!` 内を spec ごとのコメント区画に分け、各 spec は自分の区画に command 関数名を追記するだけとする規約にする。
- 通知が OS のフォーカスアシスト(応答不可)で表示されない — 設定画面を開いたときにホットキー状態を常に画面上でも表示する。

## References
- [Global Shortcut | Tauri](https://v2.tauri.app/plugin/global-shortcut/) — Rust API と permission
- [plugin-global-shortcut README](https://cdn.jsdelivr.net/npm/@tauri-apps/plugin-global-shortcut@2.3.2/README.md) — with_handler / ShortcutState
- [Single Instance | Tauri](https://v2.tauri.app/plugin/single-instance/) — 最初に登録する必要
- [Autostart | Tauri](https://v2.tauri.app/plugin/autostart/) — enable / disable / is_enabled
- [winaero: disable Print Screen opening Snipping Tool](https://winaero.com/how-to-disable-print-screen-from-opening-snipping-tool-in-windows-11-and-10) — 設定場所とレジストリ値
- [ShareX issue #6581](https://github.com/ShareX/ShareX/issues/6581) — PrintScreen 登録失敗の実例
