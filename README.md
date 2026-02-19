# recmari

SF6 (ストリートファイター6) のプレイ動画を解析し、フレームごとのゲーム状態を Protobuf で出力するツール。

## できること

- 録画済み MP4 ファイルからフレームを抽出
- 1P / 2P の体力バー残量を検出
- ラウンド・マッチ境界の自動検出
- 解析結果を Protobuf (`Match` メッセージ) で出力

## 前提条件

- Rust (cargo)
- [FFmpeg](https://ffmpeg.org/) — `ffmpeg` と `ffprobe` に PATH が通っていること
- [protoc](https://github.com/protocolbuffers/protobuf/releases) — Protobuf コンパイラ

```
winget install Gyan.FFmpeg
winget install Google.Protobuf
```

## ビルド

```
cargo build
```

## 使い方

```
recmari analyze --input match.mp4 --output result.pb
```

| オプション | 説明 | デフォルト |
|---|---|---|
| `--input` | 入力動画ファイルのパス | (必須) |
| `--output` | 出力 Protobuf ファイルのパス | (必須) |
| `--sample-rate N` | N フレームごとに解析 | 2 |
| `--debug-frames DIR` | 検出領域を描画したデバッグフレームを保存 | なし |

## プロジェクト構造

```
recmari/
├── proto/recmari.proto          # Protobuf スキーマ
├── crates/
│   ├── recmari-proto/           # prost 生成コード
│   ├── recmari-core/            # 解析ロジック (動画デコード, 画像解析)
│   └── recmari/                 # CLI バイナリ
└── tasks.md                     # ロードマップ
```

## データ構造

```
Match
├── SourceMetadata (oneof: VideoFileSource | ScreenCaptureSource)
└── repeated Round
     └── repeated FrameData
          ├── PlayerState (1P)
          └── PlayerState (2P)
```

詳細は [proto/recmari.proto](proto/recmari.proto) を参照。
