# FBQueue (Forblaze Queue)

![FBQueue Logo Placeholder](https://via.placeholder.com/150/0000FF/FFFFFF?text=FBQueue)

FBQueue は、Rust 製の軽量で堅牢なローカルジョブスケジューラです。共有計算サーバなどにおいて、ユーザーが自身の計算リソース（CPU/GPU 時間など）を「お行儀よく」管理し、大量の計算ジョブを効率的かつ確実に実行するために設計されています。

単一バイナリで動作し、外部依存ライブラリを一切持たないため、どのような環境でも簡単に導入・運用できます。

## 🎨 特徴 (Features)

- **超軽量＆ゼロ依存**: Rust 標準ライブラリのみで構築。単一バイナリで Linux および Windows に対応。
- **リソース管理**: 抽象的な `capacity` とジョブごとの `cost` を用いて、CPU/GPU などのリソース使用量を柔軟に制御。
- **複数キュー & 優先度**: キューごとに Capacity と Priority を設定可能。高優先度ジョブを優先的に実行。
- **実行制限 & スケジューリング**:
    - **Walltime**: ジョブの最大実行時間を制限し、暴走を防止。
    - **Dependency**: ジョブ間の依存関係を指定可能（Job A の後に Job B を実行）。
    - **Delayed Start**: 指定した時刻以降にジョブを開始。
- **自動起動＆自動停止**: ジョブ投入時にデーモンが自動起動し、アイドル状態が 5 分続くと自動的に停止。
- **PBS/SGE互換**:
    - **ディレクティブ解析**: `#PBS`, `#$`, `#SBATCH` 等のスクリプト内記述を自動認識。
    - **PBSスタイル出力**: `qstat` 互換の表形式表示。
    - **安全な実行**: ジョブスクリプトの実行権限 (`+x`) を書き換えることなく、シェル経由で安全に実行 (PBS 互換の振る舞い)。
- **バッチ処理**: `--range` や `--list` オプションにより、シンプルなコマンドで数百〜数千のジョブを一括投入。
- **IDの簡素化**: ジョブIDは 1 から始まる連番で管理。

## 🚀 インストール (Installation)

FBQueue は単一バイナリで動作するため、ビルドしてパスを通すだけです。

1.  **ビルド**: `cd fbqueue && cargo build --release`
2.  **配置**: `./target/release/fbqueue` を `PATH` の通った場所へ。

---
*Saturday, February 21, 2026 - Initial implementation and documentation.*
