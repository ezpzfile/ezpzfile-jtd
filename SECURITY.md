# Security

Please report security problems privately to **hello@ezpzfile.com** (Japanese, English or Korean).
Include the commit or version, what happens, and a sample file if you can. We reply within a few days,
keep the report private until a fix is released, and credit you in the release notes if you wish.

脆弱性やクラッシュの報告は **hello@ezpzfile.com** へ非公開でお送りください（日本語・英語・韓国語）。
修正を公開するまで内容は非公開とし、ご希望があればリリースノートでお名前を記載します。

The engine treats every `.jtd` as untrusted input; see "Opening untrusted files" in
[README.en.md](README.en.md) for the limits and the Web Worker helper.

## Thanks / 謝辞

- **HIDANO Masaki** ([KUKAN](https://github.com/kukan-project/kukan)): reported privately that crafted `.jtd` files could make memory use and page count
  grow far beyond the file size (`SsmgV.01` blocks and `QLSTV.01` pieces listed repeatedly, length fields
  trusted for allocation). Fixed in 0.3.1 and 0.3.2 (2026-11-01).
  細工された `.jtd` でメモリ使用量とページ数がファイルサイズに比べて大きく増える問題を非公開でご報告いただき、0.3.1・0.3.2 で修正しました。
