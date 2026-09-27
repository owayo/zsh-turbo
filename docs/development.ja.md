# 開発

[README に戻る](../README.ja.md)

`make dist` は実行環境向けのバイナリと `LICENSE`、`THIRD_PARTY_NOTICES.md`、`licenses/` を `target/dist/` のアーカイブへまとめます。バイナリの配布時はこれらを一緒に配布してください。依存関係の更新後は `make licenses` で文書を再生成して内容を確認し、`make licenses-check` で現在のロックファイルとの一致を検証できます。ライセンス文書の生成・検証には Python 3.7 以上と Cargo が必要で、Cargo のキャッシュへ依存クレートをダウンロードする場合があります。一覧はデフォルト機能の通常／ビルド依存を全ターゲット分含み、開発専用・無効な任意依存は除外します。
