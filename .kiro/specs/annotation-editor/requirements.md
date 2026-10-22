# Requirements Document

## Project Description (Input)
撮影後に開く編集画面で、矢印・四角・テキスト・ぼかし・切り抜き・番号付きマーカーを付けられるようにする。注釈は選択して移動・変形・削除でき、Undo/Redo できる。結果はクリップボードへのコピーやファイル書き出しができ、注釈は後から再編集できるよう保存する。詳細は `brief.md` を参照。

## Introduction
annotation-editor は Snipt の「描く」部分です。キャプチャ画像の上に注釈を重ね、共有用の画像を作ります。注釈は元画像を壊さないオブジェクトとして扱い、保存と読み込みは capture-library を通します。

## Boundary Context
- **In scope**: 編集画面の表示と拡大縮小、6種類の注釈ツール、注釈の選択・移動・変形・削除、スタイル(色・太さ・文字サイズ)、Undo/Redo、クリップボードへのコピー、名前を付けて保存、注釈の自動保存
- **Out of scope**: 保存形式と履歴(capture-library)、撮影(screen-capture)、フリーハンド描画、図形ライブラリ、複数画像の結合
- **Adjacent expectations**: 元画像と既存の注釈は capture-library から受け取り、注釈の保存も capture-library に要求する。

## Requirements

### Requirement 1: 編集画面の表示
**Objective:** As a 利用者, I want 撮った画像をすぐ編集画面で見たい, so that そのまま注釈を付け始められる

#### Acceptance Criteria
1. When キャプチャを編集画面で開くよう要求されたとき, the Snipt shall 元画像と保存済みの注釈を表示した編集画面を前面に開く
2. The Snipt shall 画像が編集画面より大きい場合は全体が収まる倍率で、小さい場合は 100% で表示する
3. When 利用者が拡大・縮小・100% 表示・全体表示を指示したとき, the Snipt shall 表示倍率を変更し、注釈の位置と画像の対応を保つ
4. When 既に編集画面で開いているキャプチャを再度開くよう要求されたとき, the Snipt shall 新しい画面を開かずに既存の編集画面を前面に出す
5. The Snipt shall キャプチャごとに別の編集画面を開けるようにする

### Requirement 2: 注釈の共通操作
**Objective:** As a 利用者, I want 描いた注釈を後から調整したい, so that 一度で正確に描けなくても仕上げられる

#### Acceptance Criteria
1. The Snipt shall ツールバーとキーボードショートカットで、選択・矢印・四角・テキスト・ぼかし・切り抜き・番号付きマーカーの各ツールを切り替えられるようにする
2. When 利用者が選択ツールで注釈をクリックしたとき, the Snipt shall その注釈を選択状態にし、移動と変形のためのハンドルを表示する
3. When 利用者が選択中の注釈をドラッグしたとき, the Snipt shall 注釈を移動する
4. When 利用者が選択中の注釈のハンドルをドラッグしたとき, the Snipt shall 注釈の大きさまたは端点を変更する
5. When 利用者が注釈を選択して Delete キーを押したとき, the Snipt shall その注釈を削除する
6. When 利用者が色・線の太さ・文字サイズを変更したとき, the Snipt shall 選択中の注釈に反映し、以降に描く注釈にも同じ設定を使う
7. The Snipt shall 前回使った色・線の太さ・文字サイズを次に開いた編集画面でも引き継ぐ

### Requirement 3: 矢印
**Objective:** As a 利用者, I want 注目してほしい場所を矢印で指したい, so that 見る人が迷わない

#### Acceptance Criteria
1. When 利用者が矢印ツールでドラッグしたとき, the Snipt shall ドラッグ開始点から終了点へ、終了点側に矢じりの付いた直線の矢印を描く
2. While Shift キーを押しながらドラッグしている間, the Snipt shall 矢印の角度を 45 度単位に制限する

### Requirement 4: 四角
**Objective:** As a 利用者, I want 画面の一部を枠で囲みたい, so that 該当箇所を明示できる

#### Acceptance Criteria
1. When 利用者が四角ツールでドラッグしたとき, the Snipt shall ドラッグ範囲に塗りつぶしの無い四角い枠を描く
2. While Shift キーを押しながらドラッグしている間, the Snipt shall 枠を正方形に制限する

### Requirement 5: テキスト
**Objective:** As a 利用者, I want 画像上に日本語でコメントを書きたい, so that 説明を画像だけで伝えられる

#### Acceptance Criteria
1. When 利用者がテキストツールで画像上をクリックしたとき, the Snipt shall その位置に入力欄を表示し、日本語入力(変換候補の表示を含む)で文字を入力できるようにする
2. The Snipt shall テキスト注釈で複数行の入力を受け付ける
3. When 利用者がテキスト注釈をダブルクリックしたとき, the Snipt shall その文字を再編集できる状態にする
4. When 利用者が入力欄の外をクリックするか Esc キーを押したとき, the Snipt shall 入力を確定する
5. If 確定したテキストが空のとき, then the Snipt shall そのテキスト注釈を作成しない
6. The Snipt shall テキスト注釈に背景色(なし / 白 / 選択色)を設定できるようにする

### Requirement 6: ぼかし / モザイク
**Objective:** As a 利用者, I want 個人情報などを隠したい, so that 安心して画像を共有できる

#### Acceptance Criteria
1. When 利用者がぼかしツールでドラッグしたとき, the Snipt shall ドラッグ範囲の元画像をモザイクで隠して表示する
2. The Snipt shall ぼかし注釈を移動・変形したとき、新しい範囲の元画像を隠すように表示を更新する
3. The Snipt shall 書き出した画像やコピーした画像から、ぼかした範囲の元の内容を復元できないようにする

### Requirement 7: 切り抜き
**Objective:** As a 利用者, I want 撮った後で不要な部分を切り落としたい, so that 撮り直さずに範囲を調整できる

#### Acceptance Criteria
1. When 利用者が切り抜きツールで範囲を指定して確定したとき, the Snipt shall 画像の表示範囲をその範囲に切り詰め、範囲外の注釈も表示しない
2. The Snipt shall 切り抜きを元画像の削除ではなく表示範囲の変更として扱い、Undo または切り抜きの解除で元の範囲に戻せるようにする
3. If 利用者が Esc キーを押したとき, then the Snipt shall 切り抜き範囲の指定を取り消す

### Requirement 8: 番号付きマーカー
**Objective:** As a 利用者, I want 操作手順に番号を振りたい, so that 手順書をすぐ作れる

#### Acceptance Criteria
1. When 利用者が番号付きマーカーツールで画像上をクリックしたとき, the Snipt shall その位置に番号入りの丸いマーカーを置く
2. The Snipt shall マーカーの番号を 1 から始め、置くたびに既存マーカーの最大番号に 1 を足した番号にする
3. When 利用者がマーカーを削除したとき, the Snipt shall 残りのマーカーの番号を変更しない

### Requirement 9: Undo / Redo
**Objective:** As a 利用者, I want 操作を取り消したりやり直したりしたい, so that 気軽に試せる

#### Acceptance Criteria
1. When 利用者が Ctrl+Z を押したとき, the Snipt shall 直前の注釈操作(追加・移動・変形・削除・スタイル変更・切り抜き)を取り消す
2. When 利用者が Ctrl+Y または Ctrl+Shift+Z を押したとき, the Snipt shall 取り消した操作をやり直す
3. The Snipt shall 少なくとも直近 100 回分の操作を取り消せるようにする

### Requirement 10: コピーと書き出し
**Objective:** As a 利用者, I want 仕上げた画像をすぐ貼り付けたり保存したりしたい, so that 共有までの手数が少ない

#### Acceptance Criteria
1. When 利用者が Ctrl+C を押すかツールバーの「コピー」を押したとき(テキスト入力中を除く), the Snipt shall 注釈を合成した画像をクリップボードにコピーする
2. When 利用者が「名前を付けて保存」を選んだとき, the Snipt shall 保存場所と形式(PNG / JPEG)を選ばせ、注釈を合成した画像を書き出す
3. The Snipt shall コピー・書き出しする画像を、切り抜き後の範囲について元画像と同じピクセル解像度で作る
4. If 書き出しに失敗したとき, then the Snipt shall 失敗した理由を表示し、編集内容を保持する

### Requirement 11: 注釈の自動保存
**Objective:** As a 利用者, I want 編集内容が勝手に残ってほしい, so that 保存し忘れて注釈を失わない

#### Acceptance Criteria
1. When 注釈が変更されてから 1 秒間操作が無かったとき, the Snipt shall 注釈と合成済み画像の保存を要求する
2. When 利用者が編集画面を閉じたとき, the Snipt shall 未保存の変更があれば保存を要求してから閉じる
3. If 注釈の保存に失敗したとき, then the Snipt shall 編集画面に保存失敗を表示し、編集画面を閉じる前に再試行するか破棄するかを選ばせる
