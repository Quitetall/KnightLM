//! Chessboard vision: optional YOLOv8m ONNX model, letterboxed inference and
//! piece-to-square mapping. The model is never shipped; the operator downloads
//! it explicitly after reviewing license and hash.

use anyhow::{Context, Result, bail, ensure};
use image::{DynamicImage, imageops::FilterType};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tract_onnx::prelude::*;

pub const NAME: &str = "NAKST YOLOv8m chess pieces";
pub const LICENSE: &str = "AGPL-3.0";
pub const URL: &str =
    "https://huggingface.co/NAKSTStudio/yolov8m-chess-piece-detection/resolve/main/best.onnx";
pub const SHA256: &str = "6fdef8213ab818a71c69250e61e213a7b5471ffb05c0fae485e7d96040f9642c";
pub const SIZE: u64 = 103_737_229;
pub const INPUT: usize = 640;
const CLASSES: [&str; 13] = [
    "board",
    "white_king",
    "white_queen",
    "white_rook",
    "white_bishop",
    "white_knight",
    "white_pawn",
    "black_king",
    "black_queen",
    "black_rook",
    "black_bishop",
    "black_knight",
    "black_pawn",
];

pub fn model_path(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("chess-yolov8m.onnx")
}

pub struct ModelStatus {
    pub installed: bool,
    pub path: String,
    pub bytes: u64,
}

pub fn status(data_dir: &Path) -> ModelStatus {
    let path = model_path(data_dir);
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    ModelStatus {
        installed: bytes == SIZE,
        path: path.display().to_string(),
        bytes,
    }
}

pub fn download(data_dir: &Path) -> Result<ModelStatus> {
    let agent = ureq::Agent::new_with_defaults();
    let mut response = agent
        .get(URL)
        .call()
        .context("download vision model")?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(SIZE + 1024)
        .read_to_vec()
        .context("read vision model")?;
    ensure!(bytes.len() as u64 == SIZE, "unexpected model size");
    let digest = format!("{:x}", Sha256::digest(&bytes));
    ensure!(digest == SHA256, "model SHA-256 mismatch");
    let path = model_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("onnx.part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(status(data_dir))
}

#[derive(Debug, Clone)]
pub struct Recognition {
    pub placement: String,
    pub confidence: f32,
    pub board: [f32; 4],
    pub detections: usize,
    pub turn: String,
}

struct Detection {
    class: usize,
    score: f32,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
}

pub fn recognize(model: &Path, image_bytes: &[u8]) -> Result<Recognition> {
    if !model.exists() {
        bail!("vision model is not installed");
    }
    let image = image::load_from_memory(image_bytes).context("decode image")?;
    let (input, scale, pad_x, pad_y) = letterbox(&image);

    let model = tract_onnx::onnx()
        .model_for_path(model)?
        .into_optimized()?
        .into_runnable()?;
    let tensor = tract_onnx::prelude::tract_ndarray::Array4::from_shape_vec(
        (1, 3, INPUT, INPUT),
        input,
    )?;
    let outputs = model.run(tvec!(tensor.into_tvalue()))?;
    let output = outputs[0]
        .to_plain_array_view::<f32>()
        .context("model output is not f32")?;
    let shape = output.shape();
    if shape.len() != 3 {
        bail!("unexpected model output shape: {shape:?}");
    }
    // YOLOv8: [1, 4 + classes, anchors] (or transposed).
    let (channels_first, anchors, stride) = if shape[1] as usize == CLASSES.len() + 4 {
        (true, shape[2] as usize, 1usize)
    } else if shape[2] as usize == CLASSES.len() + 4 {
        (false, shape[1] as usize, shape[2] as usize)
    } else {
        bail!("unexpected model output shape: {shape:?}");
    };

    let mut detections = Vec::new();
    for anchor in 0..anchors {
        let value = |channel: usize| -> f32 {
            if channels_first {
                output[[0, channel, anchor]]
            } else if stride > 0 {
                output[[0, anchor, channel]]
            } else {
                0.0
            }
        };
        let mut best_class = 0usize;
        let mut best_score = 0.0f32;
        for class in 1..CLASSES.len() {
            let score = value(4 + class);
            if score > best_score {
                best_score = score;
                best_class = class;
            }
        }
        let board_score = value(4);
        if board_score > best_score {
            best_score = board_score;
            best_class = 0;
        }
        if best_score < 0.25 {
            continue;
        }
        detections.push(Detection {
            class: best_class,
            score: best_score,
            cx: value(0),
            cy: value(1),
            w: value(2),
            h: value(3),
        });
    }
    let mut detections = nms(detections, 0.45);
    // Some YOLOv8 exports emit normalized cx/cy/w/h; convert to pixels.
    if detections
        .iter()
        .all(|d| d.cx.abs() <= 2.0 && d.cy.abs() <= 2.0 && d.w <= 2.0 && d.h <= 2.0)
    {
        for detection in &mut detections {
            detection.cx *= INPUT as f32;
            detection.cy *= INPUT as f32;
            detection.w *= INPUT as f32;
            detection.h *= INPUT as f32;
        }
    }
    let boards: Vec<&Detection> = detections.iter().filter(|d| d.class == 0).collect();
    let board = boards
        .iter()
        .copied()
        .filter(|d| {
            let aspect = d.w / d.h.max(1.0);
            (0.8..1.25).contains(&aspect) && d.w > 80.0
        })
        .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
        .or_else(|| boards.iter().copied().max_by(|a, b| a.score.total_cmp(&b.score)))
        .or_else(|| boards.iter().copied().max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h))))
        .map(|d| {
            [
                (d.cx - d.w / 2.0 - pad_x) / scale,
                (d.cy - d.h / 2.0 - pad_y) / scale,
                d.w / scale,
                d.h / scale,
            ]
        })
        .unwrap_or([0.0, 0.0, image.width() as f32, image.height() as f32]);

    let mut squares = [None::<(char, f32)>; 64];
    for d in detections.iter().filter(|d| d.class > 0) {
        let x = (d.cx - pad_x) / scale;
        let y = (d.cy - pad_y) / scale;
        let col = ((x - board[0]) / board[2] * 8.0).floor();
        let row = ((y - board[1]) / board[3] * 8.0).floor();
        if !(0.0..8.0).contains(&col) || !(0.0..8.0).contains(&row) {
            continue;
        }
        let file = col as usize;
        let rank = 7 - row as usize;
        let idx = rank * 8 + file;
        let piece = piece_char(d.class);
        if squares[idx]
            .map(|(_, score)| d.score > score)
            .unwrap_or(true)
        {
            squares[idx] = Some((piece, d.score));
        }
    }

    // Sanitize: one king per side (highest confidence), no pawns on back ranks.
    for king in ['K', 'k'] {
        let mut kings: Vec<(usize, f32)> = squares
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                Some((p, score)) if *p == king => Some((i, *score)),
                _ => None,
            })
            .collect();
        kings.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (idx, _) in kings.into_iter().skip(1) {
            squares[idx] = None;
        }
    }
    for rank in [0usize, 7] {
        for file in 0..8 {
            if matches!(squares[rank * 8 + file], Some(('P' | 'p', _))) {
                squares[rank * 8 + file] = None;
            }
        }
    }

    let mut placement = String::new();
    let mut confidence = 0.0f32;
    let mut count = 0usize;
    for rank in (0..8).rev() {
        let mut empty = 0;
        for file in 0..8 {
            match squares[rank * 8 + file] {
                Some((piece, score)) => {
                    if empty > 0 {
                        placement.push_str(&empty.to_string());
                        empty = 0;
                    }
                    placement.push(piece);
                    confidence += score;
                    count += 1;
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            placement.push_str(&empty.to_string());
        }
        if rank > 0 {
            placement.push('/');
        }
    }
    let mut turn = "w".to_owned();
    for candidate in ["w", "b"] {
        let fen = format!("{placement} {candidate} - - 0 1");
        if knightlm_core::state::parse_variant_fen(
            &fen,
            knightlm_core::state::variant_for_name("standard"),
            shakmaty::CastlingMode::Standard,
        )
        .is_ok()
        {
            turn = candidate.to_owned();
            break;
        }
    }
    Ok(Recognition {
        placement,
        confidence: if count > 0 {
            confidence / count as f32
        } else {
            0.0
        },
        board,
        detections: detections.len(),
        turn,
    })
}

fn letterbox(image: &DynamicImage) -> (Vec<f32>, f32, f32, f32) {
    let rgb = image.to_rgb8();
    let (w, h) = (rgb.width() as f32, rgb.height() as f32);
    let scale = (INPUT as f32 / w).min(INPUT as f32 / h);
    let nw = (w * scale).round().max(1.0) as u32;
    let nh = (h * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&rgb, nw, nh, FilterType::Triangle);
    let pad_x = (INPUT as f32 - nw as f32) / 2.0;
    let pad_y = (INPUT as f32 - nh as f32) / 2.0;
    let mut data = vec![114.0f32 / 255.0; 3 * INPUT * INPUT];
    for y in 0..nh as usize {
        for x in 0..nw as usize {
            let pixel = resized.get_pixel(x as u32, y as u32);
            let dx = x + pad_x as usize;
            let dy = y + pad_y as usize;
            for c in 0..3 {
                data[c * INPUT * INPUT + dy * INPUT + dx] = pixel[c] as f32 / 255.0;
            }
        }
    }
    (data, scale, pad_x, pad_y)
}

fn piece_char(class: usize) -> char {
    match class {
        1 => 'K',
        2 => 'Q',
        3 => 'R',
        4 => 'B',
        5 => 'N',
        6 => 'P',
        7 => 'k',
        8 => 'q',
        9 => 'r',
        10 => 'b',
        11 => 'n',
        12 => 'p',
        _ => '?',
    }
}

fn nms(mut detections: Vec<Detection>, threshold: f32) -> Vec<Detection> {
    detections.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    'outer: for detection in detections {
        for other in &kept {
            if detection.class == other.class && iou(&detection, other) > threshold {
                continue 'outer;
            }
        }
        kept.push(detection);
    }
    kept
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let (ax1, ay1) = (a.cx - a.w / 2.0, a.cy - a.h / 2.0);
    let (ax2, ay2) = (a.cx + a.w / 2.0, a.cy + a.h / 2.0);
    let (bx1, by1) = (b.cx - b.w / 2.0, b.cy - b.h / 2.0);
    let (bx2, by2) = (b.cx + b.w / 2.0, b.cy + b.h / 2.0);
    let ix = (ax2.min(bx2) - ax1.max(bx1)).max(0.0);
    let iy = (ay2.min(by2) - ay1.max(by1)).max(0.0);
    let inter = ix * iy;
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}
