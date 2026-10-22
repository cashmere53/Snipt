# Brief: capture-library

## Problem
撮った画像が消えたり、注釈を付けた後で直せなかったりすると、手順書づくりなどで同じ撮影をやり直すことになる。

## Current State
新規プロジェクト。コードは無い。

## Desired Outcome
撮影した画像は設定した保存先に自動保存され、履歴一覧から探して編集画面で開き直せる。注釈は元画像と分けて保存され、後から何度でも直せる。注釈を合成した画像も同じ場所に保存される。

## Approach
キャプチャ1件を「元画像 + 注釈データ + 合成済み画像 + メタデータ」の単位で保存する。保存形式はシンプルなファイルの組み合わせとし、アプリ外からも合成済み PNG を直接使えるようにする。

## Scope
- **In**: キャプチャの自動保存、注釈データの保存と読み込み、合成済み画像の保存、履歴一覧(サムネイル、撮影日時、新しい順)、履歴からの再編集・コピー・削除・エクスプローラーで表示
- **Out**: 撮影処理、注釈の描画と編集操作、クラウド同期

## Boundary Candidates
- 保存形式とファイル配置
- 履歴一覧 UI
- 保存先変更時の扱い

## Out of Boundary
- 画像の取得(screen-capture)
- 注釈の編集操作(annotation-editor)

## Upstream / Downstream
- **Upstream**: app-shell(保存先フォルダ設定)
- **Downstream**: screen-capture(撮影結果の保存)、annotation-editor(注釈の読み書き)、将来の scroll-capture

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: screen-capture, annotation-editor

## Constraints
- 保存形式は将来の機能追加(新しい注釈ツール)で既存データが開けなくならないよう、バージョンを持つこと。
