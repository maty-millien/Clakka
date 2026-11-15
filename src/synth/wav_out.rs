use std::io::Write;

pub fn encode(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let data_len = (frames * 8) as u32;
    let mut b = Vec::with_capacity(44 + frames * 8);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for (l, r) in left.iter().zip(right).take(frames) {
        b.extend_from_slice(&l.to_le_bytes());
        b.extend_from_slice(&r.to_le_bytes());
    }
    b
}

pub fn write(path: &str, left: &[f32], right: &[f32], sample_rate: u32) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    std::fs::File::create(path)?.write_all(&encode(left, right, sample_rate))
}
