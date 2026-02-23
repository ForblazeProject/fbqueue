# FBQueue (Forblaze Queue)

![FBQueue Logo Placeholder](https://via.placeholder.com/150/0000FF/FFFFFF?text=FBQueue)

FBQueue は、Rust 製の軽量で堅牢なローカルジョブスケジューラです。共有計算サーバにおいて、ユーザーが自身の計算リソース（CPU/GPU 時間など）を「お行儀よく」管理し、大量の計算ジョブを効率的かつ確実に実行するために設計されています。

## 🎨 特徴 (Features)

- **超軽量＆ゼロ依存**: Rust 標準ライブラリのみで構築。単一バイナリで Linux および Windows に対応。
- **リソース管理**: 抽象的な `capacity` とジョブごとの `cost` を用いて、CPU/GPU などのリソース使用量を柔軟に制御。
- **複数キュー & 優先度**: キューごとに Capacity と Priority を設定可能。高優先度ジョブを優先的に実行。
- **PBS/SGE互換**:
    - **ディレクティブ解析**: `#PBS`, `#$`, `#SBATCH` 等のスクリプト内記述を自動認識。
    - **PBSスタイル出力**: `qstat` 互換の表形式表示。
    - **安全な実行**: ジョブスクリプトの実行権限 (`+x`) を書き換えることなく、シェル経由で安全に実行。
- **バッチ処理**: `--range` や `--list` オプションにより、シンプルなコマンドでジョブを一括投入。
- **耐障害性**: デーモンが突然死しても、再起動時に中断されたジョブを自動検知して再開。

## 🚀 使い方 (Usage)

### ジョブの投入 (sub)
Linux でも Windows でも、カレントディレクトリにあるファイルは名前だけで投入可能です。パスの区切り文字 (`./` や `.\`) は自動で補完されます。

```bash
# Linux
fbqueue sub my_script.sh

# Windows
fbqueue sub my_script.bat
fbqueue sub my_script.ps1
```

### ステータスの確認 (stat)
現在のリソース使用状況と、ジョブの進捗（Pending/Running/Done）を確認できます。

```bash
fbqueue stat
```

### ジョブの削除 (del)
```bash
fbqueue del <job_id>
```

## 🛠️ Windows 環境でのヒント
Windows では、シンボリックリンクやエイリアス（qsub等）を使用するよりも、常に `fbqueue sub`, `fbqueue stat` といった明示的なサブコマンド形式を使用することを推奨します。

## 📂 環境の切り替え
`FBQUEUE_DIR` 環境変数を設定することで、計算プロジェクトごとに独立したキューとリソース枠を持つことができます。

```bash
export FBQUEUE_DIR=/path/to/project_a/.fbqueue
fbqueue sub ./calc.sh
```

---
*Saturday, February 21, 2026 - Verified on Linux and Windows.*