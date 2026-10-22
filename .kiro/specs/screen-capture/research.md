# Research & Design Decisions: screen-capture

## Summary
- **Feature**: `screen-capture`
- **Discovery Scope**: New Feature(greenfield)/ Complex Integration(Win32・WinRT・Tauri v2 ウィンドウ・WebView2 の組み合わせ)
- **Key Findings**:
  - Windows 上の Tauri v2 IPC は大きなバイナリ転送が遅い(10MB あたり約 200ms という報告)。4K の RGBA 静止画は 1 枚で約 33MB あり、凍結画像を WebView に送ると 200ms 目標(1.2)をそれだけで超える。凍結画像は WebView に渡さず、**ネイティブの背景ウィンドウ(backdrop)に描き、その上に透明な WebView オーバーレイを重ねる**構成にする。
  - Windows.Graphics.Capture(WGC)の `IsCursorCaptureEnabled` は Windows 10 2004(19041)以降、`IsBorderRequired` は 20348 以降(実質 Windows 11)にしかない。1903/1909 ではカーソル除外と黄色枠の非表示を API で制御できない。
  - 凍結(全モニタの静止画取得)は GDI `BitBlt` をモニタごとに行う方式が最も単純で頑健(カーソルを含まず、黄色枠も出ず、RDP やハイブリッド GPU でも動く)。DXGI Desktop Duplication は速いが、セッションの作成・維持コストと失敗パターン(ACCESS_LOST、アダプタ不一致)が多い。

## Research Log

### 全モニタの凍結方式(1.1, 1.2, 3.x, 4.1, 4.4)
- **Context**: ホットキー押下時点の全モニタを静止画として取り、200ms 以内に選択画面を出す必要がある。
- **Sources Consulted**: Microsoft Learn(Desktop Duplication API, BitBlt, GraphicsCaptureSession)、`windows-capture` crate のドキュメント、`xcap` の実装方針、OBS / WebRTC の WGC 実装。
- **Findings**:
  - GDI `BitBlt`(画面 DC → DIB セクション): PMv2 プロセスでは物理ピクセルで取得できる。カーソルは含まれない。DWM 合成済みの画面を CPU に読み戻すため、4K で数十 ms 程度が目安(要実測)。追加の GPU 資源やセッション状態が不要。
  - DXGI Desktop Duplication: 取得自体は数 ms と速く、カーソルも別扱い(画像に含まれない)。ただし出力ごとに `DuplicateOutput(1)` の作成が必要で、作成直後の初回フレーム待ち・`DXGI_ERROR_ACCESS_LOST`(モード変更・UAC・フルスクリーン切替)・ハイブリッド GPU での `UNSUPPORTED`・同時複製数の制限がある。常時保持すると常駐負荷が増える。
  - WGC(CreateForMonitor): モニタ単位で取得できるが、セッション開始から初回フレームまでの非同期待ちがあり、Windows 10 では黄色枠が表示され、1903/1909 ではカーソルを除外できない。
  - `xcap`: Windows のモニタ取得は GDI ベースで、DPI・アルファ・スレッドを細かく制御できない。`windows-capture`: WGC と DXGI の両方を持つが、ストリーミング用のハンドラ構造で 1 フレーム取得には過剰。どちらも実装の参考にとどめる。
- **Implications**: 凍結は GDI BitBlt(DIB セクションへ直接)を採用する。`CapturePlatform` トレイトの背後に置き、実測で予算を超える場合は DXGI 実装へ差し替えられるようにする(インターフェースだけを一般化し、実装は 1 つ)。

### 個別ウィンドウの撮影(2.3, 2.6, 4.4)
- **Context**: 一部が隠れたウィンドウでも、そのウィンドウ自身の内容を撮る。
- **Sources Consulted**: `IGraphicsCaptureItemInterop::CreateForWindow`、`GraphicsCaptureSession.IsCursorCaptureEnabled`(19041〜)、`IsBorderRequired`(20348〜、`GraphicsCaptureAccess.RequestAccessAsync(Borderless)` が必要)、`PrintWindow(PW_RENDERFULLCONTENT)`。
- **Findings**:
  - WGC の HWND キャプチャは DWM の合成元を取るため、隠れた部分も撮れる。権限昇格したウィンドウも撮れる。結果テクスチャは `ContentSize` で切り詰める必要がある。
  - `IsCursorCaptureEnabled=false` は 19041 以降のみ。それ以前はカーソルがウィンドウ上にあると写り込む可能性がある。オーバーレイが最前面でカーソル形状を決めているため、確定時にオーバーレイ側でカーソルを非表示(CSS `cursor: none`)にすれば写り込む形状が無くなる(要実機確認)。
  - 黄色枠は撮影画像には含まれず、画面上の表示のみ。Windows 10(20348 未満)では 1 フレーム取得の間だけ短く表示される。Windows 11 では `RequestAccessAsync(Borderless)` 後に `IsBorderRequired=false` で消せる。非パッケージアプリでの同意挙動は要実機確認。拒否されても撮影は成功する(枠が出るだけ)。
  - `PrintWindow(PW_RENDERFULLCONTENT)`: カーソルを含まず隠れた部分も撮れるが、対象へ同期メッセージを送るため応答しないウィンドウで止まる、昇格ウィンドウは UIPI で失敗する、DPI 非対応ウィンドウは論理サイズで返る(4.1 の 1:1 を満たさない)。`windows` crate では `Win32::Storage::Xps` に属する。
- **Implications**: WGC を主経路、PrintWindow を WGC 失敗時のみのフォールバックとする。どちらも失敗したら 2.6 として通知する。PrintWindow はタイムアウト付きの別スレッドで呼ぶ。

### Per-Monitor DPI Aware v2 と物理ピクセル(4.1, 4.2)
- **Context**: 100% と 150% の混在環境で、選択した範囲と撮影範囲を一致させる。
- **Sources Consulted**: Microsoft Learn(DPI_AWARENESS_CONTEXT、Setting the default DPI awareness)、tao(Tauri のウィンドウ層)の DPI 初期化。
- **Findings**:
  - tao は起動時にプロセスを PMv2 にする(`SetProcessDpiAwarenessContext`)。PMv2 では `GetMonitorInfoW`・`DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`・`SetWindowPos`・`GetCursorPos` がすべて物理ピクセルになる。
  - 念のためキャプチャ用スレッドでは `SetThreadDpiAwarenessContext(PER_MONITOR_AWARE_V2)` を明示し、起動時に実際の awareness を検証してログに残す。
  - WebView2 の `devicePixelRatio` はそのウィンドウが載るモニタの拡大率になる。オーバーレイをモニタの物理矩形ぴったりに置けば、ローカル物理座標 = `round(clientX * devicePixelRatio)` で求まる。
  - ウィンドウを別 DPI のモニタへ移すと `WM_DPICHANGED` の推奨矩形でサイズが変わるため、「位置 → サイズ」の順に設定し、表示直前に HWND へ `SetWindowPos` で物理矩形を再設定する。
- **Implications**: Rust とオーバーレイ間の座標は「モニタローカルの物理ピクセル整数」に統一し、CSS px は描画時にだけ使う。座標変換は純粋関数にして単体テストする。

### オーバーレイの事前生成と 200ms 目標(1.2)
- **Context**: ホットキーから選択画面表示までの遅延を最小化する。
- **Sources Consulted**: Tauri v2 WebviewWindowBuilder(`visible(false)`, `transparent`, `always_on_top`, `skip_taskbar`, `decorations(false)`, `shadow(false)`, `focused`)、Tauri IPC 性能に関する discussion。
- **Findings**:
  - WebView の生成・ページ読み込みは数百 ms かかるため、起動時にモニタごとに非表示で作っておく必要がある(steering の決定と一致)。
  - 非表示の WebView2 は描画が止められる場合があり、表示後の初回描画が遅れる可能性がある(要実測)。遅い場合は「透明・画面外に置いたまま表示状態を保つ」方式に切り替える。
  - `RegisterHotKey` の `WM_HOTKEY` やトレイ操作を受けたプロセスは前面化の権利を得るため、オーバーレイへフォーカスを移せる(Esc の受け取りに必要)。
  - Windows 11 の角丸は `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND` で抑止する。
- **Implications**: 想定予算は「凍結 30〜80ms + 背景表示 10ms + オーバーレイ表示と初回描画 20〜50ms」。オーバーレイは描画完了を通知し、Rust 側で遅延を計測・記録する。

### 凍結画像をオーバーレイに見せる方法(base64 禁止)
- **Context**: 契約で base64 JSON は禁止。Windows の IPC はバイナリでも遅い。
- **Findings / 候補**:
  1. IPC(`tauri::ipc::Response`)や独自 URI スキームで生画像を送る: 33MB/モニタで数百 ms。不可。
  2. 高速 PNG/JPEG 圧縮して送る: エンコード 40〜80ms + 転送 + デコード。予算ぎりぎりで、JPEG は画質が落ちる。
  3. WebView2 SharedBuffer: Tauri が API を出しておらず、遅いという報告もある。
  4. **ネイティブ背景ウィンドウに DIB を BitBlt し、透明 WebView を上に重ねる**: 転送ゼロ。描画は数 ms。WebView は暗幕・選択枠・寸法表示・拡大鏡だけを描く。
- **Implications**: 4 を採用。拡大鏡に必要な画素は小さな切り出し(例 17×17px ≒ 1KB)を `tauri::ipc::Response` の生バイトで都度取得する。契約の「大きな画像は IPC に載せない」に沿う。

### ウィンドウの列挙と当たり判定(2.1, 2.4)
- **Findings**:
  - `EnumWindows` は最前面から順に返る。除外条件: `!IsWindowVisible`、`IsIconic`、`DWMWA_CLOAKED != 0`(別仮想デスクトップ・停止中 UWP)、矩形が空、自分のオーバーレイ/背景ウィンドウの HWND、デスクトップ(`Progman`/`WorkerW`)とシェルのトレイ。
  - 矩形は `DWMWA_EXTENDED_FRAME_BOUNDS`(見えないリサイズ枠を含まない)を使う。`GetWindowRect` は Windows 10 以降で見た目より大きい。
  - オーバーレイ表示前に一覧を取るので、オーバーレイ自身は一覧に入らない。Snipt の他の画面(設定・編集)は要件上の除外対象ではないため選択可能とする。
- **Implications**: 一覧はセッション開始時のスナップショットとし、当たり判定は純粋関数(z 順で最初に点を含む候補)にする。

### 表示構成の変化の検知(4.3)
- **Findings**:
  - `WM_DISPLAYCHANGE` はトップレベルウィンドウにだけ届く(メッセージ専用ウィンドウには届かない)。拡大率だけの変更では `WM_DISPLAYCHANGE` が来ない場合があり、`WM_DPICHANGED`(そのモニタ上のウィンドウ)と `WM_SETTINGCHANGE` が来る。
  - モニタの列挙(`EnumDisplayMonitors` + `GetMonitorInfoW` + `GetDpiForMonitor`)は 1ms 未満で済む。
- **Implications**: 非表示のトップレベル監視ウィンドウでメッセージを受けるのに加え、撮影開始時と確定時に「構成指紋」(デバイス名・物理矩形・DPI)を取り直して比較する。メッセージを取りこぼしても確定時に検出できる。

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| Ports & Adapters(採用) | 状態機械とロジックを OS 非依存にし、`CapturePlatform`・`OverlayPresenter` の 2 ポートの背後に Win32/Tauri 実装を置く | steering の「薄いアダプタ」に一致。状態機械・座標・合成を `cargo test` できる | ポート定義の手間 | ポートは 2 つに絞る |
| オーバーレイ側に処理を寄せる | 当たり判定・切り出しを TS で行う | Rust 呼び出しが減る | 画素データを WebView に送る必要がある(IPC が遅い) | 不採用 |
| すべてネイティブ描画 | 選択 UI も Win32 で描く | 最速 | steering(オーバーレイは WebView)に反する。UI の作り込みが増える | 不採用 |

## Design Decisions

### Decision: 凍結は GDI BitBlt、ウィンドウは WGC
- **Context**: 1.1, 1.2, 2.3, 4.4
- **Alternatives Considered**: 1) DXGI Desktop Duplication で凍結 2) WGC で凍結とウィンドウの両方 3) xcap / windows-capture を採用
- **Selected Approach**: 凍結・全画面 = BitBlt(モニタごとに DIB セクションへ)。ウィンドウ = WGC(`CreateForWindow`、1 フレーム)、失敗時のみ PrintWindow。
- **Rationale**: BitBlt は状態を持たず失敗が少なく、カーソルも枠も出ない。WGC は隠れたウィンドウを撮れる唯一の安定手段。
- **Trade-offs**: BitBlt は DXGI より遅い。HDR モニタでの色の扱いは要確認。
- **Follow-up**: 4K×2 枚での BitBlt 所要時間を実測し、80ms を超える場合は DXGI 実装を追加する。

### Decision: ネイティブ背景ウィンドウ + 透明 WebView オーバーレイ
- **Context**: 1.2(200ms)、契約(base64 禁止)、Windows IPC の遅さ
- **Alternatives Considered**: IPC 生バイト、圧縮転送、SharedBuffer、Tauri ウィンドウ自体へのサブクラス描画
- **Selected Approach**: モニタごとに「Win32 背景ウィンドウ(凍結画像)」と「Tauri オーバーレイ(透明 WebView、UI のみ)」を重ねる。範囲モードのみ背景を出す。ウィンドウモードは背景を出さず、実際の画面の上に透明オーバーレイを重ねる。
- **Rationale**: 画素の転送が不要になり、200ms の主要因を除ける。tao のウィンドウプロシージャに手を入れずに済む。
- **Trade-offs**: モニタごとにウィンドウが 2 枚になり、z 順の管理が要る。
- **Follow-up**: 透明 WebView の透明部分でもマウス入力を受け取れることを実機で確認する。

### Decision: 座標は「モニタローカル物理ピクセル整数」で受け渡す
- **Context**: 4.1, 4.2, 1.5
- **Selected Approach**: オーバーレイは `round(client * devicePixelRatio)` で物理 px を作り、Rust が再度クランプ・検証して切り出す。
- **Rationale**: 小数の CSS px を Rust 側に持ち込まないことで、125% などの端数倍率でも 1 px のずれを起こさない。

### Generalization / Build vs Adopt / Simplification(synthesis)
- **一般化**: 範囲・全画面(カーソルのモニタ)・全画面(全モニタ)は、すべて「凍結フレーム群 + 対象矩形(グローバル物理)→ 合成」で表せる。`compose(frames, rect) -> CapturedImage` 1 つで 1.4・3.2・3.3 を扱い、範囲外は透明にする。将来の scroll-capture もこの合成を使える。
- **採用 / 自作**: `windows` crate で薄いアダプタを自作する(steering 指定)。xcap・windows-capture は制御粒度と構造が合わないため依存しない。クリップボード・保存・通知・編集画面は他 spec の既存サービスを使う。
- **簡素化**: DXGI 実装・SharedBuffer・Esc 用の一時グローバルホットキーは今回作らない(リスク対策として記録のみ)。ポートは 2 つ。オーバーレイ用コマンドは 1 ファイルに集約する。

## Risks & Mitigations
- 200ms を超える(BitBlt が遅い/非表示 WebView の初回描画が遅い)— 計測フックで実測し、DXGI 実装への差し替え、オーバーレイの「透明のまま表示状態を保つ」待機方式で対処する。
- 初回ホットキー時にオーバーレイがまだ読み込み中 — ready 通知を待つ(最大 1 秒)。その回だけ 200ms を超えることを許容する。
- 1903/1909 でカーソルが写り込む — 確定時にオーバーレイのカーソルを非表示にしてから撮る。実機確認。
- 透明 WebView がクリックを透過してしまう — tao の透明化方式(レイヤードではない)なら入力を受けると想定。透過する場合は背景を全モードで出す(ウィンドウモードでも凍結画像を表示)方式へ切り替える。
- オーバーレイへフォーカスが移らず Esc が効かない — ホットキー由来の前面化権を使う。失敗時の対策として、撮影中のみ Esc を一時登録する案を残す。
- 保存失敗時に編集画面へ渡す ID — cross-spec レビューで確定: capture-library の `SaveCaptureError.unsaved.id` で開き、通知は capture-library のみが行う。
- 全モニタ合成画像の PNG 化が重く 1 秒(5.4)を圧迫する — capture-library 側の圧縮レベル選択に依存。計測フックで確認する。
- HDR モニタでの BitBlt の色 — 実機で確認し、問題があれば DXGI(`DuplicateOutput1` + B8G8R8A8)を検討する。

## References
- [GraphicsCaptureSession.IsCursorCaptureEnabled](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.iscursorcaptureenabled?view=winrt-26100) — 19041 以降
- [GraphicsCaptureSession.IsBorderRequired](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired) — 20348 以降、RequestAccessAsync(Borderless) が必要
- [IGraphicsCaptureItemInterop::CreateForWindow](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow) — HWND から WGC アイテムを作る
- [DPI_AWARENESS_CONTEXT](https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context) — PMv2 の挙動
- [Setting the default DPI awareness for a process](https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process)
- [Tauri discussion #11915: IPC performance](https://github.com/orgs/tauri-apps/discussions/11915) — Windows で 10MB 約 200ms
- [windows-capture](https://docsearch.algolia.com/mcp/docs/repo/niiightmarexd/windows-capture) — WGC / DXGI の Rust 実装例(参考のみ)
- [WebRTC wgc_capturer_win](https://webrtc.googlesource.com/src/+/main/modules/desktop_capture/win/wgc_capturer_win.h) — WGC 実装の参考
