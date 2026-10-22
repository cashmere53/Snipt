# Technology Stack

## Architecture

単一の常駐デスクトップアプリ。Rust 側(Tauri v2 のコアプロセス)がキャプチャ・OS 連携・ファイル保存を担い、WebView 側(TypeScript)が範囲選択オーバーレイと注釈編集 UI を担います。両者は Tauri の command / event で通信します。

- **Rust (コア)**: トレイ常駐、グローバルホットキー、Windows API によるキャプチャ、クリップボード、保存・履歴、設定
- **WebView (UI)**: 範囲選択オーバーレイ、注釈エディタ、履歴一覧、設定画面

## Core Technologies

- **Language**: Rust (stable, edition 2021 以降) / TypeScript (strict)
- **Framework**: Tauri v2, React
- **Runtime**: Microsoft Edge WebView2 (Windows 10/11 標準搭載)
- **Target OS**: Windows 10 1903 以降 / Windows 11 (x64)

## Key Libraries

- `windows` crate: Win32 / WinRT API (Windows.Graphics.Capture, DXGI, GDI, DPI)
- Tauri 公式プラグイン: global-shortcut, clipboard-manager, (tray は Tauri コア)
- Konva.js / react-konva: 注釈キャンバス(図形の選択・変形・書き出し)

## Development Standards

### Type Safety
- TypeScript `strict: true`、`any` 禁止
- Rust は `clippy -D warnings` を通す

### Code Quality
- Rust: `rustfmt` / `clippy`
- TypeScript: ESLint + Prettier

### Testing
- Rust: `cargo test`(キャプチャ以外のロジック、座標変換、保存形式)
- TypeScript: Vitest(注釈モデル、Undo/Redo、シリアライズ)
- Windows API 依存部分は薄いアダプタに閉じ込め、ロジックをテスト可能に保つ

## Development Environment

### Required Tools
- [mise](https://mise.jdx.dev/): Node.js・pnpm・Rust(clippy / rustfmt 付き)のバージョンは `mise.toml` で固定し、`mise install` で揃える。ツールを直接インストールしない
- Windows 10/11 + WebView2 Runtime + Visual Studio Build Tools(MSVC)

### Common Commands
```bash
# Setup: mise install && mise run install
# Dev:   mise run dev
# Build: mise run build
# Check: mise run check   # CI と同じ lint / format / test(check:web, check:rust)
```

## Key Technical Decisions

- **Tauri を選択**: 注釈 UI を Web の Canvas ライブラリで速く作れ、日本語 IME も安定。配布サイズが小さい。次点は C# + WPF。
- **キャプチャは Rust で直接 Windows API**: WebView のキャプチャ API は使わない。
- **オーバーレイはモニタごとに事前生成した非表示ウィンドウ**: ホットキーからの表示遅延を避け、混在 DPI に対応するため。
- **座標は物理ピクセルで統一**: Per-Monitor DPI Aware v2 前提。
- **画像の受け渡しはファイル / バイナリ IPC**: base64 JSON は使わない。
- **注釈は元画像と分離して保存**: 再編集を可能にし、書き出し時のみ合成する。

---
_Document standards and patterns, not every dependency_
