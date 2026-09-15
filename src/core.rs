//! 修复流程主体：解析 opf/spine → 按话数排序 → 重命名条目 → 改写引用 → 回读校验。
//!
//! 除重命名/重排外另做一件事：**把站点插在正文中间的卡片页移出正文编号、排到卷末**
//! （详见 [`detect_card_pages`]）。它只在确实判出卡片页时才改变页序，判不出卡片的包
//! 页序保持原样；本模块不碰图片像素（回正在 [`crate::cover`]）。

use crate::cover;
use regex::Regex;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use zip::write::SimpleFileOptions;
use zip::ZipArchive;

/// 输出文件名后缀。
pub const OUT_SUFFIX: &str = "_修正版";

/// 产物里站点卡片页的命名前缀（`html/kmoe-001.html`、`image/kmoe-001.png`）。
/// 命名以字母开头，按文件名排序时落在正文之后、`theend` 之前；重跑产物时也据此识别卡片页。
pub const CARD_PREFIX: &str = "kmoe-";

/// 站点卡片页的判定阈值：实测卡片页 ≈1.000，最像的正文页 ≤0.22（15 个真实卷）。
const CARD_SIMILARITY_MIN: f32 = 0.9;
/// 与 theend 卡比对的条带（相对缩略图的 x0, y0, x1, y1）——站点卡片的页脚 Logo 都在这块。
const CARD_BAND: (f32, f32, f32, f32) = (0.55, 0.85, 1.0, 1.0);
/// 比对前的缩略尺寸与条带栅格（与定阈值的实测一致，勿随意改）。
const CARD_THUMB: (u32, u32) = (240, 320);
const CARD_GRID: (u32, u32) = (48, 16);

/// 侧放页回正的策略。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RotateCover {
    /// 自动判定并回正（**默认**）：只认同名参照图 `<名字>-RAWIMAGE.<ext>`；没有参照图
    /// 或相似度不足时不动该图。要恢复「图片一个像素都不动」用 [`RotateCover::Off`]。
    #[default]
    Auto,
    /// 不做任何图片处理：图片逐字节原样写出。
    /// （站点卡片页的移位与它无关，始终执行——那不是图片处理。）
    Off,
    /// 已判出侧放（同样只认参照图）后按指定角度（顺时针 90/180/270）回正——用于覆盖
    /// 自动判定的方向；它不改变「是否侧放」的判断，没有参照图的图不会被强制旋转。
    /// 对所有判出侧放的页生效（封面、正文第 1 页，以及任何带参照图的页）。
    Fixed(u16),
}

/// [`fix_one_with`] 的可选行为。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FixOptions {
    pub rotate_cover: RotateCover,
}

/// 单文件处理结果：`ok` 时给出写入路径（仅与 CLI 展示相关）。
pub struct FixOutcome {
    /// 是否成功（校验失败会回滚并抛错，走到这里必然是 true）。
    pub ok: bool,
    /// 实际写入的目标路径。
    pub dst: PathBuf,
}

/// fix_one 抛出的错误类型。为便于最终把 RuntimeError 信息透出，
/// 保留一个可直接格式化的描述串。
#[derive(Debug)]
pub struct KmoeError {
    pub msg: String,
}

impl std::fmt::Display for KmoeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for KmoeError {}

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T, KmoeError> {
    Err(KmoeError { msg: msg.into() })
}

fn title_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<title>\s*第\s*(\d+)\s*话</title>").unwrap())
}

fn title_fallback() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<title[^>]*>.*?(\d+).*?</title>").unwrap())
}

fn manifest_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?s)<item[^>]*\sid="([^"]+)"[^>]*href="([^"]+)"[^>]*>"#).unwrap())
}

fn manifest_re2() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?s)<item[^>]*href="([^"]+)"[^>]*id="([^"]+)"[^>]*>"#).unwrap())
}

fn spine_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<itemref[^>]*idref="([^"]+)"[^>]*>"#).unwrap())
}

fn img_src_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?s)<img[^>]+src="\.\./(image/[^"]+)"#).unwrap())
}

fn kmoetag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\s*kmoetag\s*=\s*"[^"]*""#).unwrap())
}

fn raw_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\s*kimageraw\s*=\s*"[^"]*"|\s*raw\s*=\s*"[^"]*""#).unwrap())
}

fn title_text_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)(<title[^>]*>)(.*?)(</title>)").unwrap())
}

fn alt_text_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"alt="([^"]*)""#).unwrap())
}

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+").unwrap())
}

fn ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(href|src)="([^"]*)""#).unwrap())
}

/// `<spine …>` 开标签（可能带属性）。
fn spine_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<spine[^>]*>").unwrap())
}

/// 拆扩展名：只认最后一个 `.`，路径分隔符 `/` 与 `\` 等效。
/// 注意 `image/theend` 这类无扩展名路径返回 `("image/theend", "")`。
fn split_ext(p: &str) -> (&str, &str) {
    let name_start = p.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let tail = &p[name_start..];
    match tail.rfind('.') {
        Some(0) => (p, ""), // 纯隐藏文件：.bashrc → 整个是 stem
        Some(i) => {
            let dot = name_start + i;
            (&p[..dot], &p[dot..])
        }
        None => (p, ""),
    }
}

/// 生成不覆盖已有文件的输出路径（对应 get_unique_dst）。
pub fn get_unique_dst(src: &str) -> PathBuf {
    let (base, ext) = split_ext(src);
    let dst = format!("{base}{OUT_SUFFIX}{ext}");
    if !Path::new(&dst).exists() {
        return PathBuf::from(dst);
    }
    let mut i: u32 = 1;
    loop {
        let cand = format!("{base}{OUT_SUFFIX} ({i}){ext}");
        if !Path::new(&cand).exists() {
            return PathBuf::from(cand);
        }
        i += 1;
    }
}

/// 一组（旧 href → 新 href）映射，保持插入顺序。
struct OrderedMap {
    keys: Vec<String>,
    vals: Vec<String>,
}

impl OrderedMap {
    fn new() -> Self {
        OrderedMap { keys: Vec::new(), vals: Vec::new() }
    }
    fn insert(&mut self, k: String, v: String) {
        if let Some(i) = self.keys.iter().position(|x| *x == k) {
            self.vals[i] = v;
        } else {
            self.keys.push(k);
            self.vals.push(v);
        }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.keys.iter().position(|x| x == k).map(|i| self.vals[i].as_str())
    }
    fn contains_key(&self, k: &str) -> bool {
        self.keys.iter().any(|x| x == k)
    }
}

/// 每个 spine 条目在内存中的中间态。
struct Entry {
    /// spine 中的 idref（重建 spine 段落时用）。
    ref_id: String,
    href: String,
    /// 从 `<title>` 解析出的源编号（正文页用；cover / theend / 卡片页为 0 或 None）。
    num: Option<u32>,
    img: Option<String>,
    html: String,
    /// 站点卡片页（Kmoe 插在正文里的站点卡）：不参与正文编号，产物里排到卷末。
    card: bool,
}

/// 取路径的文件名部分（`html/a.html` → `a.html`）。
fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// 正文页：既不是 cover / theend，也不是站点卡片页。
fn is_story_page(e: &Entry) -> bool {
    !e.card && !e.href.ends_with("cover.html") && !e.href.ends_with("theend.html")
}

/// 把 html 与图片两套映射并成一张（html 的新旧名字与图片不重叠）。
fn merge_refs(html_map: &OrderedMap, img_map: &OrderedMap) -> OrderedMap {
    let mut out = OrderedMap::new();
    for i in 0..html_map.keys.len() {
        out.insert(html_map.keys[i].clone(), html_map.vals[i].clone());
    }
    for i in 0..img_map.keys.len() {
        out.insert(img_map.keys[i].clone(), img_map.vals[i].clone());
    }
    out
}

/// 一次扫过 `href="…"` / `src="…"`，逐个查映射并替换（`../` 前缀原样保留）。
/// 未在映射里的引用原样返回——所以每个引用只会被改一次，不会连锁改第二遍。
fn rewrite_refs(text: &str, refs: &OrderedMap) -> String {
    ref_re()
        .replace_all(text, |c: &regex::Captures| {
            let (attr, value) = (&c[1], &c[2]);
            let (prefix, path) = match value.strip_prefix("../") {
                Some(rest) => ("../", rest),
                None => ("", value),
            };
            match refs.get(path) {
                Some(new) => format!("{attr}=\"{prefix}{new}\""),
                None => c[0].to_string(),
            }
        })
        .into_owned()
}

/// 写 zip 条目，文件名用 raw 字节（默认按 UTF-8 解释）。
pub(crate) fn write_entry_bytes<W: Write + std::io::Seek>(
    zw: &mut zip::ZipWriter<W>,
    name_bytes: &[u8],
    data: &[u8],
    compress: bool,
) -> Result<(), KmoeError> {
    let method = if compress { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
    write_entry_bytes_with(zw, name_bytes, data, method)
}

/// 同上，但显式指定压缩方式。
pub(crate) fn write_entry_bytes_with<W: Write + std::io::Seek>(
    zw: &mut zip::ZipWriter<W>,
    name_bytes: &[u8],
    data: &[u8],
    method: zip::CompressionMethod,
) -> Result<(), KmoeError> {
    let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| KmoeError {
        msg: format!("zip 文件名不是合法 UTF-8: {:?}", name_bytes),
    })?;
    let opts = SimpleFileOptions::default().compression_method(method);
    let result = zw.start_file(&name, opts);
    if let Err(e) = result {
        return Err(KmoeError { msg: format!("写入 {name} 失败: {e}") });
    }
    zw.write_all(data)
        .map_err(|e| KmoeError { msg: format!("写入 {name} 失败: {e}") })?;
    Ok(())
}

/// 读取条目原文（不存在的条目 → None）。
pub(crate) fn try_read<R: Read + std::io::Seek>(z: &mut ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    match z.by_name(name) {
        Ok(mut f) => {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).ok()?;
            Some(buf)
        }
        Err(_) => None,
    }
}

/// 按真实话数重排并重打包单个 EPUB（对应 fix_one），不改动任何图片像素。
pub fn fix_one(src: &str, dst: Option<&str>, log: Option<&dyn Fn(&str)>) -> Result<FixOutcome, KmoeError> {
    fix_one_with(src, dst, log, &FixOptions::default())
}

/// 同 [`fix_one`]，另按 `opts` 决定是否回正侧放的页。
pub fn fix_one_with(
    src: &str,
    dst: Option<&str>,
    log: Option<&dyn Fn(&str)>,
    opts: &FixOptions,
) -> Result<FixOutcome, KmoeError> {
    let _log = |s: &str| {
        if let Some(l) = log {
            l(s);
        }
    };

    // ---- 目标路径决策 ----
    let dst_path: PathBuf = match dst {
        None => get_unique_dst(src),
        Some(d) => {
            let d_path = PathBuf::from(d);
            if d_path.exists() {
                if d.contains(OUT_SUFFIX) {
                    // 用户给的 dst 已带后缀：去掉再让 get_unique_dst 自动递增
                    let stripped = d.replace(OUT_SUFFIX, "");
                    let (base, ext) = split_ext(&stripped);
                    get_unique_dst(&format!("{base}{ext}"))
                } else {
                    get_unique_dst(src)
                }
            } else {
                d_path
            }
        }
    };
    let dst_s = dst_path.to_string_lossy().into_owned();

    let src_path = Path::new(src);

    // ---- 打开源 zip，读目录 ----
    let sf = fs::File::open(src_path)
        .map_err(|e| KmoeError { msg: format!("无法打开 {src}: {e}") })?;
    let mut zin = ZipArchive::new(sf)
        .map_err(|e| KmoeError { msg: format!("无法读取 {src}（不是合法 zip）: {e}") })?;
    let namelist: Vec<String> = {
        let mut v = Vec::with_capacity(zin.len());
        for i in 0..zin.len() {
            let raw = match zin.by_index_raw(i) {
                Ok(f) => f.name_raw().to_vec(),
                Err(e) => return err(format!("读取 zip 目录失败: {e}")),
            };
            v.push(String::from_utf8_lossy(&raw).into_owned());
        }
        v
    };

    // ---- 定位 vol.opf ----
    let opf_name: String = if namelist.iter().any(|n| n == "vol.opf") {
        "vol.opf".to_string()
    } else {
        match namelist.iter().find(|n| n.ends_with("vol.opf")) {
            Some(n) => n.clone(),
            None => return err("未找到 vol.opf"),
        }
    };

    let opf_bytes = try_read(&mut zin, &opf_name).ok_or_else(|| KmoeError {
        msg: format!("读取 {opf_name} 失败"),
    })?;
    let opf_raw = String::from_utf8_lossy(&opf_bytes).into_owned();

    // ---- 解析 manifest / spine ----
    let mut manifest: Vec<(String, String)> = manifest_re()
        .captures_iter(&opf_raw)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    if manifest.is_empty() {
        manifest = manifest_re2()
            .captures_iter(&opf_raw)
            .map(|c| (c[1].to_string(), c[2].to_string()))
            .collect();
    }

    let spine: Vec<String> = spine_re()
        .captures_iter(&opf_raw)
        .map(|c| c[1].to_string())
        .collect();
    if spine.is_empty() {
        return err("spine 为空");
    }

    let href_of = |id: &str| -> Option<String> {
        manifest.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone())
    };

    // ---- 逐条目提取话数 / 图名 ----
    let mut entries: Vec<Entry> = Vec::new();
    for ref_ in &spine {
        let href = match href_of(ref_) {
            Some(h) => h,
            None => continue,
        };
        if !href.ends_with(".html") {
            continue;
        }
        let html_bytes = match try_read(&mut zin, &href) {
            Some(b) => b,
            None => continue, // 读不到（KeyError）→ 跳过
        };
        let html = String::from_utf8_lossy(&html_bytes).into_owned();

        let mut m = title_re().captures(&html);
        if m.is_none() {
            m = title_fallback().captures(&html);
            if m.is_some() && (html.contains("THE END") || html.contains("Book Cover")) {
                m = None;
            }
        }
        let num: Option<u32> = match &m {
            Some(mm) => Some(mm[1].parse().map_err(|_| KmoeError {
                msg: format!("话数解析失败: {}", &mm[1]),
            })?),
            None => None,
        };
        let num = if href.ends_with("cover.html") {
            Some(0)
        } else if href.ends_with("theend.html") {
            None
        } else {
            num
        };

        let img: Option<String> = img_src_re()
            .captures(&html)
            .map(|im| im[1].to_string());

        entries.push(Entry {
            ref_id: ref_.clone(),
            href: href.clone(),
            num,
            img,
            html,
            // 产物里的卡片页（重跑时）按命名前缀认出来，不必再比对图片
            card: base_name(&href).starts_with(CARD_PREFIX),
        });
    }

    // ---- 编号决策 ----
    // 只统计正文页：卡片的标题里若是旧页码，不能算进话数上限。
    let nums: Vec<u32> = entries
        .iter()
        .filter(|e| is_story_page(e))
        .filter_map(|e| e.num)
        .filter(|n| *n != 0)
        .collect();
    let max_n: u32 = nums.iter().max().copied().unwrap_or(0);
    if max_n == 0 {
        return err("未从任何页面解析到话数，无法重排");
    }

    for e in entries.iter_mut() {
        if e.href.ends_with("theend.html") {
            e.num = Some(max_n + 1);
        }
    }

    // ---- 站点卡片页判定 ----
    let card_count = detect_card_pages(&mut zin, &mut entries);

    // ---- 按话数排序（能力扩展：同时支持 spine 乱序的输入）----
    //
    // 开发初衷是「文件名乱序」：kmoe 下载包的 html/image 是随机文件名，真实
    // 页码在 <title> 里，spine 顺序是对的。此时 entries 按 spine 读入本就是
    // 升序，重命名后文件名 page-001..N 即正确顺序——【无需排序】。
    //
    // 对 spine 也乱的文件，按话数升序重排让产物同时通过回读校验（连续
    // 1..N）。排序对已升序输入是稳定恒等操作，对初衷场景零影响；对
    // spine 乱序输入则把「回滚报错」变成「可修复」。
    entries.sort_by(|a, b| {
        // cover(0) 置首 → 正文页按 num 升序 → 站点卡片页 → theend 殿后
        let rank = |e: &Entry| -> (u8, u32) {
            if e.href.ends_with("cover.html") {
                (0, 0)
            } else if e.card {
                (2, 0)
            } else if e.href.ends_with("theend.html") {
                (3, 0)
            } else {
                (1, e.num.unwrap_or(u32::MAX))
            }
        };
        rank(a).cmp(&rank(b))
    });

    // 卡片页占掉的号位从正文编号里去掉：正文页新编号 = 源编号 − 排在它前面的卡片数。
    // 只顺延卡片占的号，不填其它空缺——源包真缺页时仍旧缺页（回读校验会拦下）。
    let card_src_nums: Vec<u32> = entries.iter().filter(|e| e.card).filter_map(|e| e.num).collect();
    let out_num = |orig: u32| orig - card_src_nums.iter().filter(|c| **c < orig).count() as u32;

    let max_page: u32 = entries
        .iter()
        .filter(|e| is_story_page(e))
        .filter_map(|e| e.num)
        .map(out_num)
        .max()
        .unwrap_or(0);
    let width: usize = if max_page > 0 {
        usize::max(3, max_page.to_string().len())
    } else {
        // 这个分支不会再走到写盘（前面已按「未从任何页面解析到话数」报错），
        // 宽度取 3 只为兜底。
        3
    };

    // ---- 计算新名字（先确定 img_map 再建 html_map，保证 theend 的图名可见）----
    let mut html_map = OrderedMap::new();
    let mut img_map = OrderedMap::new();
    let mut card_no: u32 = 1;
    for e in &entries {
        let num = e.num;
        if e.href.ends_with("cover.html") {
            if let Some(img) = &e.img {
                img_map.insert(img.clone(), img.clone());
            }
        } else if e.href.ends_with("theend.html") {
            if let Some(img) = &e.img {
                let (_, ext) = split_ext(img);
                let ext = if ext.is_empty() { ".png" } else { ext };
                let new_img = format!("image/theend{ext}");
                img_map.insert(img.clone(), new_img);
            }
        } else if e.card {
            let new_href = format!("html/{CARD_PREFIX}{card_no:0width$}.html");
            if let Some(img) = &e.img {
                let (_, ext) = split_ext(img);
                let ext = if ext.is_empty() { ".png" } else { ext };
                let new_img = format!("image/{CARD_PREFIX}{card_no:0width$}{ext}");
                if !img_map.contains_key(img) {
                    img_map.insert(img.clone(), new_img);
                }
            }
            html_map.insert(e.href.clone(), new_href);
            card_no += 1;
        } else {
            let n = num.map(out_num).ok_or_else(|| KmoeError {
                msg: format!("无法确定 {} 的话数（解析失败且非 cover/theend）", e.href),
            })?;
            let new_href = format!("html/page-{n:0width$}.html");
            if let Some(img) = &e.img {
                let (_, ext) = split_ext(img);
                let ext = if ext.is_empty() { ".jpg" } else { ext };
                let new_img = format!("image/{n:0width$}{ext}");
                if !img_map.contains_key(img) {
                    img_map.insert(img.clone(), new_img);
                }
            }
            html_map.insert(e.href.clone(), new_href);
        }
    }
    // cover / theend 的新路径
    for e in &entries {
        if e.href.ends_with("cover.html") {
            html_map.insert(e.href.clone(), "html/cover.html".to_string());
        } else if e.href.ends_with("theend.html") {
            html_map.insert(e.href.clone(), "html/theend.html".to_string());
        }
    }

    // ---- 改写 opf / nav 中的引用 ----
    //
    // 一次扫过、每个引用只查一次映射。**不能**逐条做全局字符串替换：正文页重编号后
    // 新名字会撞上刚腾出来的旧名字（如卡片让出 `page-150.html`、旧 page-151 改成
    // page-150），逐条替换会把刚写好的名字再改一次，manifest 引用就指错了。
    let refs = merge_refs(&html_map, &img_map);
    let mut new_opf = rewrite_refs(&opf_raw, &refs);
    // 按已排序的 entries 重建 spine 段落：排序后必须把
    // <itemref> 顺序一并重写，否则回读校验必然失败。
    //
    // 注意 `<spine …>` 上可能带属性（真实 kmoe 包就是
    // `<spine page-progression-direction="rtl" toc="ncx" kmoe-pagedirect="rtl">`），
    // 只能按标签匹配、把标签本身原样留下——早期只找字面量 `<spine>`，在真包上
    // 一次都没命中过（spine 顺序实际从未被重排）。
    if let Some(open) = spine_open_re().find(&new_opf) {
        if let Some(rel_end) = new_opf[open.end()..].find("</spine>") {
            let inner_start = open.end();
            let inner_end = inner_start + rel_end;
            let mut spine_block = String::from("\n");
            for e in &entries {
                spine_block += &format!("  <itemref idref=\"{}\" />\n", e.ref_id);
            }
            new_opf = format!(
                "{}{}{}",
                &new_opf[..inner_start],
                spine_block,
                &new_opf[inner_end..]
            );
        }
    }

    // ---- nav（可选：找不到 vol.nav 就跳过）----
    let mut new_nav: Option<String> = None;
    let nav_name: Option<String> = if namelist.iter().any(|n| n == "xml/vol.nav") {
        Some("xml/vol.nav".to_string())
    } else {
        namelist.iter().find(|n| n.ends_with("vol.nav")).cloned()
    };
    if let Some(nn) = &nav_name {
        if let Some(nav_bytes) = try_read(&mut zin, nn) {
            let nav_raw = String::from_utf8_lossy(&nav_bytes).into_owned();
            new_nav = Some(rewrite_refs(&nav_raw, &refs));
        }
    }

    // ---- 侧放页回正（按参照图；`--no-rotate-cover` 时完全跳过）----
    let rotated_imgs = plan_cover_rotation(&mut zin, &namelist, &entries, opts, &_log);

    // ---- 写新 zip ----
    let tmp_dst = format!("{dst_s}.tmp");
    let tmp_path = Path::new(&tmp_dst);

    {
        let mut zout = zip::ZipWriter::new(
            fs::File::create(tmp_path)
                .map_err(|e| KmoeError { msg: format!("无法创建临时文件 {tmp_dst}: {e}") })?,
        );
        // mimetype 必须首条且 STORED
        if let Some(mime_bytes) = try_read(&mut zin, "mimetype") {
            write_entry_bytes(&mut zout, b"mimetype", &mime_bytes, false)?;
        }
        let exclude_html: Vec<&String> = html_map.keys.iter().collect();
        let exclude_img: Vec<&String> = img_map.keys.iter().collect();
        for name in &namelist {
            if name == "mimetype" {
                continue;
            }
            if exclude_html.iter().any(|k| *k == name) || exclude_img.iter().any(|k| *k == name) {
                continue;
            }
            if let Some(nn) = &nav_name {
                if name.as_str() == nn.as_str() {
                    continue;
                }
            }
            if name.as_str() == opf_name {
                continue;
            }
            if let Some(bytes) = try_read(&mut zin, name) {
                write_entry_bytes(&mut zout, name.as_bytes(), &bytes, true)?;
            }
        }
        // 新 opf / nav
        write_entry_bytes(&mut zout, opf_name.as_bytes(), new_opf.as_bytes(), true)?;
        if let (Some(nav), Some(nn)) = (&new_nav, &nav_name) {
            write_entry_bytes(&mut zout, nn.as_bytes(), nav.as_bytes(), true)?;
        }
        // 每个条目写出重命名后的 html
        for e in &entries {
            let new = html_map.get(&e.href).ok_or_else(|| KmoeError {
                msg: format!("内部错误：{} 不在 html_map", e.href),
            })?;
            let mut html = e.html.clone();
            if let Some(old_img) = &e.img {
                if let Some(new_img) = img_map.get(old_img) {
                    html = html.replace(&format!("src=\"../{old_img}\""), &format!("src=\"../{new_img}\""));
                    html = html.replace(&format!("src=\"{old_img}\""), &format!("src=\"{new_img}\""));
                }
            }
            html = kmoetag_re().replace_all(&html, "").into_owned();
            html = raw_re().replace_all(&html, "").into_owned();
            // 卡片页把号位让出去了，后面的正文页编号会前移；标题与 alt 里的页码
            // 跟着改，否则产物里「文件名 145」对「第 146 頁」自相矛盾，回读校验也过不了。
            if e.card {
                html = relabel_page_text(&html, CARD_TITLE);
            } else if let Some(orig) = e.num {
                let n = out_num(orig);
                if n != orig {
                    html = renumber_page_text(&html, n);
                }
            }
            write_entry_bytes(&mut zout, new.as_bytes(), html.as_bytes(), true)?;
        }
        // 图像（去重：同一图被多页引用只写一次）
        let mut written_images: Vec<String> = Vec::new();
        for i in 0..img_map.keys.len() {
            let old = &img_map.keys[i];
            let new = &img_map.vals[i];
            if written_images.contains(new) {
                continue;
            }
            written_images.push(new.clone());
            if let Some(data) = try_read(&mut zin, old) {
                // 判定要回正时换成旋转后的字节；其余图片逐字节原样写回。
                let data = rotated_imgs
                    .iter()
                    .find(|(name, _)| name == old)
                    .map(|(_, rotated)| rotated.clone())
                    .unwrap_or(data);
                write_entry_bytes(&mut zout, new.as_bytes(), &data, true)?;
            }
        }
        // 显式收尾：确保中央目录正确写入
        zout.finish()
            .map_err(|e| KmoeError { msg: format!("写 zip 收尾失败: {e}") })?;
    }

    // ---- 回读校验（重新打开临时产物逐项核对）----
    let mut fail_remove = false;
    let mut fail_msg: Option<String> = None;
    {
        let zf = fs::File::open(tmp_path)
            .map_err(|e| KmoeError { msg: format!("无法打开临时文件回读: {e}") })?;
        let mut zcheck = ZipArchive::new(zf)
            .map_err(|e| KmoeError { msg: format!("回读失败（产物不是合法 zip）: {e}") })?;
        let opf2_bytes = try_read(&mut zcheck, &opf_name).ok_or_else(|| KmoeError {
            msg: format!("回读校验失败：产物缺少 {opf_name}"),
        })?;
        let opf2 = String::from_utf8_lossy(&opf2_bytes).into_owned();

        let spine2: Vec<String> = spine_re()
            .captures_iter(&opf2)
            .map(|c| c[1].to_string())
            .collect();
        let mut manifest2: Vec<(String, String)> = manifest_re()
            .captures_iter(&opf2)
            .map(|c| (c[1].to_string(), c[2].to_string()))
            .collect();
        if manifest2.is_empty() {
            manifest2 = manifest_re2()
                .captures_iter(&opf2)
                .map(|c| (c[1].to_string(), c[2].to_string()))
                .collect();
        }
        let href2_of = |id: &str| -> Option<String> {
            manifest2.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone())
        };

        // spine 顺序必须与写入时的顺序一致（manifest 改写对不对、spine 段落有没有
        // 真的被重排，都在这一步露馅——早期只查页码连续，`<spine …>` 带属性时
        // 段落根本没被替换也照样通过）。
        let want_spine: Vec<String> = entries
            .iter()
            .filter_map(|e| html_map.get(&e.href).map(|s| s.to_string()))
            .collect();
        let got_spine: Vec<String> = spine2.iter().filter_map(|id| href2_of(id)).collect();
        if got_spine != want_spine {
            let diff = got_spine
                .iter()
                .zip(&want_spine)
                .position(|(a, b)| a != b)
                .unwrap_or(got_spine.len().min(want_spine.len()));
            fail_remove = true;
            fail_msg = Some(format!(
                "回读校验失败 spine 顺序与写入不一致（第 {diff} 项：产物 {:?} ≠ 期望 {:?}）",
                got_spine.get(diff),
                want_spine.get(diff)
            ));
        }

        let mut nums2: Vec<u32> = Vec::new();
        for ref_ in &spine2 {
            let href = match href2_of(ref_) {
                Some(h) => h,
                None => continue,
            };
            if !href.ends_with(".html") {
                continue;
            }
            if href.ends_with("cover.html") || href.ends_with("theend.html") {
                continue;
            }
            // 卡片页不占正文编号，标题里也没有编号，跳过（同写盘时的口径）
            if base_name(&href).starts_with(CARD_PREFIX) {
                continue;
            }
            let h = match try_read(&mut zcheck, &href) {
                Some(b) => String::from_utf8_lossy(&b).into_owned(),
                None => continue,
            };
            let m = title_re().captures(&h);
            let m = match m {
                Some(m) => Some(m),
                None => title_fallback().captures(&h),
            };
            match m {
                Some(m) => {
                    if let Ok(n) = m[1].parse::<u32>() {
                        nums2.push(n);
                    }
                }
                None => continue,
            }
        }
        if !nums2.is_empty() {
            let max2 = *nums2.iter().max().unwrap();
            let expected: Vec<u32> = (1..=max2).collect();
            if nums2 != expected {
                let preview: Vec<u32> = nums2.iter().take(10).copied().collect();
                let exp_preview: Vec<u32> = expected.iter().take(10).copied().collect();
                fail_remove = true;
                fail_msg = Some(format!("回读校验失败 spine页码 {:?}... 期望 {:?}", preview, exp_preview));
            }
        }
    }

    if fail_remove {
        let _ = fs::remove_file(tmp_path);
        return err(fail_msg.unwrap_or_else(|| "回读校验失败".to_string()));
    }

    // ---- 原子替换 ----
    fs::rename(tmp_path, &dst_path)
        .map_err(|e| KmoeError { msg: format!("替换目标文件失败 {dst_s}: {e}") })?;
    if opts.rotate_cover == RotateCover::Off {
        // 未开启回正时的固定文案（见 docs/architecture.md）
        _log("  页序已按页码重排完成（不含旋转处理）");
    } else {
        _log("  页序已按页码重排完成");
        // 逐页明细不写日志，这里只报个总数（同图被多页引用只算一张）
        if !rotated_imgs.is_empty() {
            _log(&format!("  已回正 {} 张侧放页", rotated_imgs.len()));
        }
    }
    if card_count > 0 {
        _log(&format!("  已把 {card_count} 张站点卡片页移到卷末"));
    }

    Ok(FixOutcome { ok: true, dst: dst_path })
}

/// 站点卡片页判定（`fix_one` 的「站点卡片页判定」段）。
///
/// Kmoe 按「話」拼卷：每話首尾会插站点自己的卡片（空白 logo 卡 / 作品信息卡），
/// 一卷里有两話时，前一話的卡片就落在正文中间。包里**没有任何标记**能认出它——
/// 文件名、`<title>`、`xml/vol.nav` 标签、html 模板都跟正文页一模一样（已逐项核对），
/// 差别只在图像内容，所以只能看图：把每页图片的**下部条带**与包内 `theend.html`
/// 那张站点尾卡的同一条带比相关度。卡片是同一套模板渲染的，实测 15 个真实卷里
/// 卡片页 ≈1.000、最像的正文页 ≤0.216，阈值取 [`CARD_SIMILARITY_MIN`]。
///
/// 「近白 + 体积小」这类规则**不能用**：实测会把 `[Kmoe][我們的離婚]卷01` 第 2 頁
/// （版本说明页）、`[Kmoe][今際之國的闖關者]卷01` 第 3 頁（目录页）这类合法前置页
/// 一起标成卡片。
///
/// 返回判为卡片的页数。包内没有 theend 卡（拿不到参照）时一律不判，返回 0。
fn detect_card_pages<R: Read + std::io::Seek>(
    zin: &mut ZipArchive<R>,
    entries: &mut [Entry],
) -> usize {
    // 按命名前缀认出来的（重跑产物）先算上
    let mut found = entries.iter().filter(|e| e.card).count();

    let refer_name = entries
        .iter()
        .find(|e| e.href.ends_with("theend.html"))
        .and_then(|e| e.img.clone());
    let Some(refer_name) = refer_name else {
        return found;
    };
    let Some(refer) = try_read(zin, &refer_name).and_then(|b| cover::decode(&b)) else {
        return found;
    };
    let refer_band = card_band(&refer);

    for e in entries.iter_mut() {
        if e.card || !is_story_page(e) {
            continue;
        }
        let Some(img) = e.img.clone() else { continue };
        let Some(cand) = try_read(zin, &img).and_then(|b| cover::decode(&b)) else {
            continue;
        };
        if cover::ncc_of(&card_band(&cand), &refer_band) >= CARD_SIMILARITY_MIN {
            e.card = true;
            found += 1;
        }
    }
    found
}

/// 下部条带的标准化灰度样本：先缩略到 [`CARD_THUMB`]，再按 [`CARD_BAND`] 裁剪，
/// 最后降采样到 [`CARD_GRID`]。纯色条带（无信息）返回全 0，相关度自然为 0。
fn card_band(img: &image::DynamicImage) -> Vec<f32> {
    let thumb = img.thumbnail(CARD_THUMB.0, CARD_THUMB.1);
    let (w, h) = (thumb.width(), thumb.height());
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let x0 = ((w as f32 * CARD_BAND.0).round() as u32).min(w - 1);
    let y0 = ((h as f32 * CARD_BAND.1).round() as u32).min(h - 1);
    let x1 = ((w as f32 * CARD_BAND.2).round() as u32).clamp(x0 + 1, w);
    let y1 = ((h as f32 * CARD_BAND.3).round() as u32).clamp(y0 + 1, h);
    let band = image::imageops::crop_imm(&thumb, x0, y0, x1 - x0, y1 - y0).to_image();
    cover::normalized_samples(&image::DynamicImage::ImageRgba8(band), CARD_GRID.0, CARD_GRID.1)
}

/// 卡片页的标题/alt 文字。卡片不占正文编号，留着源包的旧页码会与正文页码撞车
/// （重跑时还会把它当成占用号位、把后面的页算错），所以统一换成站点名。
const CARD_TITLE: &str = "Kmoe";

/// 把 `<title>` 文本与 `<img alt="…">` 整体换成 `text`。
fn relabel_page_text(html: &str, text: &str) -> String {
    let out = title_text_re().replace(html, |c: &regex::Captures| {
        format!("{}{}{}", &c[1], text, &c[3])
    });
    alt_text_re()
        .replace(&out, |_c: &regex::Captures| format!("alt=\"{text}\""))
        .into_owned()
}

/// 把页面里的页码数字改成 `n`：改 `<title>` 与 `<img alt="…">` 中**第一次出现的数字串**，
/// 其余字符原样保留（各卷标题模板不同，不能用固定格式重建）。
fn renumber_page_text(html: &str, n: u32) -> String {
    let out = title_text_re().replace(html, |c: &regex::Captures| {
        format!("{}{}{}", &c[1], replace_first_number(&c[2], n), &c[3])
    });
    alt_text_re()
        .replace(&out, |c: &regex::Captures| {
            format!("alt=\"{}\"", replace_first_number(&c[1], n))
        })
        .into_owned()
}

fn replace_first_number(s: &str, n: u32) -> String {
    match number_re().find(s) {
        Some(m) => format!("{}{}{}", &s[..m.start()], n, &s[m.end()..]),
        None => s.to_string(),
    }
}

/// 判定并生成「回正后」的图片字节（键 = 源 zip 里的图片名）。
/// 处理 cover.html 的图、正文第 1 页的图，以及所有带同名参照图的页；
/// 任何一步失败都只记日志、不改动该图。正常结果（已正立 / 没有参照图 /
/// 相似度不足 / 已回正）**不逐页写日志**，只由调用方汇总张数。
fn plan_cover_rotation<R: Read + std::io::Seek>(
    zin: &mut ZipArchive<R>,
    namelist: &[String],
    entries: &[Entry],
    opts: &FixOptions,
    log: &dyn Fn(&str),
) -> Vec<(String, Vec<u8>)> {
    if opts.rotate_cover == RotateCover::Off {
        return Vec::new();
    }

    // 候选：cover.html 的图（EPUB 语义封面）、正文第 1 页的图，以及**任何带同名参照图的页**。
    // kmoe 包给每一张侧放的页都留了一张 `<图名>-RAWIMAGE.<ext>` 参照图（不只封面/第 1 页），
    // 有参照图就等于包自己声明「这页是侧放的」；没有参照图的页一律不碰。
    let mut cands: Vec<(String, String)> = Vec::new();
    for e in entries {
        if e.card {
            continue; // 站点卡片页与漫画内容无关，不参与回正
        }
        let Some(img) = &e.img else { continue };
        let role = if e.href.ends_with("cover.html") {
            "封面".to_string()
        } else if e.num == Some(1) {
            "第 1 页".to_string()
        } else if raw_reference(namelist, img).is_some() {
            match e.num {
                Some(n) => format!("第 {n} 页"),
                None => "页图".to_string(),
            }
        } else {
            continue;
        };
        if !cands.iter().any(|(n, _)| n == img) {
            cands.push((img.clone(), role));
        }
    }

    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for (img_name, role) in cands {
        let Some(bytes) = try_read(zin, &img_name) else { continue };
        let Some(cand) = cover::decode(&bytes) else {
            log(&format!("  {role} {img_name} 无法解码，跳过回正"));
            continue;
        };
        let deg = match detect_sideways(zin, namelist, &img_name, &cand) {
            Ok(Some(deg)) => deg,
            // 「没有参照图」「相似度不足」「已正立」都是正常结果，逐页不写日志
            Ok(None) => continue,
            Err(warn) => {
                log(&format!("  {role} {img_name} {warn}"));
                continue;
            }
        };
        // 指定角度只覆盖「自动判出的方向」，不改变「是否侧放」的判断
        let deg = match opts.rotate_cover {
            RotateCover::Fixed(d) => d,
            _ => deg,
        };
        if !matches!(deg, 90 | 180 | 270) {
            log(&format!("  {role} {img_name} 角度 {deg}° 无效，未旋转"));
            continue;
        }
        let is_png = img_name.to_ascii_lowercase().ends_with(".png");
        match cover::encode(&cover::rotate_by(&cand, deg), is_png) {
            Some(data) => out.push((img_name, data)),
            None => log(&format!("  {role} {img_name} 重新编码失败，未旋转")),
        }
    }
    out
}

/// 判定候选页是否需要回正：`Ok(Some(顺时针角度))` = 要转、`Ok(None)` = 不动、
/// `Err(警告)` = 包内有同名参照图却读不出来（异常，值得写日志）。
/// 判定**只有一条依据**：同名参照图 `<名字>-RAWIMAGE.<ext>`（`0` = 已正立）。
/// 没有参照图、参照图读不出或解码失败、相似度不足——一律不旋转；不按尺寸猜方向。
fn detect_sideways<R: Read + std::io::Seek>(
    zin: &mut ZipArchive<R>,
    namelist: &[String],
    img_name: &str,
    cand: &image::DynamicImage,
) -> Result<Option<u16>, String> {
    let Some(refer_name) = raw_reference(namelist, img_name) else {
        // 包没给参照图 = 没声明这页侧放，不动，也不算异常
        return Ok(None);
    };
    let refer_bytes = try_read(zin, &refer_name)
        .ok_or_else(|| format!("参照图 {refer_name} 读取失败，跳过回正"))?;
    let refer = cover::decode(&refer_bytes)
        .ok_or_else(|| format!("参照图 {refer_name} 无法解码，跳过回正"))?;
    match cover::align_to_reference(cand, &refer) {
        Some((deg, _)) if deg != 0 => Ok(Some(deg)),
        // 已正立，或相似度不足 / 宽高比不符：都是「不动」，不写日志
        _ => Ok(None),
    }
}

/// 同名原始跨页图 `<名字>-RAWIMAGE.<ext>`：kmoe 包为每一张侧放的页遗留的参照图。
/// 参照图的扩展名可以与页图不同（见下方实现）。
fn raw_reference(namelist: &[String], img_name: &str) -> Option<String> {
    let (base, ext) = split_ext(img_name);
    let want = format!("{base}-RAWIMAGE{ext}");
    if let Some(n) = namelist.iter().find(|n| **n == want || n.eq_ignore_ascii_case(&want)) {
        return Some(n.clone());
    }
    // 参照图的扩展名可能与页图不同（实测 [Kmoe][電鋸人2] 是页图 `.jpg` + 参照图 `.png`），
    // 所以精确匹配不到时接受任意扩展名的同名参照图；能否用由解码结果决定。
    let prefix = format!("{base}-RAWIMAGE.").to_ascii_lowercase();
    namelist
        .iter()
        .find(|n| n.to_ascii_lowercase().starts_with(&prefix))
        .cloned()
}
