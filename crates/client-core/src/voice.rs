use crate::protocol::{self, v1};
use std::sync::OnceLock;
use thiserror::Error;

pub const MAX_VOICE_NOTE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OPUS_PACKET_BYTES: usize = 1_275;
pub const OPUS_PRESKIP_SAMPLES: u16 = 312;
pub const OPUS_VBR_ENABLED: bool = true;
pub const OPUS_DTX_ENABLED: bool = true;
pub const MAX_OPUS_DECODE_SAMPLES_PER_CHANNEL: usize = 5_760;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VoiceError {
    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),
    #[error("invalid Opus voice-note configuration")]
    InvalidConfiguration,
    #[error("invalid Ogg Opus container")]
    InvalidContainer,
    #[error("voice note exceeds size limit")]
    TooLarge,
    #[error("Opus codec unavailable on this target")]
    CodecUnavailable,
    #[error("Opus codec failed")]
    Codec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpusContainer {
    Ogg,
    Opus,
}

impl OpusContainer {
    pub fn from_proto(value: i32) -> Result<Self, VoiceError> {
        match v1::opus_audio_metadata::Container::try_from(value) {
            Ok(v1::opus_audio_metadata::Container::Ogg) => Ok(Self::Ogg),
            Ok(v1::opus_audio_metadata::Container::Opus) => Ok(Self::Opus),
            _ => Err(VoiceError::InvalidConfiguration),
        }
    }

    pub fn to_proto(self) -> i32 {
        match self {
            Self::Ogg => v1::opus_audio_metadata::Container::Ogg as i32,
            Self::Opus => v1::opus_audio_metadata::Container::Opus as i32,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Ogg => "ogg",
            Self::Opus => "opus",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Ogg => "audio/ogg",
            Self::Opus => "audio/ogg; codecs=opus",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpusVoiceProfile {
    pub container: OpusContainer,
    pub bitrate_kbps: u32,
    pub sample_rate_hz: u32,
    pub channels: u32,
    pub frame_duration_ms: u32,
}

impl OpusVoiceProfile {
    pub fn new(
        container: OpusContainer,
        bitrate_kbps: u32,
        sample_rate_hz: u32,
        channels: u32,
    ) -> Result<Self, VoiceError> {
        let profile = Self {
            container,
            bitrate_kbps,
            sample_rate_hz,
            channels,
            frame_duration_ms: protocol::OPUS_FRAME_DURATION_MS,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), VoiceError> {
        protocol::validate_opus_audio_metadata(&self.to_proto())?;
        Ok(())
    }

    pub fn to_proto(self) -> v1::OpusAudioMetadata {
        v1::OpusAudioMetadata {
            container: self.container.to_proto(),
            bitrate_kbps: self.bitrate_kbps,
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            frame_duration_ms: self.frame_duration_ms,
        }
    }

    pub fn from_proto(audio: &v1::OpusAudioMetadata) -> Result<Self, VoiceError> {
        let profile = Self {
            container: OpusContainer::from_proto(audio.container)?,
            bitrate_kbps: audio.bitrate_kbps,
            sample_rate_hz: audio.sample_rate_hz,
            channels: audio.channels,
            frame_duration_ms: audio.frame_duration_ms,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn frame_samples_per_channel(self) -> Result<usize, VoiceError> {
        self.sample_rate_hz
            .checked_mul(self.frame_duration_ms)
            .and_then(|samples| samples.checked_div(1_000))
            .map(|samples| samples as usize)
            .ok_or(VoiceError::InvalidConfiguration)
    }

    pub fn frame_samples(self) -> Result<usize, VoiceError> {
        self.frame_samples_per_channel()?
            .checked_mul(self.channels as usize)
            .ok_or(VoiceError::InvalidConfiguration)
    }

    pub fn media_metadata(
        self,
        attachment_id: String,
        ciphertext_size_bytes: u64,
        content_key: Vec<u8>,
        nonce: Vec<u8>,
        ciphertext_sha256: Vec<u8>,
        duration_ms: u64,
    ) -> Result<v1::MediaMetadata, VoiceError> {
        self.validate()?;
        let media = v1::MediaMetadata {
            attachment_id,
            mime_type: self.container.mime_type().to_owned(),
            ciphertext_size_bytes,
            content_key,
            nonce,
            ciphertext_sha256,
            width: None,
            height: None,
            duration_ms: Some(duration_ms),
            blur_hash: None,
            opus: Some(self.to_proto()),
            original_size_bytes: None,
            encryption_chunk_bytes: None,
        };
        protocol::validate_media_metadata(&media)?;
        Ok(media)
    }
}

pub struct OggOpusWriter {
    profile: OpusVoiceProfile,
    serial: u32,
    page_sequence: u32,
    granule_position: u64,
    output: Vec<u8>,
    last_audio_page: Option<usize>,
    audio_packets: u64,
    finished: bool,
}

impl OggOpusWriter {
    pub fn new(profile: OpusVoiceProfile, serial: u32) -> Result<Self, VoiceError> {
        profile.validate()?;
        let mut writer = Self {
            profile,
            serial,
            page_sequence: 0,
            granule_position: OPUS_PRESKIP_SAMPLES as u64,
            output: Vec::new(),
            last_audio_page: None,
            audio_packets: 0,
            finished: false,
        };
        writer.write_page(&opus_head(profile), 0, 0x02)?;
        writer.write_page(&opus_tags(), 0, 0)?;
        Ok(writer)
    }

    pub fn push_packet(&mut self, packet: &[u8]) -> Result<(), VoiceError> {
        if self.finished
            || packet.is_empty()
            || packet.len() > MAX_OPUS_PACKET_BYTES
            || self.audio_packets == u64::MAX
        {
            return Err(VoiceError::InvalidContainer);
        }
        self.granule_position = self
            .granule_position
            .checked_add(self.profile.frame_duration_ms as u64 * 48)
            .ok_or(VoiceError::TooLarge)?;
        self.last_audio_page = Some(self.write_page(packet, self.granule_position, 0)?);
        self.audio_packets += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<u8>, VoiceError> {
        if self.finished || self.audio_packets == 0 {
            return Err(VoiceError::InvalidContainer);
        }
        let page_start = self.last_audio_page.ok_or(VoiceError::InvalidContainer)?;
        self.output[page_start + 5] |= 0x04;
        self.recompute_page_crc(page_start)?;
        self.finished = true;
        Ok(self.output)
    }

    pub fn profile(&self) -> OpusVoiceProfile {
        self.profile
    }

    fn write_page(
        &mut self,
        packet: &[u8],
        granule_position: u64,
        header_type: u8,
    ) -> Result<usize, VoiceError> {
        if packet.len() > MAX_OPUS_PACKET_BYTES {
            return Err(VoiceError::TooLarge);
        }
        let full_segments = (packet.len() + 254) / 255;
        let segment_count = full_segments + usize::from(packet.len() % 255 == 0);
        if segment_count == 0 || segment_count > 255 {
            return Err(VoiceError::InvalidContainer);
        }
        let page_size = 27usize
            .checked_add(segment_count)
            .and_then(|size| size.checked_add(packet.len()))
            .ok_or(VoiceError::TooLarge)?;
        if self.output.len() > MAX_VOICE_NOTE_BYTES - page_size {
            return Err(VoiceError::TooLarge);
        }
        let page_start = self.output.len();
        self.output.extend_from_slice(b"OggS");
        self.output.push(0);
        self.output.push(header_type);
        self.output
            .extend_from_slice(&granule_position.to_le_bytes());
        self.output.extend_from_slice(&self.serial.to_le_bytes());
        self.output
            .extend_from_slice(&self.page_sequence.to_le_bytes());
        self.output.extend_from_slice(&[0, 0, 0, 0]);
        self.output.push(segment_count as u8);
        for index in 0..full_segments {
            let remaining = packet.len() - index * 255;
            self.output.push(remaining.min(255) as u8);
        }
        if packet.len() % 255 == 0 {
            self.output.push(0);
        }
        self.output.extend_from_slice(packet);
        self.recompute_page_crc(page_start)?;
        self.page_sequence = self
            .page_sequence
            .checked_add(1)
            .ok_or(VoiceError::TooLarge)?;
        Ok(page_start)
    }

    fn recompute_page_crc(&mut self, page_start: usize) -> Result<(), VoiceError> {
        if page_start + 26 >= self.output.len() {
            return Err(VoiceError::InvalidContainer);
        }
        self.output[page_start + 22..page_start + 26].fill(0);
        let page_end = page_end(&self.output, page_start)?;
        let checksum = ogg_crc(&self.output[page_start..page_end]);
        self.output[page_start + 22..page_start + 26].copy_from_slice(&checksum.to_le_bytes());
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpusStreamInfo {
    pub channels: u32,
    pub input_sample_rate_hz: u32,
    pub pre_skip_samples: u16,
    pub audio_packet_count: u64,
    pub granule_position: u64,
}

pub fn validate_ogg_opus(data: &[u8]) -> Result<OpusStreamInfo, VoiceError> {
    if data.is_empty() {
        return Err(VoiceError::InvalidContainer);
    }
    if data.len() > MAX_VOICE_NOTE_BYTES {
        return Err(VoiceError::TooLarge);
    }
    let mut offset = 0usize;
    let mut expected_sequence = 0u32;
    let mut stream_serial = None;
    let mut partial_packet = Vec::new();
    let mut packet_index = 0u64;
    let mut audio_packet_count = 0u64;
    let mut channels = 0u32;
    let mut input_sample_rate_hz = 0u32;
    let mut pre_skip_samples = 0u16;
    let mut last_granule = 0u64;
    let mut saw_eos = false;

    while offset < data.len() {
        if saw_eos {
            return Err(VoiceError::InvalidContainer);
        }
        if data.len() - offset < 27 || &data[offset..offset + 4] != b"OggS" {
            return Err(VoiceError::InvalidContainer);
        }
        let page_start = offset;
        let version = data[offset + 4];
        let header_type = data[offset + 5];
        if version != 0 || header_type & !0x07 != 0 {
            return Err(VoiceError::InvalidContainer);
        }
        let granule_position = read_u64(&data[offset + 6..offset + 14])?;
        let serial = read_u32(&data[offset + 14..offset + 18])?;
        let sequence = read_u32(&data[offset + 18..offset + 22])?;
        let stored_crc = read_u32(&data[offset + 22..offset + 26])?;
        let segment_count = data[offset + 26] as usize;
        if segment_count == 0 || sequence != expected_sequence {
            return Err(VoiceError::InvalidContainer);
        }
        if page_start == 0 {
            if header_type & 0x02 == 0 {
                return Err(VoiceError::InvalidContainer);
            }
        } else if header_type & 0x02 != 0 {
            return Err(VoiceError::InvalidContainer);
        }
        if header_type & 0x01 != 0 {
            if partial_packet.is_empty() {
                return Err(VoiceError::InvalidContainer);
            }
        } else if !partial_packet.is_empty() {
            return Err(VoiceError::InvalidContainer);
        }
        if let Some(previous_serial) = stream_serial {
            if previous_serial != serial {
                return Err(VoiceError::InvalidContainer);
            }
        } else {
            stream_serial = Some(serial);
        }
        let segment_table_start = offset + 27;
        let body_start = segment_table_start + segment_count;
        if body_start > data.len() {
            return Err(VoiceError::InvalidContainer);
        }
        let body_len = data[segment_table_start..body_start]
            .iter()
            .try_fold(0usize, |length, segment| {
                length.checked_add(*segment as usize)
            })
            .ok_or(VoiceError::TooLarge)?;
        let page_end = body_start
            .checked_add(body_len)
            .ok_or(VoiceError::TooLarge)?;
        if page_end > data.len() || ogg_crc(&data[page_start..page_end]) != stored_crc {
            return Err(VoiceError::InvalidContainer);
        }
        if granule_position < last_granule {
            return Err(VoiceError::InvalidContainer);
        }
        if packet_index < 2 && granule_position != 0 {
            return Err(VoiceError::InvalidContainer);
        }

        let mut body_offset = body_start;
        for segment in &data[segment_table_start..body_start] {
            let segment_len = *segment as usize;
            if partial_packet.len() > MAX_OPUS_PACKET_BYTES - segment_len {
                return Err(VoiceError::TooLarge);
            }
            partial_packet.extend_from_slice(&data[body_offset..body_offset + segment_len]);
            body_offset += segment_len;
            if *segment < 255 {
                match packet_index {
                    0 => {
                        let (parsed_channels, parsed_rate, parsed_pre_skip) =
                            parse_opus_head(&partial_packet)?;
                        channels = parsed_channels;
                        input_sample_rate_hz = parsed_rate;
                        pre_skip_samples = parsed_pre_skip;
                    }
                    1 => parse_opus_tags(&partial_packet)?,
                    _ => {
                        if partial_packet.is_empty() {
                            return Err(VoiceError::InvalidContainer);
                        }
                        audio_packet_count = audio_packet_count
                            .checked_add(1)
                            .ok_or(VoiceError::TooLarge)?;
                    }
                }
                packet_index = packet_index.checked_add(1).ok_or(VoiceError::TooLarge)?;
                partial_packet.clear();
            }
        }
        if header_type & 0x04 != 0 {
            if page_end != data.len() || !partial_packet.is_empty() {
                return Err(VoiceError::InvalidContainer);
            }
            saw_eos = true;
        }
        last_granule = granule_position;
        offset = page_end;
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or(VoiceError::TooLarge)?;
    }
    if !saw_eos || packet_index < 3 || audio_packet_count == 0 {
        return Err(VoiceError::InvalidContainer);
    }
    Ok(OpusStreamInfo {
        channels,
        input_sample_rate_hz,
        pre_skip_samples,
        audio_packet_count,
        granule_position: last_granule,
    })
}

fn opus_head(profile: OpusVoiceProfile) -> Vec<u8> {
    let mut packet = Vec::with_capacity(19);
    packet.extend_from_slice(b"OpusHead");
    packet.push(1);
    packet.push(profile.channels as u8);
    packet.extend_from_slice(&OPUS_PRESKIP_SAMPLES.to_le_bytes());
    packet.extend_from_slice(&profile.sample_rate_hz.to_le_bytes());
    packet.extend_from_slice(&0i16.to_le_bytes());
    packet.push(0);
    packet
}

fn opus_tags() -> Vec<u8> {
    let vendor = b"Links";
    let mut packet = Vec::with_capacity(16 + vendor.len());
    packet.extend_from_slice(b"OpusTags");
    packet.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    packet.extend_from_slice(vendor);
    packet.extend_from_slice(&0u32.to_le_bytes());
    packet
}

fn parse_opus_head(packet: &[u8]) -> Result<(u32, u32, u16), VoiceError> {
    if packet.len() != 19 || &packet[..8] != b"OpusHead" || packet[8] != 1 || packet[18] != 0 {
        return Err(VoiceError::InvalidContainer);
    }
    let channels = packet[9] as u32;
    let sample_rate_hz = read_u32(&packet[12..16])?;
    if !protocol::OPUS_CHANNELS.contains(&channels)
        || !protocol::OPUS_SAMPLE_RATE_HZ.contains(&sample_rate_hz)
    {
        return Err(VoiceError::InvalidContainer);
    }
    Ok((
        channels,
        sample_rate_hz,
        u16::from_le_bytes([packet[10], packet[11]]),
    ))
}

fn parse_opus_tags(packet: &[u8]) -> Result<(), VoiceError> {
    if packet.len() < 16 || &packet[..8] != b"OpusTags" {
        return Err(VoiceError::InvalidContainer);
    }
    let vendor_len = read_u32(&packet[8..12])? as usize;
    let comments_start = 12usize
        .checked_add(vendor_len)
        .ok_or(VoiceError::TooLarge)?;
    let comments_count_end = comments_start.checked_add(4).ok_or(VoiceError::TooLarge)?;
    if comments_count_end > packet.len() {
        return Err(VoiceError::InvalidContainer);
    }
    let comments_count = read_u32(&packet[comments_start..comments_count_end])? as usize;
    let mut offset = comments_count_end;
    for _ in 0..comments_count {
        if offset + 4 > packet.len() {
            return Err(VoiceError::InvalidContainer);
        }
        let comment_len = read_u32(&packet[offset..offset + 4])? as usize;
        offset = offset
            .checked_add(4)
            .and_then(|value| value.checked_add(comment_len))
            .ok_or(VoiceError::TooLarge)?;
        if offset > packet.len() {
            return Err(VoiceError::InvalidContainer);
        }
    }
    if offset != packet.len() {
        return Err(VoiceError::InvalidContainer);
    }
    Ok(())
}

fn page_end(data: &[u8], page_start: usize) -> Result<usize, VoiceError> {
    if page_start + 27 > data.len() {
        return Err(VoiceError::InvalidContainer);
    }
    let segment_count = data[page_start + 26] as usize;
    let table_end = page_start
        .checked_add(27)
        .and_then(|value| value.checked_add(segment_count))
        .ok_or(VoiceError::TooLarge)?;
    if table_end > data.len() {
        return Err(VoiceError::InvalidContainer);
    }
    let body_len = data[page_start + 27..table_end]
        .iter()
        .try_fold(0usize, |length, segment| {
            length.checked_add(*segment as usize)
        })
        .ok_or(VoiceError::TooLarge)?;
    table_end.checked_add(body_len).ok_or(VoiceError::TooLarge)
}

fn read_u32(bytes: &[u8]) -> Result<u32, VoiceError> {
    let array: [u8; 4] = bytes.try_into().map_err(|_| VoiceError::InvalidContainer)?;
    Ok(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8]) -> Result<u64, VoiceError> {
    let array: [u8; 8] = bytes.try_into().map_err(|_| VoiceError::InvalidContainer)?;
    Ok(u64::from_le_bytes(array))
}

fn ogg_crc(page: &[u8]) -> u32 {
    let table = ogg_crc_table();
    let mut crc = 0u32;
    for (index, byte) in page.iter().copied().enumerate() {
        let byte = if (22..26).contains(&index) { 0 } else { byte };
        crc = (crc << 8) ^ table[((crc >> 24) as u8 ^ byte) as usize];
    }
    crc
}

fn ogg_crc_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, value) in table.iter_mut().enumerate() {
            let mut crc = (index as u32) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ 0x04c1_1db7
                } else {
                    crc << 1
                };
            }
            *value = crc;
        }
        table
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub struct NativeOpusEncoder {
    profile: OpusVoiceProfile,
    encoder: opus::Encoder,
    writer: OggOpusWriter,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeOpusEncoder {
    pub fn new(profile: OpusVoiceProfile, serial: u32) -> Result<Self, VoiceError> {
        profile.validate()?;
        let channels = match profile.channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err(VoiceError::InvalidConfiguration),
        };
        let mut encoder =
            opus::Encoder::new(profile.sample_rate_hz, channels, opus::Application::Voip)
                .map_err(|_| VoiceError::Codec)?;
        encoder
            .set_bitrate(opus::Bitrate::Bits((profile.bitrate_kbps * 1_000) as i32))
            .map_err(|_| VoiceError::Codec)?;
        encoder
            .set_vbr(OPUS_VBR_ENABLED)
            .map_err(|_| VoiceError::Codec)?;
        encoder
            .set_vbr_constraint(true)
            .map_err(|_| VoiceError::Codec)?;
        encoder
            .set_dtx(OPUS_DTX_ENABLED)
            .map_err(|_| VoiceError::Codec)?;
        Ok(Self {
            profile,
            encoder,
            writer: OggOpusWriter::new(profile, serial)?,
        })
    }

    pub fn encode_frame(&mut self, pcm: &[i16]) -> Result<(), VoiceError> {
        if pcm.len() != self.profile.frame_samples()? {
            return Err(VoiceError::InvalidConfiguration);
        }
        let packet = self
            .encoder
            .encode_vec(pcm, MAX_OPUS_PACKET_BYTES)
            .map_err(|_| VoiceError::Codec)?;
        self.writer.push_packet(&packet)
    }

    pub fn finish(self) -> Result<Vec<u8>, VoiceError> {
        self.writer.finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Encode interleaved PCM, padding only the final 20 ms frame when needed.
/// The duration carried in private media metadata remains the unpadded capture
/// duration supplied by the host.
pub fn encode_ogg_opus(pcm: &[i16], profile: OpusVoiceProfile) -> Result<Vec<u8>, VoiceError> {
    profile.validate()?;
    if pcm.is_empty() {
        return Err(VoiceError::InvalidConfiguration);
    }
    let frame_samples = profile.frame_samples()?;
    let mut serial_bytes = [0u8; 4];
    getrandom::fill(&mut serial_bytes).map_err(|_| VoiceError::Codec)?;
    let mut encoder = NativeOpusEncoder::new(profile, u32::from_le_bytes(serial_bytes))?;
    for frame in pcm.chunks(frame_samples) {
        if frame.len() == frame_samples {
            encoder.encode_frame(frame)?;
        } else {
            let mut padded = vec![0i16; frame_samples];
            padded[..frame.len()].copy_from_slice(frame);
            encoder.encode_frame(&padded)?;
        }
    }
    encoder.finish()
}

#[cfg(not(target_arch = "wasm32"))]
/// Decode a validated Ogg Opus voice note into interleaved signed PCM.
/// Playback hosts use this after attachment decryption; the server never sees
/// or invokes this decoder.
pub fn decode_ogg_opus(data: &[u8], profile: OpusVoiceProfile) -> Result<Vec<i16>, VoiceError> {
    profile.validate()?;
    let stream = validate_ogg_opus(data)?;
    if stream.channels != profile.channels || stream.input_sample_rate_hz != profile.sample_rate_hz
    {
        return Err(VoiceError::InvalidConfiguration);
    }
    let packets = audio_packets(data)?;
    let channels = match profile.channels {
        1 => opus::Channels::Mono,
        2 => opus::Channels::Stereo,
        _ => return Err(VoiceError::InvalidConfiguration),
    };
    let mut decoder =
        opus::Decoder::new(profile.sample_rate_hz, channels).map_err(|_| VoiceError::Codec)?;
    let mut pcm = Vec::new();
    for packet in packets {
        let samples = decoder
            .get_nb_samples(&packet)
            .map_err(|_| VoiceError::Codec)?;
        if samples == 0 || samples > MAX_OPUS_DECODE_SAMPLES_PER_CHANNEL {
            return Err(VoiceError::InvalidContainer);
        }
        let mut frame = vec![0i16; MAX_OPUS_DECODE_SAMPLES_PER_CHANNEL * profile.channels as usize];
        let decoded = decoder
            .decode(&packet, &mut frame, false)
            .map_err(|_| VoiceError::Codec)?;
        if decoded == 0 || decoded > MAX_OPUS_DECODE_SAMPLES_PER_CHANNEL {
            return Err(VoiceError::InvalidContainer);
        }
        let decoded_samples = decoded
            .checked_mul(profile.channels as usize)
            .ok_or(VoiceError::TooLarge)?;
        pcm.extend_from_slice(&frame[..decoded_samples]);
        if pcm.len() > MAX_VOICE_NOTE_BYTES * profile.channels as usize {
            return Err(VoiceError::TooLarge);
        }
    }

    // The decoder returns the audio packets' PCM. The OpusHead pre-skip is
    // already represented in the container granule positions and must not be
    // removed a second time by a host playback engine.
    Ok(pcm)
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_packets(data: &[u8]) -> Result<Vec<Vec<u8>>, VoiceError> {
    let mut packets = Vec::new();
    let mut offset = 0usize;
    let mut packet_index = 0u64;
    let mut partial_packet = Vec::new();
    while offset < data.len() {
        let segment_count = *data.get(offset + 26).ok_or(VoiceError::InvalidContainer)? as usize;
        let table_start = offset.checked_add(27).ok_or(VoiceError::TooLarge)?;
        let body_start = table_start
            .checked_add(segment_count)
            .ok_or(VoiceError::TooLarge)?;
        let body_len = data[table_start..body_start]
            .iter()
            .try_fold(0usize, |length, segment| {
                length.checked_add(*segment as usize)
            })
            .ok_or(VoiceError::TooLarge)?;
        let body_end = body_start
            .checked_add(body_len)
            .ok_or(VoiceError::TooLarge)?;
        let mut body_offset = body_start;
        for segment in &data[table_start..body_start] {
            let segment_len = *segment as usize;
            partial_packet.extend_from_slice(&data[body_offset..body_offset + segment_len]);
            body_offset += segment_len;
            if *segment < 255 {
                if packet_index >= 2 {
                    packets.push(std::mem::take(&mut partial_packet));
                } else {
                    partial_packet.clear();
                }
                packet_index = packet_index.checked_add(1).ok_or(VoiceError::TooLarge)?;
            }
        }
        offset = body_end;
    }
    if !partial_packet.is_empty() || packets.is_empty() {
        return Err(VoiceError::InvalidContainer);
    }
    Ok(packets)
}

#[cfg(target_arch = "wasm32")]
pub struct NativeOpusEncoder;

#[cfg(target_arch = "wasm32")]
impl NativeOpusEncoder {
    pub fn new(_profile: OpusVoiceProfile, _serial: u32) -> Result<Self, VoiceError> {
        Err(VoiceError::CodecUnavailable)
    }
}

#[cfg(target_arch = "wasm32")]
pub fn decode_ogg_opus(_data: &[u8], _profile: OpusVoiceProfile) -> Result<Vec<i16>, VoiceError> {
    Err(VoiceError::CodecUnavailable)
}

#[cfg(target_arch = "wasm32")]
pub fn encode_ogg_opus(_pcm: &[i16], _profile: OpusVoiceProfile) -> Result<Vec<u8>, VoiceError> {
    Err(VoiceError::CodecUnavailable)
}
