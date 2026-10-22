# Requirements Document

## Project Description (Input)
ホットキーやトレイから、範囲・ウィンドウ・全画面のいずれかを即座に撮れるようにする。撮影直後に画像をクリップボードへコピーし、保存したうえで編集画面を開く。マルチモニタと、モニタごとに拡大率が異なる環境でも正確に撮れること。詳細は `brief.md` を参照。

## Introduction
screen-capture は Snipt の「撮る」部分です。撮影の開始指示を受けて画面を取得し、範囲やウィンドウを選ばせ、結果を保管庫(capture-library)と編集画面(annotation-editor)に引き渡します。

## Boundary Context
- **In scope**: 範囲キャプチャ、ウィンドウキャプチャ、全画面キャプチャ、範囲 / ウィンドウ選択画面、マルチモニタ・混在拡大率への対応、撮影後のクリップボードコピー、保存の要求、編集画面を開く要求
- **Out of scope**: ホットキーとトレイメニュー(app-shell)、保存形式と履歴(capture-library)、注釈編集(annotation-editor)、スクロールキャプチャ(将来の scroll-capture)、動画、タイマー撮影
- **Adjacent expectations**: 撮影の開始指示は app-shell から届く。保存は capture-library が行い、注釈は annotation-editor が扱う。

## Requirements

### Requirement 1: 範囲キャプチャ
**Objective:** As a 利用者, I want 画面の一部をドラッグで選んで撮りたい, so that 必要な部分だけを共有できる

#### Acceptance Criteria
1. When 範囲キャプチャの開始が要求されたとき, the Snipt shall その時点の全モニタの表示内容を静止させ、すべてのモニタに範囲選択画面を表示する
2. The Snipt shall 範囲キャプチャの開始要求から範囲選択画面の表示までを 200 ミリ秒以内に完了する
3. While 範囲選択画面を表示している間, the Snipt shall 選択中の範囲以外を暗く表示し、選択範囲の幅と高さ(ピクセル)とカーソル付近の拡大表示を示す
4. When 利用者がドラッグを終えたとき, the Snipt shall 選択した範囲の画像をキャプチャ結果として確定する
5. The Snipt shall 範囲選択を1つのモニタ内に限定し、ドラッグがモニタの端を越えた場合は範囲をそのモニタの端で止める
6. If 利用者が Esc キーまたは右クリックを押したとき, then the Snipt shall 範囲選択画面を閉じ、何も保存・コピーせずに撮影を取り消す
7. If 選択された範囲の幅または高さが 1 ピクセル未満のとき, then the Snipt shall 撮影を確定せず、範囲選択を続ける

### Requirement 2: ウィンドウキャプチャ
**Objective:** As a 利用者, I want 特定のウィンドウだけをきれいに撮りたい, so that 余計な背景を切り取る手間が省ける

#### Acceptance Criteria
1. When ウィンドウキャプチャの開始が要求されたとき, the Snipt shall ウィンドウ選択画面を表示し、マウスカーソルの下にあるウィンドウを枠で強調する
2. When 利用者が強調中のウィンドウをクリックしたとき, the Snipt shall そのウィンドウの画像をキャプチャ結果として確定する
3. The Snipt shall 選択したウィンドウがほかのウィンドウに一部重なって隠れていても、そのウィンドウ自身の内容を撮影する
4. The Snipt shall 最小化されたウィンドウ、非表示のウィンドウ、Snipt 自身の選択画面を選択対象から除く
5. If 利用者が Esc キーまたは右クリックを押したとき, then the Snipt shall ウィンドウ選択画面を閉じ、何も保存・コピーせずに撮影を取り消す
6. If 選択したウィンドウを撮影できなかったとき, then the Snipt shall 撮影できなかったことを通知し、何も保存・コピーしない

### Requirement 3: 全画面キャプチャ
**Objective:** As a 利用者, I want 画面全体を一度で撮りたい, so that 選択の手間なく状況全体を残せる

#### Acceptance Criteria
1. When 全画面キャプチャの開始が要求されたとき, the Snipt shall 選択画面を表示せずに直ちに撮影する
2. The Snipt shall 初期設定では、マウスカーソルがあるモニタ1枚分を撮影する
3. Where 設定で「全モニタ」が選ばれている場合, the Snipt shall すべてのモニタを実際の配置どおりに並べた1枚の画像として撮影し、モニタのない領域は透明にする

### Requirement 4: マルチモニタと拡大率
**Objective:** As a 複数モニタの利用者, I want どのモニタでも正確な位置と大きさで撮りたい, so that ずれやぼやけを直す手間がない

#### Acceptance Criteria
1. The Snipt shall 撮影した画像の各ピクセルを、モニタ上の実際の画素と1対1で対応させる(拡大縮小しない)
2. While モニタごとに異なる拡大率(例: 100% と 150%)が設定されている間, the Snipt shall 範囲選択画面に表示した範囲と撮影された画像の範囲を一致させる
3. When 撮影中にモニタの接続・切断や拡大率の変更が起きたとき, the Snipt shall 撮影を取り消し、次回以降の撮影では新しいモニタ構成を使う
4. The Snipt shall 撮影した画像にマウスカーソルを含めない

### Requirement 5: 撮影後の処理
**Objective:** As a 利用者, I want 撮った直後に貼り付けも編集もできること, so that 共有までの手数が少ない

#### Acceptance Criteria
1. When キャプチャ結果が確定したとき, the Snipt shall その画像をクリップボードにコピーする
2. When キャプチャ結果が確定したとき, the Snipt shall その画像と撮影情報(撮影方法、日時、ウィンドウキャプチャの場合はウィンドウタイトル)の保存を要求する
3. When キャプチャ結果が確定したとき, the Snipt shall そのキャプチャを編集画面で開くよう要求する
4. The Snipt shall キャプチャ結果の確定から編集画面が表示されるまでを 1 秒以内に完了する
5. If クリップボードへのコピーに失敗したとき, then the Snipt shall 失敗を通知したうえで、保存と編集画面の表示は続行する
6. While 撮影の操作中である間, the Snipt shall 新しい撮影の開始要求を受け付けない
