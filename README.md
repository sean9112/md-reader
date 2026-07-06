# md-reader

用 Rust 寫的 Markdown 閱讀器。以 [comrak](https://github.com/kivikakk/comrak) 解析
Markdown，把文件渲染成**單一自包含的 HTML 檔**（所有樣式與腳本都內嵌），再用
`file://` 在瀏覽器打開——**不開伺服器、不佔用連接埠、沒有背景程序**。在瀏覽器中
渲染 LaTeX 數學（MathJax）、Mermaid 圖表、程式碼語法高亮，以及完整的 GFM 語法。

## 安裝與使用

```sh
cargo build --release
./target/release/md-reader sample.md              # 渲染後自動打開瀏覽器
./target/release/md-reader ~/notes                # 開啟目錄（找 README.md / index.md）
./target/release/md-reader doc.md -o out.html     # 輸出到指定 HTML 檔
./target/release/md-reader doc.md --no-open       # 只印出產生的 HTML 路徑
```

也可以 `cargo install --path .` 之後直接用 `md-reader <檔案>`。

### 在 Finder 中用它開啟 .md

```sh
./install-macos.sh            # 建立 md-reader.app 並安裝到 ~/Applications
./install-macos.sh --default  # 再設成所有 .md 的預設開啟程式
```

之後在 Finder 對 `.md` 按右鍵 ▸ 開啟檔案的應用程式 ▸ md-reader 即可。因為程式
渲染完就結束、沒有常駐程序，關掉瀏覽器分頁不會留下任何東西。

## 支援的語法

| 類別 | 內容 |
|------|------|
| 數學 | `$…$` 行內、`$$…$$` 區塊（含多行矩陣 / aligned 環境），由 MathJax 3 渲染 |
| 圖表 | ```` ```mermaid ```` 程式碼區塊，由 Mermaid 11 渲染 |
| GFM | 表格、任務清單、刪除線、自動連結、註腳 |
| 警示區塊 | `> [!NOTE]` / `[!TIP]` / `[!IMPORTANT]` / `[!WARNING]` / `[!CAUTION]` |
| 排版 | `__底線__`、`~下標~`、`^上標^`、定義清單、多行引用區塊 |
| Obsidian | `![[圖.png]]` 圖片嵌入（含 `\|寬`、`\|寬x高`、`\|替代文字` 變體）；筆記轉嵌與 `[[wikilink]]` 不處理 |
| 其他 | Emoji shortcode（`:rocket:`）、YAML front matter（自動隱藏）、內嵌 HTML、標題錨點 |
| 高亮 | highlight.js（自動偵測語言，支援深色模式） |

## 運作方式

- Rust 端：comrak 把 Markdown 轉成 HTML；多行 `$$…$$` 區塊會先被抽出保護，
  避免內容被 Markdown 語法（`\\`、`=`、`_` 等）破壞，渲染後再塞回。
- 產物是放在系統暫存目錄下的一個 HTML 檔（檔名依來源路徑決定，重開同一檔會覆蓋）。
- 圖片、影片等資源路徑三種寫法都支援：**相對路徑**（以文件所在目錄為基準，
  渲染時改寫成絕對 `file://` 連結）、**絕對路徑**（`/Users/...`）、以及
  **家目錄縮寫**（`~/Pictures/...`）。含空格的路徑在標準語法要用
  `![x](<my pic.png>)` 包起來；Obsidian 嵌入 `![[my pic.png]]` 可直接寫。（連到其他 `.md` 檔的相對連結會直接以原始碼
  開啟，不會再渲染——這是無伺服器模式的取捨。）
- MathJax（tex-svg-full）、Mermaid、highlight.js 直接打包在 binary 裡（`vendor/`），
  並**只在文件真的用到時才內嵌**：純文字筆記只有約 8 KB，有程式碼約 140 KB，
  用到數學/圖表才會變大。全程 **完全離線可用**，不需要任何網路連線。
- 配色採用 Anthropic 品牌規範：奶油白 `#faf9f5` / 墨黑 `#141413` 底色、
  橘 `#d97757` 藍 `#6a9bcc` 綠 `#788c5d` 強調色，深淺色模式都套用，
  Mermaid 圖表也用同一套配色；字型一律使用系統預設（`system-ui`）。

## 範例

`sample.md` 展示了所有支援的語法，可直接開來看：

```sh
cargo run -- sample.md
```
