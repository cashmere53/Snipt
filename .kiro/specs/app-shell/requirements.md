# Requirements Document

## Project Description (Input)
Snipt を Windows 上で常駐させ、トレイアイコンとグローバルホットキーから各キャプチャを開始できるようにする。ホットキー・保存先フォルダ・自動起動などの設定を画面から変更でき、次回起動時も保持される。詳細は `brief.md` を参照。

## Introduction
app-shell は Snipt の「入口」です。常駐、ホットキー、設定、起動と終了を扱い、キャプチャや保存そのものは扱いません。利用者がいつでもキー一発で撮影を始められ、自分の環境に合わせて設定を変えられることを目的とします。

## Boundary Context
- **In scope**: トレイアイコンとメニュー、グローバルホットキーの登録と競合時の通知、設定画面、設定の永続化、Windows 起動時の自動起動、多重起動防止、終了
- **Out of scope**: キャプチャの実行内容(screen-capture)、画像の保存と履歴(capture-library)、注釈編集(annotation-editor)
- **Adjacent expectations**: ホットキーやトレイメニューで撮影を指示すると、screen-capture が該当するキャプチャを開始する。保存先フォルダの設定値は capture-library が使用する。

## Requirements

### Requirement 1: トレイ常駐
**Objective:** As a 利用者, I want Snipt が通知領域に常駐していること, so that 必要なときにすぐ撮影や設定を呼び出せる

#### Acceptance Criteria
1. When Snipt が起動したとき, the Snipt shall 通知領域にトレイアイコンを表示し、メインウィンドウは表示せずに待機する
2. When 利用者がトレイアイコンを右クリックしたとき, the Snipt shall 「範囲キャプチャ」「ウィンドウキャプチャ」「全画面キャプチャ」「履歴を開く」「設定」「終了」を含むメニューを表示する
3. When 利用者がトレイメニューのキャプチャ項目を選んだとき, the Snipt shall 対応するキャプチャの開始を要求する
4. When 利用者がトレイアイコンをダブルクリックしたとき, the Snipt shall 履歴一覧を開く
5. The Snipt shall 待機中にトレイアイコンのツールチップでアプリ名と範囲キャプチャのホットキーを示す

### Requirement 2: グローバルホットキー
**Objective:** As a 利用者, I want どのアプリを操作中でもキー一発で撮影を始めたい, so that 撮りたい瞬間を逃さない

#### Acceptance Criteria
1. The Snipt shall 初期設定として、範囲キャプチャに PrintScreen、ウィンドウキャプチャに Ctrl+PrintScreen、全画面キャプチャに Shift+PrintScreen を割り当てる
2. When 割り当て済みのホットキーが押されたとき, the Snipt shall 前面のアプリに関係なく対応するキャプチャの開始を要求する
3. If ホットキーの登録が他のアプリや Windows の機能と競合して失敗したとき, then the Snipt shall 失敗したキーと操作名を通知し、設定画面から別のキーを割り当てられることを案内する
4. If PrintScreen の登録が Windows の「PrintScreen キーで画面キャプチャを開く」設定により失敗したとき, then the Snipt shall その Windows 設定を無効にする方法を通知内で案内する
5. While キャプチャの操作中である間, the Snipt shall ほかのキャプチャ用ホットキーの押下を無視する

### Requirement 3: 設定画面
**Objective:** As a 利用者, I want ホットキーや保存先を自分の環境に合わせて変えたい, so that ほかのアプリと衝突せず、好きな場所に画像を残せる

#### Acceptance Criteria
1. When 利用者がトレイメニューの「設定」を選んだとき, the Snipt shall 設定画面を開く(既に開いている場合は前面に出す)
2. The Snipt shall 設定画面で、3種類のキャプチャそれぞれのホットキー、全画面キャプチャの対象(カーソルのあるモニタ / 全モニタ)、保存先フォルダ、Windows ログイン時の自動起動の有無を変更できるようにする
3. When 利用者がホットキー入力欄でキーの組み合わせを押したとき, the Snipt shall その組み合わせを入力欄に表示する
4. If 利用者が同じキーの組み合わせを複数の操作に割り当てようとしたとき, then the Snipt shall 重複していることを表示し、その変更を確定させない
5. If 新しいホットキーの登録に失敗したとき, then the Snipt shall 失敗したことを設定画面に表示し、直前に有効だったホットキーを維持する
6. When 利用者が保存先フォルダを変更したとき, the Snipt shall フォルダ選択ダイアログで選ばれたフォルダを設定値として表示する
7. If 選ばれた保存先フォルダに書き込めないとき, then the Snipt shall 書き込めない旨を表示し、変更を確定させない
8. When 利用者が設定を確定したとき, the Snipt shall アプリを再起動せずに新しい設定を反映する

### Requirement 4: 設定の保持
**Objective:** As a 利用者, I want 変更した設定が次回も残っていること, so that 毎回設定し直さなくて済む

#### Acceptance Criteria
1. When 設定が確定されたとき, the Snipt shall 設定を永続化する
2. When Snipt が起動したとき, the Snipt shall 前回確定した設定を読み込んで適用する
3. If 保存された設定が読み込めないか壊れているとき, then the Snipt shall 初期設定で起動し、設定を初期化したことを通知する
4. The Snipt shall 初回起動時の保存先フォルダを、利用者のピクチャフォルダ内の「Snipt」フォルダとする

### Requirement 5: 自動起動
**Objective:** As a 利用者, I want Windows にログインしたら Snipt が自動で立ち上がってほしい, so that 起動し忘れて撮れないことがない

#### Acceptance Criteria
1. Where 自動起動が有効な場合, the Snipt shall Windows ログイン時にトレイ常駐状態で起動する
2. When 利用者が自動起動を無効にしたとき, the Snipt shall 次回以降のログイン時に自動起動しない
3. The Snipt shall 初期設定では自動起動を有効にする

### Requirement 6: 多重起動防止
**Objective:** As a 利用者, I want Snipt が二重に起動しないこと, so that ホットキーの奪い合いやトレイアイコンの重複が起きない

#### Acceptance Criteria
1. If Snipt が既に起動している状態で再度起動されたとき, then the Snipt shall 新しいプロセスを常駐させずに終了し、既存の Snipt が「既に起動しています」と通知する

### Requirement 7: 終了
**Objective:** As a 利用者, I want Snipt を確実に終了させたい, so that 不要なときにホットキーを解放できる

#### Acceptance Criteria
1. When 利用者がトレイメニューの「終了」を選んだとき, the Snipt shall 登録したホットキーをすべて解放し、開いている画面をすべて閉じてから終了する
2. When 利用者が設定画面・履歴一覧・編集画面を閉じたとき, the Snipt shall 終了せずにトレイ常駐を続ける
