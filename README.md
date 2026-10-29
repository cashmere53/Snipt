# Snipt

Snipt は Windows 向けのスクリーンショット + 注釈ツールです。[ScreenPresso](https://www.screenpresso.com/) の代替として、ホットキー一発で画面を撮り、矢印・四角・テキストなどの注釈を付けて、すぐにクリップボードやファイルで共有できることを目指しています。

> [!NOTE]
> 開発中です。4 つの spec(app-shell / capture-library / screen-capture / annotation-editor)のコード実装は揃っており、残りは実機(Windows)での E2E・性能確認です。

## 主な機能

- **常駐 + グローバルホットキー**: タスクトレイに常駐し、ホットキーから即座に撮影
- **3 種類のキャプチャ**: 範囲(1 つのモニタ内)/ ウィンドウ / 全画面(既定はカーソルのあるモニタ、設定で全モニタも可)
- **撮影直後の共有**: クリップボードへ自動コピーし、編集画面を自動で開く
- **注釈エディタ**: 矢印、四角、テキスト、ぼかし(モザイク)、切り抜き、番号付きマーカー。Undo / Redo、コピー、名前を付けて保存
- **自動保存と履歴**: 保存フォルダへ自動保存し、履歴一覧から開き直せる。注釈は元画像と分けて保存するため、何度でも描き直せる
- **マルチモニタ・混在 DPI 対応**: 座標は物理ピクセルで統一(Per-Monitor DPI Aware v2)

動画 / GIF 録画、macOS / Linux 対応、クラウドへのアップロードは現時点では対象外です。スクロールキャプチャは今後のマイルストーンとして予定しています。

## 既定のホットキー

| 操作                 | キー                |
| -------------------- | ------------------- |
| 範囲キャプチャ       | `PrintScreen`       |
| ウィンドウキャプチャ | `Ctrl+PrintScreen`  |
| 全画面キャプチャ     | `Shift+PrintScreen` |

ホットキーは設定画面から変更できます。

### エディタのショートカット

| 操作                                 | キー                                      |
| ------------------------------------ | ----------------------------------------- |
| 選択 / 矢印 / 四角 / テキスト        | `V` / `A` / `R` / `T`                     |
| ぼかし / 切り抜き / 番号付きマーカー | `B` / `C` / `N`                           |
| 元に戻す / やり直し                  | `Ctrl+Z` / `Ctrl+Y`(`Ctrl+Shift+Z`)       |
| 画像をコピー                         | `Ctrl+C`                                  |
| 名前を付けて保存                     | `Ctrl+Shift+S`                            |
| 拡大 / 縮小 / 全体表示 / 等倍        | `Ctrl++` / `Ctrl+-` / `Ctrl+0` / `Ctrl+1` |
| 選択中の図形を削除                   | `Delete` / `Backspace`                    |

## 保存先

既定の保存フォルダは `ピクチャ\Snipt` です(設定で変更可能。変更しても既存のファイルは移動しません)。

```text
<保存フォルダ>/
├── Snipt_<id>.png             # 元画像
├── Snipt_<id>_annotated.png   # 注釈を合成した画像
└── .snipt/                    # 履歴インデックス、注釈データ(JSON)、サムネイル
```

`<id>` は撮影日時から作る `YYYYMMDD_HHMMSS[_NN]` 形式です。

## 動作環境

- Windows 10 1903 以降 / Windows 11(x64)
- Microsoft Edge WebView2 Runtime

## 開発

### 技術スタック

- **コア(Rust)**: [Tauri v2](https://v2.tauri.app/)。キャプチャは `windows` crate で Windows API(GDI / Windows.Graphics.Capture)を直接呼び出す
- **UI(WebView)**: React + TypeScript、注釈キャンバスは [Konva.js](https://konvajs.org/) / react-konva、状態管理は zustand
- **ツール**: Vite、Vitest、oxlint + oxfmt、rustfmt + clippy

### セットアップ

Node.js・pnpm・Rust のバージョンは [`mise.toml`](mise.toml) で固定しています。ツールは個別にインストールせず [mise](https://mise.jdx.dev/) で揃えてください。

前提:

- [mise](https://mise.jdx.dev/)
- Visual Studio Build Tools(MSVC)
- WebView2 Runtime(Windows 10/11 には標準搭載)

```bash
mise install       # Node.js / pnpm / Rust(clippy・rustfmt 付き)を導入
mise run install   # フロントエンドの依存関係をインストール
```

### よく使うコマンド

| コマンド                | 内容                                                            |
| ----------------------- | --------------------------------------------------------------- |
| `mise run dev`          | 開発モードで起動(`pnpm tauri dev`)                              |
| `mise run build`        | インストーラをビルド(`pnpm tauri build`)                        |
| `mise run check`        | CI と同じチェックをすべて実行(`check:web` + `check:rust`)       |
| `mise run check:web`    | フロントエンドの型検査・lint・フォーマット確認・テスト          |
| `mise run check:rust`   | Rust の `cargo fmt --check`・`clippy -D warnings`・`cargo test` |
| `mise run perf:library` | capture-library の性能測定(release ビルド)                      |
| `mise run e2e`          | Windows 実機の E2E テスト(E2E 用ビルド + tauri-driver)          |

`mise tasks` で一覧を確認できます。CI(GitHub Actions、`windows-latest`)は `jdx/mise-action` で同じツールを揃え、`check:web`・`check:rust` と `pnpm tauri build --no-bundle` を実行します。

実機のモニタやデスクトップが必要なテスト(`src-tauri/src/platform/windows/` 内)は `#[ignore]` になっており、Windows 実機で `cargo test -- --ignored` を指定して実行します。

### E2E テスト(Windows 実機)

`e2e/` に、実際のアプリを [tauri-driver](https://v2.tauri.app/develop/tests/webdriver/) と WebdriverIO で操作するテストがあります(Vitest で実行)。デスクトップに実際のウィンドウを開き、キー入力とクリップボードも使うため、CI では実行しません。

```bash
mise run e2e   # E2E 用ビルド(--features e2e)→ テスト実行
```

- 実行前に Snipt を終了してください(多重起動防止のため、起動中だとテストを中止します)。実行中はキーボードとマウスに触れないでください。
- tauri-driver は `e2e` タスクの実行時に mise が入れます。msedgedriver は、インストール済みの WebView2 Runtime と同じ版を初回に `e2e/.drivers/` へダウンロードします。
- E2E 用ビルドは `e2e` feature 付きのデバッグビルド(出力先は `src-tauri/target/e2e/`)で、通常のビルドとは次の 2 点が違います。設定を `SNIPT_E2E_CONFIG_DIR` から読むこと、起動時に履歴ウィンドウを開くこと(tauri-driver が接続するため)です。利用者の設定・保存フォルダ・WebView2 のデータには触れません。
- テスト用キャプチャは、一時フォルダの保存先へ `Snipt_<id>.png`(`snipt:meta` 付きの単色 PNG、`e2e/support/png.ts`)として置きます。起動時に履歴へ取り込まれます。テスト用の設定は撮影ホットキーを `Ctrl+Alt+Shift+F7` / `F8` / `F9` にしています。
- 性能は `e2e/specs/editor-perf.e2e.ts` で計測します(11520×2160 の画像。開く要求からウィンドウ表示まで ≤ 700 ms、Ctrl+C から合成 PNG の要求まで ≤ 2.5 秒、ドラッグは 60fps 付近)。結果は `E2E perf:` の行に出ます。Ctrl+C からコピー完了(約 8.5 秒)、クリップボードに画像が出るまで(約 8.9 秒)はデバッグ版の E2E ビルドで出力のみし、アサートしていません。その時間の大半は Rust の `copy_png`(PNG のデコードと DIB への変換)で、合成 PNG 生成の計測範囲の外です。リリースビルドの値は計測していません。
- `SNIPT_E2E_KEEP=1` を付けると、終了後も一時フォルダ(設定・保存先・WebView2 のデータ)を残します。
- 終了の E2E は、起動中のアプリへ `snipt.exe --e2e-quit` を渡すと、トレイの「終了」と同じ経路で終了します(E2E 用ビルドだけの動作。トレイのメニューは操作できないため)。終了確認は Windows の UI Automation で読み、ボタンを押します。

### ディレクトリ構成

機能(feature)単位で分割し、Rust 側と WebView 側で同じ機能名のモジュールを持ちます。機能名は `.kiro/specs/` の spec 名に揃えています。

```text
.
├── src/                       # WebView(React + TypeScript)
│   ├── app/                   # ウィンドウごとの画面振り分け
│   ├── features/
│   │   ├── app-shell/         # 設定画面
│   │   ├── capture-library/   # 履歴一覧
│   │   ├── screen-capture/    # 範囲選択オーバーレイ
│   │   └── annotation-editor/ # 注釈エディタ
│   ├── shared/ipc/            # Rust と共有する IPC の型
│   └── test/                  # テスト用セットアップ
├── src-tauri/                 # Rust コア(Tauri)
│   ├── src/
│   │   ├── app_shell/         # トレイ常駐、ホットキー、設定、ライフサイクル
│   │   ├── capture_library/   # 保存、履歴インデックス、サムネイル、フォルダ監視
│   │   ├── screen_capture/    # キャプチャの調停、オーバーレイ、撮影後処理
│   │   ├── annotation_editor/ # エディタのウィンドウ管理、セッション、書き出し
│   │   ├── platform/windows/  # Windows API の薄いアダプタ
│   │   └── shared/            # 機能間で共有する型
│   ├── capabilities/          # ウィンドウごとの Tauri 権限
│   └── tauri.conf.json
├── .kiro/
│   ├── steering/              # プロジェクト全体の方針(product / tech / structure / roadmap)
│   └── specs/                 # 機能ごとの spec
├── .claude/skills/            # cc-sdd の skill(kiro-*)
└── mise.toml                  # ツールバージョンとタスク
```

Windows API への依存は `platform/windows/` に閉じ込め、機能ロジックは OS 非依存の型で書いてテストできるようにしています。

## 開発フロー(spec 駆動開発)

[cc-sdd](https://github.com/gotalab/cc-sdd) による Kiro 方式の spec 駆動開発で進めています。実装の前に、機能ごとに要件 → 設計 → タスクを文書化し、各フェーズで人がレビュー・承認します。

- **Steering**(`.kiro/steering/`): プロダクト概要、技術スタック、構成方針、ロードマップ
- **Specs**(`.kiro/specs/<feature>/`): `requirements.md` / `design.md` / `tasks.md` など

| spec                | 内容                                                                                                   | 依存                       |
| ------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------- |
| `app-shell`         | 常駐(トレイ)、グローバルホットキー、設定画面と設定の保持、多重起動防止                                 | なし                       |
| `capture-library`   | 撮影画像と注釈の自動保存、履歴一覧、削除、再編集用の保存形式                                           | app-shell                  |
| `screen-capture`    | 範囲 / ウィンドウ / 全画面キャプチャ、範囲選択オーバーレイ、撮影後のコピーと保存・編集画面への引き渡し | app-shell, capture-library |
| `annotation-editor` | 注釈編集、Undo / Redo、コピー / 書き出し                                                               | capture-library            |

Claude Code で使う主なコマンド:

```text
/kiro-spec-init "説明"          # spec を作成
/kiro-spec-requirements <spec>  # 要件
/kiro-spec-design <spec>        # 設計
/kiro-spec-tasks <spec>         # タスク分解
/kiro-impl <spec> [tasks]       # 実装
/kiro-spec-status <spec>        # 進捗確認
```

詳しくは [CLAUDE.md](CLAUDE.md) を参照してください。実装はタスクグループごとにブランチを切り、1 グループ 1 PR で進めます。
