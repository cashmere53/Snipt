# Research & Design Decisions: capture-library

## Summary
- **Feature**: `capture-library`
- **Discovery Scope**: New Feature(グリーンフィールド、フルディスカバリー。Web 調査は保存形式・PNG・ごみ箱・エクスプローラー表示・クリップボード・ファイル監視・IPC に絞った)
- **Key Findings**:
  - 「真実の情報源はファイル、インデックスはキャッシュ」にすると、保存先フォルダ単位で自己完結し(Req 5.2)、インデックス破損・外部削除(Req 3.7)・スキーマ更新に強い。メタデータは元 PNG の iTXt チャンクに埋め込めば別のサイドカーが不要になる。
  - 1,000 件で最初のサムネイル群を 1 秒以内(Req 3.8)に出すには、起動時にメモリ上のインデックスを用意しておき、一覧は 1 回の IPC、サムネイルは小さな PNG を asset プロトコル + 仮想スクロールで遅延読み込みするのが最も単純。
  - Tauri v2 は command で生バイト列の受信(`tauri::ipc::Request` の `InvokeBody::Raw` とヘッダ)と返却(`tauri::ipc::Response`)ができる。合成済み PNG と注釈 JSON を base64 なしで 1 回の呼び出しで受け取れる。

## Research Log

### 保存レイアウトとメタデータ / インデックス
- **Context**: Req 1.1–1.4, 3.1, 3.8, 5.2、brief の制約(バージョン付き保存形式)
- **Sources Consulted**: PNG 仕様(tEXt/iTXt チャンク)、png crate のドキュメント(lib.rs/crates/png)、一般的なスクリーンショットツールの保存方式
- **Findings**:
  - 候補は (a) キャプチャごとのフォルダ、(b) 保存先直下に PNG を置き、隠しフォルダ `.snipt/` に付随データを置く、(c) SQLite。
  - (a) はエクスプローラーで画像を直接扱いにくく、brief の「合成済み PNG を直接使える」と相性が悪い。(c) は依存とマイグレーションが増え、フォルダを移したときに DB とファイルがずれる。
  - PNG の iTXt チャンク(UTF-8)に撮影メタデータ(撮影日時、撮影方法、ウィンドウタイトル)を JSON で埋め込める。画像サイズは IHDR から読める。ヘッダ部分だけ読めば再構築に使える。
  - 1,000 件 × 約 300 バイトの JSON インデックス(約 300KB)は 1 回のファイル読み込みと数 ms のパースで済む。
- **Implications**: (b) を採用。`index.json` はファイルから再構築できるキャッシュとし、スキーマ不一致や破損時は再構築する。

### PNG エンコード速度
- **Context**: screen-capture Req 5.4(確定から編集画面表示まで 1 秒)の経路上に `save_capture` がある
- **Sources Consulted**: png crate(最新安定 0.17.x、fdeflate による高速モード)、fdeflate crate
- **Findings**: png crate の高速モード(`Compression::Fast`、fdeflate)は libpng の最速モードより大幅に速く、圧縮率も良いとされる。4K RGBA を数十 ms 程度で書ける見込み。iTXt の書き込みも Encoder API で可能。
- **Implications**: 元画像の保存は png crate を直接使い `Compression::Fast`。サムネイル生成・デコードには `image` crate(0.25 系、内部で png crate を使用)を使う。

### サムネイル生成とキャッシュ
- **Context**: Req 3.1, 3.2, 3.8
- **Findings**:
  - 保存時にはメモリ上に RGBA があるので、その場で縮小(最大 320×320、アスペクト維持)して PNG で書けば数 ms〜十数 ms。
  - 全モニタ撮影は透明領域を含むため JPEG は不適。サムネイルは PNG。
  - 再構築時や欠損時は、バックグラウンドワーカーが不足分を生成し、完了ごとに更新イベントを出す。
  - サムネイル URL にバージョン(`thumbnailVersion`)をクエリで付け、注釈保存後の差し替えで WebView のキャッシュを無効化する。
- **Implications**: `.snipt/thumbs/<id>.png` に保存。表示は `convertFileSrc` + asset プロトコル。asset のスコープはサムネイルフォルダだけに動的に許可する。

### ごみ箱への移動
- **Context**: Req 4.2
- **Sources Consulted**: trash crate(docs.rs、最新 5.2.x、`windows` crate 依存)
- **Findings**: `trash::delete_all` で複数パスをまとめてごみ箱へ移動できる。Windows 実装はシェル API(IFileOperation)を使う。
- **Implications**: platform アダプタ経由で `trash` crate を使う。自作の Shell API 呼び出しはしない。

### エクスプローラーで表示
- **Context**: Req 3.6
- **Sources Consulted**: Tauri 公式 opener プラグイン(v2.tauri.app/plugin/opener、docs.rs/tauri-plugin-opener)
- **Findings**: `tauri-plugin-opener` の `reveal_item_in_dir` がファイルを選択した状態でファイルマネージャーを開く(Windows では SHOpenFolderAndSelectItems 相当)。
- **Implications**: 公式プラグインの Rust API を使う。WebView からは直接呼ばせず、capture-library の command 経由にする(パスを WebView に決めさせない)。

### 画像のクリップボード
- **Context**: Req 3.5、共有契約(`platform::windows::clipboard::copy_image` は capture-library が所有し、screen-capture / annotation-editor も使う)
- **Sources Consulted**: arboard(GitHub)、Windows クリップボード形式(CF_DIBV5、登録形式 "PNG")に関する資料
- **Findings**: arboard の README には Windows でどの形式を書くかの記述が無い。チャットツールやブラウザは登録形式 "PNG" を優先して読み、Office などは CF_DIB/CF_DIBV5 を読む。透明度を保つには PNG 形式の併載が確実。
- **Implications**: `windows` crate で直接 CF_DIBV5 と "PNG" の両方を 1 回の OpenClipboard 内で設定する薄いアダプタを作る。クリップボードが他プロセスに握られている場合に備えて短い再試行を入れる。

### 外部削除の検知
- **Context**: Req 3.7
- **Findings**: `notify` crate(Windows では ReadDirectoryChangesW)で保存先フォルダを非再帰で監視できる。ネットワークドライブなどでイベントが欠けることがあるため、監視だけに頼らない。
- **Implications**: (1) インデックス読み込み時の突き合わせ、(2) 監視イベント、(3) UI でサムネイル読み込みに失敗したときの存在確認、の 3 段で除外する。

### 大きな画像の IPC
- **Context**: 共有契約(base64 JSON 禁止)、Req 2.1, 2.2, 2.4
- **Sources Consulted**: Tauri v2 の command の生ボディ / 生レスポンスの使い方(`tauri::ipc::Request`、`InvokeBody::Raw`、`tauri::ipc::Response`)
- **Findings**: `invoke(cmd, Uint8Array, { headers })` で生ボディとヘッダを送れる。ヘッダは ASCII 前提のため、日本語を含む注釈 JSON はボディに入れる必要がある。
- **Implications**: 注釈保存は「4 バイト LE の JSON 長 + JSON(UTF-8)+ PNG」の 1 本のボディにする。cross-spec レビューで、この封筒と生 PNG 応答の command は annotation-editor(`editor_save` / `editor_read_original`、ID はウィンドウラベル由来)が所有し、capture-library は Rust サービス `save_annotations` / `read_original_png` のみを提供する形に一本化した。

### スキーマのバージョン管理
- **Context**: brief の制約、Req 2.5
- **Findings**: 3 種類のバージョンがある。(1) `index.json` の `schemaVersion`(キャッシュなので不一致時は再構築)、(2) PNG iTXt メタの `v`(読み取り側は未知のフィールドを無視)、(3) 注釈ドキュメントの `schemaVersion`(annotation-editor が所有、capture-library は整数であることだけ検証)。
- **Implications**: 注釈ドキュメントの移行(古い版の読み込み)は annotation-editor の責務。capture-library は保存されたバイト列をそのまま返し、破損(JSON として読めない、`schemaVersion` が無い)だけを判定する。

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| サービス + ファイルリポジトリ(採用) | `CaptureLibrary` サービスが、純粋なレイアウト / 命名 / インデックスのモジュールと platform アダプタを組み合わせる | テストしやすい、依存が少ない、steering の platform 分離に合う | インデックスとファイルの整合性を自分で保つ必要がある | 突き合わせで補う |
| SQLite (rusqlite) | メタデータを DB に保存 | クエリが柔軟 | 依存が増える。フォルダ単位で持ち運べない、二重管理 | 要件に検索が無いので過剰 |
| キャプチャごとのフォルダ | `<id>/original.png` など | 1 件の削除が単純 | エクスプローラーで扱いにくい | brief の意図に反する |

## Design Decisions

### Decision: ファイルを真実の情報源とし、インデックスはキャッシュ
- **Context**: Req 3.7, 3.8, 5.2、brief のバージョン制約
- **Alternatives Considered**:
  1. `index.json` を唯一の情報源にする — 破損するとメタデータを失う
  2. キャプチャごとのサイドカー JSON — 1,000 ファイルの読み込みが起動ごとに必要
- **Selected Approach**: 元 PNG(iTXt にメタ)・注釈 JSON・合成済み PNG を真実とし、`.snipt/index.json` は読み込み高速化用のキャッシュ。読み込み時にディレクトリ一覧(名前のみ)と突き合わせる。
- **Rationale**: 保存先フォルダだけで自己完結し、旧フォルダに戻しても履歴が復元される(Req 5.2)。
- **Trade-offs**: キャッシュが無いときの初回再構築は 1,000 件で数百 ms〜数秒かかる(起動時のバックグラウンドで行う)。
- **Follow-up**: HDD・ネットワークドライブでの再構築時間を実測する。

### Decision: 保存失敗時は「未保存キャプチャ」としてメモリに保持
- **Context**: Req 1.6、共有契約(保存失敗時も screen-capture は編集画面を開く)
- **Selected Approach**: `save_capture` は失敗時に `SaveCaptureError { reason, unsaved: CaptureItem }` を返し、PNG バイト列をメモリ上の `UnsavedStore` に保持する。通知は capture-library が `Notifier` で行う。screen-capture は `unsaved.id` で編集画面を開く。注釈の保存時に元画像の保存を再試行し、成功すれば通常の履歴項目になる。
- **Rationale**: `EditorLauncher::open_editor(&CaptureId)` の契約を変えずに済む。
- **Trade-offs**: アプリ終了時に未保存キャプチャは失われる(要件は「編集画面で開けるように保持」のみ)。

### Decision: 削除時の編集画面クローズは `EditorLauncher::close_editor` に依頼する
- **Context**: Req 4.3。当初の共有契約の `EditorLauncher` には open しか無かった。
- **Selected Approach**: cross-spec レビューで `close_editor`(未保存注釈を flush してから閉じる)を契約に追加。capture-library は書き込みロック外でこれを呼び、閉じ終わってからごみ箱へ移動する。
- **Trade-offs**: 削除が最大 3 秒待つことがある。ウィンドウのライフサイクルは annotation-editor に一元化される。

### Decision: 一般化 / 採用 / 簡素化(Synthesis)
- **一般化**: 「履歴項目のサムネイルの元」は常に「合成済み画像があればそれ、無ければ元画像」。コピー(Req 3.5)・エクスプローラー表示(Req 3.6)・サムネイル(Req 3.2)は同じ `CaptureItem::display_path()` で決める。
- **採用**: ごみ箱は `trash`、エクスプローラー表示は `tauri-plugin-opener`、監視は `notify`、エンコードは `png` / `image`。クリップボードのみ、PNG 形式の併載を確実にするため `windows` crate で自作(数十行)。
- **簡素化**: リポジトリ trait やストレージ抽象は作らない(実装は 1 つ)。テストのために必要な OS 操作(ごみ箱・表示・クリップボード)だけを `DesktopOps` trait にまとめる。DB・検索・タグは作らない。

## Risks & Mitigations
- 4K / 全モニタ画像のエンコードが screen-capture の 1 秒予算を圧迫する — `Compression::Fast`、サムネイル生成は保存後に行い戻り値を待たせない
- asset プロトコルのスコープに隠しフォルダ(`.snipt`)を許可したときの挙動差 — Windows では先頭ドットの制限は既定で無効だが、実装時に確認する。駄目ならサムネイルを `library_read_thumbnail` の生バイトで返す
- 削除と自動保存の競合(編集画面が閉じる直前に保存要求が届く) — 削除済み ID は `NotFound` を返し、ファイルを再作成しない
- 外部ツールが保存先にファイルを大量追加した場合の監視イベント洪水 — 300ms のデバウンスとファイル名パターンでの絞り込み

## References
- [png crate (lib.rs)](https://lib.rs/crates/png) — fdeflate による高速エンコードモード
- [trash crate (docs.rs)](https://docs.rs/crate/trash/latest) — ごみ箱への移動
- [Tauri opener plugin](https://v2.tauri.app/ja/plugin/opener/) / [docs.rs](https://docs.rs/tauri-plugin-opener) — `reveal_item_in_dir`
- [Tauri: calling Rust (raw request / response)](https://playbooks.com/skills/dchuk/claude-code-tauri-skills/tauri-calling-rust) — `InvokeBody::Raw`、`tauri::ipc::Response`
- [arboard](https://github.com/1Password/arboard) — 比較検討(Windows の形式が文書化されていないため不採用)
- PNG Specification — iTXt / tEXt チャンク
