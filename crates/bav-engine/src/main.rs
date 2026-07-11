use bav_format::Movie;
use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::{
    env, fs,
    fs::File,
    io::{self, BufRead, Write},
    process::ExitCode,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const FRAME_PATCH: u8 = 1;
const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

enum Command {
    ToggleMute,
    TogglePause,
    Seek(f64),
    Restart,
    Resize(usize, usize),
}

struct Clock {
    base: Duration,
    started: Instant,
    paused: bool,
}

impl Clock {
    fn new() -> Self {
        Self {
            base: Duration::ZERO,
            started: Instant::now(),
            paused: false,
        }
    }

    fn position(&self) -> Duration {
        if self.paused {
            self.base
        } else {
            self.base + self.started.elapsed()
        }
    }

    fn toggle_pause(&mut self) {
        if self.paused {
            self.started = Instant::now();
        } else {
            self.base = self.position();
        }
        self.paused = !self.paused;
    }

    fn seek(&mut self, position: Duration) {
        self.base = position;
        self.started = Instant::now();
    }
}

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
    if !(4..=5).contains(&args.len()) {
        return Err("usage: bav-engine MOVIE.bav COLUMNS ROWS [AUDIO.mp3]".into());
    }
    let mut columns: usize = args[2].parse()?;
    let mut rows: usize = args[3].parse()?;
    if columns == 0 || rows == 0 {
        return Err("display dimensions must be positive".into());
    }
    let mut movie = Movie::open(fs::read(&args[1])?)?;
    let metadata = movie.metadata();
    let audio_path = args.get(4);
    let audio = audio_path.and_then(|path| match start_audio(path) {
        Ok(audio) => Some(audio),
        Err(error) => {
            eprintln!("audio unavailable, continuing silently: {error}");
            None
        }
    });
    let commands = listen_for_commands();
    let mut clock = Clock::new();
    let mut muted = false;
    let mut previous: Vec<String> = Vec::new();
    let mut last_frame = None;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let duration = f64::from(metadata.frame_count) / f64::from(metadata.fps);

    loop {
        for command in commands.try_iter() {
            match command {
                Command::ToggleMute => {
                    muted = !muted;
                    if let Some((_, player)) = &audio {
                        player.set_volume(if muted { 0.0 } else { 1.0 });
                    }
                }
                Command::TogglePause => {
                    clock.toggle_pause();
                    if let Some((_, player)) = &audio {
                        if player.is_paused() {
                            player.play();
                        } else {
                            player.pause();
                        }
                    }
                }
                Command::Seek(delta) => {
                    let current = position(&audio, &clock).as_secs_f64();
                    seek(&audio, &mut clock, (current + delta).clamp(0.0, duration));
                    last_frame = None;
                    previous.clear();
                }
                Command::Restart => {
                    seek(&audio, &mut clock, 0.0);
                    last_frame = None;
                    previous.clear();
                }
                Command::Resize(new_columns, new_rows) => {
                    columns = new_columns.max(1);
                    rows = new_rows.max(1);
                    last_frame = None;
                    previous.clear();
                }
            }
        }

        let elapsed = position(&audio, &clock).as_secs_f64();
        let frame_index = (elapsed * f64::from(metadata.fps)).floor() as u32;
        if frame_index >= metadata.frame_count {
            break;
        }
        if last_frame != Some(frame_index) {
            let frame = movie.frame(frame_index)?;
            let lines = render_braille(frame, metadata.width, metadata.height, columns, rows);
            write_patch(&mut output, frame_index, &previous, &lines)?;
            previous = lines;
            last_frame = Some(frame_index);
        }
        thread::sleep(Duration::from_millis(4));
    }
    Ok(())
}

fn listen_for_commands() -> Receiver<Command> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            if let Some(command) = parse_command(&line) {
                if sender.send(command).is_err() {
                    break;
                }
            }
        }
    });
    receiver
}

fn parse_command(line: &str) -> Option<Command> {
    let mut parts = line.split_whitespace();
    match parts.next()? {
        "m" => Some(Command::ToggleMute),
        "p" => Some(Command::TogglePause),
        "seek" => Some(Command::Seek(parts.next()?.parse().ok()?)),
        "restart" => Some(Command::Restart),
        "resize" => Some(Command::Resize(
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        )),
        _ => None,
    }
}

fn position(audio: &Option<(rodio::MixerDeviceSink, Player)>, clock: &Clock) -> Duration {
    audio
        .as_ref()
        .map_or_else(|| clock.position(), |(_, player)| player.get_pos())
}

fn seek(audio: &Option<(rodio::MixerDeviceSink, Player)>, clock: &mut Clock, seconds: f64) {
    let target = Duration::from_secs_f64(seconds);
    clock.seek(target);
    if let Some((_, player)) = audio {
        if let Err(error) = player.try_seek(target) {
            eprintln!("audio seek failed: {error}");
        }
    }
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
