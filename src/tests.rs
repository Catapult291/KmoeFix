//! fix_one 的行为测试：核心修复流程、侧放页回正、站点卡片页、命令行分发与 GUI 配置映射。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use zip::write::SimpleFileOptions;
use zip::ZipArchive;

use crate::{fix_one, fix_one_with, get_unique_dst, FixOptions, RotateCover};

#[allow(dead_code)]
fn write_zip(path: &Path, entries: &[(&str, &[u8], bool)]) {
    let f = fs::File::create(path).unwrap();
    let mut zw = zip::ZipWriter::new(f);
    for (name, data, compress) in entries {
        zw.start_file(
            *name,
            SimpleFileOptions::default().compression_method(if *compress {
                zip::CompressionMethod::Deflated
            } else {
                zip::CompressionMethod::Stored
            }),
        )
        .unwrap();
        zw.write_all(data).unwrap();
    }
    zw.finish().unwrap();
}

#[allow(clippy::too_many_arguments)]
fn make_epub(
    path: &Path,
    spine_order: &[(&str, &str)],
    with_nav: bool,
    width: Option<usize>,
) -> (Vec<u8>, String) {
    // (href, title_val: "cover"|"theend"|"N")
    let mut manifest_items: Vec<(String, String, String)> = Vec::new();
    let mut spine_refs: Vec<String> = Vec::new();
    let mut html_img_map: Vec<(String, String)> = Vec::new();

    for (idx, (href, tv)) in spine_order.iter().enumerate() {
        let iid = format!("item{idx}");
        manifest_items.push((iid.clone(), href.to_string(), "application/xhtml+xml".to_string()));
        spine_refs.push(iid);
        let img = if href.ends_with("cover.html") {
            "image/cover.jpg".to_string()
        } else if href.ends_with("theend.html") {
            "image/theend.jpg".to_string()
        } else if let Ok(n) = tv.parse::<u32>() {
            format!("image/page-{n}.jpg")
        } else {
            format!("image/page-{idx}.jpg")
        };
        html_img_map.push((href.to_string(), img));
    }
    for (idx, (href, _)) in spine_order.iter().enumerate() {
        let img = html_img_map.iter().find(|(h, _)| h == href).unwrap().1.clone();
        manifest_items.push((format!("img{idx}"), img, "image/jpeg".to_string()));
    }

    let mut opf = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<package version=\"3.0\">\n<manifest>\n");
    for (iid, href, mt) in &manifest_items {
        opf += &format!("  <item id=\"{iid}\" href=\"{href}\" media-type=\"{mt}\" />\n");
    }
    opf += "</manifest>\n<spine>\n";
    for ref_ in &spine_refs {
        opf += &format!("  <itemref idref=\"{ref_}\" />\n");
    }
    opf += "</spine>\n</package>";

    let mut html_contents: Vec<(String, String)> = Vec::new();
    for (href, tv) in spine_order {
        let title = match *tv {
            "cover" => "Book Cover".to_string(),
            "theend" => "THE END".to_string(),
            _ => format!("第{tv}话"),
        };
        let img = html_img_map.iter().find(|(h, _)| h == href).unwrap().1.clone();
        let html = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head><title>{title}</title></head>\n<body>\n<div><img src=\"../{img}\" kmoetag=\"dirty\" kimageraw=\"dirty\" raw=\"dirty\" /></div>\n<p>content for {href}</p>\n</body>\n</html>"
        );
        html_contents.push((href.to_string(), html));
    }

    let nav_content: Option<String> = if with_nav {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<nav><ol>\n");
        for (href, _) in spine_order {
            s += &format!("  <li><a src=\"../{href}\">{href}</a></li>\n");
        }
        s += "</ol></nav>";
        Some(s)
    } else {
        None
    };

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    entries.push(("mimetype".to_string(), b"application/epub+zip".to_vec()));
    entries.push(("vol.opf".to_string(), opf.clone().into_bytes()));
    for (href, html) in &html_contents {
        entries.push((href.clone(), html.clone().into_bytes()));
    }
    for img in html_img_map.iter().map(|(_, i)| i.clone()).collect::<std::collections::HashSet<_>>() {
        entries.push((img.clone(), format!("fake jpeg {img}").into_bytes()));
    }
    if let Some(nav) = &nav_content {
        entries.push(("xml/vol.nav".to_string(), nav.clone().into_bytes()));
    }
    entries.push((
        "META-INF/container.xml".to_string(),
        br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="vol.opf" /></rootfiles></container>"#.to_vec(),
    ));
    if let Some(w) = width {
        entries.push(("padding.txt".to_string(), format!("{:0w$}", 0).into_bytes()));
    }

    let f = fs::File::create(path).unwrap();
    let mut zw = zip::ZipWriter::new(f);
    for (name, data) in &entries {
        let stored = name == "mimetype";
        zw.start_file(
            name,
            SimpleFileOptions::default().compression_method(if stored {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            }),
        )
        .unwrap();
        zw.write_all(data).unwrap();
    }
    zw.finish().unwrap();

    (opf.into_bytes(), String::new())
}

fn read_text(z: &mut ZipArchive<fs::File>, name: &str) -> String {
    let mut buf = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut buf).unwrap();
    String::from_utf8_lossy(&buf).into_owned()
}

use std::io::Read;

fn tdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("KmoeFix_test_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_sorted_epub_fix() {
    let td = tdir("sorted");
    let src = td.join("a.epub");
    let spine = [
        ("html/cover.html", "cover"),
        ("html/page-1.html", "1"),
        ("html/page-2.html", "2"),
        ("html/page-3.html", "3"),
        ("html/theend.html", "theend"),
    ];
    let (_, _) = make_epub(&src, &spine, true, None);

    let out = fix_one(&src.to_string_lossy(), None, None).unwrap();
    assert!(out.dst.exists());
    assert!(out.dst.to_string_lossy().ends_with("_修正版.epub"));
    assert!(src.exists());

    let tmp = format!("{}.tmp", out.dst.display());
    assert!(!Path::new(&tmp).exists());

    let f = fs::File::open(&out.dst).unwrap();
    let mut z = ZipArchive::new(f).unwrap();
    let namelist: Vec<String> = z.file_names().map(|s| s.to_string()).collect();
    assert_eq!(namelist[0], "mimetype");
    assert_eq!(z.by_name("mimetype").unwrap().compression(), zip::CompressionMethod::Stored);
    let mut mime_buf = Vec::new();
    z.by_name("mimetype").unwrap().read_to_end(&mut mime_buf).unwrap();
    assert_eq!(mime_buf, b"application/epub+zip");

    for want in ["html/cover.html", "html/theend.html", "html/page-001.html", "html/page-002.html", "html/page-003.html"] {
        assert!(namelist.iter().any(|n| n == want), "缺少 {want}");
    }
    for gone in ["html/page-1.html", "html/page-2.html", "html/page-3.html"] {
        assert!(!namelist.iter().any(|n| n == gone), "不应存在 {gone}");
    }
    for want in ["image/001.jpg", "image/002.jpg", "image/003.jpg", "image/cover.jpg", "image/theend.jpg"] {
        assert!(namelist.iter().any(|n| n == want), "缺少 {want}");
    }

    for name in &namelist {
        if name.ends_with(".html") {
            let html = read_text(&mut z, name);
            assert!(!html.contains("kmoetag"), "{name} 仍有 kmoetag");
            assert!(!html.contains("kimageraw"), "{name} 仍有 kimageraw");
            assert!(!html.contains(" raw="), "{name} 仍有 raw=");
        }
    }

    let opf = read_text(&mut z, "vol.opf");
    for want in [
        "href=\"html/page-001.html\"",
        "href=\"html/page-002.html\"",
        "href=\"html/page-003.html\"",
        "href=\"html/cover.html\"",
        "href=\"html/theend.html\"",
        "href=\"image/001.jpg\"",
    ] {
        assert!(opf.contains(want), "opf 缺少 {want}");
    }

    if namelist.iter().any(|n| n == "xml/vol.nav") {
        let nav = read_text(&mut z, "xml/vol.nav");
        assert!(nav.contains("html/page-001.html") || nav.contains("page-001"), "nav 未改写");
    }

    // get_unique_dst 递增行为
    let nxt = get_unique_dst(&src.to_string_lossy());
    assert!(nxt.to_string_lossy().ends_with("_修正版 (1).epub"));
    assert_ne!(nxt, out.dst);
    fs::write(&nxt, b"").unwrap();
    let nxt2 = get_unique_dst(&src.to_string_lossy());
    assert!(nxt2.to_string_lossy().ends_with("_修正版 (2).epub"));

    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_shuffled_epub_repairs() {
    // spine 乱序输入按话数升序重排（见 core.rs「按话数排序」注释），应修复成功，
    // 且产物必须通过回读校验（连续 1..N，否则 fix_one 自己会抛错）。
    let td = tdir("shuffled_repair");
    let src = td.join("b.epub");
    let spine = [
        ("html/page-3.html", "3"),
        ("html/page-1.html", "1"),
        ("html/cover.html", "cover"),
        ("html/page-2.html", "2"),
        ("html/theend.html", "theend"),
    ];
    make_epub(&src, &spine, true, None);

    let out = fix_one(&src.to_string_lossy(), None, None);
    assert!(out.is_ok(), "乱序应被修复: {:?}", out.err());
    let dst = out.unwrap().dst;
    assert!(dst.exists());

    let f = fs::File::open(&dst).unwrap();
    let mut z = ZipArchive::new(f).unwrap();
    let opf = read_text(&mut z, "vol.opf");
    let idrefs = crate::test_helpers::spine_idrefs(&opf);
    let hrefs: Vec<String> = idrefs
        .iter()
        .filter_map(|id| {
            opf.find(&format!("id=\"{id}\" href=\""))
                .map(|pos| opf[pos + format!("id=\"{id}\" href=\"").len()..].split('"').next().unwrap().to_string())
        })
        .collect();
    // 顺序应为：cover 置首、page 按话数升序、theend 殿后
    let mut expect = vec!["html/cover.html".to_string()];
    for i in 1..=3 {
        expect.push(format!("html/page-00{i}.html"));
    }
    expect.push("html/theend.html".to_string());
    assert_eq!(hrefs, expect, "spine 未按话数重排");
    // 且各页 title 与文件名一致，回读校验已在 fix_one 内部通过
    for i in 1..=3 {
        let html = read_text(&mut z, &format!("html/page-00{i}.html"));
        assert!(html.contains(&format!("<title>第{i}话</title>")), "page-00{i} 标题不符");
    }
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_get_unique_dst() {
    let td = tdir("unique");
    let a = td.join("a.epub");
    fs::write(&a, b"").unwrap();
    let fixed = td.join("a_修正版.epub");
    fs::write(&fixed, b"").unwrap();

    let result = get_unique_dst(&a.to_string_lossy());
    assert_eq!(result, td.join("a_修正版 (1).epub"));

    // 仅存在原文件
    let td2 = td.join("solo");
    fs::create_dir_all(&td2).unwrap();
    let b = td2.join("b.epub");
    fs::write(&b, b"").unwrap();
    let r2 = get_unique_dst(&b.to_string_lossy());
    assert_eq!(r2, td2.join("b_修正版.epub"));

    // (1) 已存在 → (2)
    fs::write(&result, b"").unwrap();
    let result2 = get_unique_dst(&a.to_string_lossy());
    assert_eq!(result2, td.join("a_修正版 (2).epub"));
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_shuffled_no_cover_theend_repairs() {
    // 乱序且无 cover/theend 也应修复成功（排序键不依赖 cover/theend 存在）
    let td = tdir("repairable_nocent");
    let src = td.join("c.epub");
    let spine = [
        ("html/page-3.html", "3"),
        ("html/page-1.html", "1"),
        ("html/page-2.html", "2"),
    ];
    make_epub(&src, &spine, true, None);

    let out = fix_one(&src.to_string_lossy(), None, None);
    assert!(out.is_ok(), "乱序应被修复: {:?}", out.err());
    let dst = out.unwrap().dst;
    let f = fs::File::open(&dst).unwrap();
    let mut z = ZipArchive::new(f).unwrap();
    let opf = read_text(&mut z, "vol.opf");
    for want in [
        "href=\"html/page-001.html\"",
        "href=\"html/page-002.html\"",
        "href=\"html/page-003.html\"",
    ] {
        assert!(opf.contains(want), "opf 缺少 {want}");
    }
    // 文件名顺序即修复顺序
    let names: Vec<String> = z.file_names().map(|s| s.to_string()).collect();
    let idx1 = names.iter().position(|n| n == "html/page-001.html").unwrap();
    let idx2 = names.iter().position(|n| n == "html/page-002.html").unwrap();
    let idx3 = names.iter().position(|n| n == "html/page-003.html").unwrap();
    assert!(idx1 < idx2 && idx2 < idx3, "产物内 page 文件未按话数顺序排列");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_no_nav_is_optional() {
    // 找不到 xml/vol.nav 不应报错：跳过 nav 改写
    let td = tdir("nonav");
    let src = td.join("nonav.epub");
    let spine = [("html/page-1.html", "1"), ("html/page-2.html", "2")];
    make_epub(&src, &spine, false, None);

    let out = fix_one(&src.to_string_lossy(), None, None);
    assert!(out.is_ok(), "无 nav 时应成功: {:?}", out.err());
    let dst = out.unwrap().dst;
    let f = fs::File::open(&dst).unwrap();
    let mut z = ZipArchive::new(f).unwrap();
    let namelist: Vec<String> = z.file_names().map(|s| s.to_string()).collect();
    assert!(!namelist.iter().any(|n| n == "xml/vol.nav"), "产物不应凭空出现 nav");
    let opf = read_text(&mut z, "vol.opf");
    assert!(opf.contains("html/page-001.html"));
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cli_help_and_version() {
    // 单 exe 分发（无参数=GUI 的分支在 cli.rs 的单测里断言，这里只跑不会开窗口的两个）：
    // --help 输出用法、--version 输出版本，都走同一份解析逻辑
    let Ok(exe) = std::env::var("CARGO_BIN_EXE_KmoeFix") else { return };
    let help = Command::new(&exe).arg("--help").output().unwrap();
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("用法"), "stdout: {text}");

    let ver = Command::new(&exe).arg("--version").output().unwrap();
    assert!(ver.status.success());
    let text = String::from_utf8_lossy(&ver.stdout);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "stdout: {text}");
}

#[test]
fn test_suffix_collision_and_padding() {
    // 已有 (N) 输出时自动递增；三位填充生效
    let td = tdir("pad");
    let src = td.join("d.epub");
    let spine = [("html/page-1.html", "1"), ("html/page-2.html", "2"), ("html/page-3.html", "3")];
    make_epub(&src, &spine, true, None);
    let nxt = get_unique_dst(&src.to_string_lossy());
    fs::write(&nxt, b"").unwrap(); // 模拟已存在输出
    let out = fix_one(&src.to_string_lossy(), None, None).unwrap();
    assert!(out.dst.to_string_lossy().ends_with("_修正版 (1).epub"));
    let f = fs::File::open(&out.dst).unwrap();
    let mut z = ZipArchive::new(f).unwrap();
    let opf = read_text(&mut z, "vol.opf");
    assert!(opf.contains("html/page-001.html"));
    let _ = fs::remove_dir_all(&td);
}

// ---------------- 侧放页回正 ----------------
//
// 这些用例必须用**真图**（上面的 make_epub 写的是假 JPEG 字节，无法解码）。
// 判定只有一条依据：侧放的页在 kmoe 包里会遗留一张同名 `<名字>-RAWIMAGE.jpg`
// 原始跨页图，把候选图的四个朝向与它比相似度即可定方向（2026-09-13 实测：
// 卷 01 顺时针 90° 0.925 / 逆时针 −0.12，卷 02 1.000 / 0.10）。
// 候选 = cover.html 的封面图、正文第 1 页的图，以及任何带同名参照图的页。
// 没有参照图（或相似度不足）**一律不旋转**——曾有的「尺寸兜底」猜方向规则已删除。

/// 有条纹与渐变的真图：内容随坐标变化，方向不同相似度就不同。
fn synth_image(w: u32, h: u32, seed: u32) -> image::RgbImage {
    image::RgbImage::from_fn(w, h, |x, y| {
        let band = if (x * 4 / w.max(1)) % 2 == 0 { 190u32 } else { 60u32 };
        let v = (x * 5 + y * 11 + seed * 37) % 200;
        image::Rgb([(band / 2 + v / 4) as u8, (v / 2 + 30) as u8, (255 - band) as u8])
    })
}

fn encode_jpeg(img: &image::RgbImage) -> Vec<u8> {
    let mut buf = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut std::io::Cursor::new(&mut buf), 95)
        .encode_image(img)
        .unwrap();
    buf
}

fn encode_png(img: &image::RgbImage) -> Vec<u8> {
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img.clone())
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

/// 构造带真图的 EPUB：pages = (href, title 值, 图名, 图字节)，extra 为额外条目（如 RAWIMAGE）。
fn make_epub_real(path: &Path, pages: &[(&str, &str, &str, &[u8])], extra: &[(&str, &[u8])]) {
    make_epub_real_spine(path, pages, extra, "<spine>");
}

/// 同上，但可指定 `<spine …>` 开标签——真实 kmoe 包写的是
/// `<spine page-progression-direction="rtl" toc="ncx" kmoe-pagedirect="rtl">`。
fn make_epub_real_spine(
    path: &Path,
    pages: &[(&str, &str, &str, &[u8])],
    extra: &[(&str, &[u8])],
    spine_open: &str,
) {
    let mut opf =
        String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<package version=\"3.0\">\n<manifest>\n");
    let mut spine = String::from(spine_open);
    spine.push('\n');
    let mut entries: Vec<(String, Vec<u8>)> =
        vec![("mimetype".to_string(), b"application/epub+zip".to_vec())];
    for (i, (href, title, img, bytes)) in pages.iter().enumerate() {
        opf += &format!("  <item id=\"h{i}\" href=\"{href}\" media-type=\"application/xhtml+xml\" />\n");
        opf += &format!("  <item id=\"i{i}\" href=\"{img}\" media-type=\"image/jpeg\" />\n");
        spine += &format!("  <itemref idref=\"h{i}\" />\n");
        let html = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head><title>{title}</title></head>\n<body><div><img src=\"../{img}\" alt=\"{title}\" /></div></body>\n</html>"
        );
        entries.push(((*href).to_string(), html.into_bytes()));
        entries.push(((*img).to_string(), bytes.to_vec()));
    }
    opf += "</manifest>\n";
    opf += &spine;
    opf += "</spine>\n</package>";
    entries.insert(1, ("vol.opf".to_string(), opf.into_bytes()));
    for (n, b) in extra {
        entries.push(((*n).to_string(), b.to_vec()));
    }
    let rows: Vec<(&str, &[u8], bool)> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice(), n != "mimetype"))
        .collect();
    write_zip(path, &rows);
}

fn read_bytes(z: &mut ZipArchive<fs::File>, name: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut buf).unwrap();
    buf
}

fn read_image(z: &mut ZipArchive<fs::File>, name: &str) -> image::DynamicImage {
    image::load_from_memory(&read_bytes(z, name)).unwrap()
}

fn open(path: &Path) -> ZipArchive<fs::File> {
    ZipArchive::new(fs::File::open(path).unwrap()).unwrap()
}

/// 侧放的跨页样本：原始跨页图（横版）+ 它的逆时针 90° 版作为正文第 1 页。
fn make_sideways_epub(src: &Path, raw: &image::RgbImage) {
    let page1 = image::DynamicImage::ImageRgb8(raw.clone()).rotate270().to_rgb8();
    let cover = synth_image(300, 420, 3);
    let page2 = synth_image(320, 460, 11);
    let theend = synth_image(300, 420, 5);
    let raw_jpeg = encode_jpeg(raw);
    let page1_jpeg = encode_jpeg(&page1);
    let cover_jpeg = encode_jpeg(&cover);
    let page2_jpeg = encode_jpeg(&page2);
    let theend_png = encode_png(&theend);
    make_epub_real(
        src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &cover_jpeg),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &page1_jpeg),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &page2_jpeg),
            ("html/theend.html", "THE END", "image/theend.png", &theend_png),
        ],
        &[("image/moe-000001-RAWIMAGE.jpg", &raw_jpeg)],
    );
}

#[test]
fn test_cover_rotation_by_reference() {
    // 参照图定方向：逆时针 90° 存进来的第 1 页应被转回横版，且与原始跨页图高度相似
    let td = tdir("rot_ref");
    let src = td.join("a.epub");
    let raw = synth_image(800, 400, 7);
    make_sideways_epub(&src, &raw);
    let cover_before = read_bytes(&mut open(&src), "image/cover.jpg");

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Auto },
    )
    .unwrap();
    let mut z = open(&out.dst);
    let got = read_image(&mut z, "image/001.jpg");
    assert_eq!((got.width(), got.height()), (800, 400), "第 1 页未转回横版");
    let s = crate::cover::similarity(&got, &image::DynamicImage::ImageRgb8(raw.clone()));
    assert!(s > 0.8, "回正后应与原始跨页图高度相似，实际 {s}");
    // 封面页判定为已正立，像素不动
    assert_eq!(read_bytes(&mut z, "image/cover.jpg"), cover_before, "封面图不该被动");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_reference_already_upright() {
    // 参照图与第 1 页同向（都是竖版）→ 判为已正立，一个像素都不改
    let td = tdir("rot_upright");
    let src = td.join("a.epub");
    let raw = synth_image(600, 900, 51);
    let page1 = encode_jpeg(&raw);
    let cover = encode_jpeg(&synth_image(300, 420, 3));
    let page2 = encode_jpeg(&synth_image(320, 460, 11));
    make_epub_real(
        &src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &cover),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &page1),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &page2),
        ],
        &[("image/moe-000001-RAWIMAGE.jpg", &encode_jpeg(&raw))],
    );

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Auto },
    )
    .unwrap();
    let mut z = open(&out.dst);
    assert_eq!(read_bytes(&mut z, "image/001.jpg"), page1, "已正立的页不该被重编码");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_fixed_angle_overrides_direction() {
    // 指定角度用于覆盖自动判定的方向：同一张逆时针 90° 存进来的第 1 页，
    // 自动应转 90°，而 Fixed(270) 强制按 270° 转（等于用户说自动方向反了）
    let td = tdir("rot_fixed");
    let src = td.join("a.epub");
    let raw = synth_image(800, 400, 21);
    make_sideways_epub(&src, &raw);
    let cover_before = read_bytes(&mut open(&src), "image/cover.jpg");

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Fixed(270) },
    )
    .unwrap();
    let mut z = open(&out.dst);
    let got = read_image(&mut z, "image/001.jpg");
    // 顺/逆 90° 互换尺寸但不换内容方向，所以这里要按内容（= 原始跨页图 180°）判别
    assert_eq!((got.width(), got.height()), (800, 400));
    let expect = image::DynamicImage::ImageRgb8(raw.clone()).rotate180();
    assert!(crate::cover::similarity(&got, &expect) > 0.95, "未按指定角度旋转");
    // 判定为已正立的封面不受指定角度影响
    assert_eq!(read_bytes(&mut z, "image/cover.jpg"), cover_before);
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_fixed_angle_ignores_healthy_pages() {
    // Fixed 不改变「是否侧放」的判断：全卷正常时指定角度也不动任何像素
    let td = tdir("rot_fixed_ok");
    let src = td.join("a.epub");
    let cover = encode_jpeg(&synth_image(300, 420, 3));
    let page1 = encode_jpeg(&synth_image(800, 1200, 5));
    let page2 = encode_jpeg(&synth_image(800, 1200, 11));
    make_epub_real(
        &src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &cover),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &page1),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &page2),
        ],
        &[],
    );

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Fixed(90) },
    )
    .unwrap();
    let mut z = open(&out.dst);
    assert_eq!(read_bytes(&mut z, "image/cover.jpg"), cover);
    assert_eq!(read_bytes(&mut z, "image/001.jpg"), page1);
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_without_reference_keeps_pixels() {
    // 无参照图：即便页宽只有正常页的一半、宽高比也异常（旧尺寸规则会判成横躺跨页），
    // 现在一律不旋转——不许猜方向
    let td = tdir("rot_noref");
    let src = td.join("a.epub");
    let narrow = synth_image(400, 800, 31);
    let normal = encode_jpeg(&synth_image(800, 1200, 41));
    let narrow_jpeg = encode_jpeg(&narrow);
    make_epub_real(
        &src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &normal),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &narrow_jpeg),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &normal),
            ("html/page-3.html", "第3话", "image/moe-000003.jpg", &normal),
        ],
        &[],
    );

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Auto },
    )
    .unwrap();
    let mut z = open(&out.dst);
    assert_eq!(read_bytes(&mut z, "image/001.jpg"), narrow_jpeg, "无参照图时不该旋转");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_any_page_with_reference() {
    // 侧放的页不在封面 / 正文第 1 页上：参照图照样定方向，指定角度也照样覆盖。
    // 真实样本（2026-09-14，[Kmoe][電鋸人2]話001-005[話098-102]）全卷 11 页带参照图，
    // 只把封面 / 第 1 页当候选会漏掉其余 10 页；那本的参照图是 png、页图是 jpg，
    // 所以这里也用「页图 .jpg + 参照图 .png」压住扩展名不一致的匹配。
    let td = tdir("rot_any");
    let src = td.join("a.epub");
    let raw = synth_image(800, 400, 71);
    let page2 = image::DynamicImage::ImageRgb8(raw.clone()).rotate270().to_rgb8();
    let cover = encode_jpeg(&synth_image(300, 420, 3));
    let page1 = encode_jpeg(&synth_image(800, 1200, 5));
    let page2_jpeg = encode_jpeg(&page2);
    let page3 = encode_jpeg(&synth_image(800, 1200, 13));
    make_epub_real(
        &src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &cover),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &page1),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &page2_jpeg),
            ("html/page-3.html", "第3话", "image/moe-000003.jpg", &page3),
        ],
        &[("image/moe-000002-RAWIMAGE.png", &encode_png(&raw))],
    );

    let out = fix_one(&src.to_string_lossy(), None, None).unwrap();
    let mut z = open(&out.dst);
    let got = read_image(&mut z, "image/002.jpg");
    assert_eq!((got.width(), got.height()), (800, 400), "第 2 话（非第 1 页）未按参照图回正");
    let s = crate::cover::similarity(&got, &image::DynamicImage::ImageRgb8(raw.clone()));
    assert!(s > 0.8, "回正后应与原始跨页图高度相似，实际 {s}");
    assert_eq!(read_bytes(&mut z, "image/001.jpg"), page1, "无参照图的页不该被动");
    assert_eq!(read_bytes(&mut z, "image/003.jpg"), page3, "无参照图的页不该被动");
    assert_eq!(read_bytes(&mut z, "image/cover.jpg"), cover, "封面不该被动");

    let out2 = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Fixed(270) },
    )
    .unwrap();
    let mut z2 = open(&out2.dst);
    let got2 = read_image(&mut z2, "image/002.jpg");
    assert_eq!((got2.width(), got2.height()), (800, 400));
    let expect = image::DynamicImage::ImageRgb8(raw.clone()).rotate180();
    assert!(crate::cover::similarity(&got2, &expect) > 0.95, "指定角度未覆盖到第 2 话");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_low_similarity_keeps_pixels() {
    // 有同名参照图，但内容与候选页无关（相似度不足）→ 不旋转
    let td = tdir("rot_lowsim");
    let src = td.join("a.epub");
    let page1 = encode_jpeg(&synth_image(800, 1200, 61));
    let cover = encode_jpeg(&synth_image(300, 420, 3));
    let page2 = encode_jpeg(&synth_image(320, 460, 11));
    let unrelated = encode_jpeg(&synth_image(900, 1300, 77));
    make_epub_real(
        &src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &cover),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &page1),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &page2),
        ],
        &[("image/moe-000001-RAWIMAGE.jpg", &unrelated)],
    );

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Auto },
    )
    .unwrap();
    let mut z = open(&out.dst);
    assert_eq!(read_bytes(&mut z, "image/001.jpg"), page1, "相似度不足时不该旋转");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_not_needed() {
    // 负向对照：正常竖版页、无参照图 → 两张候选图都逐字节不动
    let td = tdir("rot_none");
    let src = td.join("a.epub");
    let cover = encode_jpeg(&synth_image(300, 420, 3));
    let page1 = encode_jpeg(&synth_image(800, 1200, 5));
    let page2 = encode_jpeg(&synth_image(800, 1200, 11));
    make_epub_real(
        &src,
        &[
            ("html/cover.html", "封面", "image/cover.jpg", &cover),
            ("html/page-1.html", "第1话", "image/moe-000001.jpg", &page1),
            ("html/page-2.html", "第2话", "image/moe-000002.jpg", &page2),
        ],
        &[],
    );

    let out = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Auto },
    )
    .unwrap();
    let mut z = open(&out.dst);
    assert_eq!(read_bytes(&mut z, "image/cover.jpg"), cover);
    assert_eq!(read_bytes(&mut z, "image/001.jpg"), page1);
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cover_rotation_default_on_and_off_keeps_pixels() {
    // 默认（fix_one = FixOptions::default）已开启回正：有参照图的侧放页被转正；
    // 显式 Off 才保留图片原字节
    let td = tdir("rot_off");
    let src = td.join("a.epub");
    let raw = synth_image(800, 400, 7);
    make_sideways_epub(&src, &raw);
    let sideways_bytes = read_bytes(&mut open(&src), "image/moe-000001.jpg");

    let out = fix_one(&src.to_string_lossy(), None, None).unwrap();
    let mut z = open(&out.dst);
    let got = read_image(&mut z, "image/001.jpg");
    assert_eq!((got.width(), got.height()), (800, 400), "默认路径应回正");
    assert_ne!(read_bytes(&mut z, "image/001.jpg"), sideways_bytes);

    let out2 = fix_one_with(
        &src.to_string_lossy(),
        None,
        None,
        &FixOptions { rotate_cover: RotateCover::Off },
    )
    .unwrap();
    let mut z2 = open(&out2.dst);
    assert_eq!(read_bytes(&mut z2, "image/001.jpg"), sideways_bytes, "Off 路径不该改图片像素");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_cli_rotate_cover_flag() {
    // CLI 开关：--help 提到开关；--no-rotate-cover 可用；取值非法时以退出码 2 拒绝
    let Ok(exe) = std::env::var("CARGO_BIN_EXE_KmoeFix") else { return };
    let help = Command::new(&exe).arg("--help").output().unwrap();
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("--rotate-cover"), "stdout: {text}");
    assert!(text.contains("--no-rotate-cover"), "stdout: {text}");
    let bad = Command::new(&exe).arg("--rotate-cover=45").output().unwrap();
    assert_eq!(bad.status.code(), Some(2));
}

// ---- 站点卡片页（Kmoe 插在正文中间的站点卡片）----
//
// 判定只认一条：页图**下部条带**与包内 theend 卡的同一条带相关度 ≥0.9。
// 实测（15 个真实卷）卡片页 ≈1.000，最像的正文页 ≤0.216。

/// 真实 kmoe 包的 spine 开标签（带属性，不是字面量 `<spine>`）。
const KMOE_SPINE_OPEN: &str =
    r#"<spine page-progression-direction="rtl" toc="ncx" kmoe-pagedirect="rtl">"#;

/// 白底 + 右下角一块棋盘：模拟站点卡片与 theend 卡共用的页脚 Logo 区块。
fn card_like(w: u32, h: u32) -> image::RgbImage {
    image::RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        if fx > 0.60 && fy > 0.90 {
            let c = if (x / 6 + y / 6) % 2 == 0 { 20u8 } else { 235u8 };
            image::Rgb([c, c, c])
        } else {
            image::Rgb([255, 255, 255])
        }
    })
}

/// 普通正文页：上半张有图案、下半张留白（右下条带纯白 → 与卡片页脚相关度为 0）。
fn plain_page(w: u32, h: u32, seed: u32) -> image::RgbImage {
    let base = synth_image(w, h, seed);
    image::RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        if fx < 0.5 && fy < 0.5 {
            *base.get_pixel(x, y)
        } else {
            image::Rgb([255, 255, 255])
        }
    })
}

/// 读产物 spine 的 href 顺序（测试里的 opf 由 make_epub_real 生成，属性格式固定）。
fn spine_order(z: &mut ZipArchive<fs::File>) -> Vec<String> {
    let opf = read_text(z, "vol.opf");
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in opf.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("<item ") else { continue };
        let attr = |key: &str| -> Option<String> {
            let start = rest.find(key)? + key.len();
            let rest = &rest[start..];
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        };
        if let (Some(id), Some(href)) = (attr("id=\""), attr("href=\"")) {
            map.insert(id, href);
        }
    }
    let start = match opf.find("<spine") {
        Some(i) => opf[i..].find('>').map(|j| i + j + 1).unwrap_or(opf.len()),
        None => opf.len(),
    };
    let end = opf[start..].find("</spine>").map(|j| start + j).unwrap_or(opf.len());
    opf[start..end]
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("<itemref ")?;
            let start = rest.find("idref=\"")? + "idref=\"".len();
            let rest = &rest[start..];
            let end = rest.find('"')?;
            map.get(&rest[..end]).cloned()
        })
        .collect()
}

/// 六个条目的样本包：cover、正文 1~4 话（第 3 话其实是站点卡片）、theend 卡。
/// `with_theend=false` 时去掉 theend 页（拿不到参照 → 不判卡片）。
/// 源文件名用**已补齐的** `page-00N.html`（实测 `[Kmoe][我們的離婚]` 这类包就是这样），
/// spine 开标签也照真实包写成带属性的形式——字面量 `<spine>` 找不到时 spine 不会被重排。
fn make_card_epub(path: &Path, with_theend: bool) {
    let cover = encode_jpeg(&plain_page(300, 420, 2));
    let page1 = encode_jpeg(&plain_page(300, 420, 3));
    let page2 = encode_jpeg(&plain_page(300, 420, 4));
    let card = encode_png(&card_like(300, 420));
    let page4 = encode_jpeg(&plain_page(300, 420, 6));
    let theend = encode_png(&card_like(300, 420));
    let mut pages = vec![
        ("html/cover.html", "封面", "image/cover.jpg", cover.as_slice()),
        ("html/page-001.html", "第1话", "image/moe-000001.jpg", page1.as_slice()),
        ("html/page-002.html", "第2话", "image/moe-000002.jpg", page2.as_slice()),
        ("html/page-003.html", "第3话", "image/moe-000003.png", card.as_slice()),
        ("html/page-004.html", "第4话", "image/moe-000004.jpg", page4.as_slice()),
    ];
    if with_theend {
        pages.push(("html/theend.html", "THE END", "image/theend.png", theend.as_slice()));
    }
    make_epub_real_spine(path, &pages, &[], KMOE_SPINE_OPEN);
}

#[test]
fn test_site_card_page_moved_to_end() {
    // 站点卡片占着「第 3 话」的号位：产物里它应改名为 kmoe-001 排到卷末，
    // 后面的正文页编号前移（原第 4 话 → 第 3 话），标题跟着改
    let td = tdir("card_move");
    let src = td.join("a.epub");
    make_card_epub(&src, true);

    let lines: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let log = |s: &str| lines.borrow_mut().push(s.to_string());
    let out = fix_one_with(&src.to_string_lossy(), None, Some(&log), &FixOptions::default()).unwrap();

    let mut z = open(&out.dst);
    let names: Vec<String> = z.file_names().map(|s| s.to_string()).collect();
    assert!(names.iter().any(|n| n == "html/kmoe-001.html"), "卡片页未改名: {names:?}");
    assert!(names.iter().any(|n| n == "image/kmoe-001.png"), "卡片图未改名: {names:?}");
    for gone in ["html/page-4.html", "image/moe-000003.png", "image/moe-000004.jpg"] {
        assert!(!names.iter().any(|n| n == gone), "不应存在 {gone}");
    }
    assert!(names.iter().any(|n| n == "image/003.jpg"), "前移的正文页图名不对: {names:?}");

    // spine：cover 置首 → 正文 1..3 → 卡片 → theend
    assert_eq!(
        spine_order(&mut z),
        vec![
            "html/cover.html",
            "html/page-001.html",
            "html/page-002.html",
            "html/page-003.html",
            "html/kmoe-001.html",
            "html/theend.html",
        ]
    );

    // 原第 4 话变成了第 3 话：文件名、标题、alt 三者一致
    let page3 = read_text(&mut z, "html/page-003.html");
    assert!(page3.contains("<title>第3话</title>"), "{page3}");
    assert!(page3.contains("alt=\"第3话\""), "{page3}");
    assert!(page3.contains("src=\"../image/003.jpg\""), "{page3}");
    // 卡片页标题不再冒充页码
    let card = read_text(&mut z, "html/kmoe-001.html");
    assert!(card.contains("<title>Kmoe</title>"), "{card}");
    assert!(card.contains("src=\"../image/kmoe-001.png\""), "{card}");
    // 没有卡片的页一个字节都没动
    let page1 = read_text(&mut z, "html/page-001.html");
    assert!(page1.contains("<title>第1话</title>") && page1.contains("alt=\"第1话\""), "{page1}");
    // 卡片图的字节原样搬过去
    let card_bytes = read_bytes(&mut z, "image/kmoe-001.png");
    let mut src_z = open(&src);
    assert_eq!(card_bytes, read_bytes(&mut src_z, "image/moe-000003.png"));

    assert!(
        lines.borrow().iter().any(|l| l.contains("已把 1 张站点卡片页移到卷末")),
        "日志缺汇总行: {:?}",
        lines.borrow()
    );
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_site_card_rerun_is_stable() {
    // 重跑产物：卡片页靠命名前缀认出来，编号不再被它挤掉，产物与上次一致
    let td = tdir("card_rerun");
    let src = td.join("a.epub");
    make_card_epub(&src, true);
    let first = fix_one(&src.to_string_lossy(), None, None).unwrap();

    let second_path = td.join("b.epub");
    let second = fix_one_with(
        &first.dst.to_string_lossy(),
        Some(&second_path.to_string_lossy()),
        None,
        &FixOptions::default(),
    )
    .unwrap();

    let mut a = open(&first.dst);
    let mut b = open(&second.dst);
    assert_eq!(spine_order(&mut a), spine_order(&mut b));
    assert_eq!(read_text(&mut b, "html/page-003.html"), read_text(&mut a, "html/page-003.html"));
    assert_eq!(read_text(&mut b, "html/kmoe-001.html"), read_text(&mut a, "html/kmoe-001.html"));
    let names: Vec<String> = b.file_names().map(|s| s.to_string()).collect();
    assert!(!names.iter().any(|n| n == "html/page-004.html"), "{names:?}");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn test_site_card_without_theend_keeps_original_names() {
    // 包里没有 theend 卡 → 拿不到参照，不判卡片，一切照旧
    let td = tdir("card_noref");
    let src = td.join("a.epub");
    make_card_epub(&src, false);

    let out = fix_one(&src.to_string_lossy(), None, None).unwrap();
    let mut z = open(&out.dst);
    let names: Vec<String> = z.file_names().map(|s| s.to_string()).collect();
    assert!(names.iter().any(|n| n == "html/page-003.html"), "{names:?}");
    assert!(names.iter().any(|n| n == "image/003.png"), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with("html/kmoe-")), "{names:?}");
    let page3 = read_text(&mut z, "html/page-003.html");
    assert!(page3.contains("<title>第3话</title>"), "{page3}");
    let _ = fs::remove_dir_all(&td);
}
