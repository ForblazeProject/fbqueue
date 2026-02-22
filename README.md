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
- **PBS互換のログ出力**: ジョブスクリプトからのディレクティブ解析（`#$`, `#PBS`, `#SBATCH`）に対応。出力ファイルは PBS スタイル (`.oJobID`, `.eJobID`) でジョブ実行ディレクトリに生成。
- **バッチ処理**: `--range` や `--list` オプションにより、シンプルなコマンドで数百〜数千のジョブを一括投入。
- **堅牢性**: ディレクトリベースのキュー管理（Maildir 形式）により、デーモンがクラッシュしてもジョブの状態を保持し、自動復旧。
- **IDの簡素化**: ジョブIDは 1 から始まる連番で管理。

## 🚀 インストール (Installation)

FBQueue は単一バイナリで動作するため、インストールは非常に簡単です。

1.  **Rust のインストール**: [Rust](https://www.rust-lang.org/tools/install)
2.  **ビルド**: `cd fbqueue && cargo build --release`
3.  **パスへの追加**: `./target/release/fbqueue` を `PATH` の通った場所へ配置。

## 💡 使い方 (Usage)

### 基本コマンド

```bash
# ジョブの投入 (qsub のエイリアスも可能)
fbqueue sub [options] <command> [args...]

# ジョブのステータス表示 (qstat のエイリアスも可能)
fbqueue stat

# ジョブの削除/キャンセル (qdel のエイリアスも可能)
fbqueue del <job_id>
```

### オプション例

- **実行時間制限 (2時間)**: `fbqueue sub -W 02:00:00 ./script.sh`
- **依存関係 (ID 10 の終了を待つ)**: `fbqueue sub -hold_jid 10 ./next.sh`
- **開始時刻指定 (UNIXタイムスタンプ)**: `fbqueue sub -a 1771766817 ./midnight_task.sh`
- **一括投入**: `fbqueue sub --range 1-100 echo "Task {}"`

詳細は `MANUAL.md` を参照してください。

---
*Saturday, February 21, 2026 - Initial implementation and documentation.*