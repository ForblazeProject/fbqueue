# FBQueue (Forblaze Queue) - Development Manual

このドキュメントは、FBQueue の詳細な設計、実装、およびロードマップを記述するものです。

---

## 1. プロジェクトの哲学 (Core Philosophy)

*   **ゼロ依存 (Zero Dependency)**: Rust 標準ライブラリのみを使用。
*   **シングルバイナリ (Single Binary)**: コンパイルされた実行ファイル一つで Linux/Windows に対応。
*   **お行儀の良さ (Auto-Off)**: 仕事がなければ 5 分で自動終了。

## 2. Windows 環境での動作と制限事項

### スクリプトの実行方式
Windows では、ファイル拡張子に基づいて適切なインタプリタを自動選択します：
- **`.bat`, `.cmd`**: `cmd /c` を介して実行されます。
- **`.ps1` (PowerShell)**: `powershell -ExecutionPolicy Bypass -File` を介して実行されます。
- **その他**: 実行可能なバイナリとして直接起動を試みます。

### パスの指定
カレントディレクトリのスクリプトを指定する場合、Windows では自動的に `.\` プレフィックスが付与されます（例: `fbqueue sub test.bat` -> `.\test.bat`）。これは Windows の `cmd.exe` の制限に対応するためです。

### 互換エイリアスの制限
Windows 環境では、`qsub.exe`, `qstat.exe` などのエイリアス（コピー）による呼び出しは、互換性の観点から **非推奨（または未サポート）** とします。Windows ユーザーには常に以下の明示的な形式を推奨します：
- `fbqueue sub ...`
- `fbqueue stat`
- `fbqueue del ...`

## 3. 高度なスケジューリング機能

### 実行制限と依存関係
- **Walltime (`-W`)**: 指定時間を超えたジョブを自動強制終了。
- **Dependency (`-hold_jid`)**: 指定したジョブ ID が成功するまで待機。
- **Delayed Start (`-a`)**: 指定時刻（UNIXタイムスタンプ）まで待機。

---
*Saturday, February 21, 2026 - Windows specific guidelines and PowerShell support added.*
