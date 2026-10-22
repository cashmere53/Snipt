# Requirements Document

## Project Description (Input)
撮影した画像を自動保存し、履歴一覧から探して編集画面で開き直せるようにする。注釈は元画像と分けて保存して何度でも直せるようにし、注釈を合成した画像も保存する。詳細は `brief.md` を参照。

## Introduction
capture-library は撮影結果と注釈の「保管庫」です。キャプチャ1件を、元画像・注釈・注釈を合成した画像のまとまりとして保存し、履歴一覧で管理します。撮影(screen-capture)と編集(annotation-editor)は、どちらもこの保管庫を通して画像と注釈をやり取りします。

## Boundary Context
- **In scope**: キャプチャの自動保存、注釈の保存と読み込み、合成済み画像の保存、履歴一覧、履歴からの再編集・コピー・削除・エクスプローラー表示、保存失敗時の扱い
- **Out of scope**: 画像の取得(screen-capture)、注釈の描画と編集操作(annotation-editor)、クラウド同期
- **Adjacent expectations**: 保存先フォルダは app-shell の設定値を使う。撮影直後のクリップボードコピーは screen-capture が行う。

## Requirements

### Requirement 1: キャプチャの自動保存
**Objective:** As a 利用者, I want 撮った画像が自動で残ること, so that 保存し忘れて撮り直すことがない

#### Acceptance Criteria
1. When キャプチャ結果を受け取ったとき, the Snipt shall 保存先フォルダに元画像を PNG 形式で保存し、それを1件のキャプチャとして履歴に登録する
2. The Snipt shall 保存するファイル名に撮影日時(年月日・時分秒)を含め、同じ秒に複数撮影しても名前が重複しないようにする
3. The Snipt shall キャプチャ1件ごとに、撮影日時・撮影方法(範囲 / ウィンドウ / 全画面)・画像サイズ(ピクセル)を記録する
4. Where 撮影方法がウィンドウキャプチャの場合, the Snipt shall 撮影したウィンドウのタイトルも記録する
5. If 保存先フォルダが存在しないとき, then the Snipt shall フォルダを作成してから保存する
6. If 保存に失敗したとき(容量不足・書き込み権限なしなど), then the Snipt shall 失敗した理由を通知し、撮影した画像は編集画面で開けるように保持する

### Requirement 2: 注釈と合成済み画像の保存
**Objective:** As a 利用者, I want 注釈を後から直せる形で残したい, so that 矢印の位置や文言を何度でも修正できる

#### Acceptance Criteria
1. When 編集画面から注釈の保存を要求されたとき, the Snipt shall 注釈を元画像とは別に保存し、元画像を変更しない
2. When 注釈が保存されたとき, the Snipt shall 元画像に注釈を合成した画像を PNG 形式で保存し、既存の合成済み画像があれば置き換える
3. The Snipt shall 合成済み画像を、エクスプローラーからそのまま開いたり他のアプリに添付したりできる通常の画像ファイルとして保存する
4. When 履歴のキャプチャを編集画面で開くよう要求されたとき, the Snipt shall 元画像と保存済みの注釈をそろえて提供する
5. The Snipt shall 注釈データに形式のバージョンを含め、以前のバージョンの Snipt で保存した注釈を新しいバージョンでも開けるようにする
6. If 注釈データが読み込めないとき, then the Snipt shall 元画像だけを開けるようにし、注釈を読み込めなかったことを通知する

### Requirement 3: 履歴一覧
**Objective:** As a 利用者, I want 過去に撮った画像を一覧で探したい, so that 以前の撮影結果をすぐ再利用できる

#### Acceptance Criteria
1. When 履歴一覧を開くよう要求されたとき, the Snipt shall 保存先フォルダ内のキャプチャをサムネイル付きで新しい順に表示する
2. The Snipt shall 各キャプチャに撮影日時と画像サイズを表示し、注釈付きのものは合成済み画像をサムネイルに使う
3. When 新しいキャプチャが保存されたとき, the Snipt shall 開いている履歴一覧の先頭にそのキャプチャを追加する
4. When 利用者が履歴のキャプチャをダブルクリックしたとき, the Snipt shall そのキャプチャを編集画面で開く
5. When 利用者が履歴のキャプチャで「コピー」を選んだとき, the Snipt shall 合成済み画像(注釈が無い場合は元画像)をクリップボードにコピーする
6. When 利用者が履歴のキャプチャで「エクスプローラーで表示」を選んだとき, the Snipt shall そのファイルを選択した状態でエクスプローラーを開く
7. If 履歴に登録されたファイルが Snipt の外で削除されていたとき, then the Snipt shall エラーを出さずにそのキャプチャを一覧から除く
8. While 履歴が 1,000 件ある状態でも, the Snipt shall 履歴一覧を開いてから最初のサムネイル群を 1 秒以内に表示する

### Requirement 4: 削除
**Objective:** As a 利用者, I want 不要なキャプチャを消したい, so that 履歴と保存先が散らからない

#### Acceptance Criteria
1. When 利用者が履歴で1件以上のキャプチャを選んで削除を指示したとき, the Snipt shall 件数を示した確認を表示する
2. When 利用者が削除を確定したとき, the Snipt shall 対象キャプチャの元画像・注釈・合成済み画像をごみ箱へ移動し、一覧から除く
3. If 削除対象のキャプチャを編集画面で開いているとき, then the Snipt shall その編集画面を閉じてから削除する

### Requirement 5: 保存先の変更
**Objective:** As a 利用者, I want 保存先を変えても混乱しないこと, so that 過去の画像を見失わない

#### Acceptance Criteria
1. When 保存先フォルダの設定が変更されたとき, the Snipt shall 以降のキャプチャを新しいフォルダに保存する
2. When 保存先フォルダの設定が変更されたとき, the Snipt shall 既存のキャプチャを移動せず、履歴一覧を新しいフォルダの内容で表示し直す
