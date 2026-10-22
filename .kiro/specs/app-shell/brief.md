# Brief: app-shell

## Problem
ScreenPresso の利用者は、いつでもキー一発で撮影を始めたい。アプリを毎回起動する手間や、設定が保存されない不便があると使われなくなる。

## Current State
新規プロジェクト。コードは無い。

## Desired Outcome
Snipt が Windows ログイン時から常駐し、トレイアイコンとグローバルホットキーから各キャプチャを開始できる。ホットキーや保存先などの設定を画面から変更でき、次回起動時も保持される。

## Approach
Tauri v2 のトレイと global-shortcut プラグインを使う。設定は Rust 側で読み書きし、各機能に提供する。

## Scope
- **In**: トレイ常駐とメニュー、グローバルホットキー登録(既定: PrintScreen = 範囲、ほかは設定で変更)、ホットキー登録失敗時の通知、設定画面(ホットキー、保存先フォルダ、Windows 起動時に自動起動)、設定の永続化、多重起動防止、終了
- **Out**: キャプチャそのもの、画像の保存処理、注釈編集

## Boundary Candidates
- トレイとメニュー
- ホットキーの登録・競合処理
- 設定の保持と変更通知

## Out of Boundary
- 各キャプチャモードの実行内容(screen-capture)
- 保存ファイルの形式と履歴(capture-library)

## Upstream / Downstream
- **Upstream**: なし
- **Downstream**: screen-capture(ホットキー起点の撮影開始)、capture-library(保存先フォルダ設定)

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: screen-capture, capture-library

## Constraints
- Windows 11 では PrintScreen が既定で Snipping Tool に割り当てられているため、登録に失敗する場合がある。その場合の案内が必要。
