use bav_format::Movie;
use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::{
    env, fs,
    fs::File,
    io::{self, Read, Write},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const FRAME_PATCH: u8 = 1;
const MASK_PATCH: u8 = 2;
const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bav-engine: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if !(4..=6).contains(&args.len()) {
        return Err("usage: bav-engine MOVIE.bav COLUMNS ROWS [AUDIO.mp3] [--mask]".into());
    }
    let columns: usize = args[2].parse()?;
    let rows: usize = args[3].parse()?;
    if columns == 0 || rows == 0 {
        return Err("display dimensions must be positive".into());
    }
    let movie = Movie::open(fs::read(&args[1])?)?;
    let metadata = movie.metadata();
    let mask_mode = args[4..].iter().any(|argument| argument == "--mask");
    let audio_path = args[4..].iter().find(|argument| *argument != "--mask");
    let audio = audio_path.and_then(|path| match start_audio(path) {
        Ok(audio) => Some(audio),
        Err(error) => {
            eprintln!("audio unavailable, continuing silently: {error}");
            None
        }
    });
    let muted = Arc::new(AtomicBool::new(false));
    listen_for_commands(Arc::clone(&muted));
    let started = Instant::now();
    let mut previous: Vec<String> = Vec::new();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    for frame_index in 0..metadata.frame_count {
        let target = Duration::from_secs_f64(f64::from(frame_index) / f64::from(metadata.fps));
        loop {
            let position = audio
                .as_ref()
                .map_or_else(|| started.elapsed(), |(_, player)| player.get_pos());
            if position >= target {
                break;
            }
            thread::sleep((target - position).min(Duration::from_millis(2)));
        }
        if let Some((_, player)) = &audio {
            player.set_volume(if muted.load(Ordering::Relaxed) {
                0.0
            } else {
                1.0
            });
        }
        let frame = movie.frame(frame_index)?;
        let lines = if mask_mode {
            render_mask(&frame, metadata.width, metadata.height, columns, rows)
        } else {
            render_braille(&frame, metadata.width, metadata.height, columns, rows)
        };
        write_patch(
            &mut output,
            if mask_mode { MASK_PATCH } else { FRAME_PATCH },
            frame_index,
            &previous,
            &lines,
        )?;
        previous = lines;
    }
    Ok(())
}

fn listen_for_commands(muted: Arc<AtomicBool>) {
    thread::spawn(move || {
        for byte in io::stdin().lock().bytes().flatten() {
            if byte == b'm' {
                muted.fetch_xor(true, Ordering::Relaxed);
            }
        }
    });
}

fn start_audio(path: &str) -> Result<(rodio::MixerDeviceSink, Player), Box<dyn std::error::Error>> {
    let sink = DeviceSinkBuilder::open_default_sink()?;
    let player = Player::connect_new(sink.mixer());
    player.append(Decoder::try_from(File::open(path)?)?);
    Ok((sink, player))
}

fn pixel(frame: &[u8], width: u16, x: usize, y: usize) -> bool {
    let row_bytes = usize::from(width).div_ceil(8);
    frame[y * row_bytes + x / 8] & (0x80 >> (x % 8)) != 0
}

fn render_braille(
    frame: &[u8],
    width: u16,
    height: u16,
    columns: usize,
    rows: usize,
) -> Vec<String> {
    let dot_width = columns * 2;
    let dot_height = rows * 4;
    (0..rows)
        .map(|cell_y| {
            let mut line = String::with_capacity(columns * 3);
            for cell_x in 0..columns {
                let mut bits = 0_u8;
                for (dot_y, dot_row) in DOTS.iter().enumerate() {
                    for (dot_x, dot) in dot_row.iter().enumerate() {
                        let source_x = (cell_x * 2 + dot_x) * usize::from(width) / dot_width;
                        let source_y = (cell_y * 4 + dot_y) * usize::from(height) / dot_height;
                        if pixel(frame, width, source_x, source_y) {
                            bits |= *dot;
                        }
                    }
                }
                line.push(
                    char::from_u32(0x2800 + u32::from(bits)).expect("valid braille codepoint"),
                );
            }
            line
        })
        .collect()
}

fn render_mask(frame: &[u8], width: u16, height: u16, columns: usize, rows: usize) -> Vec<String> {
    (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    let source_x = column * usize::from(width) / columns;
                    let source_y = row * usize::from(height) / rows;
                    if pixel(frame, width, source_x, source_y) {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect()
        })
        .collect()
}

fn write_patch(
    output: &mut impl Write,
    message_type: u8,
    frame_index: u32,
    previous: &[String],
    current: &[String],
) -> io::Result<()> {
    let changed: Vec<(u16, &[u8])> = current
        .iter()
        .enumerate()
        .filter(|(row, line)| previous.get(*row) != Some(line))
        .map(|(row, line)| (row as u16, line.as_bytes()))
        .collect();
    let payload_size = 1
        + 4
        + 2
        + changed
            .iter()
            .map(|(_, line)| 2 + 4 + line.len())
            .sum::<usize>();
    output.write_all(&(payload_size as u32).to_le_bytes())?;
    output.write_all(&[message_type])?;
    output.write_all(&frame_index.to_le_bytes())?;
    output.write_all(&(changed.len() as u16).to_le_bytes())?;
    for (row, line) in changed {
        output.write_all(&row.to_le_bytes())?;
        output.write_all(&(line.len() as u32).to_le_bytes())?;
        output.write_all(line)?;
    }
    output.flush()
}
