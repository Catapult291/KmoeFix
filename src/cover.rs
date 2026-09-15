//! 「侧放」（横躺存成竖版）的判定与回正（CLI `--rotate-cover`、GUI 配置 `rotate_cover`）。
//!
//! 本模块只做图像判定与像素变换，zip 搬运与日志在 `core.rs`。
//!
//! 判定**只有一条依据**：**参照图**——kmoe 包会为每一张侧放的页遗留一张同名的原始跨页图
//! `<名字>-RAWIMAGE.<ext>`（不只是封面 / 正文第 1 页；参照图的扩展名可能与页图不同，
//! 实测 `[Kmoe][電鋸人2]` 是页图 `.jpg` + 参照图 `.png`）。把候选图的四个朝向分别与它比相似度，
//! 取「宽高比一致且最像」的朝向，方向直接判出来，不用猜。实测（2026-09-13 的两个样本）：
//! 卷 01 顺时针 90° 相关度 0.925、逆时针 −0.12；卷 02 顺时针 90° 1.000、逆时针 0.10。
//!
//! 没有参照图（或相似度不足）时**不旋转**。2026-09-13 之前曾有一条「尺寸兜底」规则
//! （页宽 < 全卷页宽中位数×0.85 且宽高比 < 0.6 就按顺时针 90° 转），它判不出顺/逆、
//! 属于猜方向，会把本来正常的窄页转坏，已按用户要求删除。
//!
//! 注意：JPEG 旋转后是重编码（质量见 `JPEG_QUALITY`），有一次代际损失；PNG 走无损。

use image::{DynamicImage, ImageFormat};
use std::io::Cursor;

/// JPEG 重编码质量：一次代际损失，取高值使其肉眼不可见。
pub const JPEG_QUALITY: u8 = 95;
/// 参照图相似度阈值（实测同内容 ≈0.9~1.0，方向不对 ≈0.1）。
pub const REF_SIMILARITY_MIN: f32 = 0.5;
/// 参照图定方向时允许的宽高比偏差（横竖不符的朝向不可能「正立」）。
const ASPECT_TOLERANCE: f32 = 0.15;
const GRID_WIDE: (u32, u32) = (64, 32);
const GRID_TALL: (u32, u32) = (32, 64);
const ALL_DEGREES: [u16; 4] = [0, 90, 180, 270];

/// 解码整张图（拿不到像素时返回 None，调用方应跳过而非报错）。
pub fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    image::load_from_memory(bytes).ok()
}

/// 顺时针旋转 `degrees`（0/90/180/270，其他值按 0 处理）。
pub fn rotate_by(img: &DynamicImage, degrees: u16) -> DynamicImage {
    match degrees {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img.clone(),
    }
}

/// 按原格式重编码：PNG 无损，JPEG 质量 [`JPEG_QUALITY`]。
pub fn encode(img: &DynamicImage, is_png: bool) -> Option<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    if is_png {
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).ok()?;
    } else {
        let rgb = img.to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut Cursor::new(&mut buf), JPEG_QUALITY)
            .encode_image(&rgb)
            .ok()?;
    }
    Some(buf)
}

/// 归一化相关度（-1..1）：两图灰度降采样到同一栅格，各自去均值、单位化后求像素积均值。
/// 纯色图（无信息）返回 0，避免除零把它算成「完全一致」。
pub fn similarity(a: &DynamicImage, b: &DynamicImage) -> f32 {
    let (gw, gh) = if b.width() >= b.height() { GRID_WIDE } else { GRID_TALL };
    ncc(&samples(a, gw, gh), &samples(b, gw, gh))
}

/// 把图按 `w×h` 栅格降采样成标准化灰度样本（去均值、单位化；纯色 → 全 0）。
/// 供需要按区域比对的调用方（如 `core.rs` 的站点卡片页判定）自行裁剪后再比对。
pub fn normalized_samples(img: &DynamicImage, w: u32, h: u32) -> Vec<f32> {
    samples(img, w, h)
}

/// 两个标准化向量的相关度（长度不等或为空 → 0）。
pub fn ncc_of(a: &[f32], b: &[f32]) -> f32 {
    ncc(a, b)
}

fn samples(img: &DynamicImage, w: u32, h: u32) -> Vec<f32> {
    let g = image::imageops::resize(&img.to_luma8(), w, h, image::imageops::FilterType::Triangle);
    let raw: Vec<f32> = g.pixels().map(|p| p.0[0] as f32).collect();
    let n = raw.len() as f32;
    if n == 0.0 {
        return Vec::new();
    }
    let mean = raw.iter().sum::<f32>() / n;
    let std = (raw.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n).sqrt();
    if std < 1e-6 {
        return vec![0.0; raw.len()];
    }
    raw.iter().map(|v| (v - mean) / std).collect()
}

fn ncc(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>() / a.len() as f32
}

/// 求把候选图转到与参照图同一朝向所需的顺时针角度，返回 (角度, 相似度)。
/// 相似度不足（没把握）、或四个朝向的宽高比都与参照图不符时返回 None，调用方应跳过该图。
/// 0 表示「本来就正立」。
pub fn align_to_reference(cand: &DynamicImage, refer: &DynamicImage) -> Option<(u16, f32)> {
    if refer.height() == 0 {
        return None;
    }
    let ref_ar = refer.width() as f32 / refer.height() as f32;
    let mut best: Option<(u16, f32)> = None;
    for deg in ALL_DEGREES {
        let r = rotate_by(cand, deg);
        let ar = r.width() as f32 / r.height() as f32;
        if (ar / ref_ar - 1.0).abs() > ASPECT_TOLERANCE {
            continue;
        }
        let s = similarity(&r, refer);
        if best.map_or(true, |(_, bs)| s > bs) {
            best = Some((deg, s));
        }
    }
    best.filter(|(_, s)| *s >= REF_SIMILARITY_MIN)
}
