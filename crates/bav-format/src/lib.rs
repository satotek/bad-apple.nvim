use std::fmt;

const MAGIC: &[u8; 4] = b"BAV2";
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 20;
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid BAV2 file: {message}"),
            Self::Truncated => write!(f, "truncated BAV2 file"),
            Self::FrameOutOfRange => write!(f, "frame index is out of range"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy)]
struct Record {
    kind: u8,
    payload_start: usize,
    payload_end: usize,
}

pub struct Movie {
    bytes: Vec<u8>,
    metadata: Metadata,
    records: Vec<Record>,
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
        if metadata.width == 0
            || metadata.height == 0
            || metadata.fps == 0
            || metadata.frame_count == 0
            || metadata.keyframe_interval == 0
        {
            return Err(Error::Invalid("zero-valued metadata"));
        }

        let mut records = Vec::with_capacity(metadata.frame_count as usize);
        let mut cursor = HEADER_SIZE;
        for _ in 0..metadata.frame_count {
            let kind = *bytes.get(cursor).ok_or(Error::Truncated)?;
            let payload_len = read_u32(&bytes, cursor + 1)? as usize;
            let payload_start = cursor + 5;
            let payload_end = payload_start
                .checked_add(payload_len)
                .ok_or(Error::Truncated)?;
            if payload_end > bytes.len() {
                return Err(Error::Truncated);
            }
            if kind != KEYFRAME && kind != DELTA {
                return Err(Error::Invalid("unknown record type"));
            }
            records.push(Record {
                kind,
                payload_start,
                payload_end,
            });
            cursor = payload_end;
        }
        if records.first().is_none_or(|record| record.kind != KEYFRAME) {
            return Err(Error::Invalid("first frame is not a keyframe"));
        }

        Ok(Self {
            bytes,
            metadata,
            records,
        })
    }

    pub fn metadata(&self) -> Metadata {
        self.metadata
    }

    pub fn frame(&self, index: u32) -> Result<Vec<u8>, Error> {
        if index >= self.metadata.frame_count {
            return Err(Error::FrameOutOfRange);
        }
        let target = index as usize;
        let key = (0..=target)
            .rev()
            .find(|candidate| self.records[*candidate].kind == KEYFRAME)
            .ok_or(Error::Invalid("missing keyframe"))?;
        let record = self.records[key];
        let payload = &self.bytes[record.payload_start..record.payload_end];
        if payload.len() != self.metadata.frame_size() {
            return Err(Error::Invalid("incorrect keyframe size"));
        }
        let mut frame = payload.to_vec();
        for record in &self.records[key + 1..=target] {
            self.apply_delta(&mut frame, *record)?;
        }
        Ok(frame)
    }

    fn apply_delta(&self, frame: &mut [u8], record: Record) -> Result<(), Error> {
        if record.kind != DELTA {
            return Err(Error::Invalid("unexpected keyframe in delta sequence"));
        }
        let payload = &self.bytes[record.payload_start..record.payload_end];
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

    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&metadata.width.to_le_bytes());
    output.extend_from_slice(&metadata.height.to_le_bytes());
    output.extend_from_slice(&metadata.fps.to_le_bytes());
    output.extend_from_slice(&metadata.frame_count.to_le_bytes());
    output.extend_from_slice(&metadata.keyframe_interval.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());

    for (index, frame) in frames.iter().enumerate() {
        if index % usize::from(metadata.keyframe_interval) == 0 {
            write_record(&mut output, KEYFRAME, frame);
        } else {
            let payload = encode_delta(&frames[index - 1], frame);
            write_record(&mut output, DELTA, &payload);
        }
    }
    Ok(output)
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

fn write_record(output: &mut Vec<u8>, kind: u8, payload: &[u8]) {
    output.push(kind);
    output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    output.extend_from_slice(payload);
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let data: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    Ok(u16::from_le_bytes(data))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let data: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    Ok(u32::from_le_bytes(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_keyframes_and_deltas() {
        let frames = vec![
            vec![0b0000_0000, 0b1111_0000],
            vec![0b0000_0001, 0b1111_0000],
            vec![0b0000_0011, 0b1100_0000],
            vec![0b1111_1111, 0b0000_0000],
        ];
        let metadata = Metadata {
            width: 8,
            height: 2,
            fps: 30,
            frame_count: frames.len() as u32,
            keyframe_interval: 3,
        };
        let movie = Movie::open(encode(metadata, &frames).unwrap()).unwrap();
        assert_eq!(movie.metadata(), metadata);
        for (index, expected) in frames.iter().enumerate() {
            assert_eq!(&movie.frame(index as u32).unwrap(), expected);
        }
    }
}
