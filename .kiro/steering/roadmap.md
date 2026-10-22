# Roadmap

## Overview

Snipt は Windows 専用の ScreenPresso 代替アプリです。常駐してホットキーで画面を撮り、撮影直後にクリップボードへコピーしつつ編集画面を開き、矢印・四角・テキストなどの注釈を付けて共有できるようにします。撮影した画像は自動保存して履歴から再編集できます。

技術構成は Rust + Tauri v2 で、キャプチャと OS 連携は Rust から Windows API を直接呼び、範囲選択オーバーレイと注釈エディタは WebView(React + Konva.js)で作ります。

## Approach Decision
- **Chosen**: Rust + Tauri v2(キャプチャは Rust / 編集 UI は WebView)
- **Why**: 注釈 UI の開発効率、日本語 IME の安定性、配布サイズ(5〜10MB)の両立
- **Rejected alternatives**:
  - C# + WPF: 有力な次点。配布サイズが大きい(自己完結 60MB〜)
  - C# + WinUI 3: トレイ非対応、配布に癖
  - Rust ネイティブ (egui / Slint): IME と編集 UI の自作量
  - Electron: サイズとキャプチャの制約

## Scope
- **In**: 常駐・ホットキー・設定、範囲 / ウィンドウ / 全画面キャプチャ、撮影後のクリップボードコピーと編集画面表示、注釈(矢印・四角・テキスト・ぼかし・切り抜き・番号付きマーカー)、自動保存・履歴・再編集
- **Out**: 動画 / GIF 録画、macOS / Linux、クラウドアップロード

## Constraints
- Windows 10 1903 以降 / Windows 11、x64
- マルチモニタ・モニタごとに異なる拡大率(混在 DPI)で正しく動くこと
- 日本語 UI、日本語 IME でテキスト注釈を入力できること

## Boundary Strategy
- **Why this split**: 「常駐と設定」「画像の保管」「撮る」「描く」は変更理由が別々で、それぞれ独立に実装・テストできる。保管(capture-library)をキャプチャと編集の共通の受け皿にすることで、両者が直接依存しない。
- **Shared seams to watch**:
  - キャプチャ結果の受け渡し形式(screen-capture → capture-library)
  - 注釈ドキュメントの保存形式(annotation-editor ↔ capture-library)
  - 設定項目の所有(app-shell が保持し、各機能が読む)
  - 撮影後フロー(コピー → 保存 → 編集画面表示)の順序と責任

## Specs (dependency order)
- [ ] app-shell -- 常駐(トレイ)、グローバルホットキー、設定画面と設定の保持、多重起動防止。Dependencies: none
- [ ] capture-library -- 撮影画像と注釈の自動保存、履歴一覧、削除、再編集用の保存形式。Dependencies: app-shell
- [ ] screen-capture -- 範囲 / ウィンドウ / 全画面キャプチャ、範囲選択オーバーレイ、撮影後のクリップボードコピーと保存・編集画面への引き渡し。Dependencies: app-shell, capture-library
- [ ] annotation-editor -- 注釈編集(矢印・四角・テキスト・ぼかし・切り抜き・番号付きマーカー)、Undo/Redo、コピー / 書き出し。Dependencies: capture-library

## Future Milestones
- scroll-capture -- スクロールキャプチャ(縦に長いページやウィンドウを自動スクロールしてつなげる)。上記 4 spec の完了後に `/kiro-discovery` で brief を作成する。Dependencies: screen-capture, capture-library
