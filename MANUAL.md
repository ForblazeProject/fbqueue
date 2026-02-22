# FBQueue (Forblaze Queue) - Development Manual

このドキュメントは、FBQueue の詳細な設計、実装、および今後の開発ロードマップを記述するものです。

---

## 目次

1.  [プロジェクトの哲学](#1-プロジェクトの哲学)
2.  [ディレクトリ構造](#2-ディレクトリ構造)
3.  [リソース管理 (Capacity & Cost)](#3-リソース管理)
4.  [複数キューと優先度](#4-複数キューと優先度)
5.  [高度なスケジューリング機能](#5-高度なスケジューリング機能)
6.  [ジョブスクリプトのプリパース](#6-ジョブスクリプトのプリパース)
7.  [ToDo / Roadmap](#7-todo--roadmap)

---

## 3. リソース管理 (Capacity & Cost)

FBQueue は、抽象的なリソース管理モデルを採用しています。

*   **`capacity`**: マシン上で利用できる合計リソース。`~/.fbqueue/config` で設定（デフォルトは論理コア数）。
*   **`cost`**: 各ジョブが消費するリソース量。`fbqueue sub -c N` で指定。

## 4. 複数キューと優先度

`~/.fbqueue/config` で複数のキューを定義し、優先度ベースの実行制御が可能です。

```text
# ~/.fbqueue/config
capacity: 16          # マシン全体の最大キャパシティ
default_queue: batch

queue: batch
  capacity: 8         # batchキューで使える最大量
  priority: 10        # 低優先度

queue: express
  capacity: 4         # expressキューで使える最大量
  priority: 100       # 高優先度
```

## 5. 高度なスケジューリング機能

### 実行時間制限 (Walltime)
`-W HH:MM:SS` オプションで、ジョブの最大実行時間を制限できます。制限時間を超えたジョブはデーモンによって自動的に強制終了されます。
```bash
fbqueue sub -W 01:30:00 ./long_task.sh
```

### ジョブ依存関係 (Dependency)
`-hold_jid JOB_ID` オプションで、特定のジョブが成功（Done）するまで実行を待機させることができます。
```bash
fbqueue sub -hold_jid 123 ./dependent_task.sh
```

### 開始時刻指定 (Delayed Start)
`-a TIMESTAMP` オプションで、指定した UNIX タイムスタンプ以降にジョブを開始するように予約できます。
```bash
fbqueue sub -a 1771766817 ./future_task.sh
```

## 6. ジョブスクリプトのプリパース

ジョブスクリプト内のディレクティブ（`#$`, `#PBS`, `#SBATCH`）を解析し、以下のオプションを自動的に読み取ります。

*   `-N`, `-J`: ジョブ名
*   `-c`, `-n`, `-pe smp`: コスト（並列数）
*   `-q`: キュー名
*   `-o`, `-e`: 出力・エラーファイルパス
*   `-hold_jid`: 依存関係
*   `-l h_rt=`: 実行時間制限 (SGE形式)

## 7. ToDo / Roadmap

*   **バックグラウンド・アーカイブ**: アイドル時に古いジョブファイルを `tar.gz` に圧縮。
*   **ディレクトリ探索の柔軟化**: `FBQUEUE_DIR` 環境変数のサポート。
*   **サマリー表示**: `fbqueue stat` での終了コード統計表示。

---
*Saturday, February 21, 2026 - Sequential ID and advanced scheduling implemented.*