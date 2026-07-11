use std::{fmt, io::Cursor};

const MAGIC: &[u8; 4] = b"BAV2";
const VERSION: u16 = 2;
const HEADER_SIZE: usize = 32;
const INDEX_ENTRY_SIZE: usize = 24;
const KEYFRAME: u8 = 0;
const DELTA: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub width: u16,
    pub height: u16,
    pub fps: u16,
    pub frame_count: u32,
    pub keyframe_interval: u16,
}

impl Metadata {
    pub fn row_bytes(self) -> usize {
        usize::from(self.width).div_ceil(8)
    }

    pub fn frame_size(self) -> usize {
        self.row_bytes() * usize::from(self.height)
    }
}

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Truncated,
    FrameOutOfRange,
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid BAV2 file: {message}"),
            Self::Truncated => write!(f, "truncated BAV2 file"),
            Self::FrameOutOfRange => write!(f, "frame index is out of range"),
            Self::Io(error) => write!(f, "BAV2 compression error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy)]
struct Chunk {
    first_frame: u32,
    frame_count: u16,
    offset: usize,
    compressed_len: usize,
    raw_len: usize,
}

pub struct Movie {
    bytes: Vec<u8>,
    metadata: Metadata,
    chunks: Vec<Chunk>,
    cache: Option<(usize, Vec<Vec<u8>>)>,
}

impl Movie {
    pub fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() < HEADER_SIZE {
            return Err(Error::Truncated);
        }
        if &bytes[0..4] != MAGIC {
            return Err(Error::Invalid("bad magic"));
        }
        if read_u16(&bytes, 4)? != VERSION {
            return Err(Error::Invalid("unsupported version"));
        }

        let metadata = Metadata {
            width: read_u16(&bytes, 6)?,
            height: read_u16(&bytes, 8)?,
            fps: read_u16(&bytes, 10)?,
            frame_count: read_u32(&bytes, 12)?,
            keyframe_interval: read_u16(&bytes, 16)?,
        };
        let chunk_count = usize::try_from(read_u32(&bytes, 20)?)
            .map_err(|_| Error::Invalid("chunk count is too large"))?;
        let index_offset = usize::try_from(read_u64(&bytes, 24)?)
            .map_err(|_| Error::Invalid("index offset is too large"))?;
        if metadata.width == 0
            || metadata.height == 0
            || metadata.fps == 0
            || metadata.frame_count == 0
            || metadata.keyframe_interval == 0
            || chunk_count == 0
        {
            return Err(Error::Invalid("zero-valued metadata"));
        }

        let index_size = chunk_count
            .checked_mul(INDEX_ENTRY_SIZE)
            .ok_or(Error::Truncated)?;
        let index_end = index_offset
            .checked_add(index_size)
            .ok_or(Error::Truncated)?;
        if index_offset < HEADER_SIZE || index_end > bytes.len() {
            return Err(Error::Truncated);
        }
        let mut chunks = Vec::with_capacity(chunk_count);
        let mut expected_frame = 0;
        for index in 0..chunk_count {
            let cursor = index_offset + index * INDEX_ENTRY_SIZE;
            let chunk = Chunk {
                first_frame: read_u32(&bytes, cursor)?,
                frame_count: read_u16(&bytes, cursor + 4)?,
                offset: usize::try_from(read_u64(&bytes, cursor + 8)?)
                    .map_err(|_| Error::Invalid("chunk offset is too large"))?,
                compressed_len: usize::try_from(read_u32(&bytes, cursor + 16)?)
                    .map_err(|_| Error::Invalid("compressed chunk is too large"))?,
                raw_len: usize::try_from(read_u32(&bytes, cursor + 20)?)
                    .map_err(|_| Error::Invalid("raw chunk is too large"))?,
            };
            let chunk_end = chunk
                .offset
                .checked_add(chunk.compressed_len)
                .ok_or(Error::Truncated)?;
            if chunk.frame_count == 0
                || chunk.first_frame != expected_frame
                || chunk.offset < HEADER_SIZE
                || chunk_end > index_offset
            {
                return Err(Error::Invalid("invalid chunk index"));
            }
            expected_frame = expected_frame
                .checked_add(u32::from(chunk.frame_count))
                .ok_or(Error::Invalid("frame count overflow"))?;
            chunks.push(chunk);
        }
        if expected_frame != metadata.frame_count {
            return Err(Error::Invalid("chunk index does not cover all frames"));
        }

        Ok(Self {
            bytes,
            metadata,
            chunks,
            cache: None,
        })
    }

    pub fn metadata(&self) -> Metadata {
        self.metadata
    }

    pub fn frame(&mut self, index: u32) -> Result<&[u8], Error> {
        if index >= self.metadata.frame_count {
            return Err(Error::FrameOutOfRange);
        }
        let chunk_index = self
            .chunks
            .partition_point(|chunk| chunk.first_frame <= index)
            .saturating_sub(1);
        if self
            .cache
            .as_ref()
            .is_none_or(|(cached, _)| *cached != chunk_index)
        {
            let frames = self.decode_chunk(chunk_index)?;
            self.cache = Some((chunk_index, frames));
        }
        let chunk = self.chunks[chunk_index];
        let local_index = (index - chunk.first_frame) as usize;
        self.cache
            .as_ref()
            .and_then(|(_, frames)| frames.get(local_index))
            .map(Vec::as_slice)
            .ok_or(Error::FrameOutOfRange)
    }

    fn decode_chunk(&self, index: usize) -> Result<Vec<Vec<u8>>, Error> {
        let chunk = self.chunks[index];
        let compressed = &self.bytes[chunk.offset..chunk.offset + chunk.compressed_len];
        let raw = zstd::stream::decode_all(Cursor::new(compressed))?;
        if raw.len() != chunk.raw_len {
            return Err(Error::Invalid("incorrect uncompressed chunk size"));
        }
        decode_records(&raw, chunk.frame_count, self.metadata.frame_size())
    }
}

pub fn encode(metadata: Metadata, frames: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    if frames.len() != metadata.frame_count as usize {
        return Err(Error::Invalid("frame count does not match metadata"));
    }
    if frames
        .iter()
        .any(|frame| frame.len() != metadata.frame_size())
    {
        return Err(Error::Invalid("incorrect frame size"));
    }

    let interval = usize::from(metadata.keyframe_interval);
    let chunk_count = frames.len().div_ceil(interval);
    let mut output = vec![0; HEADER_SIZE];
    let mut chunks = Vec::with_capacity(chunk_count);

    for (chunk_index, chunk_frames) in frames.chunks(interval).enumerate() {
        let raw = encode_records(chunk_frames);
        let compressed = zstd::stream::encode_all(Cursor::new(&raw), 12)?;
        let offset = output.len();
        output.extend_from_slice(&compressed);
        chunks.push(Chunk {
            first_frame: (chunk_index * interval) as u32,
            frame_count: chunk_frames.len() as u16,
            offset,
            compressed_len: compressed.len(),
            raw_len: raw.len(),
        });
    }

    let index_offset = output.len();
    for chunk in &chunks {
        output.extend_from_slice(&chunk.first_frame.to_le_bytes());
        output.extend_from_slice(&chunk.frame_count.to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        output.extend_from_slice(&(chunk.offset as u64).to_le_bytes());
        output.extend_from_slice(&(chunk.compressed_len as u32).to_le_bytes());
        output.extend_from_slice(&(chunk.raw_len as u32).to_le_bytes());
    }

    output[0..4].copy_from_slice(MAGIC);
    output[4..6].copy_from_slice(&VERSION.to_le_bytes());
    output[6..8].copy_from_slice(&metadata.width.to_le_bytes());
    output[8..10].copy_from_slice(&metadata.height.to_le_bytes());
    output[10..12].copy_from_slice(&metadata.fps.to_le_bytes());
    output[12..16].copy_from_slice(&metadata.frame_count.to_le_bytes());
    output[16..18].copy_from_slice(&metadata.keyframe_interval.to_le_bytes());
    output[20..24].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
    output[24..32].copy_from_slice(&(index_offset as u64).to_le_bytes());
    Ok(output)
}

fn encode_records(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut output = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        if index == 0 {
            write_record(&mut output, KEYFRAME, frame);
        } else {
            write_record(&mut output, DELTA, &encode_delta(&frames[index - 1], frame));
        }
    }
    output
}

fn decode_records(raw: &[u8], count: u16, frame_size: usize) -> Result<Vec<Vec<u8>>, Error> {
    let mut frames = Vec::with_capacity(usize::from(count));
    let mut cursor = 0;
    for index in 0..count {
        let kind = *raw.get(cursor).ok_or(Error::Truncated)?;
        let payload_len = read_u32(raw, cursor + 1)? as usize;
        let start = cursor + 5;
        let end = start.checked_add(payload_len).ok_or(Error::Truncated)?;
        let payload = raw.get(start..end).ok_or(Error::Truncated)?;
        if index == 0 {
            if kind != KEYFRAME || payload.len() != frame_size {
                return Err(Error::Invalid("chunk does not start with a keyframe"));
            }
            frames.push(payload.to_vec());
        } else {
            if kind != DELTA {
                return Err(Error::Invalid("unexpected record type"));
            }
            let mut frame = frames.last().ok_or(Error::Truncated)?.clone();
            apply_delta(&mut frame, payload)?;
            frames.push(frame);
        }
        cursor = end;
    }
    Ok(frames)
}

fn encode_delta(previous: &[u8], current: &[u8]) -> Vec<u8> {
    let mut runs: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut cursor = 0;
    while cursor < current.len() {
        if previous[cursor] == current[cursor] {
            cursor += 1;
            continue;
        }
        let start = cursor;
        let mut bytes = Vec::new();
        while cursor < current.len()
            && previous[cursor] != current[cursor]
            && bytes.len() < u16::MAX as usize
        {
            bytes.push(previous[cursor] ^ current[cursor]);
            cursor += 1;
        }
        runs.push((start as u32, bytes));
    }
    let mut payload = Vec::new();
    payload.extend_from_slice(&(runs.len() as u32).to_le_bytes());
    for (offset, bytes) in runs {
        payload.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(&bytes);
    }
    payload
}

fn apply_delta(frame: &mut [u8], payload: &[u8]) -> Result<(), Error> {
    let run_count = read_u32(payload, 0)? as usize;
    let mut cursor = 4;
    for _ in 0..run_count {
        let offset = read_u32(payload, cursor)? as usize;
        let length = read_u16(payload, cursor + 4)? as usize;
        cursor += 6;
        let end = cursor.checked_add(length).ok_or(Error::Truncated)?;
        let frame_end = offset.checked_add(length).ok_or(Error::Truncated)?;
        if end > payload.len() || frame_end > frame.len() {
            return Err(Error::Truncated);
        }
        for (destination, delta) in frame[offset..frame_end]
            .iter_mut()
            .zip(&payload[cursor..end])
        {
            *destination ^= delta;
        }
        cursor = end;
    }
    Ok(())
}

fn write_record(output: &mut Vec<u8>, kind: u8, payload: &[u8]) {
    output.push(kind);
    output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    output.extend_from_slice(payload);
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], Error> {
    let end = offset.checked_add(N).ok_or(Error::Truncated)?;
    bytes
        .get(offset..end)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_indexed_compressed_chunks_and_seeks() {
        let frames = vec![
            vec![0b0000_0000, 0b1111_0000],
            vec![0b0000_0001, 0b1111_0000],
            vec![0b0000_0011, 0b1100_0000],
            vec![0b1111_1111, 0b0000_0000],
        ];
        let metadata = Metadata {
            width: 8,
            height: 2,
            fps: 2,
            frame_count: frames.len() as u32,
            keyframe_interval: 2,
        };
        let encoded = encode(metadata, &frames).unwrap();
        assert!(encoded.len() < 512);
        let mut movie = Movie::open(encoded).unwrap();
        assert_eq!(movie.metadata(), metadata);
        for index in [3, 0, 2, 1] {
            assert_eq!(movie.frame(index).unwrap(), frames[index as usize]);
        }
    }

    #[test]
    fn rejects_an_index_that_does_not_cover_the_movie() {
        let metadata = Metadata {
            width: 8,
            height: 1,
            fps: 1,
            frame_count: 1,
            keyframe_interval: 1,
        };
        let mut encoded = encode(metadata, &[vec![0]]).unwrap();
        let index_offset = read_u64(&encoded, 24).unwrap() as usize;
        encoded[index_offset..index_offset + 4].copy_from_slice(&1_u32.to_le_bytes());
        assert!(matches!(
            Movie::open(encoded),
            Err(Error::Invalid("invalid chunk index"))
        ));
    }
}
