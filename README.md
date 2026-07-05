# md-reader

用 Rust 寫的 Markdown 閱讀器。以 [comrak](https://github.com/kivikakk/comrak) 解析
Markdown，內建本地 HTTP 伺服器，在瀏覽器中渲染 LaTeX 數學（MathJax）、Mermaid
圖表、程式碼語法高亮，以及完整的 GFM 語法。檔案存檔後頁面自動重新載入。

## 安裝與使用

```sh
cargo build --release
./target/release/md-reader sample.md          # 開啟檔案（自動打開瀏覽器）
./target/release/md-reader ~/notes            # 開啟目錄（找 README.md / index.md）
./target/release/md-reader doc.md -p 9000 --no-open
```

也可以 `cargo install --path .` 之後直接用 `md-reader <檔案>`。

## 支援的語法

| 類別 | 內容 |
|------|------|
| 數學 | `$…$` 行內、`$$…$$` 區塊（含多行矩陣 / aligned 環境），由 MathJax 3 渲染 |
| 圖表 | ```` ```mermaid ```` 程式碼區塊，由 Mermaid 11 渲染 |
| GFM | 表格、任務清單、刪除線、自動連結、註腳 |
| 警示區塊 | `> [!NOTE]` / `[!TIP]` / `[!IMPORTANT]` / `[!WARNING]` / `[!CAUTION]` |
| 排版 | `__底線__`、`~下標~`、`^上標^`、定義清單、多行引用區塊 |
| 其他 | Emoji shortcode（`:rocket:`）、YAML front matter（自動隱藏）、內嵌 HTML、標題錨點 |
| 高亮 | highlight.js（自動偵測語言，支援深色模式） |

## 運作方式

- Rust 端：comrak 把 Markdown 轉成 HTML；多行 `$$…$$` 區塊會先被抽出保護，
  避免內容被 Markdown 語法（`\\`、`=`、`_` 等）破壞，渲染後再塞回。
- 伺服器只綁 `127.0.0.1`，並擋下跳脫文件目錄的路徑（path traversal）。
- 文件目錄下的圖片、影片等相對路徑資源會直接供應；連到其他 `.md` 檔的
  相對連結也會被渲染成頁面。
- 瀏覽器每秒輪詢 `/__mtime`，檔案變更就自動重新載入。
- MathJax（tex-svg-full）、Mermaid、highlight.js 直接打包在 binary 裡
  （`vendor/`，經由 `/__vendor/*` 供應），**完全離線可用**，不需要任何網路連線。
- 配色採用 Anthropic 品牌規範：奶油白 `#faf9f5` / 墨黑 `#141413` 底色、
  橘 `#d97757` 藍 `#6a9bcc` 綠 `#788c5d` 強調色，深淺色模式都套用，
  Mermaid 圖表也用同一套配色；字型一律使用系統預設（`system-ui`）。

## 範例

`sample.md` 展示了所有支援的語法，可直接開來看：

```sh
cargo run -- sample.md
```
