# Brief: annotation-editor

## Problem
撮った画面に説明を加えるには、矢印や枠、文字を素早く描けることが必要。描いた後の位置調整や取り消しができないと手戻りが多い。

## Current State
新規プロジェクト。コードは無い。

## Desired Outcome
撮影後に開く編集画面で、矢印・四角・テキスト・ぼかし・切り抜き・番号付きマーカーを付けられる。描いた注釈は選択して移動・変形・削除でき、Undo/Redo できる。結果をクリップボードにコピーしたりファイルに書き出したりでき、注釈は後から再編集できるように保存される。

## Approach
WebView 上の Canvas ライブラリ(Konva.js)で注釈をオブジェクトとして扱う。注釈データは capture-library の形式で保存し、書き出し時に元解像度で合成する。

## Scope
- **In**: 矢印、四角、テキスト(日本語入力)、ぼかし / モザイク、切り抜き、番号付きマーカー(自動採番)、色・線の太さ・文字サイズの選択、選択・移動・リサイズ・削除、Undo/Redo、拡大縮小表示、クリップボードへのコピー、名前を付けて保存、注釈の自動保存
- **Out**: フリーハンド描画、図形ライブラリ、画像の合成・複数画像の結合(必要になったら別 spec)

## Boundary Candidates
- 注釈モデル(図形データ、Undo/Redo)
- ツール群と操作 UI
- 書き出し(合成)

## Out of Boundary
- 保存形式・履歴(capture-library)
- 撮影(screen-capture)

## Upstream / Downstream
- **Upstream**: capture-library(画像と注釈の読み書き)
- **Downstream**: なし

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: capture-library, screen-capture

## Constraints
- 書き出し画像は元画像と同じ解像度(物理ピクセル)であること
- ぼかし / モザイクは書き出し後の画像から元の内容を復元できないこと
