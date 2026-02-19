# Tasks

## 完了

- [x] Phase 1: スケルトン + 動画デコード
  - [x] Cargo ワークスペース・3クレート初期化 (recmari-proto, recmari-core, recmari)
  - [x] `.proto` ファイル作成 + prost-build セットアップ
  - [x] `video::decoder` — ffmpeg CLI パイプで MP4 → RGB24 フレーム抽出
  - [x] CLI 実装 (clap): `recmari analyze --input --output --sample-rate --debug-frames`
  - [x] 動作確認: テスト動画の最初のフレームを PNG 保存

## Phase 2: 体力バー解析

- [ ] HUD 領域の正規化座標キャリブレーション
  - テスト動画の試合画面フレームから P1/P2 体力バーのピクセル範囲を特定
  - `config::regions` に SF6 用の `NormalizedRect` 定数を定義
- [ ] `analysis::hud` — HudLayout 構造体、領域スケーリング
- [ ] `analysis::health` — 体力バー検出
  - RGB → HSV 変換ヘルパー
  - 体力バー領域の水平スキャンで充填率を算出
  - P1: 左→右 (バーは右から減少)、P2: 右→左
- [ ] パイプライン実装 (`pipeline.rs`)
  - デコード → サンプリング (N フレームごと) → 体力解析 → 結果収集
- [ ] ラウンド境界検出
  - 両プレイヤーの health_ratio が同時に 1.0 にリセット → 新ラウンド
- [ ] マッチ境界検出
  - ラウンド間のギャップが一定時間以上 → 新マッチ
- [ ] Protobuf 出力 — Match メッセージをファイルに書き出し
- [ ] `--debug-frames` — 検出領域をオーバーレイ描画したフレームを保存

## Phase 3 (将来): プレイヤー位置検出

- [ ] YOLO + ONNX Runtime (`ort` crate) によるキャラクター Bounding Box 検出
- [ ] 学習データ作成・モデル訓練
- [ ] PlayerState に Position フィールド追加

## Phase 4 (将来): リアルタイムキャプチャ

- [ ] `windows-capture` で画面キャプチャ実装
- [ ] ScreenCaptureSource 対応
- [ ] feature flag で切り替え
