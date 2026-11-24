# FBQueue Windows 実機テストキット

## まず何を実行するか

ZIP を **展開してから**使ってください。管理者として実行する必要はありません。

この版はハーネスv3です。`fbqueue.exe` の明示コマンドへ統一した開発版は、2026-11-13にWindows AMD64 / PowerShell 5.1で同梱ソースから再ビルドされ、履歴・IDフィルタと別名拒否を含む19/19ケースが成功しました（スキップ・エラー・未実行なし）。詳細はリポジトリの `docs/WINDOWS_VERIFICATION.md` に記録しています。**別名廃止前の.exeではなく、検証対象のソースから再ビルドしてください。**

今回の検証対象は **v0.9.7候補** です。既存の19ケースに加え、`stat` の終了コードとJSON、および Windows での `--exec`（ネイティブEXEの直接実行とバッチファイルの拒否）を確認する計21ケースを実行します。上記の19/19は旧スナップショットの結果です。今回の候補の21/21の結果は [Windows実機検証記録](../../docs/WINDOWS_VERIFICATION.md) を参照してください。

### v0.9.7候補のソースを Windows でビルドして調べる場合（推奨）

1. Windows の Rust / Cargo と MSVC ビルド環境を準備済みであれば、**`Build-And-Test.cmd` をダブルクリック**。
2. 同梱の `source/` に対して `cargo build --release --locked` を実行し、その実行ファイルでテストします。**x64/MSVC (`x86_64-pc-windows-msvc`) と実行ごとの専用 target-dir を明示**するため、別ターゲットや以前の実行ファイルを誤って使いません。ビルド先は結果の `build_output` に記録します。
3. ビルド失敗時も `build.stdout.txt` / `build.stderr.txt` と結果 ZIP を残します。Rust や Visual Studio を自動インストールすることはありません。Cargo が依存情報の取得にネットワークを使う場合はあります。

**このキットには Windows 実行ファイルを同梱していません。** 同梱ソースからのビルド、または手元の Windows 版 `fbqueue.exe` が必要です。

### 同じ v0.9.7 候補の `fbqueue.exe` がある場合

**`Run-Tests.cmd` をダブルクリック**し、実行ファイルのフルパスを入力します。同じディレクトリに `fbqueue.exe` を置いておくこともできます。

PowerShell からは次のように実行できます。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Tests.ps1 -Binary 'C:\path\fbqueue.exe'
```

### 公開版 v0.9.5 を調べる場合

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Tests.ps1 -Binary 'C:\path\fbqueue.exe' -Profile Released095
```

この場合、v0.9.6で導入した4ケースとv0.9.7候補の追加2ケースは SKIP とします。**旧版の結果ではv0.9.7候補を検証できません。** 今回は既定の `Development` プロファイルを使ってください。選択したプロファイルとバイナリの SHA-256 を結果に残します。

## 結果を返す方法

終了画面に表示される **`results\run-...\FBQueue-Windows-Results.zip` を、このチャットに添付してください。** 自動アップロードはしません。失敗した場合も同じ ZIP を渡してください。ケースごとの結果、標準出力・標準エラー・終了コード、ジョブ状態、後始末をこちらで確認できます。

ZIP を作れなかった場合は、同じディレクトリの `results.json`、`run.log`、`cases/` をまとめてください。途中でウィンドウごと強制終了した場合も、各ケース後に保存した `results.json` があれば渡せます。完全な OS 停止時までログ保存を保証するものではありません。

| 判定 | 意味 |
|---|---|
| PASS | そのケースの全アサーションを満たした |
| FAIL | アサーション不成立。実装の問題か環境依存かはログで調べる |
| SKIP | 公開版向けの対象外、または任意の tar がない。合格数には含めない |
| ERROR | 起動・タイムアウト・プロトコル・後始末など基盤の問題 |
| NOT_RUN | 途中停止などで未実行 |

終了コードは **0=実行した必須ケース成功、1=テスト失敗、2=基盤エラー・未完了・ゼロ件実行**。SKIP や未実行を PASS に数えません。

## 条件とテスト対象

- Windows 10/11、Windows PowerShell 5.1。Python / Pester / PowerShell 7 は不要。
- C# の `Add-Type` と Windows Job Object を利用できること。組織ポリシーや親のプロセス制限で利用できなければ、安全性を落として続行せず ERROR にします。
- `cmd` / `powershell` / `tasklist` / `taskkill` / `hostname` を利用できること。`tar.exe` はアーカイブ検証だけの任意条件です。
- 対象は計21ケース。従来の19ケース（スクリプト、履歴、キャンセル、プロセス封じ込めなど）に `stat_contract_097` と `direct_executable_097` を追加しています。
- Linux 専用の pidfd / flock / UNKNOWN 復旧などは要求しません。詳しくは **`WINDOWS_SUPPORT.md`** を参照。
- 通常の所要時間は環境依存。各ケースは最大120秒、全体は15分を目安に打ち切り判定します。セキュリティスキャンなどが遅い環境ではタイムアウトが出る場合があります。

ケースを絞る場合：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Tests.ps1 -Binary 'C:\path\fbqueue.exe' -Case help
```

出力先を変える場合は `-OutputDirectory 'C:\writable\fbqueue-results'` を指定します。選択したケース一覧も結果に残るため、一部だけの成功を全件成功とは扱いません。

## 既存のジョブ・情報の扱い

- テストごとに `%TEMP%` の GUID 付き専用領域と `FBQUEUE_DIR` / HOME 相当の専用パスを使います。既存の `.fbqueue` を読み書きする設計ではありません。
- デーモンやワーカーを**名前で一括検索・停止しません**。各ケースのワーカーを生成時点で専用 Job Object に所属させ、その配下だけを後始末し、活動中プロセス数がゼロになったことを記録します。別のテスト用 Job Object の sentinel が後始末を生き残ることも確認します。
- 子プロセスの環境は許可リストに絞ります。生の `.job`、ユーザー環境変数一覧、HOME は返却 ZIP に含めません。パス・エラー文にユーザー名などが含まれる可能性はあるため、送信前に確認してください。
- テスト用 Job Object は **FBQueue 本体の Windows 機能ではありません**。既存の PowerShell プロファイルや任意のプログラムの副作用まで隔離するセキュリティサンドボックスではありません。本体の `.ps1` 起動は `-NoProfile` を付けないため、プロファイルが結果に影響する可能性があります。
- デバッグ用の専用作業領域は自動削除せず、`results.json` の `scratch` に記録します。結果を確認した後、その GUID の領域だけを手動削除できます。報告 ZIP にこの作業領域全体を入れる必要はありません。

## キット自体の検証について

配布前に PowerShell の構文解析、C# ヘルパーのコンパイル、引数の引用処理、集計・失敗検出・結果 JSON の整合性を検証します。これらは Linux 上でも可能ですが、**Windows 実機でのプロセス管理や FBQueue の合格を意味しません**。Windows の実結果は、返却される ZIP から確認します。

`SOURCE-MANIFEST.json` は同梱ソース・スクリプトの SHA-256 一覧です。ソースはローカルの未コミット差分を含むスナップショットで、Git の HEAD だけでは再現できません。
