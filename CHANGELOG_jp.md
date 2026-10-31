# Changelog

[English](CHANGELOG.md) | 日本語

このプロジェクトの主な変更を記録します。
形式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に従い、バージョンは [Semantic Versioning](https://semver.org/lang/ja/) に従います。

## [Unreleased]

## [0.7.0] - 2026-10-31

FDS の中でコマンドを打って実行できるようにしたリリースです。

### Added
- `!` でコマンドを打って今のフォルダで実行する。パイプ・ワイルドカードも使え、Ctrl+C はコマンドだけを止める。終了後は Enter で一覧に戻り、再読み込みする。`↑` / `↓` で今回の履歴をたどれる（R-412）
- `x` はマーク（` {files}`）・実行ファイル（`./build.sh `）・カーソル位置（` {file}`）を入力欄に入れて開く。ファイル名は引用して渡すので、空白や記号があっても1つの引数のまま届く（R-412）
- `cd <パス>` だけの行は FDS の場所を移す（R-412）

- FDS のライセンスを GPL-3.0-only にした。`LICENSE` を置き、ヘルプ画面と `--help` に表示を足した
- `sbom/fds.cdx.json`: 依存関係の SBOM（CycloneDX）。GitHub Actions が最新に保つ

### Changed
- タイトル帯の著作権表示を `© 2026 Qraft Node Kyoto` にした（R-200-12）
- `README.md` と `CHANGELOG.md` を英語にし、日本語版を `README_jp.md` と `CHANGELOG_jp.md` にした

### Fixed
- `o` や外部エディタなどの実行中に Ctrl+C を押すと、FDS まで終了していた（R-412-7）

## [0.6.0] - 2026-10-31

### Added
- ZIP / tar / tar.gz / tgz を追加ツールなしで閲覧・作成・展開できる。`p` で作成、`u` で展開する
- 画面の一番上に、名前・バージョン・`©Qraft Node Kyoto`・現在時刻の帯を出す（R-200-12）。高さ15未満では出さない
- ディレクトリで Tab を押すと、右のプレビューを上下に分けて上の一覧へカーソルを移し、下にカーソル位置の項目の中身を出す（R-200-13）。ここでは移動と確認だけで、ファイル操作はできない。一覧の先頭に `[..]` があり、そこで Enter を押すか Tab / Esc で左へ戻る。ディレクトリで Enter を押すと左の一覧ごと移動する
- 一覧で q / F10 / Esc を押すと、すぐ終了せず一番下に確認を出す。y または Enter で終了し、n / Esc で戻る（R-200-14）

- `o` で OS の既定のアプリで開く。実行できるファイルは `(y/N)` で確認し、y を押したときだけ実行する（R-110-3）
- `[open.by_extension]` で、Enter で開くアプリを拡張子ごとに設定できる。`wait = false` で待たずに起動する（R-110-4）

### Changed
- バイナリのプレビューを16進からサイズの 1 行に変えた。Enter で今までどおり16進 Viewer を開く
- ディレクトリの表示を `[DIR] name` から `[name]/` に変え、一覧の幅を空けた。太字・シアンで描くので、`[a]` という名前のファイルと区別できる（R-200-11）
- スクロールバーを白い四角から細い線に変え、溝を描かないようにした
- 枠の上辺の ` fds ` の表示をやめた（帯に移した）

## [0.5.1] - 2026-10-25

バージョンの確認と、コマンドラインのヘルプを足したリリースです。

### Added
- `fds --version`（`-V`）でバージョンを、`fds --help`（`-h`）で使い方・キー一覧・設定ファイルの場所を表示する（R-111-2）。`--` の後ろはオプションではなく path として扱う
- ヘルプ画面（F1）の見出しにバージョンを出す（R-111-3）

## [0.5.0] - 2026-10-24

一覧の右にプレビューを出すリリースです。現在位置は一覧の1つのままで、Dual-pane ではありません。

### Added
- プレビュー: 幅80以上の端末では、一覧の右にカーソル位置の項目のプレビューを出す（R-200-9）。テキストと Markdown は先頭 64 KiB、バイナリは先頭を Hex、画像は形式とサイズ、フォルダは中の項目（1000 件まで）。外部 Pack は使わず、FIFO やデバイスは開かない。既定で表示し、`P` で切り替え、`[ui] preview = false` で消せる。現在位置は一覧の1つのままで、Dual-pane ではない
- プレビューの画像: PNG / JPEG は右枠に画像そのものを描く（R-200-10）。16 MiB 以下を読み込みスレッドで縮小（長辺 512px）。描き方は `[viewer] image_protocol` に従い、一覧でも `p` で切り替えられる。`[ui] preview_images = false` で形式とサイズの表示に戻せる
- Viewer の vi 風キー: `[viewer] vi_keys = true` で h / l が左右スクロールになる（R-500-6）。Backspace・q・Esc・F3・v は閉じるまま

### Changed
- 情報パネルは、一覧とプレビューの下に全幅で出る
- Viewer は h / Backspace でも一覧に戻る（一覧の「上の階層へ」と同じ向き。l で開き h で戻る）
- 右枠が画面に出ていないとき（幅80未満、ヘルプや Viewer の表示中）は、プレビューを読み込まない

## [0.4.0] - 2026-10-20

Phase 4（Supporter Infrastructure）です。

### Added
- Supporter ライセンス: 設定ディレクトリの `supporter.license`（`fds-license-v1`）を、本体に埋め込んだ公開鍵で Ed25519 の署名をオフラインで検証する。ネットワークは使わない。ライセンスが無い・壊れていても Core は普段どおり動く。本番の公開鍵はまだ入っていない
- Pack: `<設定ディレクトリ>/packs/<フォルダ>/pack.toml` に書いた外部コマンドで、拡張子ごとの表示を追加する。shell を通さず、`{path}` だけをファイルの絶対パスに置き換える。出力は 8 MiB まで、タイムアウト（既定 10 秒、`[providers] timeout_secs`）と読み込み中の q / Esc で止まる。Tab で組み込み Viewer へ切り替え
- 公式 Pack（`official = true`）は署名（`pack.toml.sig`）とライセンスが揃ったときだけ読み込む。community Pack はライセンスなしで読み込む。`[packs] disabled` で止められる
- Help 画面（F1）の下に Supporter 欄（ライセンスの状態と、各 Pack の状態・読み込まなかった理由）

### Changed
- いちばん合う Viewer で開けず次の Viewer で開いたとき（Pack のコマンドの失敗・タイムアウト、壊れた画像を 16進で開く場合など）、状態行に「（Viewer 名）で表示できません: （理由）」を出す
- Viewer の中のメッセージは次のキーで消え、文字コードと行番号の表示に戻る

## [0.3.1] - 2026-10-20

Phase 3 の保留項目と、Windows 実機での手動確認（M22〜M34）で見つかった不具合の修正です。

### Added
- 設定 `[ops] delete = "trash"` で、F8 の対象をゴミ箱へ移す（Windows はごみ箱、macOS は Finder のゴミ箱、Linux は freedesktop の Trash）。ゴミ箱へ移せない項目はエラーとして残し、完全削除には切り替えない。値が不明なときはゴミ箱扱いにして起動時に知らせる
- Viewer で E を押すと文字コードを切り替える（自動 → UTF-8 → Shift_JIS → EUC-JP → ISO-2022-JP → UTF-16LE → UTF-16BE → Windows-1252 → 自動）。読んでいた位置は保つ。状態行に「（手動）」
- Copy / Move の実行中に、バイト数（例 `1.1 GiB / 4.0 GiB`）も表示
- 一覧の下に情報パネルを表示（`i` で切り替え）: 種類、権限（Unix は `755 rwxr-xr-x` と所有者、Windows は R/H/S/A 属性）、読み取り専用、サイズ、更新・作成日時、symlink のリンク先とリンク切れ、folder の項目数。マーク中は件数と合計サイズも表示
- 一覧と貼り付け先の選択で ← を「親ディレクトリへ」に割り当て（h / Backspace と同じ。Viewer では従来どおり横スクロール）

### Changed
- 下部のキーガイドは一覧と貼り付け先の選択でだけ表示する。確認・競合・操作中・ポップアップなど、一覧のキーが効かない画面では出さない
- ドライブ・場所のポップアップを開いている間も、状態行に Copy / Move の対象件数を表示したままにする
- 狭い画面用のキーガイドで「q 終了」を前に出す（幅 40〜50 で切れていた）
- ヘルプのキー一覧から「(Phase 2)」「(Phase 3)」の表記を削除
- 削除の確認文を「n 件を完全に削除しますか?」に変更（ゴミ箱の設定では「n 件をゴミ箱へ移しますか?」）

### Fixed
- 貼り付け先の選択のキーガイドが「←/h: 上へ」と案内していたのに、← が効かなかった
- 情報パネルで、リンク先のパスが長いと「リンク切れ」の表示が行末で切れて見えなかった
- Copy / Move を中止・失敗したとき、この操作で作った空のフォルダが貼り付け先に残っていた（中身のあるフォルダ・元からあったフォルダは残す）

## [0.3.0] - 2026-09-24

Phase 3（File Operations）のリリースです。

### Added
- F5 / c で Copy、F6 / m で Move、F8 / d で Delete（確認あり）、F2 / r で Rename、F7 / n で新規フォルダ
- / で検索（入力しながら一致する項目へ移る。Esc で元の位置へ戻る）
- F4 / e で外部エディタを開く。設定 `[editor] command` と拡張子別の指定、なければ VISUAL → EDITOR → OS の既定
- Copy / Move は貼り付け先選択に入る。上部の帯に「何を・どの操作で」「貼り付け先のパス」「元との関係（同じ / 上 / 中 / 別ドライブ）」を表示し、一覧先頭の「ここに Copy / Move」行で Enter を押すと実行。下部のキーガイドはカーソル位置で変わる
- L / F9 でドライブ・ホーム・マウント先の一覧をポップアップ（Windows で別ドライブへ移る手段）
- Copy は一時名（`.<name>.fds-partial.<pid>-<job>`）に書いてから公開し、中止・失敗で途中の file を残さない。FAT32 / exFAT でも動く
- Move は同じファイルシステムでは rename、別のファイルシステムでは copy 完了後にだけ元を消す
- 同名がある場合は 上書き / スキップ / 別名（`name (2).ext`）/ 中止 を選べる。「以降すべて」も可
- 実行中は進捗を表示し、Esc で中止。失敗はエラー一覧に表示
- Rename / 新規フォルダ / 検索は枠付きの入力ボックスで表示し、カーソルを反転表示
- 画面全体を罫線の枠で囲む。設定 `[ui] frame = false` で消せる。幅 42 未満または高さ 12 未満では枠なし
- file 一覧が長いとき、右端にスクロールバーを表示
- 対象をマークすると、状態行に使える操作（F5 Copy など）を表示

### Changed
- Phase 2 まで未割り当てだった F2 / F7 などのキーを、操作に割り当てた
- 自分自身の中への Copy / Move は開始前に拒否。symlink はリンク自体を複製し、Delete はリンクだけを消す。同じ folder への Move は何もしない

## [0.2.0] - 2026-09-23

Phase 2（Universal Viewer）のリリースです。

### Added
- F3 / v（または通常ファイルで Enter）で開く全画面のViewer: テキスト・Markdown・16進（Hex）・画像（PNG / JPEG）
- テキストの文字コード自動判定（UTF-8 / UTF-16 / Shift_JIS / EUC-JP / ISO-2022-JP）。判定結果をstatus行に表示
- Tab で同じファイルを別のViewerで表示、w で折り返しの切り替え、← → で横スクロール
- 8 MiB を超えるテキストは先頭 8 MiB を表示。Hex は何GBのファイルでも表示範囲だけを読む
- 画像の表示方式を設定 `[viewer] image_protocol`（auto / kitty / sixel / iterm2 / halfblocks / none）で指定可能
- 画像表示中に p で表示方式を切り替え（half-block / Sixel / Kitty / iTerm2）
- 画像の表示方式を環境から自動選択（Windows Terminal は Sixel、Kitty / Ghostty は Kitty、iTerm2 / WezTerm は iTerm2、tmux 内とその他は half-block）

### Changed
- Ctrl+C はどの画面からでもFDSを終了。q / F10 / Esc はViewerを閉じる

## [0.1.0] - 2026-09-23

Phase 1（Core Filer）の最初のリリースです。

### Added
- Single-paneのディレクトリ一覧（`[..]` → ディレクトリ → ファイルの順、大文字小文字を区別しない自然順）
- カーソル移動（↑↓ / j k、PageUp/PageDown / Ctrl+B Ctrl+F、Home/End / g G）とスクロール
- ディレクトリ移動（Enter / l、Backspace / h）。親へ戻ると直前にいたディレクトリにカーソルを置く
- マーク（Space）。ディレクトリを移動すると解除、再読み込みでは維持
- 隠しファイルの表示切り替え（`.`）、再読み込み（Ctrl+R）、キー一覧（F1 / ?）
- `fds [path]` で起動ディレクトリを指定
- 画面サイズに応じた表示（40x10未満では警告を表示し、戻すと復帰）
- 日本語・絵文字・結合文字を含むファイル名の表示幅計算と切り詰め、制御文字の無害化
- symlinkのリンク先の種別表示（ディレクトリ / ファイル / リンク切れ）
- 多言語対応（英語・日本語を内蔵、`[ui] language` またはOSの言語で切り替え、言語ファイルで追加・上書き）
- 正常終了・エラー・panic・SIGINT/SIGTERM/SIGHUP・SSH切断・Ctrl+Z（Unix）でのterminal状態の復旧
- Windows / macOS / Linux のCI

[0.7.0]: https://github.com/tsucky230/fds/releases/tag/v0.7.0
[0.6.0]: https://github.com/tsucky230/fds/releases/tag/v0.6.0
[0.5.1]: https://github.com/tsucky230/fds/releases/tag/v0.5.1
[0.5.0]: https://github.com/tsucky230/fds/releases/tag/v0.5.0
[0.4.0]: https://github.com/tsucky230/fds/releases/tag/v0.4.0
[0.3.1]: https://github.com/tsucky230/fds/releases/tag/v0.3.1
[0.3.0]: https://github.com/tsucky230/fds/releases/tag/v0.3.0
[0.2.0]: https://github.com/tsucky230/fds/releases/tag/v0.2.0
[0.1.0]: https://github.com/tsucky230/fds/releases/tag/v0.1.0
