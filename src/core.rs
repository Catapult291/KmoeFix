//! fix_one / get_unique_dst 的 Rust 移植（对应 src/core.py）。
//!
//! 结构上按 Python 原文件的段落排布，便于日后与原版逐行对拍。

use regex::Regex;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use zip::write::SimpleFileOptions;
use zip::ZipArchive;

/// 输出文件名后缀（与 src/config.py 的 OUT_SUFFIX 一致）。
pub const OUT_SUFFIX: &str = "_修正版";

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

fn err<T>(msg: impl Into<String>) -> Result<T, KmoeError> {
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

/// `os.path.splitext`（Windows 语义）的 Rust 移植。
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

/// 一组（旧 href → 新 href）映射，对应 Python 的 dict（插入序）。
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
    num: Option<u32>,
    img: Option<String>,
    html: String,
}

/// 写 zip 条目，文件名用 raw 字节（默认按 UTF-8 解释）。
fn write_entry_bytes<W: Write + std::io::Seek>(
    zw: &mut zip::ZipWriter<W>,
    name_bytes: &[u8],
    data: &[u8],
    compress: bool,
) -> Result<(), KmoeError> {
    let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| KmoeError {
        msg: format!("zip 文件名不是合法 UTF-8: {:?}", name_bytes),
    })?;
    let opts = SimpleFileOptions::default()
        .compression_method(if compress { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored });
    let result = zw.start_file(&name, opts);
    if let Err(e) = result {
        return Err(KmoeError { msg: format!("写入 {name} 失败: {e}") });
    }
    zw.write_all(data)
        .map_err(|e| KmoeError { msg: format!("写入 {name} 失败: {e}") })?;
    Ok(())
}

/// 读取条目原文（不存在的条目 → None）。
fn try_read<R: Read + std::io::Seek>(z: &mut ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    match z.by_name(name) {
        Ok(mut f) => {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).ok()?;
            Some(buf)
        }
        Err(_) => None,
    }
}

/// 按真实话数重排并重打包单个 EPUB（对应 fix_one）。
pub fn fix_one(src: &str, dst: Option<&str>, log: Option<&dyn Fn(&str)>) -> Result<FixOutcome, KmoeError> {
    let _log = |s: &str| {
        if let Some(l) = log {
            l(s);
        }
    };

    // ---- 目标路径决策（与 Python 完全一致）----
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
        });
    }

    // ---- 编号决策 ----
    let nums: Vec<u32> = entries
        .iter()
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

    // ---- 按话数排序（能力扩展：同时支持 spine 乱序的输入）----
    //
    // 开发初衷是「文件名乱序」：kmoe 下载包的 html/image 是随机文件名，真实
    // 页码在 <title> 里，spine 顺序是对的。此时 entries 按 spine 读入本就是
    // 升序，重命名后文件名 page-001..N 即正确顺序——【无需排序】。
    //
    // 对 spine 也乱的文件，按话数升序重排让产物同时通过回读校验（连续
    // 1..N）。排序对已升序输入是稳定恒等操作，对初衷场景零影响；对
    // spine 乱序输入则把「回滚报错」变成「可修复」。
    //
    // 原版 Python（kmoe_fix_src）没有这段排序：spine 乱序输入一律在回读
    // 校验失败回滚。
    entries.sort_by(|a, b| {
        // cover(0) 置首 → 普通页按 num 升序 → theend(None) 殿后
        let rank = |e: &Entry| -> (u8, Option<u32>) {
            if e.href.ends_with("cover.html") {
                (0, Some(0))
            } else if e.num.is_none() {
                (2, None)
            } else {
                (1, e.num)
            }
        };
        rank(a).cmp(&rank(b))
    });

    let max_page: u32 = nums.iter().max().copied().unwrap_or(max_n);
    let width: usize = if max_page > 0 {
        usize::max(3, max_page.to_string().len())
    } else {
        // 保持原句柄：max_page==0 时 Python 侧已抛“未从任何页面解析到话数”，
        // 但为可读性仍给默认宽度 3。
        3
    };

    // ---- 计算新名字（先确定 img_map 再建 html_map，保证 theend 的图名可见）----
    let mut html_map = OrderedMap::new();
    let mut img_map = OrderedMap::new();
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
        } else {
            let n = num.ok_or_else(|| KmoeError {
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
    let mut new_opf = opf_raw.clone();
    for i in 0..html_map.keys.len() {
        let old = &html_map.keys[i];
        let new = &html_map.vals[i];
        new_opf = new_opf.replace(&format!("href=\"{old}\""), &format!("href=\"{new}\""));
    }
    for i in 0..img_map.keys.len() {
        let old = &img_map.keys[i];
        let new = &img_map.vals[i];
        if old != new {
            new_opf = new_opf.replace(&format!("href=\"{old}\""), &format!("href=\"{new}\""));
        }
    }
    // 按已排序的 entries 重建 spine 段落（能力扩展的一部分：排序后必须把
    // <itemref> 顺序一并重写，否则回读校验必然失败；原版 Python 不重排
    // spine，仅靠 entries 顺序写文件）。
    if let Some(spine_start) = new_opf.find("<spine>") {
        if let Some(spine_end) = new_opf.find("</spine>") {
            let mut spine_block = String::from("<spine>\n");
            for e in &entries {
                spine_block += &format!("  <itemref idref=\"{}\" />\n", e.ref_id);
            }
            spine_block += "</spine>";
            new_opf = format!(
                "{}{}{}",
                &new_opf[..spine_start],
                spine_block,
                &new_opf[spine_end + "</spine>".len()..]
            );
        }
    }

    // ---- nav（可选：找不到 vol.nav 就跳过，与 Python `if nav_name:` 一致）----
    let mut new_nav: Option<String> = None;
    let nav_name: Option<String> = if namelist.iter().any(|n| n == "xml/vol.nav") {
        Some("xml/vol.nav".to_string())
    } else {
        namelist.iter().find(|n| n.ends_with("vol.nav")).cloned()
    };
    if let Some(nn) = &nav_name {
        if let Some(nav_bytes) = try_read(&mut zin, nn) {
            let nav_raw = String::from_utf8_lossy(&nav_bytes).into_owned();
            let mut nav = nav_raw.clone();
            for i in 0..html_map.keys.len() {
                let old = &html_map.keys[i];
                let new = &html_map.vals[i];
                nav = nav.replace(&format!("src=\"../{old}\""), &format!("src=\"../{new}\""));
                nav = nav.replace(&format!("src=\"{old}\""), &format!("src=\"{new}\""));
            }
            new_nav = Some(nav);
        }
    }

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
                write_entry_bytes(&mut zout, new.as_bytes(), &data, true)?;
            }
        }
        // 显式收尾：确保中央目录正确写入
        zout.finish()
            .map_err(|e| KmoeError { msg: format!("写 zip 收尾失败: {e}") })?;
    }

    // ---- 回读校验（对应 Python 的 with zipfile.ZipFile(tmp_dst, "r")）----
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
    _log("  页序已按页码重排完成（不含旋转处理）");

    Ok(FixOutcome { ok: true, dst: dst_path })
}
