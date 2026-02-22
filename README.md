# FBQueue (Forblaze Queue)

![FBQueue Logo Placeholder](https://via.placeholder.com/150/0000FF/FFFFFF?text=FBQueue)

FBQueue は、Rust 製の軽量で堅牢なローカルジョブスケジューラです。共有計算サーバなどにおいて、ユーザーが自身の計算リソース（CPU/GPU 時間など）を「お行儀よく」管理し、大量の計算ジョブを効率的かつ確実に実行するために設計されています。

単一バイナリで動作し、外部依存ライブラリを一切持たないため、どのような環境でも簡単に導入・運用できます。

## 🎨 特徴 (Features)

- **超軽量＆ゼロ依存**: Rust 標準ライブラリのみで構築。単一バイナリで Linux および Windows に対応。
- **リソース管理**: 抽象的な `capacity` とジョブごとの `cost` を用いて、CPU/GPU などのリソース使用量を柔軟に制御。
- **自動起動＆自動停止**: ジョブ投入時にデーモンが自動起動し、アイドル状態が 5 分続くと自動的に停止。リソースの無駄遣いを防止。
- **PBS互換のログ出力**: ジョブスクリプトからのディレクティブ解析（`#$`, `#PBS`, `#SBATCH`）に対応し、ジョブ名や出力先を自動設定。出力ファイルは PBS スタイル (`.oJobID`, `.eJobID`) でジョブ実行ディレクトリに生成。
- **バッチ処理**: `--range` や `--list` オプションにより、シンプルなコマンドで数百〜数千のジョブを一括投入。
- **堅牢性**: ディレクトリベースのキュー管理（Maildir 形式）により、デーモンがクラッシュしてもジョブの状態を保持し、自動復旧。
- **IDの簡素化**: ジョブIDは 1 から始まる連番で、視覚的にも管理しやすい。
- **互換性**: `qsub`, `qstat`, `qdel` のエイリアスで既存ワークフローとの親和性を提供。

## 🚀 インストール (Installation)

FBQueue は単一バイナリで動作するため、インストールは非常に簡単です。

1.  **Rust のインストール**:
    お使いのシステムに [Rust](https://www.rust-lang.org/tools/install) がインストールされていない場合は、`rustup` を使用してインストールします。
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source $HOME/.cargo/env
    ```

2.  **FBQueue のビルド**:
    プロジェクトのルートディレクトリで以下のコマンドを実行します。
    ```bash
    cd fbqueue
    cargo build --release
    ```
    これにより、`./target/release/fbqueue` に実行可能ファイルが生成されます。

3.  **パスへの追加 (任意)**:
    `fbqueue` コマンドをどこからでも実行できるように、実行可能ファイルを `PATH` の通ったディレクトリ（例: `/usr/local/bin` や `$HOME/.cargo/bin`）にコピーするか、シンボリックリンクを作成します。
    ```bash
    # 例: $HOME/.local/bin にコピー
    mkdir -p $HOME/.local/bin
    cp ./target/release/fbqueue $HOME/.local/bin/
    # $HOME/.local/bin が PATH に含まれていることを確認
    ```

## 💡 使い方 (Usage)

### 基本コマンド

```bash
# ヘルプを表示
fbqueue help

# ジョブの投入 (qsub のエイリアスも可能)
fbqueue sub [-c COST] [-N NAME] [-o OUT] [-e ERR] [--range N-M] [--list A,B,C] <command> [args...]

# ジョブのステータス表示 (qstat のエイリアスも可能)
fbqueue stat

# ジョブの削除/キャンセル (qdel のエイリアスも可能)
fbqueue del <job_id>

# デーモンの操作
fbqueue daemon <start|stop|status>
```

### 例

#### 単一ジョブの投入
```bash
fbqueue sub -N MyFirstJob -c 2 sleep 10
```

#### バッチジョブの投入 (1 から 5 までの連番でジョブを実行)
```bash
fbqueue sub -N Job_{} -c 1 --range 1-5 echo "Processing value {}"
```

#### スクリプトからのディレクティブ解析 (PBS/SGE/Slurm互換)
`my_script.sh` の内容:
```bash
#!/bin/bash
#$ -N SGE_JobName
#$ -pe smp 4
#PBS -o pbs_output.txt
#SBATCH -J Slurm_JobName_Override

echo "Hello from FBQueue job: $FBQ_JOB_ID"
sleep 5
echo "Job finished."
```
ジョブ投入:
```bash
fbqueue sub ./my_script.sh
# -> ジョブ名は "Slurm_JobName_Override" (SBATCHが優先される)
# -> コストは 4
# -> 出力は pbs_output.txt に
```

#### ステータス確認
```bash
fbqueue stat
```

#### ジョブのキャンセル
```bash
fbqueue del 123
```

## ⚙️ 設定 (Configuration)

FBQueue の設定は、ユーザーのホームディレクトリ内の隠しフォルダ `~/.fbqueue/` に保存されます。

### `~/.fbqueue/config`
最大リソースキャパシティを設定できます。
```
# ~/.fbqueue/config の内容例
capacity: 16
```
このファイルを変更すると、デーモンは次のループで自動的に変更を反映します（動的）。

## 🤝 既存ジョブスケジューラとの互換性

FBQueue は、`qsub`, `qstat`, `qdel` というコマンド名で動作するように設計されています。これにより、既存のジョブスケジューラ (PBS, SGE, Slurm など) 用に書かれたスクリプトやワークフローを、最小限の変更で FBQueue に移行できます。

**エイリアス設定 (自己責任)**:
シェルの設定ファイル (`.bashrc`, `.zshrc` など) や `PATH` を調整することで、既存のコマンドを FBQueue に置き換えることができます。

```bash
# 例: ~/.bashrc または ~/.zshrc に追加
alias qsub="$HOME/.local/bin/fbqueue sub"
alias qstat="$HOME/.local/bin/fbqueue stat"
alias qdel="$HOME/.local/bin/fbqueue del"

# もしくは、実行ファイルを PATH の通ったディレクトリに配置し、シンボリックリンクを作成
# cp ./target/release/fbqueue /usr/local/bin/
# ln -s /usr/local/bin/fbqueue /usr/local/bin/qsub
# ln -s /usr/local/bin/fbqueue /usr/local/bin/qstat
# ln -s /usr/local/bin/fbqueue /usr/local/bin/qdel
```

## 📝 開発メモ (Development Notes)

詳細は `MANUAL.md` を参照してください。

---
*Saturday, February 21, 2026 - Initial implementation and documentation.*
