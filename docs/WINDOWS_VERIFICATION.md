# Windows 実機検証記録

## v0.9.7 候補 — 2026-11-24

`stat` の待機→実行遷移の修正を含む同梱ソースを `cargo build --release --locked --target x86_64-pc-windows-msvc` でビルドし、Windows x64 / Windows PowerShell 5.1.26100.10068 で **21/21 PASS**（FAIL・SKIP・ERROR・NOT_RUN 各0、終了コード0）。追加の `stat` 終了コード・JSON、`--exec` のネイティブEXE起動と `.cmd` 拒否も合格しました。

- プロファイル: `Development`、ハーネス: v3。
- ソースマニフェスト SHA-256: `b90d4237dc87bff3ce0c67bc1874a30bbf2c5a4de1625fa851076d73669cbffb`。
- 実行ファイル SHA-256: `449c4942b5be9f0dd504cd08cb75654f03e01d259097a0c0573d0612de6a1749`。
- 返却結果と配布キットのマニフェストが一致し、照合時点の製品ソースおよび Cargo ファイルはキットと一致。21件の親・ワーカー判定、全ケースのプロセス後始末、対照プロセスが巻き込まれず最後に終了したことを確認しました。

これは候補ソースの実機検証で、GitHub の配布 ZIP に含まれる EXE の実機検証ではありません。

## v0.9.6 向け旧スナップショット — 結論と対象

2026-11-13、Windows別名起動を廃止した未リリース開発スナップショットを、同梱ソースからWindows上で再ビルドし、**19/19ケースが成功**しました。FAIL・SKIP・ERROR・NOT_RUNはいずれも0、終了コード0です。集計値をケース一覧から再計算し、各ワーカーの判定とも一致することを確認しました。

- プロファイル: `Development`
- ハーネス: v3
- OS（ログ値）: `Microsoft Windows NT 10.0.26300.0`
- アーキテクチャ: `AMD64`
- Windows PowerShell: `5.1.26100.9818`
- 検証開始（UTC）: `2026-11-13T07:26:29.0033984Z`
- 検証終了（UTC）: `2026-11-13T07:27:51.8418695Z`
- 配布キット: `fbqueue-windows-test-kit-v3-20261113T072500Z.zip`
- 返却ログ: `FBQueue-Windows-Results-2.zip`

検証時点のスナップショットはv0.9.6の実装を含みますが、Cargoの版数更新前のため、実行ファイルの表示はv0.9.5でした。その後 `Cargo.toml`・`Cargo.lock` をv0.9.6へ更新しています。この記録は、版数更新後のWindowsバイナリそのものの再検証を意味しません。

## 識別情報

| 対象 | SHA-256 |
| --- | --- |
| Windows実行ファイル | `f7a5cfef9327f47574a09e2f8f004243408b12d94697f60aecd9f1e421534baa` |
| 同梱SOURCE-MANIFEST.json | `f837b15b62f5695714eba66de428ef4a8de22c93f89e70547efab40d1e0d206e` |
| 返却ログZIP | `426d5fdb281739506dd6af29ecba3da02a0836a8deaad41823106b098f2a3ab6` |

返却ログのソースマニフェスト識別子は配布キットと一致します。さらに検証結果の受領時点で、マニフェスト内のRustソース・Cargo.toml・Cargo.lockを当時の作業ツリーと照合し、一致を確認しました。作業ツリーは未コミット差分を含むため、Git HEADやバージョン表示だけを検証対象の識別には使いません。検証後の文書更新は、この実行ファイルの再検証を意味しません。

## ケース別結果

| ケース | 結果 |
| --- | --- |
| `help` | PASS |
| `batch_cmd_bat` | PASS |
| `powershell_environment_nodefile` | PASS |
| `unicode_script_path` | PASS |
| `failure_exit_code` | PASS |
| `capacity_fifo` | PASS |
| `dependency_success` | PASS |
| `delayed_start` | PASS |
| `range_list` | PASS |
| `pbs_explicit_history_filters` | PASS |
| `windows_rejects_aliases` | PASS |
| `pending_cancel_history` | PASS |
| `running_cancel_tree` | PASS |
| `walltime_tree` | PASS |
| `windows_daemon_stop_tree` | PASS |
| `development_validation` | PASS |
| `first_run_defaults` | PASS |
| `archive_if_tar_available` | PASS |
| `harness_containment` | PASS |

## 後始末・ログの整合性

- 全ケースで専用Job Object内の活動中プロセス数が0になり、後始末の検証が成功。
- 別のテスト用Job Objectのsentinelはケースの後始末を生存。そのsentinel自体の最終後始末も成功。
- 起動エラーなし、ケースの選別なし。全19ケースを実行。
- 返却ZIPのCRC検査で異常なし。
- この記録は個人のパスや環境変数を転載せず、結果と識別情報だけを保持。

## 保証しないもの

この結果は、この開発スナップショットと記録された環境・ケースに限った実機検証です。公開済みv0.9.5の合格、他のWindows環境での網羅的検証、WindowsでのLinux相当の復旧、安全な同時デーモン起動、実際のMPI/計算ソルバー、ネットワークストレージ・複数ユーザー共有、任意の離脱プロセスや外部副作用の原子性を保証しません。テスト基盤のJob Objectを本体機能とは扱いません。

運用上の制限は [Windows版の実装範囲と制限](WINDOWS_SUPPORT.md) を参照してください。commit・push・公開・新リリースは、この検証には含みません。
