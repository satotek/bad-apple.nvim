# bad-apple.nvim

A buffer-native video experiment for Neovim, backed by a small Rust engine.
It does not run a terminal UI inside Neovim and does not require Deno, Bun, or
ffmpeg during playback.

The project currently provides:

- `bav-format`: the indexed BAV2 keyframe and XOR-run delta format;
- `bav-encode`: a dependency-free P5 PGM sequence encoder;
- `bav-engine`: real-time scaling, Braille rendering, and changed-row output;
- a Lua client that applies row patches to a regular Neovim scratch buffer.

## Requirements

For development:

- Neovim 0.11 or newer
- Rust and Cargo

For playback:

- Neovim 0.11 or newer
- `curl` and `ffmpeg` during the one-time local media generation

Normal playback after that only needs Neovim and the locally generated assets.
It does not require Rust, ffmpeg, Deno, Bun, Node.js, or a compression tool.

## Installation

With lazy.nvim:

```lua
{
  "satotek/bad-apple.nvim",
  config = true,
}
```

Run the player:

```vim
:BadApple
```

On first use, the plugin downloads the platform-specific Rust player and
encoder, then downloads the source PV to your machine and uses `ffmpeg` to
generate the high-resolution BAV2 movie and MP3 audio locally. No movie or
audio asset is distributed in this repository or its GitHub Releases. Files
are stored under `stdpath("data")/bad-apple.nvim`, outside the plugin checkout.
Remove that directory to force a fresh automatic installation.

## Development setup

Build the engine:

```sh
cargo build -p bav-engine
```

Load a local checkout with lazy.nvim:

```lua
{
  dir = vim.fn.expand("~/ghq/github.com/satotek/bad-apple.nvim"),
  config = function()
    require("bad-apple").setup({
      engine_path = vim.fn.expand("~/path/to/bav-engine"),
      movie_path = vim.fn.expand("~/path/to/movie.bav"),
    })
  end,
}
```

During local development, `target/debug/bav-engine` is detected automatically
when neither `engine_path` nor a `bav-engine` executable on `PATH` is found.

## Usage

```vim
:BadApplePlay ~/.local/share/bad-apple/movie.bav
:checkhealth bad-apple
```

Inside the player buffer:

- `<Space>` pauses or resumes playback.
- `m` toggles audio mute.
- `h` seeks backward five seconds.
- `l` seeks forward five seconds.
- `r` restarts from the beginning.
- `q` stops playback and closes the buffer.

## Encoding

`bav-encode` intentionally accepts simple binary PGM images rather than video.
This keeps video codecs and media tools outside the core format and player.

```sh
cargo run -p bav-encode -- movie.bav 30 frames/*.pgm
```

Every input image must use the P5 format, 8-bit samples, and the same size.
Pixels with a value of at least 128 become lit bits.

## BAV2 format

The file starts with fixed metadata and ends with an index table. Each indexed
one-second chunk is compressed independently with zstd, starts with a full
keyframe, and stores subsequent frames as XOR runs relative to the previous
frame. Seeking therefore decompresses at most one small chunk instead of
scanning the movie from the beginning.

The Rust engine reconstructs source frames, scales them directly from packed
1-bit pixels, converts each 2x4 dot group to one Unicode Braille character,
and sends only changed UTF-8 rows to Neovim over a length-prefixed protocol.
The release movie is generated at 480x360 from the source PV. Audio is decoded
inside the Rust engine, and its playback position drives video frame selection.
If no audio device is available, playback continues silently.
The Lua client also tracks the player window size and asks the engine to
rerender the current frame when the window changes.

## Test

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo build -p bav-engine

BAD_APPLE_TEST_ENGINE=$PWD/target/debug/bav-engine \
BAD_APPLE_TEST_MOVIE=/path/to/test.bav \
  nvim --headless -u NONE -l tests/player.lua
```

## Release assets

Tagged releases publish only the Rust player and encoder binaries for Apple
Silicon macOS, x86-64 Linux, and ARM64 Linux. The source PV, derived BAV2 movie,
and MP3 audio are never included. Media conversion happens on the user's own
machine during the first run.

See [NOTICE.md](NOTICE.md) for source and attribution information.
