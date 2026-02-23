# FBQueue (Forblaze Queue) - Development Manual

このドキュメントは、FBQueue の詳細な設計、実装、およびロードマップを記述するものです。

---

## 1. プロジェクトの哲学 (Core Philosophy)

*   **ゼロ依存 (Zero Dependency)**: Rust 標準ライブラリのみを使用。
*   **シングルバイナリ (Single Binary)**: 実行ファイル一つで Linux/Windows に対応。
*   **お行儀の良さ (Auto-Off)**: 仕事がなければ 5 分で自動終了。

## 2. ディレクトリ構造 (Directory Structure)

`FBQUEUE_DIR` 環境変数または `~/.fbqueue/` 以下。
```
.fbqueue/
├── config                     # 全体の設定ファイル
├── queue/                     # new, running, done, failed, cancel
├── logs/                      # デフォルトログ (旧)
└── run/                       # daemon.pid, last_id, id.lock
```

## 3. リファレンス (Reference)

### コマンドライン引数 (sub subcommand)

| オプション | 内部パラメータ | 説明 | デフォルト |
| :--- | :--- | :--- | :--- |
| `-c`, `--cost` | `cost` | ジョブが消費するリソース量 | `1` |
| `-N`, `-J` | `name` | ジョブの名前（表示・ファイル名用） | コマンド名 |
| `-q`, `--queue` | `queue` | 投入先のキュー名 | `default_queue` |
| `-W` | `walltime` | 実行時間制限 (`HH:MM:SS`) | 無制限 |
| `-hold_jid` | `depend` | 指定したIDの終了を待つ | - |
| `-a` | `start_after` | 開始時刻指定 (`UNIX TIMESTAMP`) | 即時 |
| `-o` | `stdout` | 標準出力のリダイレクト先 | `<cwd>/<name>.o<id>` |
| `-e` | `stderr` | 標準エラーのリダイレクト先 | `<cwd>/<name>.e<id>` |
| `--range` | - | 数値範囲での一括投入 (`N-M`) | - |
| `--list` | - | リストでの一括投入 (`A,B,C`) | - |

### スクリプト内ディレクティブ対応表

FBQueue はスクリプト内の命令を解析し、以下の **内部パラメータ** へ自動的にマッピング（翻訳）します。

| 機能 | 外部記法 (例) | FBQueue の内部パラメータ |
| :--- | :--- | :--- |
| **ジョブ名** | `#PBS -N name`, `#SBATCH -J name` | **`name`** |
| **並列数/コスト** | `#$ -pe smp 4`, `#SBATCH -c 4` | **`cost`** |
| **投入キュー** | `#PBS -q express`, `#SBATCH -p express` | **`queue`** |
| **出力先** | `#$ -o /path/to/log`, `#SBATCH -o ...` | **`stdout`** |
| **エラー先** | `#PBS -e /path/to/err`, `#SBATCH -e ...` | **`stderr`** |
| **依存関係** | `#$ -hold_jid 123` | **`depend`** |
| **実行制限** | `#$ -l h_rt=01:30:00` | **`walltime`** |

*※注: コマンドライン引数での明示的な指定は、スクリプト内の記述よりも常に優先されます。*

---
*Saturday, February 21, 2026 - Comprehensive Reference Update.*