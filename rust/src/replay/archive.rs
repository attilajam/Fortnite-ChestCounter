use chrono::{DateTime, NaiveDate, TimeZone, Utc};

// ── Version constants (matching Unreal.Core.Models.Enums) ───────────────────

pub mod replay_ver {
    pub const HISTORY_INITIAL: u32 = 0;
    pub const HISTORY_FIXEDSIZE_FRIENDLY_NAME: u32 = 1;
    pub const HISTORY_COMPRESSION: u32 = 2;
    pub const HISTORY_RECORDED_TIMESTAMP: u32 = 3;
    pub const HISTORY_STREAM_CHUNK_TIMES: u32 = 4;
    pub const HISTORY_FRIENDLY_NAME_ENCODING: u32 = 5;
    pub const HISTORY_ENCRYPTION: u32 = 6;
    pub const HISTORY_CUSTOM_VERSIONS: u32 = 7;
}

pub mod net_ver {
    pub const HISTORY_EXTRA_VERSION: u32 = 5;
    pub const HISTORY_MULTIPLE_LEVELS: u32 = 6;
    pub const HISTORY_DELETED_STARTUP_ACTORS: u32 = 8;
    pub const HISTORY_HEADER_FLAGS: u32 = 9;
    pub const HISTORY_LEVEL_STREAMING_FIXES: u32 = 10;
    pub const HISTORY_SAVE_FULL_ENGINE_VERSION: u32 = 11;
    pub const HISTORY_HEADER_GUID: u32 = 12;
    pub const HISTORY_SAVE_PACKAGE_VERSION_UE: u32 = 17;
    pub const HISTORY_RECORDING_METADATA: u32 = 18;
    pub const HISTORY_USE_CUSTOM_VERSION: u32 = 19;
}

pub mod engine_net_ver {
    pub const HISTORY_MAX_ACTOR_CHANNELS_CUSTOMIZATION: u32 = 3;
    pub const HISTORY_NEW_ACTOR_OVERRIDE_LEVEL: u32 = 5;
    pub const HISTORY_CHANNEL_NAMES: u32 = 6;
    pub const HISTORY_CHANNEL_CLOSE_REASON: u32 = 7;
    pub const HISTORY_ACKS_INCLUDED_IN_HEADER: u32 = 8;
    pub const HISTORY_NETEXPORT_SERIALIZATION: u32 = 9;
    pub const HISTORY_NETEXPORT_SERIALIZE_FIX: u32 = 10;
    pub const HISTORY_FAST_ARRAY_DELTA_STRUCT: u32 = 11;
    pub const HISTORY_OPTIONALLY_QUANTIZE_SPAWN_INFO: u32 = 13;
    pub const HISTORY_CLASSNETCACHE_FULLNAME: u32 = 15;
    pub const HISTORY_SUBOBJECT_OUTER_CHAIN: u32 = 18;
    pub const HISTORY_PACKED_VECTOR_LWC_SUPPORT: u32 = 23;
    pub const HISTORY_REPMOVE_SERVERFRAME_AND_HANDLE: u32 = 25;
    pub const HISTORY_21_AND_VIEWPITCH_ONLY_DO_NOT_USE: u32 = 26;
    pub const HISTORY_SUBOBJECT_DESTROY_FLAG: u32 = 30;
    pub const REP_MOVE_OPTIONAL_ACCELERATION: u32 = 35;
    pub const CUSTOM_EXPORTS: u32 = 36;
}

pub mod header_flags {
    pub const HAS_STREAMING_FIXES: u32 = 1 << 1;
    pub const DELTA_CHECKPOINTS: u32 = 1 << 2;
    pub const GAME_SPECIFIC_FRAME_DATA: u32 = 1 << 3;
}

pub mod export_flags {
    pub const HAS_PATH: u8 = 1;
    pub const HAS_NETWORK_CHECKSUM: u8 = 4;
}

pub mod chunk_type {
    pub const HEADER: u32 = 0;
    pub const REPLAY_DATA: u32 = 1;
    pub const CHECKPOINT: u32 = 2;
    pub const EVENT: u32 = 3;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ArchiveMeta {
    pub replay_version: u32,
    pub network_version: u32,
    pub engine_network_version: u32,
    pub header_flags: u32,
}

impl ArchiveMeta {
    pub fn has_level_streaming_fixes(&self) -> bool {
        (self.header_flags & header_flags::HAS_STREAMING_FIXES) != 0
    }
    pub fn has_game_specific_frame_data(&self) -> bool {
        (self.header_flags & header_flags::GAME_SPECIFIC_FRAME_DATA) != 0
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FVector {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl FVector {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FRotator {
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorQuantization {
    RoundWholeNumber,
    RoundOneDecimal,
    RoundTwoDecimals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotatorQuantization {
    ByteComponents,
    ShortComponents,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FRepMovement {
    pub location: FVector,
    pub rotation: FRotator,
    pub linear_velocity: FVector,
}

/// Converts .NET / UE `FDateTime` ticks (100-nanosecond intervals since `0001-01-01T00:00:00Z`,
/// with the top 2 bits optionally encoding `DateTimeKind` in `DateTime.FromBinary`) to `DateTime<Utc>`.
pub fn dotnet_ticks_to_utc(raw: i64) -> Option<DateTime<Utc>> {
    // Strip the 2-bit DateTimeKind prefix if present (DateTime.ToBinary stores kind in bits 62..63).
    let ticks = raw & 0x3FFF_FFFF_FFFF_FFFF;
    // Unix epoch (1970-01-01) in .NET ticks = 621_355_968_000_000_000
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    if ticks <= 0 {
        return None;
    }
    let delta = ticks - UNIX_EPOCH_TICKS;
    let secs = delta.div_euclid(10_000_000);
    let rem_ticks = delta.rem_euclid(10_000_000) as u32;
    let nanos = rem_ticks * 100;
    // Sanity check: year between 2000 and 2100
    let dt = Utc.timestamp_opt(secs, nanos).single()?;
    let min_dt = NaiveDate::from_ymd_opt(2000, 1, 1)?.and_hms_opt(0, 0, 0)?.and_utc();
    let max_dt = NaiveDate::from_ymd_opt(2100, 1, 1)?.and_hms_opt(0, 0, 0)?.and_utc();
    if dt >= min_dt && dt <= max_dt {
        Some(dt)
    } else {
        None
    }
}

fn hardcoded_unreal_name(index: u32) -> String {
    match index {
        0 => "None".to_string(),
        102 => "Actor".to_string(),
        255 => "Control".to_string(),
        256 => "Voice".to_string(),
        other => other.to_string(),
    }
}

fn bytes_to_hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

fn decode_fstring_bytes(bytes: &[u8], is_unicode: bool) -> String {
    if !is_unicode {
        let trimmed = if bytes.last() == Some(&0) {
            &bytes[..bytes.len() - 1]
        } else {
            bytes
        };
        String::from_utf8_lossy(trimmed).trim_matches(' ').to_string()
    } else {
        let trimmed = if bytes.len() >= 2 && bytes[bytes.len() - 1] == 0 && bytes[bytes.len() - 2] == 0 {
            &bytes[..bytes.len() - 2]
        } else {
            bytes
        };
        let u16s: Vec<u16> = trimmed
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s).trim_matches(' ').to_string()
    }
}

// ── Byte-aligned reader (`Unreal.Core.BinaryReader`) ────────────────────────

pub struct BinaryReader<'a> {
    data: &'a [u8],
    pub pos: usize,
    pub is_error: bool,
    pub meta: ArchiveMeta,
}

impl<'a> BinaryReader<'a> {
    pub fn new(data: &'a [u8], meta: ArchiveMeta) -> Self {
        Self {
            data,
            pos: 0,
            is_error: false,
            meta,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len() || self.is_error
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) {
        if pos <= self.data.len() {
            self.pos = pos;
        } else {
            self.pos = self.data.len();
            self.is_error = true;
        }
    }

    pub fn skip_bytes(&mut self, n: usize) {
        if let Some(next) = self.pos.checked_add(n) {
            if next <= self.data.len() {
                self.pos = next;
                return;
            }
        }
        self.pos = self.data.len();
        self.is_error = true;
    }

    pub fn read_bytes(&mut self, n: usize) -> &'a [u8] {
        if let Some(end) = self.pos.checked_add(n) {
            if end <= self.data.len() {
                let s = &self.data[self.pos..end];
                self.pos = end;
                return s;
            }
        }
        self.is_error = true;
        &[]
    }

    pub fn read_u8(&mut self) -> u8 {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            b
        } else {
            self.is_error = true;
            0
        }
    }

    pub fn read_bool_byte(&mut self) -> bool {
        self.read_u8() != 0
    }

    pub fn read_u16(&mut self) -> u16 {
        let s = self.read_bytes(2);
        if s.len() == 2 {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            0
        }
    }

    pub fn read_i32(&mut self) -> i32 {
        let s = self.read_bytes(4);
        if s.len() == 4 {
            i32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            0
        }
    }

    pub fn read_u32(&mut self) -> u32 {
        let s = self.read_bytes(4);
        if s.len() == 4 {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            0
        }
    }

    pub fn read_u32_bool(&mut self) -> bool {
        self.read_u32() != 0
    }

    pub fn read_i64(&mut self) -> i64 {
        let s = self.read_bytes(8);
        if s.len() == 8 {
            i64::from_le_bytes(s.try_into().unwrap())
        } else {
            0
        }
    }

    pub fn read_u64(&mut self) -> u64 {
        let s = self.read_bytes(8);
        if s.len() == 8 {
            u64::from_le_bytes(s.try_into().unwrap())
        } else {
            0
        }
    }

    pub fn read_f32(&mut self) -> f32 {
        let s = self.read_bytes(4);
        if s.len() == 4 {
            f32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            0.0
        }
    }

    pub fn read_f64(&mut self) -> f64 {
        let s = self.read_bytes(8);
        if s.len() == 8 {
            f64::from_le_bytes(s.try_into().unwrap())
        } else {
            0.0
        }
    }

    pub fn read_int_packed(&mut self) -> u32 {
        let mut value = 0u32;
        let mut shift = 0u32;
        for _ in 0..5 {
            if self.pos >= self.data.len() {
                self.is_error = true;
                return value;
            }
            let b = self.data[self.pos];
            self.pos += 1;
            value |= ((b >> 1) as u32).wrapping_shl(shift);
            if (b & 1) == 0 {
                break;
            }
            shift += 7;
        }
        value
    }

    pub fn read_fstring(&mut self) -> String {
        let length = self.read_i32();
        if length == 0 || self.is_error {
            return String::new();
        }
        let is_unicode = length < 0;
        let byte_len = if is_unicode {
            (-length as usize).saturating_mul(2)
        } else {
            length as usize
        };
        if byte_len > self.remaining() || byte_len > 16 * 1024 * 1024 {
            self.is_error = true;
            return String::new();
        }
        let bytes = self.read_bytes(byte_len);
        decode_fstring_bytes(bytes, is_unicode)
    }

    pub fn read_fname(&mut self) -> String {
        let is_hardcoded = self.read_bool_byte();
        if is_hardcoded {
            let idx = if self.meta.engine_network_version < engine_net_ver::HISTORY_CHANNEL_NAMES {
                self.read_u32()
            } else {
                self.read_int_packed()
            };
            return hardcoded_unreal_name(idx);
        }
        let s = self.read_fstring();
        let _num = self.read_i32();
        s
    }

    pub fn read_guid(&mut self) -> String {
        let b = self.read_bytes(16);
        bytes_to_hex_upper(b)
    }

    pub fn skip_ftransform(&mut self) {
        // Quat (4 floats or 4 doubles) + Translation (3) + Scale3D (3)
        let elem = if self.meta.engine_network_version >= engine_net_ver::HISTORY_PACKED_VECTOR_LWC_SUPPORT {
            8
        } else {
            4
        };
        self.skip_bytes(10 * elem);
    }
}

// ── Bit-level reader (`Unreal.Core.BitReader` + `NetBitReader`) ─────────────

#[derive(Debug, Clone)]
pub struct BitReader {
    buf: Vec<u8>,
    pub pos: usize,
    pub last_bit: usize,
    saved_ends: Vec<usize>,
    pub is_error: bool,
    pub meta: ArchiveMeta,
}

impl BitReader {
    pub fn from_bytes(bytes: &[u8], bit_len: usize, meta: ArchiveMeta) -> Self {
        let max_bits = bytes.len() * 8;
        Self {
            buf: bytes.to_vec(),
            pos: 0,
            last_bit: bit_len.min(max_bits),
            saved_ends: Vec::with_capacity(4),
            is_error: false,
            meta,
        }
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.last_bit || self.is_error
    }

    pub fn bits_left(&self) -> usize {
        self.last_bit.saturating_sub(self.pos)
    }

    pub fn can_read(&self, bits: usize) -> bool {
        !self.is_error && self.pos.saturating_add(bits) <= self.last_bit
    }

    pub fn set_temp_end(&mut self, bits: usize) {
        let new_end = self.pos.saturating_add(bits).min(self.last_bit);
        self.saved_ends.push(self.last_bit);
        self.last_bit = new_end;
    }

    pub fn restore_temp_end(&mut self) {
        // Advance to end of the scoped payload (same as Unreal.Core RestoreTempEnd)
        self.pos = self.last_bit;
        if let Some(prev) = self.saved_ends.pop() {
            self.last_bit = prev;
        }
        self.is_error = false;
    }

    pub fn append_bits(&mut self, data: &[u8], num_bits: usize) {
        if num_bits == 0 {
            return;
        }
        let target_bits = self.last_bit + num_bits;
        let target_bytes = (target_bits + 7) >> 3;
        if self.buf.len() < target_bytes {
            self.buf.resize(target_bytes, 0);
        }
        if (self.last_bit & 7) == 0 {
            let byte_offset = self.last_bit >> 3;
            let copy_bytes = (num_bits + 7) >> 3;
            self.buf[byte_offset..byte_offset + copy_bytes]
                .copy_from_slice(&data[..copy_bytes]);
            // Clear unused trailing bits in last byte
            let rem = target_bits & 7;
            if rem != 0 {
                let last_idx = byte_offset + copy_bytes - 1;
                self.buf[last_idx] &= (1u8 << rem) - 1;
            }
        } else {
            for i in 0..num_bits {
                let bit = (data[i >> 3] >> (i & 7)) & 1;
                let dst = self.last_bit + i;
                if bit != 0 {
                    self.buf[dst >> 3] |= 1u8 << (dst & 7);
                } else {
                    self.buf[dst >> 3] &= !(1u8 << (dst & 7));
                }
            }
        }
        self.last_bit = target_bits;
    }

    pub fn skip_bits(&mut self, bits: usize) {
        if self.can_read(bits) {
            self.pos += bits;
        } else {
            self.pos = self.last_bit;
            self.is_error = true;
        }
    }

    #[inline]
    pub fn read_bit(&mut self) -> bool {
        if self.pos >= self.last_bit || self.is_error {
            self.is_error = true;
            return false;
        }
        let b = (self.buf[self.pos >> 3] >> (self.pos & 7)) & 1;
        self.pos += 1;
        b != 0
    }

    #[inline]
    pub fn read_u8(&mut self) -> u8 {
        if !self.can_read(8) {
            self.is_error = true;
            return 0;
        }
        let byte_idx = self.pos >> 3;
        let bit_off = self.pos & 7;
        self.pos += 8;
        if bit_off == 0 {
            self.buf[byte_idx]
        } else {
            let lo = self.buf[byte_idx] >> bit_off;
            let hi = self.buf[byte_idx + 1] << (8 - bit_off);
            lo | hi
        }
    }

    pub fn read_bytes_into(&mut self, dst: &mut [u8]) {
        let bits = dst.len() * 8;
        if !self.can_read(bits) {
            self.is_error = true;
            dst.fill(0);
            return;
        }
        let bit_off = self.pos & 7;
        let byte_idx = self.pos >> 3;
        if bit_off == 0 {
            dst.copy_from_slice(&self.buf[byte_idx..byte_idx + dst.len()]);
            self.pos += bits;
        } else {
            let rshift = bit_off;
            let lshift = 8 - bit_off;
            for (i, out) in dst.iter_mut().enumerate() {
                let idx = byte_idx + i;
                *out = (self.buf[idx] >> rshift) | (self.buf[idx + 1] << lshift);
            }
            self.pos += bits;
        }
    }

    pub fn read_bytes(&mut self, n: usize) -> Vec<u8> {
        if !self.can_read(n.saturating_mul(8)) {
            self.is_error = true;
            return Vec::new();
        }
        let mut out = vec![0u8; n];
        self.read_bytes_into(&mut out);
        out
    }

    pub fn read_bits(&mut self, num_bits: usize) -> Vec<u8> {
        if num_bits == 0 {
            return Vec::new();
        }
        if !self.can_read(num_bits) {
            self.is_error = true;
            return Vec::new();
        }
        let full_bytes = num_bits >> 3;
        let rem_bits = num_bits & 7;
        let total_bytes = full_bytes + usize::from(rem_bits > 0);
        let mut out = vec![0u8; total_bytes];
        if full_bytes > 0 {
            self.read_bytes_into(&mut out[..full_bytes]);
        }
        if rem_bits > 0 {
            let mut last = 0u8;
            for k in 0..rem_bits {
                if self.read_bit() {
                    last |= 1u8 << k;
                }
            }
            out[full_bytes] = last;
        }
        out
    }

    pub fn read_bits_to_u64(&mut self, num_bits: usize) -> u64 {
        if num_bits == 0 {
            return 0;
        }
        if !self.can_read(num_bits) || num_bits > 64 {
            self.is_error = true;
            return 0;
        }
        let mut val = 0u64;
        let full_bytes = num_bits >> 3;
        let rem_bits = num_bits & 7;
        for i in 0..full_bytes {
            val |= (self.read_u8() as u64) << (i * 8);
        }
        if rem_bits > 0 {
            let mut rem = 0u64;
            for k in 0..rem_bits {
                if self.read_bit() {
                    rem |= 1u64 << k;
                }
            }
            val |= rem << (full_bytes * 8);
        }
        val
    }

    pub fn read_bits_to_u32(&mut self, num_bits: usize) -> u32 {
        self.read_bits_to_u64(num_bits) as u32
    }

    pub fn read_u16(&mut self) -> u16 {
        let mut b = [0u8; 2];
        self.read_bytes_into(&mut b);
        u16::from_le_bytes(b)
    }

    pub fn read_i16(&mut self) -> i16 {
        let mut b = [0u8; 2];
        self.read_bytes_into(&mut b);
        i16::from_le_bytes(b)
    }

    pub fn read_i32(&mut self) -> i32 {
        let mut b = [0u8; 4];
        self.read_bytes_into(&mut b);
        i32::from_le_bytes(b)
    }

    pub fn read_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.read_bytes_into(&mut b);
        u32::from_le_bytes(b)
    }

    pub fn read_i64(&mut self) -> i64 {
        let mut b = [0u8; 8];
        self.read_bytes_into(&mut b);
        i64::from_le_bytes(b)
    }

    pub fn read_f32(&mut self) -> f32 {
        let mut b = [0u8; 4];
        self.read_bytes_into(&mut b);
        f32::from_le_bytes(b)
    }

    pub fn read_f64(&mut self) -> f64 {
        let mut b = [0u8; 8];
        self.read_bytes_into(&mut b);
        f64::from_le_bytes(b)
    }

    pub fn read_int_packed(&mut self) -> u32 {
        let mut val = 0u32;
        let mut shift = 0u32;
        for _ in 0..5 {
            if !self.can_read(8) {
                self.is_error = true;
                return val;
            }
            let b = self.read_u8();
            val |= ((b >> 1) as u32).wrapping_shl(shift);
            if (b & 1) == 0 {
                break;
            }
            shift += 7;
        }
        val
    }

    pub fn read_serialized_int(&mut self, max_value: u32) -> u32 {
        let mut value = 0u32;
        let mut mask = 1u32;
        while value.saturating_add(mask) < max_value {
            if self.read_bit() {
                value |= mask;
            }
            if mask >= (1u32 << 31) {
                break;
            }
            mask <<= 1;
        }
        value
    }

    pub fn read_fstring(&mut self) -> String {
        let length = self.read_i32();
        if length == 0 || self.is_error {
            return String::new();
        }
        let is_unicode = length < 0;
        let byte_len = if is_unicode {
            (-length as usize).saturating_mul(2)
        } else {
            length as usize
        };
        if !self.can_read(byte_len.saturating_mul(8)) {
            self.is_error = true;
            return String::new();
        }
        let bytes = self.read_bytes(byte_len);
        decode_fstring_bytes(&bytes, is_unicode)
    }

    pub fn read_fname(&mut self) -> String {
        let is_hardcoded = self.read_bit();
        if is_hardcoded {
            let idx = if self.meta.engine_network_version < engine_net_ver::HISTORY_CHANNEL_NAMES {
                self.read_u32()
            } else {
                self.read_int_packed()
            };
            return hardcoded_unreal_name(idx);
        }
        let s = self.read_fstring();
        let _num = self.read_i32();
        s
    }

    pub fn read_fvector(&mut self) -> FVector {
        if self.meta.engine_network_version >= engine_net_ver::HISTORY_PACKED_VECTOR_LWC_SUPPORT {
            FVector::new(self.read_f64(), self.read_f64(), self.read_f64())
        } else {
            FVector::new(
                self.read_f32() as f64,
                self.read_f32() as f64,
                self.read_f32() as f64,
            )
        }
    }

    pub fn read_packed_vector(&mut self, scale_factor: i32, max_bits: u32) -> FVector {
        if self.meta.engine_network_version >= engine_net_ver::HISTORY_PACKED_VECTOR_LWC_SUPPORT
            && self.meta.engine_network_version != engine_net_ver::HISTORY_21_AND_VIEWPITCH_ONLY_DO_NOT_USE
        {
            self.read_quantized_vector(scale_factor)
        } else {
            self.read_packed_vector_legacy(scale_factor, max_bits)
        }
    }

    fn read_quantized_vector(&mut self, scale_factor: i32) -> FVector {
        let info = self.read_serialized_int(1 << 7);
        let bit_count = (info & 63) as usize;
        let extra = info >> 6;
        if bit_count > 0 {
            let x = self.read_bits_to_u64(bit_count);
            let y = self.read_bits_to_u64(bit_count);
            let z = self.read_bits_to_u64(bit_count);
            let sign_bit = 1u64 << (bit_count - 1);
            let mut fx = ((x ^ sign_bit) as i64).wrapping_sub(sign_bit as i64) as f64;
            let mut fy = ((y ^ sign_bit) as i64).wrapping_sub(sign_bit as i64) as f64;
            let mut fz = ((z ^ sign_bit) as i64).wrapping_sub(sign_bit as i64) as f64;
            if extra > 0 {
                let sf = scale_factor as f64;
                fx /= sf;
                fy /= sf;
                fz /= sf;
            }
            FVector::new(fx, fy, fz)
        } else if extra == 0 {
            FVector::new(
                self.read_f32() as f64,
                self.read_f32() as f64,
                self.read_f32() as f64,
            )
        } else {
            FVector::new(self.read_f64(), self.read_f64(), self.read_f64())
        }
    }

    fn read_packed_vector_legacy(&mut self, scale_factor: i32, max_bits: u32) -> FVector {
        let bits = self.read_serialized_int(max_bits);
        if self.is_error {
            return FVector::default();
        }
        let bias = 1i64 << (bits + 1);
        let max = 1u32.wrapping_shl(bits + 2);
        let dx = self.read_serialized_int(max) as i64;
        let dy = self.read_serialized_int(max) as i64;
        let dz = self.read_serialized_int(max) as i64;
        if self.is_error {
            return FVector::default();
        }
        let sf = scale_factor as f64;
        FVector::new(
            (dx - bias) as f64 / sf,
            (dy - bias) as f64 / sf,
            (dz - bias) as f64 / sf,
        )
    }

    pub fn read_rotation(&mut self) -> FRotator {
        let pitch = if self.read_bit() {
            (self.read_u8() as f32) * 360.0 / 256.0
        } else {
            0.0
        };
        let yaw = if self.read_bit() {
            (self.read_u8() as f32) * 360.0 / 256.0
        } else {
            0.0
        };
        let roll = if self.read_bit() {
            (self.read_u8() as f32) * 360.0 / 256.0
        } else {
            0.0
        };
        FRotator { pitch, yaw, roll }
    }

    pub fn read_rotation_short(&mut self) -> FRotator {
        let pitch = if self.read_bit() {
            (self.read_u16() as f32) * 360.0 / 65536.0
        } else {
            0.0
        };
        let yaw = if self.read_bit() {
            (self.read_u16() as f32) * 360.0 / 65536.0
        } else {
            0.0
        };
        let roll = if self.read_bit() {
            (self.read_u16() as f32) * 360.0 / 65536.0
        } else {
            0.0
        };
        FRotator { pitch, yaw, roll }
    }

    pub fn serialize_quantized_vector(&mut self, q: VectorQuantization) -> FVector {
        match q {
            VectorQuantization::RoundTwoDecimals => self.read_packed_vector(100, 30),
            VectorQuantization::RoundOneDecimal => self.read_packed_vector(10, 27),
            VectorQuantization::RoundWholeNumber => self.read_packed_vector(1, 24),
        }
    }

    pub fn serialize_rep_movement(
        &mut self,
        loc_q: VectorQuantization,
        rot_q: RotatorQuantization,
        vel_q: VectorQuantization,
    ) -> FRepMovement {
        let _sleep = self.read_bit();
        let b_rep_physics = self.read_bit();
        let mut b_rep_server_frame = false;
        let mut b_rep_server_handle = false;

        if self.meta.engine_network_version >= engine_net_ver::HISTORY_REPMOVE_SERVERFRAME_AND_HANDLE
            && self.meta.engine_network_version != engine_net_ver::HISTORY_21_AND_VIEWPITCH_ONLY_DO_NOT_USE
        {
            b_rep_server_frame = self.read_bit();
            b_rep_server_handle = self.read_bit();
        }

        let location = self.serialize_quantized_vector(loc_q);
        let rotation = match rot_q {
            RotatorQuantization::ByteComponents => self.read_rotation(),
            RotatorQuantization::ShortComponents => self.read_rotation_short(),
        };
        let linear_velocity = self.serialize_quantized_vector(vel_q);

        if b_rep_physics {
            let _ang_vel = self.serialize_quantized_vector(vel_q);
        }
        if b_rep_server_frame {
            let _sf = self.read_int_packed();
        }
        if b_rep_server_handle {
            let _sh = self.read_int_packed();
        }
        if self.meta.engine_network_version >= engine_net_ver::REP_MOVE_OPTIONAL_ACCELERATION {
            if self.read_bit() {
                let _acc = self.serialize_quantized_vector(vel_q);
            }
        }
        // ChestCounter patch: skip 1..=8 trailing unknown flag bits on physics/vehicle RepMovement.
        let left = self.bits_left();
        if !self.is_error && left > 0 && left <= 8 {
            self.skip_bits(left);
        }

        FRepMovement {
            location,
            rotation,
            linear_velocity,
        }
    }

    pub fn serialize_property_net_id(&mut self) -> String {
        const IS_ENCODED: u8 = 1 << 0;
        const IS_EMPTY: u8 = 1 << 1;
        const FLAGS_MASK: u8 = (1 << 3) - 1;
        const TYPE_MASK: u8 = 255 ^ FLAGS_MASK;
        const TYPE_HASH_OTHER: u8 = 31;

        let flags = self.read_u8();
        let mut encoded = false;
        if (flags & IS_ENCODED) != 0 {
            encoded = true;
            if (flags & IS_EMPTY) != 0 {
                return String::new();
            }
        }
        let type_hash = (flags & TYPE_MASK) >> 3;
        if type_hash == 0 {
            return "NULL".to_string();
        }
        let mut valid = true;
        if type_hash == TYPE_HASH_OTHER {
            let type_str = self.read_fstring();
            if type_str == "None" {
                valid = false;
            }
        }
        if valid {
            if encoded {
                let sz = self.read_u8() as usize;
                let bytes = self.read_bytes(sz);
                return bytes_to_hex_upper(&bytes);
            } else {
                return self.read_fstring();
            }
        }
        String::new()
    }
}
