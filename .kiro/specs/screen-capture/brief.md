# Brief: screen-capture

## Problem
既存ツールでは、ホットキーを押してから選択画面が出るまでの遅れや、拡大率の違うモニタをまたぐと位置やサイズがずれる問題があり、ストレスになる。

## Current State
新規プロジェクト。コードは無い。

## Desired Outcome
ホットキーやトレイから、範囲・ウィンドウ・全画面のいずれかを即座に撮れる。撮影直後に画像がクリップボードにコピーされ、保存されたうえで編集画面が開く。

## Approach
Rust から Windows API を直接呼ぶ。範囲 / 全画面は押下時点の全モニタを静止画として取得し、モニタごとに事前生成したオーバーレイ上で範囲を選ぶ。ウィンドウは Windows.Graphics.Capture で個別ウィンドウを取得する。

## Scope
- **In**: 範囲キャプチャ(ドラッグ選択、拡大鏡・座標表示、Esc でキャンセル)、ウィンドウキャプチャ(マウス下のウィンドウをハイライトして選択)、全画面キャプチャ(全モニタまたはカーソルのあるモニタ)、マルチモニタ・混在 DPI 対応、撮影後のクリップボードコピーと capture-library への保存、編集画面を開く要求
- **Out**: スクロールキャプチャ(将来の scroll-capture)、動画、遅延撮影(タイマー)

## Boundary Candidates
- 画面の取得(OS 依存)
- 範囲 / ウィンドウ選択 UI
- 撮影後フロー(コピー → 保存 → 編集を開く)

## Out of Boundary
- ホットキー登録(app-shell)
- 保存形式(capture-library)
- 注釈(annotation-editor)

## Upstream / Downstream
- **Upstream**: app-shell(撮影開始トリガー)、capture-library(保存)
- **Downstream**: annotation-editor(撮影後に開く)、将来の scroll-capture

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: app-shell, capture-library, annotation-editor

## Constraints
- 物理ピクセル単位で正確に切り出すこと(拡大率 100% / 150% 混在でもずれない)
- ホットキー押下から選択画面表示までの体感遅延を小さくすること
