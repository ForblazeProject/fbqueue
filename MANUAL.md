# FBQueue (Forblaze Queue) - Development Manual

このドキュメントは、FBQueue の詳細な設計、実装、および今後の開発ロードマップを記述するものです。開発者向けのメモと、公開時の詳細マニュアルの両方を兼ねます。

---

## 目次

1.  [プロジェクトの哲学 (Core Philosophy)](#1-プロジェクトの哲学-core-philosophy)
2.  [ディレクトリ構造 (Directory Structure)](#2-ディレクトリ構造-directory-structure)
3.  [ジョブファイル形式 (.job)](#3-ジョブファイル形式-job)
4.  [リソース管理 (Capacity & Cost)](#4-リソース管理-capacity--cost)
5.  [複数キューと優先度](#5-複数キューと優先度)
6.  [高度なスケジューリング機能](#6-高度なスケジューリング機能)
7.  [Windows 環境での動作とガイドライン](#7-windows-環境での動作とガイドライン)
8.  [リファレンス (CLI & スクリプト対応表)](#8-リファレンス-cli--スクリプト対応表)
9.  [ToDo / Roadmap](#9-todo--roadmap)

---

## 1. プロジェクトの哲学 (Core Philosophy)

FBQueue は、以下の原則に基づいて設計・実装されています。

*   **ゼロ依存 (Zero Dependency)**: Rust 標準ライブラリのみを使用。外部クレートに依存せず、ポータビリティを追求。
*   **シングルバイナリ (Single Binary)**: 実行ファイル一つで Linux/Windows 両対応。
*   **透過性 (Transparency)**: すべてのジョブ状態をファイルシステムで見える化。
*   **オートオフ (Auto-Off)**: 仕事がなければ 5 分で自動終了する「お行儀の良さ」。
*   **堅牢性 (Robustness)**: デーモン死や PC 再起動からの自動復旧機能を搭載。

## 2. ディレクトリ構造 (Directory Structure)

FBQueue は、`FBQUEUE_DIR` 環境変数で指定された場所、または `~/.fbqueue/` を使用します。

```
.fbqueue/
├── config                     # 全体の設定ファイル
├── queue/                     # ジョブキュー本体
│   ├── new/                   # 待機中
│   ├── running/               # 実行中
│   ├── done/                  # 正常終了
│   ├── failed/                # 失敗・強制終了
│   └── cancel/                # キャンセルシグナル
├── logs/                      # (旧) ログ出力先
└── run/                       # デーモン管理ファイル
    ├── daemon.pid             # PIDファイル
    ├── last_id                # 次に発行する連番ID
    └── id.lock/               # ID発行用ロック
```

## 3. ジョブファイル形式 (.job)

```
id: 1
name: MyJob
cmd: ./script.sh
cost: 1
user: username
queue: batch
cwd: /home/user/work
env: KEY=VALUE
stdout: /path/to/out
stderr: /path/to/err
walltime: 3600
depend: 10,11
start_after: 1771766817
```

## 4. リソース管理 (Capacity & Cost)

*   **`capacity`**: マシン全体の最大リソース許容量。`config` で設定。
*   **`cost`**: 各ジョブが消費するリソースの重み。投入時に `-c` で指定。
*   **判定**: `used_capacity + cost <= capacity` の場合のみジョブが開始されます。

## 5. 複数キューと優先度

`~/.fbqueue/config` で定義し、リソースの配分と実行順序を制御します。

```text
capacity: 16          # マシン全体の最大容量
default_queue: batch

queue: batch
  capacity: 8         # このキューで使える最大量
  priority: 10        # 低優先度

queue: express
  capacity: 4         # このキューで使える最大量
  priority: 100       # 高優先度
```

## 6. 高度なスケジューリング機能

### Walltime (`-W`)
指定時間を超えたジョブを自動強制終了。
```bash
fbqueue sub -W 01:30:00 ./long_task.sh
```

### Dependency (`-hold_jid`)
指定したジョブ ID が成功（Done）するまで実行を待機。
```bash
fbqueue sub -hold_jid 123 ./next_task.sh
```

### Delayed Start (`-a`)
指定時刻（UNIXタイムスタンプ）以降にジョブを開始。
```bash
fbqueue sub -a 1771766817 ./future_task.sh
```

## 7. Windows 環境での動作とガイドライン

### スクリプト実行
拡張子に応じて適切なインタプリタを自動選択：
- **`.bat`, `.cmd`**: `cmd /c` で実行。
- **`.ps1`**: `powershell -ExecutionPolicy Bypass -File` で実行。

### 安全な実行方式 (PBS互換)
ジョブスクリプトの実行権限 (`+x`) を書き換えることなく、シェル経由で安全に実行します。オリジナルファイルを汚しません。

### 推奨事項
Windows ではエイリアス（qsub.exe等）よりも、常に `fbqueue sub` などの明示的なサブコマンド形式を推奨します。

## 8. リファレンス (CLI & スクリプト対応表)

### CLI オプション (fbqueue sub)

| オプション | 内部パラメータ | 説明 | デフォルト |
| :--- | :--- | :--- | :--- |
| `-c`, `--cost` | `cost` | 消費リソース量 | `1` |
| `-N`, `-J` | `name` | ジョブ名（表示用） | コマンド名 |
| `-q`, `--queue` | `queue` | 投入先のキュー名 | `default_queue` |
| `-W` | `walltime` | 実行時間制限 (`HH:MM:SS`) | 無制限 |
| `-hold_jid` | `depend` | 指定IDの終了を待つ | - |
| `-a` | `start_after` | 開始予約時刻 (`UNIX TIMESTAMP`) | 即時 |
| `-o` | `stdout` | 標準出力リダイレクト先 | `<cwd>/<name>.o<id>` |
| `-e` | `stderr` | 標準エラーリダイレクト先 | `<cwd>/<name>.e<id>` |
| `--range` | - | 数値範囲一括投入 (`N-M`) | - |
| `--list` | - | リスト一括投入 (`A,B,C`) | - |

### スクリプトディレクティブ対応

FBQueue はスクリプト内の命令を以下の内部パラメータへ自動マッピングします。

| 機能 | 外部記法 (例) | FBQueue 内部パラメータ |
| :--- | :--- | :--- |
| **ジョブ名** | `#PBS -N name`, `#SBATCH -J name` | **`name`** |
| **並列数/コスト** | `#$ -pe smp 4`, `#SBATCH -c 4` | **`cost`** |
| **投入キュー** | `#PBS -q express`, `#SBATCH -p express` | **`queue`** |
| **出力先** | `#$ -o /path/to/log`, `#SBATCH -o ...` | **`stdout`** |
| **エラー先** | `#PBS -e /path/to/err`, `#SBATCH -e ...` | **`stderr`** |
| **依存関係** | `#$ -hold_jid 123` | **`depend`** |
| **実行制限** | `#$ -l h_rt=01:30:00` | **`walltime`** |

## 9. ToDo / Roadmap

*   **バックグラウンド・アーカイブ**: アイドル時の古いジョブの `tar.gz` 圧縮。
*   **ディレクトリ探索の柔軟化**: 親ディレクトリを遡る `.fbqueue` 探索。
*   **詳細サマリー**: `stat` での統計情報の表示。

---
*Saturday, February 21, 2026 - Comprehensive Manual finalized.*
