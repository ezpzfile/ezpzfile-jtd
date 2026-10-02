# EZPZ File JTD

**日本語** · [English](README.en.md) · [한국어](README.ko.md)

**一太郎の `.jtd` 文書を、Mac・Linux・スマホ・ブラウザで開いて、直して、保存。**
ジャストシステム 一太郎のファイルを扱うための、オープンソースの読み込みエンジン・エディタ・ファイル形式仕様書です。
HWP における [rhwp](https://github.com/edwardkim/rhwp) と同じ考え方で作っています。ファイルは端末の外に送信されません。

状態: **v0.3。`.jtd` に保存し直せる読み込みエンジン + エディタ（開発者向けプレビュー）。**
株式会社ジャストシステムとは関係ありません。

## できること

官公庁などがインターネットで公開している一太郎ファイル 95 件（一太郎 8 から 2018 まで）で確認しています。

- 95 件すべてが開く。1 ファイルあたり約 2 ms
- 本文: 見える文字の 99.9 % が **一太郎ビューアそのもの** の表示と一致（94 件中 87 件は完全一致）。
  ジャストシステムの無料ビューアで自動的に確認
- 編集したファイルを **`.jtd` に保存し直せ**、一太郎ビューアで開ける（下記参照）
- 段落と揃え、インデント、改行幅
- 罫線の表（結合セル、列幅、縦罫線と**横罫線**、行間罫線・通常罫線、**線種**: 太線・点線・二重線など 16 種）
- 文書スタイル: **用紙サイズと向き、余白、字数・行数**、標準の文字サイズ
- 文字サイズ、太字、下線、文字色、ルビ、改ページ
- 文書情報。一太郎がファイルの中に残す **元の保存場所（パス）** も表示
- 書き出し: テキスト、Markdown、HTML、JSON

まだできないこと: 図、縦書き（設定は読めます）、ヘッダ・フッタ、段組、`.jttc`（圧縮）、*新規* 文書の
`.jtd` 保存。詳しくは [企画書](docs/PLAN.ja.md)（[English](docs/PLAN.en.md)、[한국어](docs/PLAN.ko.md)）を参照。

## エディタ

`web/dist/ezpzjtd-editor.html` は 1 ファイルで完結したワープロです。ダブルクリックで開き、`.jtd` を
ウィンドウにドロップして（または新規文書から）編集し、**一太郎（.jtd）**・Word・PDF・HTML・テキストで
保存します。rhwp-studio と同じく、ページの配置と描画はエンジンが自分で行います（canvas）。ブラウザが
受け持つのはキー入力、日本語入力（IME）、画面だけです。

画面とキー操作は一太郎に合わせているので、一太郎の利用者はすぐに使えます。罫線メニューのあるメニューバー、
ツールバー、左のジャンプパレット（ページ、文書情報）、右のツールパレット、字単位のルーラ、`nページ n行 n字`
と 挿入/上書 のステータスバー、編集記号（改行マーク、全角スペースの □）、一太郎のショートカット
（Ctrl+5/6 センタリング/右寄せ、Ctrl+↑/↓ 文字サイズ、Ctrl+Y 改ページ、Ctrl+¥ 表、F7 フォント、
Ctrl+2 名前を付けて保存、Esc メニュー、Ctrl+F の Windows 型 / 一太郎型の切り替え）。
ジャストシステムの画像やアイコンは使っていません。
詳しくは [docs/EDITOR.ja.md](docs/EDITOR.ja.md)（[English](docs/EDITOR.en.md)、[한국어](docs/EDITOR.ko.md)）。

### `.jtd` で保存

`.jtd` から開いた文書は `.jtd` に保存し直せます（Ctrl+S）。エンジンはファイルを作り直さず、
**元のファイルを書き換えます**。そのため、まだ解読していない部分（罫線の座標、隠しフィールド、マクロ、図）は
1 バイトも変わりません。書き換えるのは文字、段落、太字・サイズ・下線・色、揃え、インデント・改行幅、改ページ、
表の行です（新しい行は元の行の線種を引き継ぎます）。
保存のたびに結果を読み直してエディタの内容と比べ、違いがあれば理由を表示して保存を取りやめます
（何も書き込みません。その場合は Word か PDF で保存してください）。

確認には、ジャストシステムの一太郎ビューア 2022 を Wine で自動実行しています
（[tools/taroview](tools/taroview/README.md)）。結果は [experiments/results.md](experiments/results.md)（韓国語）。
さらに **一太郎 最新版の本体**（Wine で実行、[tools/taro2026](tools/taro2026/README.md)）でも確認しています。
エディタで編集して保存したファイル 274 件はすべて一太郎 最新版で開けました。新しく作った表も、一太郎で作った表と同じように
縦罫線・横罫線が描かれます。詳しくは [docs/research/ichitaro-latest.md](docs/research/ichitaro-latest.md)（英語）。

### PDF

PDF 保存では、ページを画面に描いたとおりに書き出し、見えない文字層を重ねます。そのため PDF でも
文字の検索・コピーができます。

## 使ってみる

**ブラウザ（インストール不要）:** 一度ビルドしてから `web/dist/ezpzjtd-editor.html`（エディタ）または
`web/dist/ezpzjtd-viewer.html`（ビューア）をダブルクリックし、`.jtd` ファイルをドロップします。
どちらもオフラインで動きます。

```sh
./web/build.sh
```

**コマンドライン:**

```sh
cd engine
cargo run --release -p ezpzjtd-cli -- text  sample.jtd      # テキスト
cargo run --release -p ezpzjtd-cli -- html  sample.jtd > sample.html
cargo run --release -p ezpzjtd-cli -- md    sample.jtd      # Markdown
cargo run --release -p ezpzjtd-cli -- json  sample.jtd      # 文書モデル
cargo run --release -p ezpzjtd-cli -- info  sample.jtd      # 文書情報、フォント、シート
```

ほかに `ezpzjtd docx <file> <out.docx>`。解析用のコマンド: `streams`、`dump <path>`、`tokens`、`styles`、
`experiment <file> <dir>`（一太郎ビューアで確かめるための変形ファイルを作る）。

**ライブラリとして（Rust）:**

```rust
let doc = ezpzjtd_core::open(std::fs::read("sample.jtd")?)?;
println!("{}", doc.plain_text());
let html = ezpzjtd_core::export::to_html(&doc);
```

**ライブラリとして（JavaScript / WebAssembly）:** `./web/build.sh` のあとの `web/pkg/`。

```js
import init, { JtdDocument } from "./pkg/ezpzjtd_wasm.js";
await init();
const doc = new JtdDocument(new Uint8Array(await file.arrayBuffer()));
element.innerHTML = doc.html();
```

## ファイル形式

[`docs/spec/JTD-FORMAT.md`](docs/spec/JTD-FORMAT.md)（英語）が作業中の仕様書です。
すべての記述に *confirmed / strong / observed / candidate / unknown* の確からしさを付けています。

一太郎なしで解読する方法: 官公庁は同じ様式を `.jtd` **と** `.doc` の両方で公開することがよくあります。
Word 側の書式はわかっているので、2 つのファイルを 1 文字ずつ突き合わせれば、jtd の未知の項目の意味がわかります。
そのためのスクリプトは `tools/research/` にあります。

v0.3 からは **ジャストシステムの無料の一太郎ビューアを審判** にしています。変更したファイルを本物のビューア
（Wine、日本語環境）で開き、表示された文字をコピーして比べます。この方法で、格納庫ごとのストリーム一覧
（`\x04JSRV_SegmentInformation`）がすべてのストリームのサイズと一致していなければならないことを見つけました。

## リポジトリの構成

```
engine/                Rust のワークスペース
  crates/ezpzjtd-core    読み込み（CFB → ブロック格納庫 → レコード → 書式 → モデル）、
                       編集（編集、字×行の格子への配置）、書き出し（docx, pdf, html, md）、
                       保存（save.rs: 元のファイルの書き換え、cfbw.rs: CFB の書き込み）
  crates/ezpzjtd-cli     `ezpzjtd` コマンド
  crates/ezpzjtd-wasm    WebAssembly バインディング
web/                   ブラウザのエディタ（editor.html + editor/app.js）とビューア。1 ファイルにビルド
docs/spec/             ファイル形式の仕様書（英語）
docs/PLAN.*.md         企画書、docs/EDITOR.*.md エディタ設計（日本語、English、한국어）
docs/research/         研究メモ、サンプル作りの手引き
tools/                 コーパスのダウンロードと解析用スクリプト
tools/taroview/        本物の一太郎ビューア（Wine）でファイルを開き、表示内容を読み取る
tools/taro2026/        一太郎 最新版（Wine）で対になるサンプルを作り、保存したファイルを開いて確かめる
experiments/           ビューアでの確認の記録と結果
corpus/manifest.tsv    公開サンプルファイルの URL（ファイル自体はコミットしない）
```

## 協力のお願い

いちばん助かるのは **対になるサンプル** です。一太郎で同じ文書を、設定を 1 つだけ変えて 2 回保存したものです。
作り方は [`docs/research/paired-samples.ja.md`](docs/research/paired-samples.ja.md) を参照してください。
共有する権利のない文書はコミットしないでください。

```sh
python3 tools/fetch_corpus.py --docx   # 公開コーパス → corpus/local/
cd engine && EZPZJTD_CORPUS=$PWD/../corpus/local cargo test
```

## 謝辞

[OpenJTD](https://github.com/KimEJ/OpenJTD) と [Tika-JTD](https://github.com/KHiyowa/Tika-JTD) の
公開研究をもとにしています。[NOTICE](NOTICE) を参照してください。

## ライセンス

MIT または Apache-2.0（選択可）。「一太郎」は株式会社ジャストシステムの商標です。
