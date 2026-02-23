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
7.  [ジョブスクリプトのプリパース機能](#7-ジョブスクリプトのプリパース機能)
8.  [ToDo / Roadmap](#8-todo--roadmap)

---

## 1. プロジェクトの哲学 (Core Philosophy)

FBQueue は、以下の原則に基づいて設計・実装されています。

*   **ゼロ依存 (Zero Dependency)**: Rust 標準ライブラリのみを使用し、外部クレートに一切依存しません。
*   **シングルバイナリ (Single Binary)**: 実行ファイル一つで Linux および Windows に対応します。
*   **透過性 (Transparency)**: すべてのジョブ状態はファイルシステム上のディレクトリ構造に保存され、OS標準コマンドで確認可能です。
*   **オートオフ (Auto-Off Daemon)**: アイドル状態が 5 分続くとデーモンは自動終了し、不要なリソース消費を抑えます。
*   **堅牢性 (Robustness)**: デーモンがクラッシュしてもジョブの状態は保持され、再起動時に自動復旧・再開されます。

## 2. ディレクトリ構造 (Directory Structure)

FBQueue は、`FBQUEUE_DIR` 環境変数で指定された場所、または `~/.fbqueue/` を使用します。

```
.fbqueue/
├── config                     # 全体の設定ファイル (capacity, queue定義)
├── queue/                     # ジョブキュー
│   ├── new/                   # 待機中
│   ├── running/               # 実行中
│   ├── done/                  # 正常終了
│   ├── failed/                # 失敗・強制終了
│   └── cancel/                # キャンセルシグナル
├── logs/                      # (旧) ログ出力先
└── run/                       # 実行時管理ファイル
    ├── daemon.pid             # デーモンのプロセスID
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

*   **`capacity`**: マシン全体の許容リソース量。`config` で設定。
*   **`cost`**: 各ジョブが消費するリソース量。投入時に `-c` で指定。
*   **判定**: `used_capacity + job.cost <= capacity` の場合のみジョブを開始します。

## 5. 複数キューと優先度

`~/.fbqueue/config` で定義します。

```text
capacity: 16
default_queue: batch

queue: batch
  capacity: 8
  priority: 10

queue: express
  capacity: 4
  priority: 100
```

## 6. 高度なスケジューリング機能

### 安全な実行方式 (PBS Style Execution)
ジョブスクリプトの実行権限 (`+x`) を書き換えることなく、Unix では `sh`、Windows では `cmd /c` を介して実行します。これによりオリジナルファイルを汚さずにシバン (`#!`) に基づいた実行が可能です。

### パスの自動補完
`fbqueue sub script.sh` のように投入した際、カレントディレクトリにファイルがあれば自動的に `./script.sh` として登録されます。

### 実行制限と依存関係
- **Walltime (`-W`)**: 指定時間を超えたジョブを自動強制終了。
- **Dependency (`-hold_jid`)**: 指定したジョブ ID が `done` になるまで待機。
- **Delayed Start (`-a`)**: 指定時刻まで待機。

## 7. ジョブスクリプトのプリパース

スクリプト内の `#PBS`, `#$`, `#SBATCH` ディレクティブを解析します。
- `-N`, `-J`: ジョブ名
- `-c`, `-n`, `-pe smp`: コスト
- `-q`: キュー名
- `-o`, `-e`: 出力パス
- `-hold_jid`: 依存関係
- `-l h_rt=`: 実行時間制限

## 8. ToDo / Roadmap

*   **バックグラウンド・アーカイブ**: アイドル時の古いファイルの `tar.gz` 化。
*   **ディレクトリ探索の柔軟化**: 親ディレクトリを遡る `.fbqueue` 探索。
*   **詳細サマリー**: `stat` での統計表示。

---
*Saturday, February 21, 2026 - Comprehensive Manual Update.*