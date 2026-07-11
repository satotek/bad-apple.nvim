use bav_format::Movie;
use std::{
    env, fs,
    io::{self, Write},
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

const FRAME_PATCH: u8 = 1;
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
    if args.len() != 4 {
        return Err("usage: bav-engine MOVIE.bav COLUMNS ROWS".into());
    }
    let columns: usize = args[2].parse()?;
    let rows: usize = args[3].parse()?;
    if columns == 0 || rows == 0 {
        return Err("display dimensions must be positive".into());
    }
    let movie = Movie::open(fs::read(&args[1])?)?;
    let metadata = movie.metadata();
    let frame_duration = Duration::from_secs_f64(1.0 / f64::from(metadata.fps));
    let started = Instant::now();
    let mut previous: Vec<String> = Vec::new();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    for frame_index in 0..metadata.frame_count {
        let deadline = started + frame_duration.mul_f64(f64::from(frame_index));
        if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
            thread::sleep(delay);
        }
        let frame = movie.frame(frame_index)?;
        let lines = render(&frame, metadata.width, metadata.height, columns, rows);
        write_patch(&mut output, frame_index, &previous, &lines)?;
        previous = lines;
    }
    Ok(())
}

fn pixel(frame: &[u8], width: u16, x: usize, y: usize) -> bool {
    let row_bytes = usize::from(width).div_ceil(8);
    frame[y * row_bytes + x / 8] & (0x80 >> (x % 8)) != 0
}

fn render(frame: &[u8], width: u16, height: u16, columns: usize, rows: usize) -> Vec<String> {
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

fn write_patch(
    output: &mut impl Write,
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
    output.write_all(&[FRAME_PATCH])?;
    output.write_all(&frame_index.to_le_bytes())?;
    output.write_all(&(changed.len() as u16).to_le_bytes())?;
    for (row, line) in changed {
        output.write_all(&row.to_le_bytes())?;
        output.write_all(&(line.len() as u32).to_le_bytes())?;
        output.write_all(line)?;
    }
    output.flush()
}
