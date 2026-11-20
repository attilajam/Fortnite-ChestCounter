use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use regex::Regex;

use super::archive::{
    ArchiveMeta, BinaryReader, BitReader, FRepMovement, FVector, RotatorQuantization,
    VectorQuantization, chunk_type, dotnet_ticks_to_utc, engine_net_ver, export_flags, net_ver,
    replay_ver,
};
use super::crypto::{decompress_oodle, decrypt_aes_ecb_pkcs7};
use super::guid_cache::{ExternalData, NetFieldExport, NetFieldExportGroup, NetGuidCache};

pub const FILE_MAGIC: u32 = 0x1CA2E27F;
pub const NETWORK_MAGIC: u32 = 0x2CF5A13D;
const MAX_PACKET_SIZE_IN_BITS: u32 = 16384;
const OLD_MAX_ACTOR_CHANNELS: u32 = 10240;
const MAX_GUID_COUNT: i32 = 2048;

#[derive(Debug, Clone, Default)]
pub struct ReplayInfo {
    pub file_version: u32,
    pub length_in_ms: u32,
    pub network_version: u32,
    pub changelist: u32,
    pub friendly_name: String,
    pub is_live: bool,
    pub timestamp: Option<DateTime<Utc>>,
    pub is_compressed: bool,
    pub is_encrypted: bool,
    pub encryption_key: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct ReplayHeader {
    pub network_version: u32,
    pub engine_network_version: u32,
    pub game_network_protocol_version: u32,
    pub guid: String,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub changelist: u32,
    pub branch: String,
    pub level_names_and_times: Vec<(String, u32)>,
    pub flags: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ReplayTeamStats {
    pub position: u32,
    pub total_players: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ReplayStats {
    pub eliminations: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ReplayGameData {
    pub game_session_id: Option<String>,
    pub utc_time_started_match: Option<DateTime<Utc>>,
    pub current_playlist: Option<String>,
    pub is_custom_match: Option<bool>,
    pub recorder_id: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerData {
    pub id: Option<i32>,
    pub epic_id: Option<String>,
    pub is_bot: bool,
    pub player_name: Option<String>,
    pub team_index: Option<i32>,
    pub placement: Option<i32>,
    pub is_replay_owner: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ReplayData {
    pub info: ReplayInfo,
    pub header: ReplayHeader,
    pub game_data: ReplayGameData,
    pub team_stats: Option<ReplayTeamStats>,
    pub stats: Option<ReplayStats>,
    pub players: Vec<PlayerData>,
    pub is_partial_replay: bool,
}

#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub actor_guid: u32,
    pub archetype_guid: Option<u32>,
    pub location: Option<FVector>,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub index: u32,
    pub actor: Option<ActorInfo>,
}

impl Channel {
    pub fn actor_id(&self) -> Option<u32> {
        self.actor.as_ref().map(|a| a.actor_guid)
    }
    pub fn archetype_id(&self) -> Option<u32> {
        self.actor.as_ref().and_then(|a| a.archetype_guid)
    }
}

#[derive(Debug, Clone)]
pub struct DataBunch {
    pub packet_id: i32,
    pub ch_index: u32,
    pub ch_sequence: i32,
    pub b_open: bool,
    pub b_close: bool,
    pub b_dormant: bool,
    pub b_is_replication_paused: bool,
    pub b_reliable: bool,
    pub b_has_package_map_exports: bool,
    pub b_has_must_be_mapped_guids: bool,
    pub b_partial: bool,
    pub b_partial_initial: bool,
    pub b_partial_final: bool,
    pub close_reason: u32,
    pub archive: Option<BitReader>,
}

impl Default for DataBunch {
    fn default() -> Self {
        Self {
            packet_id: 0,
            ch_index: 0,
            ch_sequence: 0,
            b_open: false,
            b_close: false,
            b_dormant: false,
            b_is_replication_paused: false,
            b_reliable: false,
            b_has_package_map_exports: false,
            b_has_must_be_mapped_guids: false,
            b_partial: false,
            b_partial_initial: false,
            b_partial_final: false,
            close_reason: 0,
            archive: None,
        }
    }
}

// ── Decoded NetFieldExport groups needed by ChestCounter ────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    GameState,
    FortPlayerState,
    PlayerPawn,
    Container,
    Vehicle,
    SeatComponent,
    LootCannonFired,
    TreasureHuntChest,
    Weapon,
    FortPickup,
}

#[derive(Debug, Clone)]
pub enum ParsedExport {
    GameState {
        game_session_id: Option<String>,
        utc_time_started_match: Option<DateTime<Utc>>,
        b_is_custom_match: Option<bool>,
        recorder_player_state: Option<u32>,
    },
    PlaylistInfo {
        id: u32,
        name: Option<String>,
    },
    FortPlayerState {
        player_id: Option<i32>,
        b_is_bot: Option<bool>,
        b_only_spectator: Option<bool>,
        team_index: Option<i32>,
        place: Option<i32>,
        unique_id: Option<String>,
    },
    PlayerPawn {
        player_state: Option<u32>,
        replicated_movement: Option<FRepMovement>,
    },
    Container {
        searching_pawn: Option<u32>,
        b_already_searched: Option<bool>,
        b_destroyed: bool,
    },
    Vehicle {
        replicated_movement: Option<FRepMovement>,
    },
    SeatComponent {
        player_slots: Option<Vec<Option<u32>>>,
    },
    LootCannonFired {
        interacting_pawn: Option<u32>,
    },
    TreasureHuntChest {
        instigator: Option<u32>,
        source_container: Option<u32>,
        opening_pawn: Option<u32>,
        b_success: Option<bool>,
        prize_template_id_bytes: Option<Vec<u8>>,
    },
    Weapon {
        instigator: Option<u32>,
        owner: Option<u32>,
        weapon_data_name: Option<String>,
    },
    FortPickup {
        b_tossed_from_container: Option<bool>,
        item_definition_guid: Option<u32>,
        item_definition_name: Option<String>,
        owner: Option<u32>,
    },
}

fn vehicle_class_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"_Vehicle(_Static)?_C$|MeatballVehicle").unwrap())
}

fn treasure_actor_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)FlameCeleb|RelicExtractor|RelicFinder|LuckyLocator|SmugglerExtractionKey|ExtractionAccelerat").unwrap()
    })
}

pub fn classify_group(path: &str) -> Option<GroupKind> {
    match path {
        "/Game/Athena/Athena_GameState.Athena_GameState_C" => return Some(GroupKind::GameState),
        "/Script/FortniteGame.FortPlayerStateAthena" => return Some(GroupKind::FortPlayerState),
        "/Game/Athena/PlayerPawn_Athena.PlayerPawn_Athena_C" => return Some(GroupKind::PlayerPawn),
        "/Script/FortniteGame.FortVehicleSeatComponent" => return Some(GroupKind::SeatComponent),
        "/DelilahTreasureHuntGameplay/Gameplay/Actors/BGA_DelilahTreasureHuntAnimatedChest.BGA_DelilahTreasureHuntAnimatedChest_C" => {
            return Some(GroupKind::TreasureHuntChest);
        }
        "/Script/FortniteGame.FortPickupAthena" => return Some(GroupKind::FortPickup),
        "/LootCannon/Gameplay/BGA_LootCannon.BGA_LootCannon_C:FireLootCannon" => {
            return Some(GroupKind::LootCannonFired);
        }
        _ => {}
    }

    if path.contains(':') || path.ends_with("_ClassNetCache") {
        return None;
    }

    let last = path.rsplit('/').next().unwrap_or(path);
    if last.starts_with("BGA_DelilahTreasureHuntAnimatedChest") {
        return Some(GroupKind::TreasureHuntChest);
    }
    if last.starts_with("B_") && treasure_actor_re().is_match(last) {
        return Some(GroupKind::Weapon);
    }
    if vehicle_class_re().is_match(last) {
        return Some(GroupKind::Vehicle);
    }
    if path.to_ascii_lowercase().contains("/containers/")
        || last.to_ascii_lowercase().contains("chest")
    {
        return Some(GroupKind::Container);
    }
    None
}

fn is_player_controller_path(path: &str) -> bool {
    path == "BP_ReplayPC_Athena_C" || path.ends_with(".BP_ReplayPC_Athena_C")
}

/// Decodes Fortnite's obfuscated player name in `ExternalData` (`PlayerNameData.cs`).
pub fn decode_player_name_data(bytes: &[u8], meta: ArchiveMeta) -> Option<String> {
    let mut r = BinaryReader::new(bytes, meta);
    let _handle = r.read_u8();
    let _unk1 = r.read_u8();
    let is_player = r.read_bool_byte();
    let encoded = r.read_fstring();
    if r.is_error {
        return None;
    }
    if !is_player {
        return Some(encoded);
    }
    // Exact C# UTF-16 code unit shifting:
    // shift = (EncodedName.Length % 4 * 3 % 8 + 1 + i) * 3 % 8
    let units: Vec<u16> = encoded.encode_utf16().collect();
    let len = units.len();
    let base = ((len % 4) * 3) % 8;
    let decoded: Vec<u16> = units
        .iter()
        .enumerate()
        .map(|(i, &u)| {
            let shift = ((base + 1 + i) * 3) % 8;
            u.wrapping_add(shift as u16)
        })
        .collect();
    Some(String::from_utf16_lossy(&decoded))
}

// ── Callbacks trait for ReplayAnalyzer ──────────────────────────────────────

pub trait ReplayCallbacks {
    fn on_net_guid_exported(&mut self, _time: f32, _net_guid: u32, _path_name: &str) {}
    fn on_channel_opened(&mut self, _time: f32, _channel: &Channel, _cache: &NetGuidCache) {}
    fn on_export_read(
        &mut self,
        _time: f32,
        _channel_index: u32,
        _channel: Option<&Channel>,
        _export: &ParsedExport,
        _cache: &NetGuidCache,
    ) {
    }
    fn on_raw_probe(
        &mut self,
        _time: f32,
        _channel_index: u32,
        _actor_id: u32,
        _group: &str,
        _field: &str,
        _data: &[u8],
        _bits: usize,
    ) {
    }
}

pub struct NoopCallbacks;
impl ReplayCallbacks for NoopCallbacks {}

// ── Replay Header Probe (fast check without decrypting data chunks) ─────────

pub struct ReplayHeaderProbe;

impl ReplayHeaderProbe {
    pub fn is_live(path: &Path) -> Option<bool> {
        let mut f = File::open(path).ok()?;
        let mut buf = vec![0u8; 4096];
        let n = f.read(&mut buf).ok()?;
        buf.truncate(n);
        let mut r = BinaryReader::new(&buf, ArchiveMeta::default());
        if r.read_u32() != FILE_MAGIC {
            return None;
        }
        let file_version = r.read_u32();
        if file_version >= replay_ver::HISTORY_CUSTOM_VERSIONS {
            let count = r.read_i32();
            if !(0..=64).contains(&count) {
                return None;
            }
            r.skip_bytes((count as usize) * 20);
        }
        let _length_in_ms = r.read_u32();
        let _net_ver = r.read_u32();
        let _changelist = r.read_u32();
        let _friendly_name = r.read_fstring();
        let live = r.read_u32_bool();
        if r.is_error { None } else { Some(live) }
    }

    pub fn key_missing(path: &Path) -> bool {
        let Ok(mut f) = File::open(path) else {
            return true;
        };
        let mut buf = vec![0u8; 4096];
        let Ok(n) = f.read(&mut buf) else {
            return true;
        };
        buf.truncate(n);
        let mut r = BinaryReader::new(&buf, ArchiveMeta::default());
        if r.read_u32() != FILE_MAGIC {
            return true;
        }
        let file_version = r.read_u32();
        if file_version >= replay_ver::HISTORY_CUSTOM_VERSIONS {
            let count = r.read_i32();
            if !(0..=64).contains(&count) {
                return true;
            }
            r.skip_bytes((count as usize) * 20);
        }
        r.skip_bytes(12);
        let _friendly_name = r.read_fstring();
        let _is_live = r.read_u32();
        let _timestamp = r.read_i64();
        let _compressed = r.read_u32();
        if r.read_u32() == 0 {
            return false; // not encrypted
        }
        let key_len = r.read_i32();
        if key_len <= 0 || key_len > 256 {
            return true;
        }
        let key = r.read_bytes(key_len as usize);
        if r.is_error || key.len() != key_len as usize {
            return true;
        }
        key.iter().all(|&b| b == 0)
    }

    pub fn map(path: &Path) -> Option<String> {
        let mut f = File::open(path).ok()?;
        let mut buf = vec![0u8; 256 * 1024];
        let n = f.read(&mut buf).ok()?;
        buf.truncate(n);
        let mut parser = ReplayParser::new(true);
        let mut noop = NoopCallbacks;
        let _ = parser.parse(&buf, &mut noop);
        parser
            .replay
            .header
            .level_names_and_times
            .iter()
            .map(|(l, _)| l)
            .find(|l| !l.is_empty())
            .cloned()
    }
}

// ── Full Replay Parser ──────────────────────────────────────────────────────

pub struct ReplayParser {
    pub events_only: bool,
    pub replay: ReplayData,
    pub meta: ArchiveMeta,
    pub current_time: f32,
    pub guid_cache: NetGuidCache,
    pub channels: HashMap<u32, Channel>,
    partial_bunch: Option<DataBunch>,
    in_packet_id: i32,
    in_reliable: i32,

    // Builder state
    players_by_channel: HashMap<u32, PlayerData>,
    only_spectating: HashSet<u32>,
    channel_to_actor: HashMap<u32, u32>,
}

impl ReplayParser {
    pub fn new(events_only: bool) -> Self {
        Self {
            events_only,
            replay: ReplayData::default(),
            meta: ArchiveMeta::default(),
            current_time: 0.0,
            guid_cache: NetGuidCache::default(),
            channels: HashMap::new(),
            partial_bunch: None,
            in_packet_id: 0,
            in_reliable: 0,
            players_by_channel: HashMap::new(),
            only_spectating: HashSet::new(),
            channel_to_actor: HashMap::new(),
        }
    }

    pub fn parse<C: ReplayCallbacks>(&mut self, data: &[u8], cb: &mut C) -> Result<ReplayData, String> {
        let mut archive = BinaryReader::new(data, ArchiveMeta::default());
        self.read_replay_info(&mut archive)?;
        archive.meta = self.meta;
        self.read_replay_chunks(&mut archive, cb)?;

        let mut out = std::mem::take(&mut self.replay);
        out.players = self.players_by_channel.values().cloned().collect();
        Ok(out)
    }

    fn read_replay_info(&mut self, archive: &mut BinaryReader) -> Result<(), String> {
        let magic = archive.read_u32();
        if magic != FILE_MAGIC {
            return Err(format!("Invalid replay file magic: 0x{magic:08x}"));
        }
        let file_version = archive.read_u32();
        self.meta.replay_version = file_version;
        if file_version >= replay_ver::HISTORY_CUSTOM_VERSIONS {
            let count = archive.read_i32();
            if count < 0 || count > 1024 {
                return Err(format!("Invalid custom version count: {count}"));
            }
            archive.skip_bytes((count as usize) * 20);
        }

        let mut info = ReplayInfo {
            file_version,
            length_in_ms: archive.read_u32(),
            network_version: archive.read_u32(),
            changelist: archive.read_u32(),
            friendly_name: archive.read_fstring(),
            is_live: archive.read_u32_bool(),
            ..Default::default()
        };

        if file_version >= replay_ver::HISTORY_RECORDED_TIMESTAMP {
            info.timestamp = dotnet_ticks_to_utc(archive.read_i64());
        }
        if file_version >= replay_ver::HISTORY_COMPRESSION {
            info.is_compressed = archive.read_u32_bool();
        }
        if file_version >= replay_ver::HISTORY_ENCRYPTION {
            info.is_encrypted = archive.read_u32_bool();
            let key_len = archive.read_u32() as usize;
            info.encryption_key = archive.read_bytes(key_len).to_vec();
        }

        if archive.is_error {
            return Err("Truncated replay info header".to_string());
        }
        if info.is_encrypted && info.encryption_key.is_empty() {
            return Err("Replay is marked encrypted but has no key".to_string());
        }

        self.replay.info = info;
        Ok(())
    }

    fn read_replay_chunks<C: ReplayCallbacks>(
        &mut self,
        archive: &mut BinaryReader,
        cb: &mut C,
    ) -> Result<(), String> {
        while !archive.at_end() {
            if archive.remaining() < 8 {
                self.replay.is_partial_replay = true;
                return Ok(());
            }
            let ctype = archive.read_u32();
            let csize = archive.read_i32();
            let offset = archive.pos;

            if csize <= 0 {
                return Ok(());
            }
            let csize_u = csize as usize;
            if offset.saturating_add(csize_u) > archive.len() {
                self.replay.is_partial_replay = true;
                return Ok(());
            }

            match ctype {
                chunk_type::HEADER => {
                    self.read_replay_header(archive)?;
                    archive.meta = self.meta;
                }
                chunk_type::REPLAY_DATA if !self.events_only => {
                    if let Err(e) = self.read_replay_data(archive, csize_u, cb) {
                        if self.replay.info.is_live {
                            self.replay.is_partial_replay = true;
                            return Ok(());
                        }
                        return Err(e);
                    }
                }
                chunk_type::EVENT => {
                    if let Err(e) = self.read_event(archive) {
                        if !self.replay.info.is_live {
                            // Non-fatal for unknown events, skip chunk
                            let _ = e;
                        }
                    }
                }
                chunk_type::CHECKPOINT | _ => {}
            }

            let next_pos = offset + csize_u;
            if archive.pos != next_pos {
                archive.seek(next_pos);
            }
        }
        Ok(())
    }

    fn read_replay_header(&mut self, archive: &mut BinaryReader) -> Result<(), String> {
        let magic = archive.read_u32();
        if magic != NETWORK_MAGIC {
            return Err(format!("Invalid network header magic: 0x{magic:08x}"));
        }
        let network_version = archive.read_u32();
        if network_version <= net_ver::HISTORY_EXTRA_VERSION {
            return Err(format!("Unsupported network version: {network_version}"));
        }
        if network_version >= net_ver::HISTORY_USE_CUSTOM_VERSION {
            let count = archive.read_i32();
            if count > 0 && count <= 1024 {
                archive.skip_bytes((count as usize) * 20);
            }
        }

        let _checksum = archive.read_u32();
        let engine_network_version = archive.read_u32();
        let game_network_protocol_version = archive.read_u32();

        let guid = if network_version >= net_ver::HISTORY_HEADER_GUID {
            archive.read_guid()
        } else {
            String::new()
        };

        let (major, minor, patch, changelist, branch) =
            if network_version >= net_ver::HISTORY_SAVE_FULL_ENGINE_VERSION {
                let maj = archive.read_u16();
                let min = archive.read_u16();
                let pat = archive.read_u16();
                let cl = archive.read_u32();
                let br = archive.read_fstring();
                (maj, min, pat, cl, br)
            } else {
                (0, 0, 0, archive.read_u32(), String::new())
            };

        if network_version >= net_ver::HISTORY_RECORDING_METADATA {
            let _ue4 = archive.read_u32();
            let _ue5 = archive.read_u32();
            let _lic = archive.read_u32();
        }

        let level_count = archive.read_u32() as usize;
        let mut level_names_and_times = Vec::with_capacity(level_count.min(64));
        for _ in 0..level_count {
            let lvl = archive.read_fstring();
            let time = archive.read_u32();
            if archive.is_error {
                break;
            }
            level_names_and_times.push((lvl, time));
        }

        let flags = if network_version >= net_ver::HISTORY_HEADER_FLAGS {
            archive.read_u32()
        } else {
            0
        };

        let gs_count = archive.read_u32() as usize;
        for _ in 0..gs_count {
            let _ = archive.read_fstring();
            if archive.is_error {
                break;
            }
        }

        if network_version >= net_ver::HISTORY_SAVE_PACKAGE_VERSION_UE {
            let _min_hz = archive.read_f32();
            let _max_hz = archive.read_f32();
            let _frame_limit = archive.read_f32();
            let _cp_limit = archive.read_f32();
            let _platform = archive.read_fstring();
            let _build_cfg = archive.read_u8();
            let _build_target = archive.read_u8();
        }

        self.meta.network_version = network_version;
        self.meta.engine_network_version = engine_network_version;
        self.meta.header_flags = flags;

        self.replay.header = ReplayHeader {
            network_version,
            engine_network_version,
            game_network_protocol_version,
            guid,
            major,
            minor,
            patch,
            changelist,
            branch,
            level_names_and_times,
            flags,
        };
        Ok(())
    }

    fn decrypt_bytes(&self, raw: &[u8]) -> Result<Vec<u8>, String> {
        if !self.replay.info.is_encrypted {
            return Ok(raw.to_vec());
        }
        decrypt_aes_ecb_pkcs7(raw, &self.replay.info.encryption_key).map_err(|e| e.to_string())
    }

    fn decompress_bytes(&self, decrypted: &[u8]) -> Result<Vec<u8>, String> {
        if !self.replay.info.is_compressed {
            return Ok(decrypted.to_vec());
        }
        let mut r = BinaryReader::new(decrypted, self.meta);
        let decompressed_size = r.read_i32();
        let compressed_size = r.read_i32();
        if r.is_error || decompressed_size < 0 || compressed_size < 0 {
            return Err("Invalid compressed block sizes".to_string());
        }
        let compressed = r.read_bytes(compressed_size as usize);
        if r.is_error {
            return Err("Truncated compressed buffer".to_string());
        }
        decompress_oodle(compressed, decompressed_size as usize).map_err(|e| e.to_string())
    }

    fn read_event(&mut self, archive: &mut BinaryReader) -> Result<(), String> {
        let _id = archive.read_fstring();
        let _group = archive.read_fstring();
        let metadata = archive.read_fstring();
        let _start_time = archive.read_u32();
        let _end_time = archive.read_u32();
        let size = archive.read_i32();
        if archive.is_error || size < 0 {
            return Err("Invalid event header".to_string());
        }
        let raw = archive.read_bytes(size as usize);
        if archive.is_error {
            return Err("Truncated event payload".to_string());
        }
        if metadata != "AthenaMatchStats" && metadata != "AthenaMatchTeamStats" {
            return Ok(());
        }
        let dec = self.decrypt_bytes(raw)?;
        let mut er = BinaryReader::new(&dec, self.meta);
        if metadata == "AthenaMatchTeamStats" {
            let _unk = er.read_u32();
            let position = er.read_u32();
            let total_players = er.read_u32();
            if !er.is_error {
                self.replay.team_stats = Some(ReplayTeamStats {
                    position,
                    total_players,
                });
            }
        } else if metadata == "AthenaMatchStats" {
            let _unk = er.read_u32();
            let _accuracy = er.read_f32();
            let _assists = er.read_u32();
            let eliminations = er.read_u32();
            if !er.is_error {
                self.replay.stats = Some(ReplayStats { eliminations });
            }
        }
        Ok(())
    }

    fn read_replay_data<C: ReplayCallbacks>(
        &mut self,
        archive: &mut BinaryReader,
        fallback_chunk_size: usize,
        cb: &mut C,
    ) -> Result<(), String> {
        let length = if (self.meta.replay_version & replay_ver::HISTORY_STREAM_CHUNK_TIMES) != 0
            || self.meta.replay_version >= replay_ver::HISTORY_STREAM_CHUNK_TIMES
        {
            let _start = archive.read_u32();
            let _end = archive.read_u32();
            archive.read_u32() as usize
        } else {
            fallback_chunk_size
        };

        if (self.meta.replay_version & replay_ver::HISTORY_ENCRYPTION)
            == replay_ver::HISTORY_ENCRYPTION
            || self.meta.replay_version >= replay_ver::HISTORY_ENCRYPTION
        {
            let _memory_size_in_bytes = archive.read_i32();
        }

        let raw = archive.read_bytes(length);
        if archive.is_error {
            return Err("Truncated replay data chunk".to_string());
        }
        let dec = self.decrypt_bytes(raw)?;
        let stream_bytes = self.decompress_bytes(&dec)?;
        let mut br = BinaryReader::new(&stream_bytes, self.meta);
        while !br.at_end() {
            self.read_demo_frame(&mut br, cb);
        }
        Ok(())
    }

    fn read_demo_frame<C: ReplayCallbacks>(&mut self, archive: &mut BinaryReader, cb: &mut C) {
        if self.meta.network_version >= net_ver::HISTORY_MULTIPLE_LEVELS {
            let _current_level_index = archive.read_i32();
        }
        let time_seconds = archive.read_f32();
        if archive.is_error {
            return;
        }
        self.current_time = time_seconds;

        if self.meta.network_version >= net_ver::HISTORY_LEVEL_STREAMING_FIXES {
            self.read_export_data(archive, cb);
        }

        if self.meta.has_level_streaming_fixes() {
            let n = archive.read_int_packed();
            for _ in 0..n {
                let _ = archive.read_fstring();
                if archive.is_error {
                    return;
                }
            }
        } else {
            let n = archive.read_int_packed();
            for _ in 0..n {
                let _ = archive.read_fstring();
                let _ = archive.read_fstring();
                archive.skip_ftransform();
                if archive.is_error {
                    return;
                }
            }
        }

        if self.meta.has_level_streaming_fixes() {
            let _skip_external_offset = archive.read_u64();
        }

        self.read_external_data(archive);

        if self.meta.has_game_specific_frame_data() {
            let skip = archive.read_u64();
            if skip > 0 {
                archive.skip_bytes(skip as usize);
            }
        }

        loop {
            if self.meta.has_level_streaming_fixes() {
                let _seen_level_index = archive.read_int_packed();
            }
            let buf_size = archive.read_i32();
            if archive.is_error || buf_size <= 0 || buf_size > 2048 {
                break;
            }
            let packet = archive.read_bytes(buf_size as usize);
            if archive.is_error {
                break;
            }
            self.received_raw_packet(packet, cb);
        }
    }

    fn read_external_data(&mut self, archive: &mut BinaryReader) {
        loop {
            let num_bits = archive.read_int_packed();
            if num_bits == 0 || archive.is_error {
                return;
            }
            let net_guid = archive.read_int_packed();
            let byte_len = ((num_bits as usize) + 7) >> 3;
            let bytes = archive.read_bytes(byte_len).to_vec();
            if archive.is_error {
                return;
            }
            self.guid_cache
                .external_data
                .insert(net_guid, ExternalData { net_guid, bytes });
        }
    }

    fn read_export_data<C: ReplayCallbacks>(&mut self, archive: &mut BinaryReader, cb: &mut C) {
        self.read_net_field_exports(archive);
        self.read_net_export_guids(archive, cb);
    }

    fn read_net_field_export(&self, archive: &mut BinaryReader) -> Option<NetFieldExport> {
        if !archive.read_bool_byte() {
            return None;
        }
        let handle = archive.read_int_packed();
        let compatible_checksum = archive.read_u32();
        let name = if self.meta.engine_network_version < engine_net_ver::HISTORY_NETEXPORT_SERIALIZATION {
            let n = archive.read_fstring();
            let _ty = archive.read_fstring();
            n
        } else if self.meta.engine_network_version < engine_net_ver::HISTORY_NETEXPORT_SERIALIZE_FIX {
            archive.read_fstring()
        } else {
            archive.read_fname()
        };
        if archive.is_error {
            return None;
        }
        Some(NetFieldExport {
            handle,
            compatible_checksum,
            name,
            incompatible: false,
        })
    }

    fn read_net_field_exports(&mut self, archive: &mut BinaryReader) {
        let num = archive.read_int_packed();
        for _ in 0..num {
            if archive.is_error {
                return;
            }
            let path_name_index = archive.read_int_packed();
            let is_exported = archive.read_int_packed() == 1;
            let group_key: Option<String> = if is_exported {
                let path_name = archive.read_fstring();
                let num_exports = archive.read_int_packed() as usize;
                if let Some(existing) = self.guid_cache.get_group_mut(&path_name) {
                    if num_exports > existing.len() {
                        existing.exports.resize(num_exports, None);
                    }
                } else {
                    let grp = NetFieldExportGroup::new(path_name.clone(), path_name_index, num_exports);
                    self.guid_cache.add_export_group(path_name.clone(), grp);
                }
                Some(path_name)
            } else {
                self.guid_cache.index_to_group_name.get(&path_name_index).cloned()
            };

            let net_field = self.read_net_field_export(archive);
            if let (Some(key), Some(nf)) = (group_key, net_field) {
                if let Some(group) = self.guid_cache.get_group_mut(&key) {
                    if group.is_valid_index(nf.handle) {
                        let h = nf.handle as usize;
                        group.exports[h] = Some(nf);
                    }
                }
            }
        }
    }

    fn read_net_export_guids<C: ReplayCallbacks>(&mut self, archive: &mut BinaryReader, cb: &mut C) {
        let num_guids = archive.read_int_packed();
        for _ in 0..num_guids {
            let size = archive.read_i32();
            if archive.is_error || size < 0 {
                return;
            }
            let bytes = archive.read_bytes(size as usize);
            if archive.is_error {
                return;
            }
            let mut br = BinaryReader::new(bytes, self.meta);
            self.internal_load_object_binary(&mut br, true, 0, cb);
        }
    }

    fn internal_load_object_binary<C: ReplayCallbacks>(
        &mut self,
        archive: &mut BinaryReader,
        is_exporting: bool,
        depth: usize,
        cb: &mut C,
    ) -> u32 {
        if depth > 16 || archive.is_error {
            return 0;
        }
        let guid = archive.read_int_packed();
        if guid == 0 {
            return 0;
        }
        if guid == 1 || is_exporting {
            let flags = archive.read_u8();
            if (flags & export_flags::HAS_PATH) != 0 {
                self.internal_load_object_binary(archive, true, depth + 1, cb);
                let path_name = archive.read_fstring();
                if (flags & export_flags::HAS_NETWORK_CHECKSUM) != 0 {
                    let _ = archive.read_u32();
                }
                if is_exporting && !archive.is_error {
                    let clean = super::guid_cache::remove_all_path_prefixes(&path_name).to_string();
                    self.guid_cache.net_guid_to_path.insert(guid, clean);
                    cb.on_net_guid_exported(self.current_time, guid, &path_name);
                }
                return guid;
            }
        }
        guid
    }

    fn internal_load_object_bit<C: ReplayCallbacks>(
        &mut self,
        archive: &mut BitReader,
        is_exporting: bool,
        depth: usize,
        cb: &mut C,
    ) -> u32 {
        if depth > 16 || archive.is_error {
            return 0;
        }
        let guid = archive.read_int_packed();
        if guid == 0 {
            return 0;
        }
        if guid == 1 || is_exporting {
            let flags = archive.read_u8();
            if (flags & export_flags::HAS_PATH) != 0 {
                self.internal_load_object_bit(archive, true, depth + 1, cb);
                let path_name = archive.read_fstring();
                if (flags & export_flags::HAS_NETWORK_CHECKSUM) != 0 {
                    let _ = archive.read_u32();
                }
                if is_exporting && !archive.is_error {
                    let clean = super::guid_cache::remove_all_path_prefixes(&path_name).to_string();
                    self.guid_cache.net_guid_to_path.insert(guid, clean);
                    cb.on_net_guid_exported(self.current_time, guid, &path_name);
                }
                return guid;
            }
        }
        guid
    }

    fn receive_net_field_exports_compat(&mut self, bits: &mut BitReader) {
        let num = bits.read_u32();
        for _ in 0..num {
            if bits.is_error {
                return;
            }
            let path_name_index = bits.read_int_packed();
            let is_exported = bits.read_int_packed() == 1;
            let group_key = if is_exported {
                let path_name = bits.read_fstring();
                let num_exports = bits.read_int_packed() as usize;
                if let Some(existing) = self.guid_cache.get_group_mut(&path_name) {
                    if num_exports > existing.len() {
                        existing.exports.resize(num_exports, None);
                    }
                } else {
                    let grp = NetFieldExportGroup::new(path_name.clone(), path_name_index, num_exports);
                    self.guid_cache.add_export_group(path_name.clone(), grp);
                }
                Some(path_name)
            } else {
                self.guid_cache.index_to_group_name.get(&path_name_index).cloned()
            };

            let is_field_exported = bits.read_u8() != 0;
            if is_field_exported {
                let handle = bits.read_int_packed();
                let compatible_checksum = bits.read_u32();
                let name = if self.meta.engine_network_version < engine_net_ver::HISTORY_NETEXPORT_SERIALIZATION {
                    let n = bits.read_fstring();
                    let _ty = bits.read_fstring();
                    n
                } else if self.meta.engine_network_version < engine_net_ver::HISTORY_NETEXPORT_SERIALIZE_FIX {
                    bits.read_fstring()
                } else {
                    bits.read_fname()
                };
                if let Some(key) = group_key {
                    if let Some(group) = self.guid_cache.get_group_mut(&key) {
                        if group.is_valid_index(handle) {
                            group.exports[handle as usize] = Some(NetFieldExport {
                                handle,
                                compatible_checksum,
                                name,
                                incompatible: false,
                            });
                        }
                    }
                }
            }
        }
    }

    fn receive_net_guid_bunch<C: ReplayCallbacks>(&mut self, bits: &mut BitReader, cb: &mut C) {
        if bits.read_bit() {
            self.receive_net_field_exports_compat(bits);
            return;
        }
        let num_guids = bits.read_i32();
        if num_guids < 0 || num_guids > MAX_GUID_COUNT {
            return;
        }
        for _ in 0..num_guids {
            if bits.is_error {
                break;
            }
            self.internal_load_object_bit(bits, true, 0, cb);
        }
    }

    fn received_raw_packet<C: ReplayCallbacks>(&mut self, packet: &[u8], cb: &mut C) {
        let Some(&last_byte) = packet.last() else {
            return;
        };
        if last_byte == 0 {
            return;
        }
        let leading = last_byte.leading_zeros() as usize;
        let bit_size = packet.len() * 8 - 1 - leading;
        let mut bit_reader = BitReader::from_bytes(packet, bit_size, self.meta);
        self.received_packet(&mut bit_reader, cb);
    }

    fn received_packet<C: ReplayCallbacks>(&mut self, bit_reader: &mut BitReader, cb: &mut C) {
        self.in_packet_id += 1;
        let has_custom_exports_bit =
            self.meta.engine_network_version >= engine_net_ver::CUSTOM_EXPORTS;

        while !bit_reader.at_end() {
            if self.meta.engine_network_version < engine_net_ver::HISTORY_ACKS_INCLUDED_IN_HEADER {
                let _ = bit_reader.read_bit();
            }
            let b_control = bit_reader.read_bit();
            let b_open = b_control && bit_reader.read_bit();
            let b_close = b_control && bit_reader.read_bit();

            let (close_reason, b_dormant) =
                if self.meta.engine_network_version < engine_net_ver::HISTORY_CHANNEL_CLOSE_REASON {
                    let dorm = b_close && bit_reader.read_bit();
                    (if dorm { 1 } else { 0 }, dorm)
                } else {
                    let reason = if b_close {
                        bit_reader.read_serialized_int(15)
                    } else {
                        0
                    };
                    (reason, reason == 1)
                };

            let b_is_replication_paused = bit_reader.read_bit();
            let b_reliable = bit_reader.read_bit();

            let ch_index = if self.meta.engine_network_version
                < engine_net_ver::HISTORY_MAX_ACTOR_CHANNELS_CUSTOMIZATION
            {
                bit_reader.read_serialized_int(OLD_MAX_ACTOR_CHANNELS)
            } else {
                bit_reader.read_int_packed()
            };

            let b_has_package_map_exports = bit_reader.read_bit();
            let b_has_must_be_mapped_guids = bit_reader.read_bit();
            let b_partial = bit_reader.read_bit();

            let ch_sequence = if b_reliable {
                self.in_reliable + 1
            } else if b_partial {
                self.in_packet_id
            } else {
                0
            };

            let b_partial_initial = b_partial && bit_reader.read_bit();
            if b_partial && has_custom_exports_bit {
                let _ = bit_reader.read_bit();
            }
            let b_partial_final = b_partial && bit_reader.read_bit();

            if self.meta.engine_network_version < engine_net_ver::HISTORY_CHANNEL_NAMES {
                let _type = bit_reader.read_serialized_int(8);
            } else if b_reliable || b_open {
                let _name = bit_reader.read_fname();
            }

            let bunch_data_bits = bit_reader.read_serialized_int(MAX_PACKET_SIZE_IN_BITS) as usize;
            if bit_reader.is_error || !bit_reader.can_read(bunch_data_bits) {
                break;
            }

            let mut bunch = DataBunch {
                packet_id: self.in_packet_id,
                ch_index,
                ch_sequence,
                b_open,
                b_close,
                b_dormant,
                b_is_replication_paused,
                b_reliable,
                b_has_package_map_exports,
                b_has_must_be_mapped_guids,
                b_partial,
                b_partial_initial,
                b_partial_final,
                close_reason,
                archive: None,
            };

            if b_partial {
                let raw = bit_reader.read_bits(bunch_data_bits);
                let mut b_arch = BitReader::from_bytes(&raw, bunch_data_bits, self.meta);
                if bunch.b_has_package_map_exports {
                    self.receive_net_guid_bunch(&mut b_arch, cb);
                }
                bunch.archive = Some(b_arch);
                self.channels.entry(ch_index).or_insert(Channel {
                    index: ch_index,
                    actor: None,
                });
                self.received_next_bunch(bunch, cb);
            } else {
                bit_reader.set_temp_end(bunch_data_bits);
                if bunch.b_has_package_map_exports {
                    self.receive_net_guid_bunch(bit_reader, cb);
                }
                self.channels.entry(ch_index).or_insert(Channel {
                    index: ch_index,
                    actor: None,
                });
                if bunch.b_reliable {
                    self.in_reliable = bunch.ch_sequence;
                }
                self.received_sequenced_bunch(&bunch, bit_reader, cb);
                bit_reader.restore_temp_end();
            }
        }
    }

    fn received_next_bunch<C: ReplayCallbacks>(&mut self, mut bunch: DataBunch, cb: &mut C) {
        if bunch.b_reliable {
            self.in_reliable = bunch.ch_sequence;
        }
        if bunch.b_partial {
            if bunch.b_partial_initial {
                if let Some(ref pb) = self.partial_bunch {
                    if !pb.b_partial_final && pb.b_reliable {
                        return;
                    }
                }
                let bits_left = bunch.archive.as_ref().map(|a| a.bits_left()).unwrap_or(0);
                if !bunch.b_has_package_map_exports && bits_left > 0 && (bits_left % 8) != 0 {
                    return;
                }
                self.partial_bunch = Some(bunch);
                return;
            } else {
                let matches = self.partial_bunch.as_ref().is_some_and(|pb| {
                    let rel_match = bunch.ch_sequence == pb.ch_sequence + 1;
                    let unrel_match = rel_match || bunch.ch_sequence == pb.ch_sequence;
                    let seq_match = if pb.b_reliable { rel_match } else { unrel_match };
                    !pb.b_partial_final && seq_match && pb.b_reliable == bunch.b_reliable
                });
                if !matches {
                    return;
                }
                let mut bits_left = 0usize;
                if let Some(ref mut arch) = bunch.archive {
                    bits_left = arch.bits_left();
                    if !bunch.b_has_package_map_exports && bits_left > 0 {
                        let extra = arch.read_bits(bits_left);
                        if let Some(ref mut pb) = self.partial_bunch {
                            if let Some(ref mut pb_arch) = pb.archive {
                                pb_arch.append_bits(&extra, bits_left);
                            }
                        }
                    }
                }
                if !bunch.b_has_package_map_exports && !bunch.b_partial_final && (bits_left % 8) != 0 {
                    return;
                }
                if let Some(ref mut pb) = self.partial_bunch {
                    pb.ch_sequence = bunch.ch_sequence;
                    if bunch.b_partial_final {
                        if bunch.b_has_package_map_exports {
                            return;
                        }
                        pb.b_partial_final = true;
                        pb.b_close = bunch.b_close;
                        pb.b_dormant = bunch.b_dormant;
                        pb.close_reason = bunch.close_reason;
                        pb.b_is_replication_paused = bunch.b_is_replication_paused;
                        pb.b_has_must_be_mapped_guids = bunch.b_has_must_be_mapped_guids;
                        let mut complete = self.partial_bunch.take().unwrap();
                        if let Some(mut arch) = complete.archive.take() {
                            self.received_sequenced_bunch(&complete, &mut arch, cb);
                        }
                    }
                }
                return;
            }
        }

        if let Some(mut arch) = bunch.archive.take() {
            self.received_sequenced_bunch(&bunch, &mut arch, cb);
        }
    }

    fn received_sequenced_bunch<C: ReplayCallbacks>(
        &mut self,
        bunch: &DataBunch,
        archive: &mut BitReader,
        cb: &mut C,
    ) {
        if bunch.b_has_must_be_mapped_guids {
            let n = archive.read_u16();
            for _ in 0..n {
                let _ = archive.read_int_packed();
            }
        }
        self.process_bunch(bunch, archive, cb);
        if bunch.b_close {
            self.channels.remove(&bunch.ch_index);
            self.channel_to_actor.remove(&bunch.ch_index);
        }
    }

    fn conditionally_serialize_quantized_vector(
        &self,
        archive: &mut BitReader,
        default_vec: FVector,
    ) -> FVector {
        if archive.read_bit() {
            let quantize = self.meta.engine_network_version
                < engine_net_ver::HISTORY_OPTIONALLY_QUANTIZE_SPAWN_INFO
                || archive.read_bit();
            if quantize {
                archive.read_packed_vector(10, 24)
            } else {
                archive.read_fvector()
            }
        } else {
            default_vec
        }
    }

    fn process_bunch<C: ReplayCallbacks>(
        &mut self,
        bunch: &DataBunch,
        archive: &mut BitReader,
        cb: &mut C,
    ) {
        let needs_actor = self
            .channels
            .get(&bunch.ch_index)
            .is_some_and(|c| c.actor.is_none());

        if needs_actor {
            if !bunch.b_open {
                return;
            }
            let actor_guid = self.internal_load_object_bit(archive, false, 0, cb);
            let is_dynamic = actor_guid > 0 && (actor_guid & 1) == 0;
            if archive.at_end() && is_dynamic {
                return;
            }
            let mut archetype_guid = None;
            let mut location = None;
            if is_dynamic {
                let arch = self.internal_load_object_bit(archive, false, 0, cb);
                archetype_guid = Some(arch);
                if self.meta.engine_network_version
                    >= engine_net_ver::HISTORY_NEW_ACTOR_OVERRIDE_LEVEL
                {
                    let _level = self.internal_load_object_bit(archive, false, 0, cb);
                }
                location = Some(self.conditionally_serialize_quantized_vector(
                    archive,
                    FVector::new(0.0, 0.0, 0.0),
                ));
                if archive.read_bit() {
                    let _rot = archive.read_rotation_short();
                }
                let _scale = self.conditionally_serialize_quantized_vector(
                    archive,
                    FVector::new(1.0, 1.0, 1.0),
                );
                let _vel = self.conditionally_serialize_quantized_vector(
                    archive,
                    FVector::new(0.0, 0.0, 0.0),
                );
            }

            let actor = ActorInfo {
                actor_guid,
                archetype_guid,
                location,
            };
            self.channel_to_actor.insert(bunch.ch_index, actor_guid);
            if let Some(ch) = self.channels.get_mut(&bunch.ch_index) {
                ch.actor = Some(actor);
            }
            if let Some(ch) = self.channels.get(&bunch.ch_index) {
                cb.on_channel_opened(self.current_time, ch, &self.guid_cache);
            }
            if let Some(arch) = archetype_guid {
                if let Some(path) = self.guid_cache.try_get_path_name(arch) {
                    if is_player_controller_path(path) {
                        let _net_player_index = archive.read_u8();
                    }
                }
            }
        }

        while !archive.at_end() {
            let (rep_obj, obj_deleted, has_rep_layout, payload) =
                self.read_content_block_payload(bunch.ch_index, archive, cb);
            // ChestCounter patch: abort on ReadContentBlockPayload error to prevent infinite loop.
            if archive.is_error {
                break;
            }
            let Some(payload_bits) = payload else {
                continue;
            };
            archive.set_temp_end(payload_bits as usize);
            if !obj_deleted && !archive.is_error {
                if let Some(obj) = rep_obj {
                    if !archive.at_end() {
                        self.received_replicator_bunch(bunch.ch_index, archive, obj, has_rep_layout, cb);
                    }
                }
            }
            archive.restore_temp_end();
        }
    }

    fn read_content_block_payload<C: ReplayCallbacks>(
        &mut self,
        ch_index: u32,
        archive: &mut BitReader,
        cb: &mut C,
    ) -> (Option<u32>, bool, bool, Option<u32>) {
        let has_rep_layout = archive.read_bit();
        let is_actor = archive.read_bit();
        let rep_obj = if is_actor {
            let ch = self.channels.get(&ch_index);
            ch.and_then(|c| c.archetype_id().or_else(|| c.actor_id()))
        } else {
            let net_guid = self.internal_load_object_bit(archive, false, 0, cb);
            let stably_named = archive.read_bit();
            if stably_named {
                Some(net_guid)
            } else {
                let mut delete_subobject = false;
                let mut serialize_class = true;
                if self.meta.engine_network_version
                    >= engine_net_ver::HISTORY_SUBOBJECT_DESTROY_FLAG
                {
                    if archive.read_bit() {
                        delete_subobject = true;
                        serialize_class = false;
                        let _destroy_flags = archive.read_u8();
                    }
                }
                let mut class_guid = 0u32;
                if serialize_class {
                    class_guid = self.internal_load_object_bit(archive, false, 0, cb);
                    delete_subobject = class_guid == 0;
                }
                if delete_subobject {
                    return (None, true, has_rep_layout, None);
                }
                if self.meta.engine_network_version
                    >= engine_net_ver::HISTORY_SUBOBJECT_OUTER_CHAIN
                {
                    let actor_is_outer = archive.at_end() || archive.read_bit();
                    if !actor_is_outer {
                        let _ = self.internal_load_object_bit(archive, false, 0, cb);
                    }
                }
                Some(class_guid)
            }
        };

        let payload = archive.read_int_packed();
        (rep_obj, false, has_rep_layout, Some(payload))
    }

    fn received_replicator_bunch<C: ReplayCallbacks>(
        &mut self,
        ch_index: u32,
        archive: &mut BitReader,
        rep_object: u32,
        has_rep_layout: bool,
        cb: &mut C,
    ) {
        let Some(group_path) = self.guid_cache.resolve_group_name(rep_object) else {
            return;
        };
        let group_kind = classify_group(&group_path);
        let probe_group = group_path.contains("FortAbilitySystemComponent");

        if group_kind.is_none() && probe_group {
            self.probe_replicator_bunch(ch_index, archive, &group_path, has_rep_layout, cb);
            return;
        }

        if has_rep_layout {
            if let Some(kind) = group_kind {
                if !self.receive_properties(archive, &group_path, kind, ch_index, true, cb) {
                    return;
                }
                self.receive_external_data(ch_index);
            } else {
                // Group not needed, skip RepLayout properties to reach ClassNetCache if needed
                let _checksum = archive.read_bit();
                loop {
                    let handle = archive.read_int_packed();
                    if handle == 0 || archive.is_error {
                        break;
                    }
                    let num_bits = archive.read_int_packed() as usize;
                    if archive.is_error || num_bits > archive.bits_left() {
                        return;
                    }
                    archive.skip_bits(num_bits);
                }
            }
        }

        if archive.at_end() {
            return;
        }

        let use_full =
            self.meta.engine_network_version >= engine_net_ver::HISTORY_CLASSNETCACHE_FULLNAME;
        let Some(cnc) = self
            .guid_cache
            .try_get_class_net_cache(&group_path, use_full)
            .cloned()
        else {
            return;
        };

        let is_game_state_cnc = cnc.path_name == "Athena_GameState_C_ClassNetCache"
            || cnc.path_name.ends_with(".Athena_GameState_C_ClassNetCache");
        let is_loot_cannon_cnc = cnc.path_name.to_ascii_lowercase().contains("lootcannon")
            && cnc.path_name.ends_with("_ClassNetCache");
        let is_ability_cnc = cnc.path_name.contains("AbilitySystemComponent")
            && cnc.path_name.ends_with("_ClassNetCache");

        while let Some((field_opt, payload)) = self.read_field_header_and_payload(archive, &cnc) {
            archive.set_temp_end(payload as usize);
            if !archive.is_error && !archive.at_end() {
                match field_opt {
                    None => {
                        if is_ability_cnc {
                            let bits = archive.bits_left();
                            if bits > 0 {
                                let raw = archive.read_bits(bits);
                                let actor_id = self
                                    .channels
                                    .get(&ch_index)
                                    .and_then(|c| c.actor_id())
                                    .unwrap_or(0);
                                cb.on_raw_probe(
                                    self.current_time,
                                    ch_index,
                                    actor_id,
                                    &cnc.path_name,
                                    "?",
                                    &raw,
                                    bits,
                                );
                            }
                        }
                    }
                    Some(field) => {
                        if is_game_state_cnc && field.name == "CurrentPlaylistInfo" {
                            if self.meta.engine_network_version
                                >= engine_net_ver::HISTORY_FAST_ARRAY_DELTA_STRUCT
                            {
                                let _ = archive.read_bit();
                            }
                            let _ = archive.read_bit();
                            let id = archive.read_int_packed();
                            archive.skip_bits(31);
                            let name = self.guid_cache.try_get_path_name(id).map(|s| s.to_string());
                            if self.replay.game_data.current_playlist.is_none() {
                                self.replay.game_data.current_playlist = name.clone();
                            }
                            let exp = ParsedExport::PlaylistInfo { id, name };
                            let ch = self.channels.get(&ch_index);
                            cb.on_export_read(self.current_time, ch_index, ch, &exp, &self.guid_cache);
                        } else if is_loot_cannon_cnc && field.name == "FireLootCannon" {
                            let rpc_path = "/LootCannon/Gameplay/BGA_LootCannon.BGA_LootCannon_C:FireLootCannon";
                            let _ = self.receive_properties(
                                archive,
                                rpc_path,
                                GroupKind::LootCannonFired,
                                ch_index,
                                true,
                                cb,
                            );
                        } else if is_ability_cnc && field.name == "ActivatableAbilities" {
                            let bits = archive.bits_left();
                            if bits > 0 {
                                let raw = archive.read_bits(bits);
                                let actor_id = self
                                    .channels
                                    .get(&ch_index)
                                    .and_then(|c| c.actor_id())
                                    .unwrap_or(0);
                                cb.on_raw_probe(
                                    self.current_time,
                                    ch_index,
                                    actor_id,
                                    &cnc.path_name,
                                    &field.name,
                                    &raw,
                                    bits,
                                );
                            }
                        }
                    }
                }
            }
            archive.restore_temp_end();
        }
    }

    fn probe_replicator_bunch<C: ReplayCallbacks>(
        &mut self,
        ch_index: u32,
        archive: &mut BitReader,
        group_path: &str,
        has_rep_layout: bool,
        cb: &mut C,
    ) {
        if has_rep_layout {
            let _ = archive.read_bit();
            loop {
                let handle = archive.read_int_packed();
                if handle == 0 || archive.is_error {
                    break;
                }
                let num_bits = archive.read_int_packed() as usize;
                if archive.is_error || num_bits > archive.bits_left() {
                    return;
                }
                if num_bits > 0 {
                    archive.skip_bits(num_bits);
                }
            }
        }
        if archive.at_end() {
            return;
        }
        let use_full =
            self.meta.engine_network_version >= engine_net_ver::HISTORY_CLASSNETCACHE_FULLNAME;
        let Some(cnc) = self
            .guid_cache
            .try_get_class_net_cache(group_path, use_full)
            .cloned()
        else {
            return;
        };
        while let Some((field_opt, payload)) = self.read_field_header_and_payload(archive, &cnc) {
            archive.set_temp_end(payload as usize);
            let bits = archive.bits_left();
            if bits > 0 && !archive.is_error {
                let raw = archive.read_bits(bits);
                let fname = field_opt.as_ref().map(|f| f.name.as_str()).unwrap_or("?");
                let actor_id = self
                    .channels
                    .get(&ch_index)
                    .and_then(|c| c.actor_id())
                    .unwrap_or(0);
                cb.on_raw_probe(
                    self.current_time,
                    ch_index,
                    actor_id,
                    &cnc.path_name,
                    fname,
                    &raw,
                    bits,
                );
            }
            archive.restore_temp_end();
        }
    }

    fn read_field_header_and_payload(
        &self,
        archive: &mut BitReader,
        group: &NetFieldExportGroup,
    ) -> Option<(Option<NetFieldExport>, u32)> {
        if archive.at_end() {
            return None;
        }
        let max_val = (group.len() as u32).max(2);
        let handle = archive.read_serialized_int(max_val) as usize;
        if archive.is_error {
            return None;
        }
        let field = group.exports.get(handle).and_then(|o| o.clone());
        let payload = archive.read_int_packed();
        if archive.is_error || !archive.can_read(payload as usize) {
            return None;
        }
        Some((field, payload))
    }

    fn receive_external_data(&mut self, ch_index: u32) {
        let Some(actor_id) = self.channels.get(&ch_index).and_then(|c| c.actor_id()) else {
            return;
        };
        if let Some(ext) = self.guid_cache.take_external_data(actor_id) {
            if let Some(name) = decode_player_name_data(&ext.bytes, self.meta) {
                if let Some(pd) = self.players_by_channel.get_mut(&ch_index) {
                    pd.player_name = Some(name);
                }
            }
        }
    }

    fn receive_properties<C: ReplayCallbacks>(
        &mut self,
        archive: &mut BitReader,
        group_path: &str,
        kind: GroupKind,
        ch_index: u32,
        enable_checksum: bool,
        cb: &mut C,
    ) -> bool {
        if enable_checksum {
            let _do_checksum = archive.read_bit();
        }
        let Some(group) = self.guid_cache.get_group(group_path).cloned() else {
            return false;
        };

        // Accumulators per kind
        let mut gs_session: Option<String> = None;
        let mut gs_utc: Option<DateTime<Utc>> = None;
        let mut gs_custom: Option<bool> = None;
        let mut gs_recorder: Option<u32> = None;

        let mut ps_player_id: Option<i32> = None;
        let mut ps_bot: Option<bool> = None;
        let mut ps_spectator: Option<bool> = None;
        let mut ps_team: Option<i32> = None;
        let mut ps_place: Option<i32> = None;
        let mut ps_unique_id: Option<String> = None;

        let mut pawn_ps: Option<u32> = None;
        let mut pawn_move: Option<FRepMovement> = None;

        let mut cont_searcher: Option<u32> = None;
        let mut cont_searched: Option<bool> = None;
        let mut cont_destroyed = false;

        let mut veh_move: Option<FRepMovement> = None;
        let mut seat_slots: Option<Vec<Option<u32>>> = None;
        let mut cannon_pawn: Option<u32> = None;

        let mut th_instigator: Option<u32> = None;
        let mut th_source_container: Option<u32> = None;
        let mut th_opening_pawn: Option<u32> = None;
        let mut th_success: Option<bool> = None;
        let mut th_prize_bytes: Option<Vec<u8>> = None;

        let mut weap_instigator: Option<u32> = None;
        let mut weap_owner: Option<u32> = None;
        let mut weap_data_name: Option<String> = None;

        let mut pick_tossed: Option<bool> = None;
        let mut pick_item_guid: Option<u32> = None;
        let mut pick_item_name: Option<String> = None;
        let mut pick_owner: Option<u32> = None;

        let mut has_data = false;

        loop {
            let handle = archive.read_int_packed();
            if handle == 0 || archive.is_error {
                break;
            }
            let handle = handle - 1;
            let num_bits = archive.read_int_packed() as usize;
            if archive.is_error || num_bits > archive.bits_left() {
                return false;
            }
            if num_bits == 0 {
                continue;
            }

            let field_name = group
                .exports
                .get(handle as usize)
                .and_then(|o| o.as_ref())
                .map(|f| f.name.as_str());

            let Some(fname) = field_name else {
                archive.skip_bits(num_bits);
                continue;
            };

            archive.set_temp_end(num_bits);
            has_data = true;

            match (kind, fname) {
                (GroupKind::GameState, "GameSessionId") => {
                    gs_session = Some(archive.read_fstring());
                }
                (GroupKind::GameState, "UtcTimeStartedMatch") => {
                    gs_utc = dotnet_ticks_to_utc(archive.read_i64());
                }
                (GroupKind::GameState, "bIsCustomMatch") => {
                    gs_custom = Some(archive.read_bit());
                }
                (GroupKind::GameState, "RecorderPlayerState") => {
                    gs_recorder = Some(archive.read_int_packed());
                }

                (GroupKind::FortPlayerState, "PlayerID") => {
                    if ps_player_id.is_none() {
                        ps_player_id = Some(archive.read_i32());
                    }
                }
                (GroupKind::FortPlayerState, "PlayerId") => {
                    ps_player_id = Some(archive.read_u32() as i32);
                }
                (GroupKind::FortPlayerState, "bIsABot") => {
                    ps_bot = Some(archive.read_bit());
                }
                (GroupKind::FortPlayerState, "bOnlySpectator") => {
                    ps_spectator = Some(archive.read_bit());
                }
                (GroupKind::FortPlayerState, "TeamIndex") => {
                    let bl = archive.bits_left();
                    ps_team = Some(archive.read_bits_to_u32(bl) as i32);
                }
                (GroupKind::FortPlayerState, "Place") => {
                    ps_place = Some(archive.read_i32());
                }
                (GroupKind::FortPlayerState, "UniqueId" | "UniqueID") => {
                    let uid = archive.serialize_property_net_id();
                    if !uid.is_empty() {
                        ps_unique_id = Some(uid);
                    }
                }

                (GroupKind::PlayerPawn, "PlayerState") => {
                    pawn_ps = Some(archive.read_int_packed());
                }
                (GroupKind::PlayerPawn, "ReplicatedMovement") => {
                    pawn_move = Some(archive.serialize_rep_movement(
                        VectorQuantization::RoundWholeNumber,
                        RotatorQuantization::ByteComponents,
                        VectorQuantization::RoundWholeNumber,
                    ));
                }

                (GroupKind::Container, "SearchingPawn") => {
                    cont_searcher = Some(archive.read_int_packed());
                }
                (GroupKind::Container, "bAlreadySearched") => {
                    cont_searched = Some(archive.read_bit());
                }
                (GroupKind::Container, "bDestroyed") => {
                    cont_destroyed = archive.read_bit();
                }

                (GroupKind::Vehicle, "ReplicatedMovement") => {
                    veh_move = Some(archive.serialize_rep_movement(
                        VectorQuantization::RoundTwoDecimals,
                        RotatorQuantization::ShortComponents,
                        VectorQuantization::RoundTwoDecimals,
                    ));
                }

                (GroupKind::SeatComponent, "PlayerSlots") => {
                    seat_slots = Some(read_seat_slots_array(&group, archive));
                }

                (GroupKind::LootCannonFired, "InteractingPawn") => {
                    cannon_pawn = Some(archive.read_int_packed());
                }

                (GroupKind::TreasureHuntChest, "Instigator") => {
                    th_instigator = Some(archive.read_int_packed());
                }
                (GroupKind::TreasureHuntChest, "SourceContainer") => {
                    th_source_container = Some(archive.read_int_packed());
                }
                (GroupKind::TreasureHuntChest, "OpeningPawn") => {
                    th_opening_pawn = Some(archive.read_int_packed());
                }
                (GroupKind::TreasureHuntChest, "bSuccess") => {
                    th_success = Some(archive.read_bit());
                }
                (GroupKind::TreasureHuntChest, "PrizeTemplateId") => {
                    let bl = archive.bits_left();
                    th_prize_bytes = Some(archive.read_bits(bl));
                }

                (GroupKind::Weapon, "Instigator") => {
                    weap_instigator = Some(archive.read_int_packed());
                }
                (GroupKind::Weapon, "Owner") => {
                    weap_owner = Some(archive.read_int_packed());
                }
                (GroupKind::Weapon, "WeaponData") => {
                    let guid = archive.read_int_packed();
                    weap_data_name = self.guid_cache.try_get_path_name(guid).map(|s| s.to_string());
                }

                (GroupKind::FortPickup, "bTossedFromContainer") => {
                    pick_tossed = Some(archive.read_bit());
                }
                (GroupKind::FortPickup, "ItemDefinition") => {
                    let guid = archive.read_int_packed();
                    pick_item_guid = Some(guid);
                    pick_item_name = self.guid_cache.try_get_path_name(guid).map(|s| s.to_string());
                }
                (GroupKind::FortPickup, "Owner") => {
                    pick_owner = Some(archive.read_int_packed());
                }

                _ => {}
            }

            archive.restore_temp_end();
        }

        if !has_data {
            return true;
        }

        let export = match kind {
            GroupKind::GameState => {
                if self.replay.game_data.game_session_id.is_none() {
                    self.replay.game_data.game_session_id = gs_session.clone();
                }
                if self.replay.game_data.utc_time_started_match.is_none() {
                    self.replay.game_data.utc_time_started_match = gs_utc;
                }
                if self.replay.game_data.is_custom_match.is_none() {
                    self.replay.game_data.is_custom_match = gs_custom;
                }
                if self.replay.game_data.recorder_id.is_none() {
                    self.replay.game_data.recorder_id = gs_recorder;
                }
                ParsedExport::GameState {
                    game_session_id: gs_session,
                    utc_time_started_match: gs_utc,
                    b_is_custom_match: gs_custom,
                    recorder_player_state: gs_recorder,
                }
            }
            GroupKind::FortPlayerState => {
                self.update_player_state(
                    ch_index,
                    ps_player_id,
                    ps_bot,
                    ps_spectator,
                    ps_team,
                    ps_place,
                    ps_unique_id.clone(),
                );
                ParsedExport::FortPlayerState {
                    player_id: ps_player_id,
                    b_is_bot: ps_bot,
                    b_only_spectator: ps_spectator,
                    team_index: ps_team,
                    place: ps_place,
                    unique_id: ps_unique_id,
                }
            }
            GroupKind::PlayerPawn => ParsedExport::PlayerPawn {
                player_state: pawn_ps,
                replicated_movement: pawn_move,
            },
            GroupKind::Container => ParsedExport::Container {
                searching_pawn: cont_searcher,
                b_already_searched: cont_searched,
                b_destroyed: cont_destroyed,
            },
            GroupKind::Vehicle => ParsedExport::Vehicle {
                replicated_movement: veh_move,
            },
            GroupKind::SeatComponent => ParsedExport::SeatComponent {
                player_slots: seat_slots,
            },
            GroupKind::LootCannonFired => ParsedExport::LootCannonFired {
                interacting_pawn: cannon_pawn,
            },
            GroupKind::TreasureHuntChest => ParsedExport::TreasureHuntChest {
                instigator: th_instigator,
                source_container: th_source_container,
                opening_pawn: th_opening_pawn,
                b_success: th_success,
                prize_template_id_bytes: th_prize_bytes,
            },
            GroupKind::Weapon => ParsedExport::Weapon {
                instigator: weap_instigator,
                owner: weap_owner,
                weapon_data_name: weap_data_name,
            },
            GroupKind::FortPickup => ParsedExport::FortPickup {
                b_tossed_from_container: pick_tossed,
                item_definition_guid: pick_item_guid,
                item_definition_name: pick_item_name,
                owner: pick_owner,
            },
        };

        let ch = self.channels.get(&ch_index);
        cb.on_export_read(self.current_time, ch_index, ch, &export, &self.guid_cache);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn update_player_state(
        &mut self,
        ch_index: u32,
        player_id: Option<i32>,
        b_is_bot: Option<bool>,
        b_only_spectator: Option<bool>,
        team_index: Option<i32>,
        place: Option<i32>,
        unique_id: Option<String>,
    ) {
        if b_only_spectator == Some(true) {
            self.only_spectating.insert(ch_index);
            return;
        }
        if self.only_spectating.contains(&ch_index) {
            return;
        }
        let recorder_id = self.replay.game_data.recorder_id;
        let actor_id = self.channel_to_actor.get(&ch_index).copied();

        let entry = self.players_by_channel.entry(ch_index).or_insert_with(|| {
            let mut pd = PlayerData {
                id: player_id,
                epic_id: unique_id.clone(),
                is_bot: b_is_bot == Some(true),
                player_name: None,
                team_index,
                placement: place,
                is_replay_owner: false,
            };
            if let (Some(a), Some(rec)) = (actor_id, recorder_id) {
                if a == rec {
                    pd.is_replay_owner = true;
                }
            }
            pd
        });

        if entry.id.is_none() && player_id.is_some() {
            entry.id = player_id;
        }
        if entry.epic_id.is_none() && unique_id.is_some() {
            entry.epic_id = unique_id;
        }
        if b_is_bot == Some(true) {
            entry.is_bot = true;
        }
        if let Some(ti) = team_index {
            if ti > 0 {
                entry.team_index = Some(ti);
            }
        }
        if entry.placement.is_none() && place.is_some() {
            entry.placement = place;
        }
    }
}

/// Decodes `SeatComponent.PlayerSlots` dynamic array (struct fields flattened into `group`).
fn read_seat_slots_array(group: &NetFieldExportGroup, archive: &mut BitReader) -> Vec<Option<u32>> {
    let array_len = archive.read_int_packed() as usize;
    if array_len > 64 || archive.is_error {
        return Vec::new();
    }
    let mut slots = vec![None; array_len];
    loop {
        let index = archive.read_int_packed();
        if index == 0 || archive.is_error {
            if archive.bits_left() == 8 {
                let _term = archive.read_int_packed();
            }
            return slots;
        }
        let idx = (index - 1) as usize;
        if idx >= array_len {
            return slots;
        }
        let mut player_val: Option<u32> = None;
        loop {
            let handle = archive.read_int_packed();
            if handle == 0 || archive.is_error {
                break;
            }
            let handle = (handle - 1) as usize;
            if handle >= group.len() {
                return slots;
            }
            let num_bits = archive.read_int_packed() as usize;
            if num_bits == 0 {
                continue;
            }
            if num_bits > archive.bits_left() {
                return slots;
            }
            let field_name = group.exports[handle].as_ref().map(|f| f.name.as_str());
            archive.set_temp_end(num_bits);
            if field_name.is_some_and(|n| n.eq_ignore_ascii_case("player")) {
                player_val = Some(archive.read_int_packed());
            }
            archive.restore_temp_end();
        }
        slots[idx] = player_val;
    }
}
