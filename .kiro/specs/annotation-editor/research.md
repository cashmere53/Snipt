# Research & Design Decisions: annotation-editor

## Summary
- **Feature**: `annotation-editor`
- **Discovery Scope**: New Feature(グリーンフィールド、フルディスカバリー)
- **Key Findings**:
  - react-konva 19.3.x は React 19.3 以上を peer に要求し、konva は `^7 || ^8 || ^9 || ^10` を許容する。新規なので React 19.3 + konva 10.x + react-konva 19.3 に揃える。
  - 元画像をアセットプロトコル(別オリジン)から `<img>` で読むと canvas が tainted になり、モザイク計算(`getImageData`)と書き出し(`toBlob`)が失敗するおそれがある。IPC の生バイト応答(`tauri::ipc::Response`)で PNG を受け取り Blob → `createImageBitmap` で同一オリジンのビットマップにする方が安全で、動的な保存先フォルダに対するアセットスコープ設定も不要になる。
  - 書き出しを表示用 Stage(ズーム・DPR に依存)から取らず、倍率 1 のオフスクリーン Konva Stage で再構築して `toCanvas({ pixelRatio: 1 })` すると、ズームや混在 DPI に関係なく元画像と同じピクセル解像度を保証できる。
  - 単純なブロック平均のモザイクは Depix 等で小さな文字が復元され得る。ブロックを文字高さ以上(画像長辺に比例、最小 16px)にし、合成画像では元画素を不透明なブロック色で完全に上書きする。

## Research Log

### react-konva / Konva のバージョンと互換性
- **Context**: steering で Konva.js / react-konva が指定済み。React のメジャーとの組み合わせを確定する必要がある。
- **Sources Consulted**: npm registry(react-konva 19.3.2, konva 10.9.0, react 19.3.1)、react-konva RELEASE_NOTES
- **Findings**:
  - react-konva 19.3.0 から React / React DOM 19.3.0 以上が必須。19.2 系に留める場合は `~19.2.7` に固定。React 18 は react-konva 18 系。
  - react-konva 19.3.2 の peer: `konva ^7.2.5 || ^8 || ^9 || ^10`。
- **Implications**: React 19.3 / react-konva 19.3 / konva 10 を採用。Konva の API は Transformer・Arrow・Text・Image・`toCanvas` のみを使い、バージョン差の影響を受けにくくする。

### Konva Transformer による選択・変形
- **Context**: 2.2〜2.4(選択ハンドル、移動、リサイズ / 端点変更)。
- **Sources Consulted**: Konva 公式 Transformer / Editable Text サンプル
- **Findings**:
  - Transformer は scaleX/scaleY を変更する。`transformend` で scale を幅・高さへ焼き込み scale=1 に戻すのが定石(線幅が拡大されない)。
  - 直線(矢印)には Transformer の外接矩形ハンドルが合わない。端点ドラッグ用の独自アンカー(Circle 2 個)が一般的。
  - `rotateEnabled: false`, `keepRatio` は Shift 押下時のみ有効化できる。
- **Implications**: 四角・ぼかし・テキストは Transformer、矢印は端点アンカー、マーカーは移動のみ。回転はスキーマに持たない。

### 日本語 IME とキャンバス上のテキスト入力
- **Context**: 5.1〜5.4(変換候補表示、複数行、再編集、Esc / 外側クリックで確定)。
- **Sources Consulted**: Konva "Text editing in HTML5 canvas"(textarea オーバーレイ)、MDN EditContext API
- **Findings**:
  - Konva 公式は Konva.Text の画面座標に `<textarea>` を重ね、編集中は Text を隠す方式。textarea はネイティブ IME(候補ウィンドウ位置含む)がそのまま動く。
  - EditContext API(Chromium 121+)はキャンバスへ直接 IME を接続できるが、キャレット・選択・候補窓位置を自前で描く必要があり実装量が大きい。
  - IME 変換中の Esc / Enter は変換操作であり、`keydown` の `isComposing === true`(または keyCode 229)を確定判定から除外する必要がある。
- **Implications**: textarea オーバーレイを採用。編集中はグローバルショートカット(Ctrl+C 等)を無効化し、`compositionstart/end` と `isComposing` で確定判定をガードする。

### 不可逆なモザイク
- **Context**: 6.1〜6.3、brief の制約「書き出し後の画像から元の内容を復元できない」。
- **Sources Consulted**: Depix に関する報道・検証記事(The Hacker News, JUMPSEC Labs "Can Depix deobfuscate your data?")
- **Findings**:
  - Depix はブロック平均でピクセル化した文字を、既知フォントの描画を総当たりで照合して復元する。ブロックが文字より十分小さいときに成立しやすい。
  - ガウスぼかしは逆畳み込みで部分的に復元され得る。
- **Implications**: ぼかしツールの描画はブロック平均モザイクのみ。ブロックサイズは `max(16, round(長辺/120))` 画像 px(1080p で 16px、4K で 32px)。モザイク矩形は整数画素に丸め、合成時は不透明ブロックで元画素を完全に置き換える。注釈ドキュメントには矩形とブロックサイズのみを持ち、画素は保存しない。

### 元解像度での書き出しと巨大画像
- **Context**: 10.3、brief の制約(物理ピクセル)。
- **Sources Consulted**: Konva `toCanvas` / `pixelRatio` ドキュメント、Chromium HTMLCanvasElement の上限
- **Findings**:
  - Konva の Layer canvas は既定で `window.devicePixelRatio` を用いる。表示 Stage から書き出すと DPR とズームが混入する。
  - Chromium の canvas は 1 辺 32,767px / 面積 268,435,456px(16384²)が上限の目安。超えると描画・`toBlob` が失敗する。
  - 3 枚の 4K 横並び(11520×2160 ≈ 25M px)は上限内。PNG エンコード(`toBlob`)は 25M px で 1〜2 秒程度かかり得る。
- **Implications**: 書き出し専用のオフスクリーン Stage(倍率 1、切り抜き範囲サイズ)を都度生成し `toCanvas({ pixelRatio: 1 })`。上限超過は事前チェックして 10.4 のエラーとして扱う。表示 Stage はビューポートサイズに固定し、画像サイズの canvas を表示用に作らない。

### PNG を base64 なしで Rust へ渡す / 元画像の取得
- **Context**: steering「画像の受け渡しはファイル / バイナリ IPC」。
- **Sources Consulted**: Tauri v2 "Calling Rust from the Frontend"、`tauri::ipc::Request` / `InvokeBody::Raw` / `tauri::ipc::Response`
- **Findings**:
  - `invoke(cmd, Uint8Array, { headers })` で生ボディを送り、Rust 側は `Request::body()` が `InvokeBody::Raw(Vec<u8>)`。`Response::new(Vec<u8>)` で ArrayBuffer を返せる。
  - ヘッダは ASCII 前提で、日本語を含む JSON を載せるのに向かない。
- **Implications**: 保存は「4 バイト LE の JSON 長 + UTF-8 JSON + PNG」の封筒形式で 1 回の invoke にまとめる(原子的で、ヘッダのエンコード問題を避ける)。キャプチャ ID は送らず、Rust が呼び出し元ウィンドウのラベル `editor-<captureId>` から決める。

### 複数ウィンドウとウィンドウ生成
- **Context**: 1.1, 1.4, 1.5、screen-capture の「確定から編集画面表示まで 1 秒以内」。
- **Sources Consulted**: Tauri v2 `WebviewWindowBuilder` ドキュメント(Windows で同期コマンド / メインスレッド待ち合わせからの生成はデッドロックし得る旨)
- **Findings**:
  - 既存ウィンドウは `get_webview_window(label)` で取得でき、`unminimize` / `show` / `set_focus` で前面化できる。Windows のフォアグラウンドロックで `set_focus` がタスクバー点滅で終わる場合がある。
  - ウィンドウ生成は async コンテキスト(`tauri::async_runtime::spawn`)から行えばデッドロックを避けられる。
  - ラベルは英数字と `-` `/` `:` `_` のみ許容。
- **Implications**: `EditorLauncher::open_editor` は非同期タスクにディスパッチしてすぐ戻る。CaptureId はラベル安全な文字のみであることを前提とし、満たさない場合は起動を拒否してログ・通知する(capture-library と確認事項)。

### Undo/Redo 方式
- **Context**: 9.1〜9.3(100 回以上)。
- **Findings**: ドキュメントは図形配列と切り抜き矩形のみで小さい(数 KB〜数十 KB)。不変スナップショット + 構造共有なら 200 世代でもメモリは無視できる。コマンドパターンは操作ごとに逆操作を書く必要があり、テキスト・スタイル・切り抜きで実装量が増える。
- **Implications**: 不変スナップショット履歴(past/present/future、上限 200)。ドラッグ中の中間状態は履歴に積まず、`dragend` / `transformend` / テキスト確定で 1 件コミットする。

### 前回スタイルの保持
- **Context**: 2.7、共有契約「annotation-editor が自前で保持」。
- **Findings**: 全ウィンドウは同一オリジンで WebView2 のユーザーデータを共有するため、`localStorage` は全編集ウィンドウと再起動後で共有される。
- **Implications**: `localStorage` キー `snipt.editor.lastStyle.v1` に保存し、読み込み時に検証して不正値は既定値にフォールバックする。Rust 側の設定ファイルは作らない。

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 純粋モデル + ストア + Konva ビュー(採用) | 注釈モデルと操作を純粋 TS、状態は単一ストア、react-konva は描画とポインタ入力のみ | Vitest で操作・Undo・シリアライズを検証可能。描画と書き出しが同じ属性写像を共有 | ストアとビューの同期を丁寧に設計する必要 | steering の Testing 方針と一致 |
| Konva ノードを真実のソースにする | Konva の `toJSON` を保存 | 実装が速い | スキーマが Konva 内部表現に依存、バージョン移行が困難 | 却下 |
| 書き出しを Rust で合成 | 図形を Rust で再描画 | WebView の canvas 上限に縛られない | テキスト(日本語フォント)・矢印の描画を二重実装、表示と結果がずれる | 却下 |

## Design Decisions

### Decision: 描画属性の単一写像(表示と書き出しの共有)
- **Context**: 表示(react-konva)と書き出し(オフスクリーン Konva)の見た目一致。
- **Alternatives Considered**:
  1. 表示 Stage を一時的にズーム 1 にして `toCanvas`
  2. 図形 → Konva 設定の純粋関数を共有し、書き出しは素の Konva で再構築
- **Selected Approach**: 2。`shape-attrs.ts` が図形ごとの Konva 設定を返し、表示コンポーネントと書き出しレンダラの両方がこれを使う。
- **Rationale**: 表示状態(選択枠・ハンドル・切り抜きマスク)を書き出しに混入させない。ズーム・DPR を一切考慮しなくてよい。
- **Trade-offs**: 書き出しのたびにノードを生成するコスト(図形数は少ないため軽微)。

### Decision: 一般化 — ドラッグ生成ツールとクリック生成ツール
- **Context**: 矢印・四角・ぼかし・切り抜きはいずれも「ドラッグで 2 点を決め、Shift で制約」。テキストとマーカーは「クリック位置に置く」。
- **Selected Approach**: `DragCreateTool<T>`(開始点・現在点・Shift から図形案を作る関数を差し替え)と `ClickCreateTool` の 2 種に一般化。切り抜きも DragCreate の一種(結果を shapes ではなく crop 候補に入れる)。
- **Rationale**: ツール追加時はファクトリ関数だけを書けばよい。実装範囲は現要件の 6 種に限定。

### Decision: Build vs Adopt
- 採用: Konva / react-konva(選択・変形・描画)、zustand 5(React 外からも購読できる小さなストア。オートセーブ制御が React 外で購読する)、tauri-plugin-dialog(保存ダイアログ)、ブラウザ標準の `toBlob` による PNG / JPEG エンコード。
- 自作: スナップショット履歴(数十行で足り、immer / undo ライブラリは不要)、モザイク(ブロック平均の数十行。Konva の Pixelate フィルタは表示ノードの cache に依存し、整数グリッドと元画素からの計算を保証しにくいため不採用)、生ボディ封筒コーデック。

### Decision: 簡素化
- 図形の回転、レイヤー順の手動変更、フリーハンド、図形のコピー&ペーストは持たない(要件外)。
- ぼかしはモザイク 1 方式のみ(強度設定なし)。
- 表示はスクロールコンテナではなくビューポート固定 Stage + パン/ズーム変換(巨大画像で canvas 上限に当たらない)。
- Rust 側は注釈スキーマを解釈しない(`schemaVersion` の整数チェックのみ)。

## Risks & Mitigations
- 編集ウィンドウ生成 + WebView2 初期化が 1 秒予算(screen-capture 4.4)を圧迫 — ウィンドウは即時表示し画像読み込みは後追い。計測で超過する場合は予備ウィンドウ方式(ラベル規約の変更が必要)を coordinator と協議。
- 巨大画像で PNG エンコードが遅い / canvas 上限超過 — 事前サイズチェック、オートセーブは直列化(実行中は次回へ合流)、上限超過は理由付きエラー。
- Windows のフォアグラウンドロックで前面化に失敗 — `set_focus` に加え一時的な always-on-top の切替でフォールバック。
- トレイの「終了」で `CloseRequested` を経ずにプロセスが終わると直近 1 秒未満の変更が失われ得る — cross-spec レビューで確定: app-shell の `BeforeExitHooks` に flush 付き `close_editor` を登録する。削除前クローズ(capture-library 4.3)も同じ `close_editor`(flush してから閉じる)を使う。
- 新しい `schemaVersion` の注釈を古い Snipt で開いて編集すると上書きされる — 警告バナーで明示。

## References
- [react-konva RELEASE_NOTES](https://raw.githubusercontent.com/konvajs/react-konva/master/RELEASE_NOTES.md) — React 19.3 要件
- [Konva: Text editing in HTML5 canvas](https://konvajs.org/docs/sandbox/Editable_Text.html) — textarea オーバーレイ方式
- [MDN: EditContext API](https://developer.mozilla.org/docs/Web/API/EditContext_API) — 却下した代替
- [Tauri v2: Calling Rust from the Frontend](https://v2.tauri.app/develop/calling-rust/) — 生ボディ / `ipc::Response`
- [JUMPSEC Labs: Can Depix deobfuscate your data?](https://labs.jumpsec.com/can-depix-deobfuscate-your-data/) — ピクセル化文字の復元リスク
- [The Hacker News: tool can retrieve pixelated text](https://thehackernews.com/2022/02/this-new-tool-can-retrieve-pixelated.html)
- [Chromium HTMLCanvasElement.cpp](https://chromium.googlesource.com/chromium/blink/+/HEAD/Source/core/html/HTMLCanvasElement.cpp) — canvas サイズ上限
