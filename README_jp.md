# FDS

[English](README.md) | 日本語 | 🌐 [ウェブサイト](https://tsucky230.github.io/fds/ja/)

キーボード操作を中心にした、軽量・高速なTUIファイルマネージャーです。
FD、FDclone、FileMente、Mielなどのファイラーが持っていた「考えるより先に操作できる軽快さ」を、
Unicode・SSH・現代のターミナル環境向けに作り直しています（既存製品の移植ではありません）。

現在は Phase 4（Supporter Infrastructure）までを実装しています。コピー・移動・削除（ゴミ箱も可）・名前変更・新規フォルダ作成・検索・外部エディタでの編集に加え、ZIP・tar系の閲覧・作成・展開ができます。Pack（外部コマンド）で表示を追加できます。0.5.0 からは、一覧の右にカーソル位置の項目のプレビュー（テキスト・フォルダ・Hex・画像）が出ます。

## 特徴

- **Single-pane first**: 画面上の「現在位置」は常に1つ
- **Keyboard-first**: すべての操作がキーボードで完結。ファンクションキーが使えない環境向けに代替キーもあります
- **SSH / headless対応**: X11・Wayland・デスクトップ環境は不要。SSH先のサーバーでもそのまま動きます
- **Universal Viewer**: F3（またはファイルの上で Enter）で、テキスト・Markdown・16進・画像（PNG / JPEG）を表示。Shift_JIS などの文字コードも自動判定します
- **File Operations**: Copy / Move / Delete / Rename / 新規フォルダ / 検索 / 外部エディタ。Copy は一時名に書いてから公開し、途中で失敗しても壊れた file を残しません。Move は別ドライブでもコピー完了後にだけ元を消します。同名は 上書き / スキップ / 別名 / 中止 を選べます
- **基本アーカイブ**: ZIP / tar / tar.gz / tgzを追加ツールやSupporterライセンスなしで階層閲覧・作成・展開できます
- **迷いにくい画面**: 貼り付け先の選択では、上部の帯に「何を・どの操作で・貼り付け先・元との関係」を表示。全体を枠で囲み、長い一覧にはスクロールバーを出します
- **多言語対応**: 英語・日本語を内蔵。言語ファイルを置けば追加・上書きできます
- **terminalを壊さない**: 正常終了・エラー・panic・signal・SSH切断のどの場合もterminalの状態を元に戻します

## インストール

### Homebrew（macOS / Linux）

```bash
brew install tsucky230/tap/fds
```

[tsucky230/homebrew-tap](https://github.com/tsucky230/homebrew-tap) の formula がタグのソースからビルドするので、初回は数分かかります。

### ソースから

Rust（stable）が必要です。リポジトリのルートで次を実行すると、`~/.cargo/bin/fds` にインストールされます。

```bash
cargo install --path .
```

Linux / WSLでRustが入っていない場合:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source ~/.cargo/env
cargo install --path <リポジトリのパス> --locked
```

## 使い方

```bash
fds           # 現在のディレクトリで起動
fds [path]    # 指定したディレクトリで起動
fds --version # バージョンを表示（-V でも可）
fds --help    # 使い方・キー一覧・設定ファイルの場所（-h でも可）
```

`--version` などと同じ名前のフォルダを開くときは `fds -- --version` のように `--` を前に付けます。
起動中のバージョンは F1 のヘルプ画面の見出しにも出ます。

### キー一覧

| キー | 代替キー | 動作 |
| --- | --- | --- |
| ↑ / ↓ | k / j | カーソル移動 |
| PageUp / PageDown | Ctrl+B / Ctrl+F | 1ページ移動 |
| Home / End | g / G | 先頭 / 末尾へ |
| Enter | l | ディレクトリに入る |
| Backspace | h / ← | 親ディレクトリへ |
| Space | | マークの切り替え |
| F1 | ? | キー一覧 |
| Ctrl+R | | 再読み込み |
| . | | 隠しファイルの表示切り替え |
| i | | 情報パネル（権限・属性・サイズ・日時など）の表示切り替え |
| P | | 右のプレビューの表示切り替え |
| Tab | | ディレクトリの中身の一覧へカーソルを移す（右のプレビューを上下に分け、下に中身を表示。Tab / Esc で戻る） |
| o | | OS の既定のアプリで開く（実行できるファイルは確認し、y のときだけ実行） |
| ! | | コマンドを打って、今のフォルダで実行 |
| x | | マーク（なければカーソル位置）に対してコマンドを打って実行。実行ファイルの上なら引数を足して実行 |
| Ctrl+P | | 一覧のプレビュー画像の表示方式を切り替え |
| F10 | q / Esc | 終了の確認を出す（y または Enter で終了、n / Esc で戻る。Ctrl+C は確認なしで終了） |
| F3 / Enter | v | ファイルを Viewer で表示 |
| F4 | e | 外部エディタで編集 |
| F5 | c | コピー |
| F6 | m | 移動 |
| F8 | d | 削除（確認あり。既定は完全削除、設定でゴミ箱へ移せる） |
| F2 | r | 名前変更 |
| F7 | n | 新規フォルダ |
| / | | 検索（入力しながら移動、Esc で元の位置へ） |
| L | F9 | ドライブ・ホーム・マウント先の一覧（貼り付け先の選択中に別ドライブへ移る） |
| p | | マークした項目（マークがなければカーソル位置）からZIP・tar系アーカイブを作成 |
| u | | カーソル位置のZIP・tar系アーカイブを新しいフォルダに展開 |

Unix系では Ctrl+Z で一時停止し、`fg` で戻れます。

コピー・移動は「貼り付け先の選択」に入ります。一覧を移動して、先頭の「ここに Copy / Move」行で Enter を押すと実行します（フォルダの上の Enter はそのフォルダを開く）。Esc で中止します。

実行中は状態行に件数と、分かる場合はバイト数（例: `コピー 0/1  1.1 GiB / 4.0 GiB`）を表示します。

コピーは、貼り付け先に一時名 `.<名前>.fds-partial.<pid>-<番号>` で書いてから正式な名前にします。中止・エラーのときは一時ファイルを消しますが、FDS が異常終了したときは残ることがあります。
残っていた場合は、名前の `<pid>` のプロセスが動いていないこと（Windows はタスクマネージャーの「詳細」タブの PID、Linux / macOS は `ps -p <pid>`）を確かめてから削除してください。動いている FDS が書き込み中の場合があるため、FDS は自動では消しません。

### コマンドの実行

`!` で入力欄を開き、今のフォルダでコマンドを実行します。パイプやワイルドカードも使えます（Unix は `/bin/sh`、Windows は `cmd`）。vim などの対話コマンドも動き、Ctrl+C はそのコマンドだけを止めます。終わると結果を表示し、Enter で一覧に戻って再読み込みします。

`x` は入力欄に内容を入れて開きます。

| 状況 | 入力欄 | 打つ例 |
| --- | --- | --- |
| マークあり | ` {files}` | `tar cf x.tar {files}` |
| 実行ファイルの上 | `./build.sh ` | `./build.sh --release` |
| ほかの項目の上 | ` {file}` | `cp {file} {file}.bak` |

- `{file}` はカーソル位置、`{files}` はマーク（なければカーソル位置）です。ファイル名に空白や記号があっても、そのまま1つの引数として渡します。引用符の中には書けません
- `↑` / `↓` で今回の履歴をたどれます。履歴はファイルに保存しません
- `cd <パス>` だけの行は、FDS の場所を移します
- Windows の `cmd` では、`%` を含む名前は安全に渡せないので実行しません。ネットワーク（UNC）フォルダでも実行しません

### アーカイブ操作

ZIP・tar・tar.gz・tgzのファイルでEnterまたはF3 / vを押すと、中身を階層表示します。Enterでフォルダに入り、Backspaceで上の階層に戻り、Escで閉じます。閲覧画面からも`u`で展開できます。`p`で作成する際は、入力する名前の拡張子で形式を選びます（例: `backup.zip`、`backup.tar`、`backup.tar.gz`）。

展開先はアーカイブと同じ場所の新しいフォルダです。既存のファイルやフォルダを上書きせず、危険なパス・リンク・特殊ファイルを含むアーカイブは拒否します。作成時もリンクや特殊ファイルを含む選択項目は拒否します。Escで処理を中止できます。処理中にFDSが異常終了した場合は、同じ場所に`.fds-archive-partial-`または`.fds-extract-partial-`で始まる一時項目が残ることがあります。

7z・RARは現在未対応で、将来のArchive Packで追加する計画です。

### Viewer のキー

| キー | 動作 |
| --- | --- |
| ↑ ↓ / j k、PageUp PageDown / Ctrl+B Ctrl+F、Home End / g G | スクロール |
| ← → | 横スクロール（折り返しオフのとき） |
| w | 折り返しの切り替え |
| Tab | 同じファイルを別の Viewer で表示（Markdown → テキスト → 16進 など） |
| E | 文字コードを切り替え（自動 → UTF-8 → Shift_JIS → EUC-JP → ISO-2022-JP → UTF-16LE → UTF-16BE → Windows-1252 → 自動）。手で選んだときは状態行に「（手動）」 |
| p | 画像の表示方式を切り替え（half-block / Sixel / Kitty / iTerm2） |
| q / Esc / F3 / v / h / Backspace | 一覧に戻る |
| Ctrl+C | FDS を終了 |

## 設定

設定ファイルの場所:

- Windows: `%APPDATA%\fds\config.toml`
- Linux / macOS: `$XDG_CONFIG_HOME/fds/config.toml`（未設定なら `~/.config/fds/config.toml`）

### 表示言語

既定ではOSの言語に従います（該当する言語がなければ英語）。設定ファイルで固定できます。

```toml
[ui]
language = "ja"
```

### 画像の表示方式

既定では環境から選びます（Windows Terminal は Sixel、Kitty / Ghostty は Kitty、iTerm2 / WezTerm は iTerm2、tmux の中とその他は half-block）。
合わない場合は設定で固定できます。

```toml
[viewer]
image_protocol = "sixel"   # auto / kitty / sixel / iterm2 / halfblocks / none
```

### Viewer の vi 風キー
`[viewer]` の `vi_keys = true` で、Viewer の中の h / l が左右スクロールになる（h では閉じない）。Backspace・q・Esc・F3・v で一覧に戻る。既定は false。

```toml
[viewer]
vi_keys = true
```

### プレビュー
幅80以上の端末では、一覧の右にカーソル位置の項目のプレビューが出る。テキストは先頭の行、フォルダは中の項目、バイナリは種類とサイズの 1 行（Enter で16進表示）、画像（PNG / JPEG）は形式とサイズの下に画像そのもの。既定は表示。`P` で切り替え、`[ui]` の `preview = false` で起動時から消せる。
画像の描き方は Viewer と同じ `[viewer] image_protocol` に従い、一覧では `Ctrl+P`、Viewerでは`p`で切り替えられる。右枠の画像だけ止めたいときは `preview_images = false`（形式とサイズの表示に戻る）。

```toml
[ui]
preview = false
preview_images = false
```

### 枠の表示
画面全体の枠。既定は表示。`[ui]` の `frame = false` で消せる。幅42未満・高さ12未満では枠なし。

```toml
[ui]
frame = false
```

### 削除の方法

既定では F8 は完全削除です（確認画面に「完全に削除しますか?」と出ます）。`trash` にすると、ゴミ箱（Windows はごみ箱、macOS は Finder のゴミ箱、Linux は freedesktop の Trash）へ移します。

```toml
[ops]
delete = "trash"   # permanent（既定） / trash
```

- ゴミ箱へ移せない場所（Windows の USB メモリ・subst / ネットワークドライブ、ごみ箱の容量を超える項目、Trash の無い Linux のファイルシステムなど）では、その項目はエラーとして残ります。完全削除に切り替えることはありません。
- 値が `permanent` / `trash` 以外のときは、ゴミ箱へ移す設定として扱い、起動時に状態行で知らせます。
- 設定ファイル全体の書式が壊れている場合は既定（完全削除）に戻ります。確認画面の文言で、どちらになるかを確かめてください。

### 外部エディタ
`[editor]` の `command`（例 `"code -w"`）と、拡張子別の指定 `[editor.by_extension]`（例 `md = "typora"`）。指定がなければ `VISUAL` → `EDITOR` → OS の既定が使用される。

`[open.by_extension]` で、Enter で開くアプリを拡張子ごとに指定できる（例 `pdf = "zathura"`、`mp4 = { command = "mpv", wait = false }`）。`wait` の既定は true で、終わるまで FDS は待つ。GUI アプリには `wait = false` を使う。F3 / v は常に FDS の中で表示する。

```toml
[editor]
command = "code -w"

[editor.by_extension]
md = "typora"
```

### 言語ファイルの追加・上書き

設定ディレクトリに `lang/<code>.toml` を置くと、言語を追加したり、内蔵の文言を上書きしたりできます。
書式は [lang/en.toml](lang/en.toml) を参照してください。書かなかった項目は英語で表示されます。

## Supporter と Pack

FDS の Core（この README の機能すべて）は無料で使用できます。

### Supporter ライセンス

Supporter ライセンスは、公式 Pack を使うためのものです。設定ディレクトリに `supporter.license` を置くと、起動時にオフラインで確かめます（ネットワークは使いません）。
状態は Help 画面（F1）の下の Supporter 欄で確かめられます。現在のビルドには本番の公開鍵がまだ入っていないため、有効になるライセンスはありません。

### Pack（外部コマンドで表示を追加する）

`<設定ディレクトリ>/packs/<フォルダ>/pack.toml` に、拡張子と、そのファイルを表示するコマンドを書きます。FDS はコマンドの標準出力をテキストとして表示します。
拡張子が合うファイルは Pack の表示で開き、Tab で組み込みの Viewer に切り替えられます。

```toml
name = "pdf-text"
version = "1.0.0"

[[provider]]
id = "pdf-text"
label = "PDF (text)"
extensions = ["pdf"]
command = ["pdftotext", "-layout", "{path}", "-"]
```

- `{path}` と完全に一致する引数だけが、ファイルの絶対パスに置き換わります。shell は通しません。
- 標準出力は 8 MiB まで。タイムアウトは既定 10 秒で、`[providers] timeout_secs`（1〜300）で変えられます。読み込み中に q / Esc で止められます。
- `official = true` の公式 Pack は、署名（`pack.toml.sig`）と、その Pack を含む Supporter ライセンスが揃ったときだけ読み込みます。それ以外（community Pack）はライセンスなしで読み込みます。
- 読み込めなかった Pack と理由は Help 画面に出ます。止めたい Pack は名前かフォルダ名で指定します。

```toml
[packs]
disabled = ["pdf-text"]

[providers]
timeout_secs = 30
```

> [!WARNING]
> Pack のコマンドは、あなたの権限で実行されます。Pack を入れることは、そのコマンドを信頼することと同じです。出所の分からない Pack は入れないでください。

## 動作環境

- OS: Windows / macOS / Linux
- 実行環境: ローカルのターミナル、SSH、tmux、GUI のない Linux
- Rust: stable（edition 2024）。1.95・1.98 で動作を確認

## 開発

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- CI: [.github/workflows/ci.yml](.github/workflows/ci.yml)（Windows / macOS / Linux）

## サポート・貢献

- 📖 **ドキュメント** — [README](README_jp.md) | 🌐 **ディスカッション** — [GitHub Discussions](https://github.com/qraft/fds/discussions)
- 🐛 **不具合報告** — [GitHub Issues](https://github.com/tsucky230/fds/issues/new)
- 🤝 **貢献する** — [CONTRIBUTING](CONTRIBUTING.md)
- ☕ **[GitHub スポンサー](https://github.com/sponsors/tsucky230)** / 💖 **⭐ Star をつける**

## ライセンス合法性

FDS は [GPL-3.0-only](LICENSE) で提供されています。GPL-3.0 の要件に適合するため、このプロジェクトは以下を含みます：

- **[LICENSE](LICENSE)**: GNU General Public License バージョン3の全文
- **[NOTICE](NOTICE.md)**: 314 個の外部依存ライブラリとそのライセンスの表示
- **[sbom/fds.cdx.json](sbom/fds.cdx.json)**: Software Bill of Materials（CycloneDX 1.5）：
  - 全依存ライブラリの完全なツリーとバージョン
  - 各コンポーネントのライセンス情報
  - 脆弱性追跡用の Package URL（purl）と CPE
  - 完全性検証用の SHA-256 ハッシュ

**ライセンス監査の結果：**
- ✓ 314 個の外部依存は全て GPL-3.0-only と互換性あり
- ✓ ライセンス：MIT（68）、Apache-2.0（200+），BSD-3-Clause（3）など
- ✓ ライセンス競合なし
- ✓ 「OR」ライセンスは GPL-3.0 互換のオプション許可（例：MIT OR Apache-2.0）

SBOM は Cargo.toml・Cargo.lock が変わるたび自動で再生成・コミットされるため、継続的なコンプライアンス追跡が実現されています。

## 開発

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- CI: [.github/workflows/ci.yml](.github/workflows/ci.yml)（Windows / macOS / Linux）

## ライセンス

Copyright © 2026 Qraft Node Kyoto

FDS は GNU General Public License バージョン3のみ（`GPL-3.0-only`）で提供するオープンソースソフトウェアです。全文は [LICENSE](LICENSE) を見てください。いかなる保証もありません。

公式 Pack は別のプログラムで、それぞれの条件に従います。依存ライブラリとそのライセンスは [sbom/fds.cdx.json](sbom/fds.cdx.json)（CycloneDX）にあり、GitHub Actions が最新に保ちます。

### ソフトウェア部品表（SBOM）

Cargo.toml・Cargo.lock が変更されるたび、GitHub Actions が自動的に CycloneDX 形式の SBOM を最新に保ちます。

**SBOM の詳細：**
- 形式: CycloneDX 1.5（JSON）
- 場所: [`sbom/fds.cdx.json`](sbom/fds.cdx.json)
- 内容:
  - 314+ 個の外部ライブラリ：ハッシュ、ライセンス、VCS 参照付き
  - 6 個の内部モジュール（ops、viewer、ui、cli、archive、i18n）：著者、ライセンス、部品ハッシュ付き
  - すべての部品に下記を含む：
    - Package URL（purl）：標準的な識別子
    - CPE（Common Platform Enumeration）：脆弱性追跡用
    - SHA-256 ハッシュ：完全性検証用
    - VCS 参照：追跡可能性

**検証方法：**
```bash
# CycloneDX スキーマに照らして検証
curl -L https://raw.githubusercontent.com/CycloneDX/specification/master/schema/bom-1.5.schema.json | \
  python -m json.tool > bom-schema.json
jq --arg sch "$(cat bom-schema.json)" '. as $bom | $sch | {"bom": $bom, "schema": .}' sbom/fds.cdx.json

# 部品の完全性を検証（SHA-256）
jq '.metadata.component.components[] | select(.hashes) | {name, hashes}' sbom/fds.cdx.json
```

**更新周期：** Cargo.toml / Cargo.lock の変更時に GitHub Actions で自動更新。手動再生成は `cargo cyclonedx --format json --target all --spec-version 1.5`
