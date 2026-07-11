use bav_format::{Metadata, encode};
use std::{
    env, fs,
    io::{self, Read},
    path::Path,
    process::{Command, ExitCode, Stdio},
};

struct FrameSet {
    width: u16,
    height: u16,
    frames: Vec<Vec<u8>>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bav-encode: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|argument| argument == "--video") {
        if args.len() != 5 {
            return Err("usage: bav-encode --video INPUT.mp4 OUTPUT.bav OUTPUT.mp3".into());
        }
        encode_video(&args[2], &args[3], &args[4])?;
        return Ok(());
    }
    if args.get(1).is_some_and(|argument| argument == "--raw-gray") {
        if args.len() != 6 {
            return Err("usage: bav-encode --raw-gray WIDTH HEIGHT FPS OUTPUT.bav".into());
        }
        let width: u16 = args[2].parse()?;
        let height: u16 = args[3].parse()?;
        let fps: u16 = args[4].parse()?;
        let frames = read_raw_frames(&mut io::stdin().lock(), width, height)?;
        write_movie(&args[5], fps, width, height, frames)?;
        return Ok(());
    }
    if args.get(1).is_some_and(|argument| argument == "--braille") {
        if args.len() != 5 {
            return Err("usage: bav-encode --braille INPUT.txt OUTPUT.bav FPS".into());
        }
        let fps: u16 = args[4].parse()?;
        let frames = read_braille_frames(Path::new(&args[2]))?;
        write_movie(&args[3], fps, frames.width, frames.height, frames.frames)?;
        return Ok(());
    }
    if args.len() < 4 {
        return Err("usage: bav-encode OUTPUT.bav FPS FRAME.pgm...".into());
    }
    let fps: u16 = args[2].parse()?;
    let mut dimensions = None;
    let mut frames = Vec::new();
    for path in &args[3..] {
        let (width, height, pixels) = read_pgm(Path::new(path))?;
        if dimensions.is_some_and(|value| value != (width, height)) {
            return Err(format!("frame dimensions differ: {path}").into());
        }
        dimensions = Some((width, height));
        frames.push(pack_frame(width, height, &pixels));
    }
    let (width, height) = dimensions.ok_or("no frames supplied")?;
    write_movie(&args[1], fps, width, height, frames)
}

fn encode_video(input: &str, movie: &str, audio: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            input,
            "-vf",
            "fps=30,scale=480:360",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    let frames = read_raw_frames(
        decoder
            .stdout
            .as_mut()
            .ok_or("ffmpeg stdout was unavailable")?,
        480,
        360,
    )?;
    if !decoder.wait()?.success() {
        return Err("ffmpeg video conversion failed".into());
    }
    write_movie(movie, 30, 480, 360, frames)?;

    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-i",
            input,
            "-vn",
            "-codec:a",
            "libmp3lame",
            "-q:a",
            "4",
            audio,
        ])
        .status()?;
    if !status.success() {
        return Err("ffmpeg audio conversion failed".into());
    }
    Ok(())
}

fn read_raw_frames(
    reader: &mut impl Read,
    width: u16,
    height: u16,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let frame_size = usize::from(width) * usize::from(height);
    let mut pixels = vec![0; frame_size];
    let mut frames = Vec::new();
    loop {
        let mut read = 0;
        while read < frame_size {
            match reader.read(&mut pixels[read..])? {
                0 if read == 0 => {
                    return if frames.is_empty() {
                        Err("raw grayscale input contains no frames".into())
                    } else {
                        Ok(frames)
                    };
                }
                0 => return Err("raw grayscale input ends with an incomplete frame".into()),
                count => read += count,
            }
        }
        frames.push(pack_frame(width, height, &pixels));
    }
}

fn write_movie(
    output: &str,
    fps: u16,
    width: u16,
    height: u16,
    frames: Vec<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = Metadata {
        width,
        height,
        fps,
        frame_count: frames.len() as u32,
        keyframe_interval: fps,
    };
    fs::write(output, encode(metadata, &frames)?)?;
    println!("wrote {} frames to {output}", frames.len());
    Ok(())
}

fn read_braille_frames(path: &Path) -> Result<FrameSet, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?.replace("\r\n", "\n");
    let mut dimensions = None;
    let mut frames = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 3 || !lines[1].contains("-->") {
            continue;
        }
        let image = &lines[2..];
        let columns = image.first().ok_or("empty braille frame")?.chars().count();
        if columns == 0 || image.iter().any(|line| line.chars().count() != columns) {
            return Err("inconsistent braille frame width".into());
        }
        let width = u16::try_from(columns * 2)?;
        let height = u16::try_from(image.len() * 4)?;
        if dimensions.is_some_and(|value| value != (width, height)) {
            return Err("inconsistent braille frame dimensions".into());
        }
        dimensions = Some((width, height));
        frames.push(unpack_braille(image, width, height)?);
    }
    let (width, height) = dimensions.ok_or("no braille frames found")?;
    Ok(FrameSet {
        width,
        height,
        frames,
    })
}

fn unpack_braille(
    lines: &[&str],
    width: u16,
    height: u16,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
    let row_bytes = usize::from(width).div_ceil(8);
    let mut frame = vec![0; row_bytes * usize::from(height)];
    for (cell_y, line) in lines.iter().enumerate() {
        for (cell_x, character) in line.chars().enumerate() {
            let codepoint = u32::from(character);
            if !(0x2800..=0x28ff).contains(&codepoint) {
                return Err(format!("non-braille character U+{codepoint:04X}").into());
            }
            let bits = (codepoint - 0x2800) as u8;
            for (dot_y, dot_row) in DOTS.iter().enumerate() {
                for (dot_x, dot) in dot_row.iter().enumerate() {
                    if bits & dot != 0 {
                        let x = cell_x * 2 + dot_x;
                        let y = cell_y * 4 + dot_y;
                        frame[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
        }
    }
    Ok(frame)
}

fn read_pgm(path: &Path) -> Result<(u16, u16, Vec<u8>), Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let mut cursor = 0;
    let magic = token(&bytes, &mut cursor).ok_or("missing PGM magic")?;
    if magic != b"P5" {
        return Err("only binary P5 PGM files are supported".into());
    }
    let width: u16 =
        std::str::from_utf8(token(&bytes, &mut cursor).ok_or("missing width")?)?.parse()?;
    let height: u16 =
        std::str::from_utf8(token(&bytes, &mut cursor).ok_or("missing height")?)?.parse()?;
    let maximum: u16 =
        std::str::from_utf8(token(&bytes, &mut cursor).ok_or("missing maximum")?)?.parse()?;
    if maximum != 255 {
        return Err("only 8-bit PGM files are supported".into());
    }
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    let size = usize::from(width) * usize::from(height);
    let pixels = bytes
        .get(cursor..cursor + size)
        .ok_or("truncated PGM pixels")?;
    Ok((width, height, pixels.to_vec()))
}

fn token<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    loop {
        while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
            *cursor += 1;
        }
        if bytes.get(*cursor) != Some(&b'#') {
            break;
        }
        while bytes.get(*cursor).is_some_and(|byte| *byte != b'\n') {
            *cursor += 1;
        }
    }
    let start = *cursor;
    while bytes
        .get(*cursor)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        *cursor += 1;
    }
    (start != *cursor).then_some(&bytes[start..*cursor])
}

fn pack_frame(width: u16, height: u16, pixels: &[u8]) -> Vec<u8> {
    let row_bytes = usize::from(width).div_ceil(8);
    let mut packed = vec![0; row_bytes * usize::from(height)];
    for y in 0..usize::from(height) {
        for x in 0..usize::from(width) {
            if pixels[y * usize::from(width) + x] >= 128 {
                packed[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    packed
}
