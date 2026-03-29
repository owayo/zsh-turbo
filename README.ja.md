<p align="center">
  <img src="docs/images/app.png" width="128" alt="zsh-turbo">
</p>

<h1 align="center">zsh-turbo</h1>

<p align="center">
  <strong>Rust 製の高性能 zsh 強化ツール</strong>
</p>

<p align="center">
  <a href="https://github.com/owayo/zsh-turbo/actions/workflows/ci.yml">
    <img alt="CI" src="https://github.com/owayo/zsh-turbo/actions/workflows/ci.yml/badge.svg?branch=main">
  </a>
  <a href="https://github.com/owayo/zsh-turbo/releases/latest">
    <img alt="Version" src="https://img.shields.io/github/v/release/owayo/zsh-turbo">
  </a>
  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/github/license/owayo/zsh-turbo">
  </a>
</p>

<p align="center">
  <a href="README.md">English</a> | 日本語
</p>

---

## 概要

プロンプト描画、入力候補、シンタックスハイライト、補完スタイリングを単一の高速な Rust バイナリで提供する zsh 強化ツール。

## 特徴

- **4つのプロンプトスタイル** — Lean、Classic（パワーライン）、Rainbow、Pure + フォントレベル自動検出
- **20以上のセグメント** — dir, git, virtualenv, kubecontext, aws, terraform, docker, ssh, 各種言語バージョン等
- **カスタムコマンドセグメント** — TOML で宣言的に任意のセグメントを定義
- **並列セグメント実行** — `std::thread::scope` で全セグメントを同時実行
- **非同期オートサジェスト** — 履歴ベース（prefix / substring / fuzzy 戦略）
- **Transient プロンプト** — コマンド実行後に前のプロンプトを簡略化
- **対話型ウィザード** — フォント検出 + スタイル選択
- **`zsh-turbo doctor`** — ターミナル・フォント・ツール・設定の診断
- **補完スタイリング** — 大小文字無視、グループ化、キャッシュ、色分け

## 動作環境

- **OS**: macOS、Linux
- **シェル**: zsh 5.4 以上
- **Rust**: 1.85 以上（ソースからビルドする場合、edition 2024）

## インストール

### ソースからビルド

```bash
cargo install --path .
```

### バイナリダウンロード

[Releases](https://github.com/owayo/zsh-turbo/releases) から最新版をダウンロード。

## セットアップ

```bash
# 対話型設定ウィザード
zsh-turbo configure

# ~/.zshrc に追加
eval "$(zsh-turbo init)"
```

## 設定

設定ファイル: `~/.config/zsh-turbo/config.toml`

```toml
[prompt]
prompt_style = "classic"  # lean, classic, rainbow, pure
font_level = "nerd"       # nerd, powerline, unicode, ascii
left_segments = ["os_icon", "dir", "git", "virtualenv"]
right_segments = ["duration", "status", "time"]
transient = true

# カスタムコマンドセグメント
[[prompt.custom]]
name = "uptime"
command = "uptime | awk '{print $3}' | tr -d ','"
icon = "⏰"
fg = "white"
bg = "238"
when = "always"  # or "env:VAR" or "file:path"
```

## コマンド

```bash
zsh-turbo configure                       # 対話型ウィザード
zsh-turbo init                            # zsh 初期化スクリプト出力
zsh-turbo prompt --side left              # プロンプト描画
zsh-turbo suggest "クエリ"                # サジェスト取得（prefix）
zsh-turbo suggest "qry" --strategy fuzzy  # ファジーサジェスト
zsh-turbo complete "prefix"               # 補完候補一覧
zsh-turbo doctor                          # 環境診断
```

## 開発

```bash
# ビルド
make build

# テスト実行
make test

# clippy と フォーマットチェック
make check

# リリースビルド
make release
```

## ライセンス

[MIT](LICENSE)
