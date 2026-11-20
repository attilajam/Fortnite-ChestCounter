use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use chrono::{Local, Utc};
use regex::Regex;

use crate::catalog::TreasureCatalog;
use crate::lang::LangCtx;
use crate::models::{
    ChestRecord, MatchRecord, SeenPrize, TeamChestCount, TreasureFind, chest_kinds,
    treasure_friendly, UNKNOWN_FR, UNKNOWN_PREFIX,
};

use super::archive::FVector;
use super::guid_cache::NetGuidCache;
use super::reader::{Channel, ParsedExport, ReplayCallbacks, ReplayParser};

/// Bump when the analysis changes so that existing replays are re-analyzed.
pub const CURRENT_VERSION: i32 = 17;

fn prize_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[A-Za-z][A-Za-z0-9_]*:[A-Za-z0-9_.\-]+|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}").unwrap()
    })
}

fn uaid_suffix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"_UAID_.*$").unwrap())
}

fn treasure_item_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)RelicExtractor|RelicFinder|LuckyLocator|FlameCeleb|SmugglerExtractionKey|ExtractionAccelerat|Treasure|PirateShop|DelilahShop").unwrap()
    })
}

fn event_item_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)RelicExtractor_Consumable").unwrap())
}

fn tracked_export_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"TreasureHandler|WID_(FlameCeleb|RelicExtractor|RelicFinder|LuckyLocator|SmugglerExtractionKey)").unwrap()
    })
}

#[derive(Debug, Clone, Copy)]
struct Pt {
    t: f32,
    x: f32,
    y: f32,
    z: f32,
}

impl Pt {
    fn dist(self, other: Pt) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

#[derive(Debug, Clone)]
struct SearchEvent {
    time: f32,
    actor_key: String,
    label: String,
    searcher_pawn: Option<u32>,
    destroyed: bool,
    stale: bool,
}

#[derive(Debug, Clone)]
struct TossEvent {
    time: f32,
    item: String,
    loc: Option<Pt>,
    owner: Option<u32>,
}

#[derive(Debug, Clone)]
struct SeatSpan {
    pawn: u32,
    vehicle: u32,
    from: f32,
    to: f32,
}

#[derive(Debug, Clone)]
struct OrphanDestroy {
    time: f32,
    actor_key: String,
    label: String,
    actor: u32,
}

#[derive(Debug, Clone)]
struct EquipEvent {
    time: f32,
    item: String,
    pawn: u32,
}

#[derive(Debug, Clone)]
struct PrizeEvent {
    time: f32,
    prize_id: String,
    pawn: Option<u32>,
    container: String,
}

#[derive(Debug, Clone)]
struct GrantEvent {
    time: f32,
    player_state_actor: u32,
    element_id: i32,
    #[allow(dead_code)]
    handler: String,
}

fn clean_name(path: &str) -> String {
    let mut name = path.rsplit('/').next().unwrap_or(path);
    if let Some(idx) = name.rfind('.') {
        name = &name[idx + 1..];
    }
    uaid_suffix_re().replace(name, "").into_owned()
}

fn stable_key(key: &str) -> bool {
    !key.starts_with("guid:") && !key.starts_with("ch:")
}

fn is_dynamic_chest(label: &str) -> bool {
    !label.to_ascii_lowercase().starts_with("prop_")
        && chest_kinds::is_chest(chest_kinds::classify(label))
}

fn read_prize_id(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let ascii: String = bytes
        .iter()
        .map(|&b| if (32..127).contains(&b) { b as char } else { ' ' })
        .collect();
    prize_id_re().find(&ascii).map(|m| m.as_str().to_string())
}

fn round_1_f32(v: f32) -> f32 {
    (v as f64 * 10.0).round_ties_even() as f32 / 10.0
}

fn track_pt(tracks: &mut HashMap<u32, Vec<Pt>>, id: u32, loc: FVector, now: f32, min_interval: f32) {
    let track = tracks.entry(id).or_default();
    if track.last().is_none_or(|last| now - last.t >= min_interval) {
        track.push(Pt {
            t: now,
            x: loc.x as f32,
            y: loc.y as f32,
            z: loc.z as f32,
        });
    }
}

fn pt_at(track: Option<&Vec<Pt>>, t: f32, max_gap: f32) -> Option<Pt> {
    let track = track?;
    if track.is_empty() {
        return None;
    }
    let mut lo = 0usize;
    let mut hi = track.len() - 1;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if track[mid].t < t {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let after = track[lo];
    if lo == 0 || after.t <= t {
        return if (after.t - t).abs() <= max_gap { Some(after) } else { None };
    }
    let before = track[lo - 1];
    if after.t - before.t <= max_gap {
        let f = (t - before.t) / (after.t - before.t);
        return Some(Pt {
            t,
            x: before.x + (after.x - before.x) * f,
            y: before.y + (after.y - before.y) * f,
            z: before.z + (after.z - before.z) * f,
        });
    }
    let nearest = if t - before.t <= after.t - t { before } else { after };
    if (nearest.t - t).abs() <= max_gap {
        Some(nearest)
    } else {
        None
    }
}

/// Decodes a FastArray delta (`ActivatableAbilities`) without knowing field names.
/// Public for unit testing.
pub fn decode_ability_grants_fast_array(
    data: &[u8],
    bits: usize,
    treasure_handler_guids: &HashMap<u32, String>,
) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    if bits < 129 {
        return out;
    }
    let mut pos: usize = 1; // bSupportsFastArrayDeltaStructSerialization

    let read_bit = |pos: &mut usize| -> Option<u32> {
        if *pos < bits && (*pos >> 3) < data.len() {
            let b = ((data[*pos >> 3] >> (*pos & 7)) & 1) as u32;
            *pos += 1;
            Some(b)
        } else {
            None
        }
    };
    let read_n = |pos: &mut usize, n: usize| -> Option<u32> {
        let mut v = 0u32;
        for k in 0..n {
            v |= read_bit(pos)? << k;
        }
        Some(v)
    };
    let read_packed = |pos: &mut usize| -> Option<u32> {
        let mut v = 0u32;
        for shift in (0..35).step_by(7) {
            let b = read_n(pos, 8)?;
            v |= (b >> 1) << shift;
            if (b & 1) == 0 {
                return Some(v);
            }
        }
        None
    };

    if read_n(&mut pos, 32).is_none() || read_n(&mut pos, 32).is_none() {
        return out;
    }
    let Some(deletes) = read_n(&mut pos, 32) else {
        return out;
    };
    let Some(changed) = read_n(&mut pos, 32) else {
        return out;
    };
    if deletes > 512 || changed > 512 {
        return out;
    }
    pos = pos.saturating_add((deletes as usize) * 32);

    for _ in 0..changed {
        let Some(elem_id_u) = read_n(&mut pos, 32) else {
            return out;
        };
        let element_id = elem_id_u as i32;
        loop {
            let Some(handle) = read_packed(&mut pos) else {
                return out;
            };
            if handle == 0 {
                break;
            }
            let Some(num_bits_u) = read_packed(&mut pos) else {
                return out;
            };
            let num_bits = num_bits_u as usize;
            if pos.saturating_add(num_bits) > bits {
                return out;
            }
            if matches!(num_bits, 8 | 16 | 24 | 32 | 40) {
                let start = pos;
                let mut guid = 0u32;
                let mut shift = 0u32;
                while pos < start + num_bits {
                    let Some(b) = read_n(&mut pos, 8) else {
                        return out;
                    };
                    guid |= (b >> 1).wrapping_shl(shift);
                    if (b & 1) == 0 {
                        break;
                    }
                    shift += 7;
                }
                pos = start + num_bits;
                if let Some(handler) = treasure_handler_guids.get(&guid) {
                    out.push((element_id, handler.clone()));
                }
            } else {
                pos += num_bits;
            }
        }
    }
    out
}

#[derive(Default)]
struct AnalyzerState {
    channel_key: HashMap<u32, String>,
    channel_label: HashMap<u32, String>,
    last_searcher: HashMap<u32, u32>,
    searched: HashSet<String>,
    searches: Vec<SearchEvent>,
    channel_open_at: HashMap<u32, f32>,
    channel_searched: HashMap<u32, bool>,
    last_opener: HashMap<String, Option<u32>>,

    pawn_track: HashMap<u32, Vec<Pt>>,
    vehicle_track: HashMap<u32, Vec<Pt>>,
    seat_spans: Vec<SeatSpan>,
    seated: HashMap<(u32, usize), usize>, // (vehicle, seat) -> index into seat_spans
    orphans: Vec<OrphanDestroy>,
    orphan_channels: HashSet<u32>,
    opened_containers: HashSet<u32>,

    tosses: Vec<TossEvent>,
    equips: Vec<EquipEvent>,
    prizes: Vec<PrizeEvent>,
    first_export: HashMap<String, f32>,
    prize_channels_seen: HashSet<u32>,
    equip_channels_seen: HashSet<u32>,
    pawn_to_player_state: HashMap<u32, u32>,
    player_state_to_player_id: HashMap<u32, i32>,

    treasure_handler_guids: HashMap<u32, String>,
    grants: Vec<GrantEvent>,
}

impl AnalyzerState {
    fn position_of(&self, pawn: u32, t: f32) -> Option<Pt> {
        for span in &self.seat_spans {
            if span.pawn == pawn && t >= span.from - 0.5 && t <= span.to + 0.5 {
                if let Some(pt) = pt_at(self.vehicle_track.get(&span.vehicle), t, 3.0) {
                    return Some(pt);
                }
            }
        }
        pt_at(self.pawn_track.get(&pawn), t, 3.0)
    }

    fn resolve_orphans(&mut self) {
        for orphan in &self.orphans {
            let loot: Vec<Pt> = self
                .tosses
                .iter()
                .filter(|x| {
                    x.loc.is_some()
                        && x.owner == Some(orphan.actor)
                        && (x.time - orphan.time).abs() <= 2.0
                })
                .filter_map(|x| x.loc)
                .collect();
            if loot.is_empty() {
                continue;
            }
            let mut nearest = 0u32;
            let mut nearest_dist = 450.0f32;
            for &pawn in self.pawn_to_player_state.keys() {
                let Some(at) = self.position_of(pawn, orphan.time) else {
                    continue;
                };
                let d = loot
                    .iter()
                    .map(|&l| at.dist(l))
                    .fold(f32::INFINITY, f32::min);
                if d < nearest_dist {
                    nearest_dist = d;
                    nearest = pawn;
                }
            }
            if nearest == 0 {
                continue;
            }
            let key = format!("{}#orphan{:.0}", orphan.actor_key, orphan.time);
            if self.searched.insert(key) {
                self.searches.push(SearchEvent {
                    time: orphan.time,
                    actor_key: orphan.actor_key.clone(),
                    label: orphan.label.clone(),
                    searcher_pawn: Some(nearest),
                    destroyed: true,
                    stale: false,
                });
            }
        }
    }

    fn vehicle_runs(&self, pawns: &HashSet<u32>) -> (i32, i32, i32) {
        let mut path: Vec<Pt> = Vec::new();
        let mut seconds = 0.0f32;
        let mut centimeters = 0.0f32;

        for span in self.seat_spans.iter().filter(|s| pawns.contains(&s.pawn)) {
            let points: Vec<Pt> = self
                .vehicle_track
                .get(&span.vehicle)
                .map(|v| {
                    v.iter()
                        .copied()
                        .filter(|p| p.t >= span.from && p.t <= span.to)
                        .collect()
                })
                .unwrap_or_default();
            if points.len() < 2 {
                continue;
            }
            seconds += points.last().unwrap().t - points[0].t;
            for i in 1..points.len() {
                let a = points[i - 1];
                let b = points[i];
                let step = a.dist(b);
                centimeters += step;
                let parts = ((step / 100.0) as usize).max(1);
                for k in 0..parts {
                    let f = k as f32 / parts as f32;
                    path.push(Pt {
                        t: a.t + (b.t - a.t) * f,
                        x: a.x + (b.x - a.x) * f,
                        y: a.y + (b.y - a.y) * f,
                        z: a.z + (b.z - a.z) * f,
                    });
                }
            }
        }

        if path.is_empty() {
            return (0, 0, 0);
        }

        let mut seen: HashMap<String, Vec<Pt>> = HashMap::new();
        let mut near: Vec<(Pt, String, f32, Option<u32>)> = Vec::new();

        for toss in self.tosses.iter().filter(|t| t.loc.is_some()) {
            let loc = toss.loc.unwrap();
            let spots = seen.entry(toss.item.clone()).or_default();
            let sent_again = spots.iter().any(|&s| s.dist(loc) < 150.0);
            spots.push(loc);
            if sent_again {
                continue;
            }
            let mut best = f32::MAX;
            let mut passed_at = 0.0f32;
            for &p in &path {
                let d = p.dist(loc);
                if d < best {
                    best = d;
                    passed_at = p.t;
                }
            }
            if best < 900.0 && toss.time >= passed_at - 1.0 {
                near.push((loc, toss.item.clone(), passed_at, toss.owner));
            }
        }

        let mut piles: Vec<Vec<(Pt, String, f32, Option<u32>)>> = Vec::new();
        for item in near {
            if let Some(pile) = piles
                .iter_mut()
                .find(|p| p.iter().any(|o| o.0.dist(item.0) < 500.0))
            {
                pile.push(item);
            } else {
                piles.push(vec![item]);
            }
        }

        let unexplained = piles
            .iter()
            .filter(|p| {
                p.iter()
                    .any(|i| !i.1.to_ascii_lowercase().starts_with("athenaammodata"))
                    && !p.iter().any(|i| {
                        i.3.is_some_and(|o| {
                            self.opened_containers.contains(&o)
                                || self.orphans.iter().any(|x| x.actor == o)
                        })
                    })
                    && !self
                        .searches
                        .iter()
                        .any(|s| (s.time - p[0].2).abs() <= 4.0)
            })
            .count() as i32;

        (seconds as i32, (centimeters / 100.0) as i32, unexplained)
    }
}

impl ReplayCallbacks for AnalyzerState {
    fn on_net_guid_exported(&mut self, time: f32, net_guid: u32, path_name: &str) {
        if tracked_export_re().is_match(path_name) {
            let name = clean_name(path_name);
            self.first_export.entry(name.clone()).or_insert(time);
            if name.contains("TreasureHandler") {
                self.treasure_handler_guids
                    .insert(net_guid, name.replace("Default__", ""));
            }
        }
    }

    fn on_channel_opened(&mut self, time: f32, channel: &Channel, cache: &NetGuidCache) {
        let archetype = channel
            .archetype_id()
            .and_then(|g| cache.try_get_path_name(g))
            .unwrap_or("");
        let actor_path = channel
            .actor_id()
            .and_then(|g| cache.try_get_path_name(g))
            .unwrap_or("");
        let label = clean_name(if !archetype.is_empty() {
            archetype
        } else {
            actor_path
        });

        let key = if !actor_path.is_empty() {
            actor_path.to_string()
        } else if let Some(loc) = channel.actor.as_ref().and_then(|a| a.location) {
            if is_dynamic_chest(&label) {
                format!("{label}@{:.0},{:.0}", loc.x, loc.y)
            } else {
                format!("guid:{}", channel.actor_id().unwrap_or(0))
            }
        } else {
            format!("guid:{}", channel.actor_id().unwrap_or(0))
        };

        let ch_idx = channel.index;
        self.channel_key.insert(ch_idx, key);
        self.channel_label.insert(ch_idx, label);
        self.channel_open_at.insert(ch_idx, time);
        self.channel_searched.remove(&ch_idx);
        self.last_searcher.remove(&ch_idx);
        self.equip_channels_seen.remove(&ch_idx);
        self.prize_channels_seen.remove(&ch_idx);
        self.orphan_channels.remove(&ch_idx);
    }

    fn on_raw_probe(
        &mut self,
        time: f32,
        _channel_index: u32,
        actor_id: u32,
        group: &str,
        field: &str,
        data: &[u8],
        bits: usize,
    ) {
        if group.contains("AbilitySystemComponent")
            && group.ends_with("_ClassNetCache")
            && (field == "?" || field == "ActivatableAbilities")
            && !self.treasure_handler_guids.is_empty()
        {
            for (element_id, handler) in
                decode_ability_grants_fast_array(data, bits, &self.treasure_handler_guids)
            {
                if !self
                    .grants
                    .iter()
                    .any(|g| g.player_state_actor == actor_id && g.element_id == element_id)
                {
                    self.grants.push(GrantEvent {
                        time,
                        player_state_actor: actor_id,
                        element_id,
                        handler,
                    });
                }
            }
        }
    }

    fn on_export_read(
        &mut self,
        time: f32,
        ch_idx: u32,
        channel: Option<&Channel>,
        export: &ParsedExport,
        cache: &NetGuidCache,
    ) {
        match export {
            ParsedExport::Container {
                searching_pawn,
                b_already_searched,
                b_destroyed,
            } => {
                if let Some(s) = *searching_pawn {
                    if s != 0 {
                        self.last_searcher.insert(ch_idx, s);
                    }
                }
                let container_key = self
                    .channel_key
                    .get(&ch_idx)
                    .cloned()
                    .unwrap_or_else(|| format!("ch:{ch_idx}"));
                let container_label = self
                    .channel_label
                    .get(&ch_idx)
                    .cloned()
                    .unwrap_or_else(|| "?".to_string());
                let open_at = self.channel_open_at.get(&ch_idx).copied().unwrap_or(f32::MIN);
                let stale = (time - open_at) < 0.5;

                if *b_already_searched == Some(false) {
                    self.channel_searched.insert(ch_idx, false);
                } else if *b_already_searched == Some(true)
                    && !self.channel_searched.get(&ch_idx).copied().unwrap_or(false)
                {
                    self.channel_searched.insert(ch_idx, true);
                    let opened_by = self.last_searcher.get(&ch_idx).copied();
                    let seen_again = stale
                        && if stable_key(&container_key) {
                            self.last_opener
                                .get(&container_key)
                                .is_some_and(|&prev| prev == opened_by)
                        } else {
                            self.searched.contains(&container_key)
                        };
                    if !seen_again {
                        self.searched.insert(container_key.clone());
                        self.last_opener.insert(container_key.clone(), opened_by);
                        self.searches.push(SearchEvent {
                            time,
                            actor_key: container_key,
                            label: container_label,
                            searcher_pawn: opened_by,
                            destroyed: false,
                            stale,
                        });
                    }
                } else if *b_destroyed
                    && searching_pawn.is_some_and(|d| d != 0)
                    && (self.searched.insert(container_key.clone())
                        || self.channel_searched.get(&ch_idx) == Some(&false))
                {
                    let destroyer = searching_pawn.unwrap();
                    self.channel_searched.insert(ch_idx, true);
                    self.last_opener
                        .insert(container_key.clone(), Some(destroyer));
                    self.searches.push(SearchEvent {
                        time,
                        actor_key: container_key,
                        label: container_label,
                        searcher_pawn: Some(destroyer),
                        destroyed: true,
                        stale,
                    });
                } else if *b_destroyed
                    && searching_pawn.unwrap_or(0) == 0
                    && !stale
                    && stable_key(&container_key)
                    && (!self.searched.contains(&container_key)
                        || self.channel_searched.get(&ch_idx) == Some(&false))
                {
                    if let Some(orphan_actor) = channel.and_then(|c| c.actor_id()) {
                        if self.orphan_channels.insert(ch_idx) {
                            self.orphans.push(OrphanDestroy {
                                time,
                                actor_key: container_key,
                                label: container_label,
                                actor: orphan_actor,
                            });
                        }
                    }
                }

                if *b_already_searched == Some(true) || *b_destroyed {
                    if let Some(shown_open) = channel.and_then(|c| c.actor_id()) {
                        self.opened_containers.insert(shown_open);
                    }
                }
            }

            ParsedExport::Vehicle {
                replicated_movement: Some(mv),
            } => {
                if let Some(vid) = channel.and_then(|c| c.actor_id()) {
                    track_pt(&mut self.vehicle_track, vid, mv.location, time, 0.25);
                }
            }

            ParsedExport::SeatComponent {
                player_slots: Some(slots),
            } => {
                if let Some(seat_vehicle) = channel.and_then(|c| c.actor_id()) {
                    for (seat, slot) in slots.iter().enumerate() {
                        let Some(occupant) = *slot else {
                            continue;
                        };
                        if let Some(prev_idx) = self.seated.remove(&(seat_vehicle, seat)) {
                            self.seat_spans[prev_idx].to = time;
                        }
                        if occupant != 0 {
                            let idx = self.seat_spans.len();
                            self.seat_spans.push(SeatSpan {
                                pawn: occupant,
                                vehicle: seat_vehicle,
                                from: time,
                                to: f32::MAX,
                            });
                            self.seated.insert((seat_vehicle, seat), idx);
                        }
                    }
                }
            }

            ParsedExport::LootCannonFired {
                interacting_pawn: Some(gunner),
            } if *gunner != 0 => {
                let base_key = self
                    .channel_key
                    .get(&ch_idx)
                    .cloned()
                    .unwrap_or_else(|| format!("ch:{ch_idx}"));
                let cannon_key = format!("{base_key}#fire{time:.0}");
                if self.searched.insert(cannon_key.clone()) {
                    let label = self
                        .channel_label
                        .get(&ch_idx)
                        .cloned()
                        .unwrap_or_else(|| "LootCannon".to_string());
                    self.searches.push(SearchEvent {
                        time,
                        actor_key: cannon_key,
                        label,
                        searcher_pawn: Some(*gunner),
                        destroyed: false,
                        stale: false,
                    });
                }
            }

            ParsedExport::TreasureHuntChest {
                instigator,
                source_container,
                opening_pawn,
                b_success,
                prize_template_id_bytes,
            } => {
                if let Some(bytes) = prize_template_id_bytes {
                    if let Some(prize_id) = read_prize_id(bytes) {
                        if *b_success != Some(false) && self.prize_channels_seen.insert(ch_idx) {
                            let container = source_container
                                .and_then(|g| cache.try_get_path_name(g))
                                .unwrap_or("")
                                .to_string();
                            self.prizes.push(PrizeEvent {
                                time,
                                prize_id,
                                pawn: opening_pawn.or(*instigator),
                                container,
                            });
                        }
                    }
                }
            }

            ParsedExport::Weapon {
                instigator,
                owner,
                weapon_data_name,
            } => {
                let holder = instigator.or(*owner);
                if let Some(h) = holder {
                    if h != 0 && self.equip_channels_seen.insert(ch_idx) {
                        let item_name = weapon_data_name
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                self.channel_label
                                    .get(&ch_idx)
                                    .cloned()
                                    .unwrap_or_else(|| "?".to_string())
                            });
                        self.equips.push(EquipEvent {
                            time,
                            item: item_name,
                            pawn: h,
                        });
                    }
                }
            }

            ParsedExport::FortPickup {
                b_tossed_from_container: Some(true),
                item_definition_guid,
                item_definition_name,
                owner,
            } => {
                let item = item_definition_name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        item_definition_guid
                            .and_then(|g| cache.try_get_path_name(g))
                            .map(clean_name)
                    })
                    .unwrap_or_default();
                if !item.is_empty() {
                    let loc = channel
                        .and_then(|c| c.actor.as_ref())
                        .and_then(|a| a.location)
                        .map(|v| Pt {
                            t: time,
                            x: v.x as f32,
                            y: v.y as f32,
                            z: v.z as f32,
                        });
                    self.tosses.push(TossEvent {
                        time,
                        item,
                        loc,
                        owner: *owner,
                    });
                }
            }

            ParsedExport::PlayerPawn {
                player_state,
                replicated_movement,
            } => {
                if let Some(pawn_id) = channel.and_then(|c| c.actor_id()) {
                    if let Some(ps) = *player_state {
                        if ps != 0 {
                            self.pawn_to_player_state.insert(pawn_id, ps);
                        }
                    }
                    if let Some(mv) = replicated_movement {
                        track_pt(&mut self.pawn_track, pawn_id, mv.location, time, 0.5);
                    }
                }
            }

            ParsedExport::FortPlayerState { player_id, .. } => {
                if let (Some(ps_id), Some(pid)) = (channel.and_then(|c| c.actor_id()), *player_id) {
                    self.player_state_to_player_id.insert(ps_id, pid);
                }
            }

            _ => {}
        }
    }
}

/// Parses one `.replay` byte slice and produces a `MatchRecord`, mirroring `ReplayAnalyzer.Analyze`.
pub fn analyze_replay_bytes(
    data: &[u8],
    file_name: &str,
    username: Option<&str>,
    catalog: &TreasureCatalog,
    lang: &LangCtx,
) -> Result<MatchRecord, String> {
    let mut parser = ReplayParser::new(false);
    let mut state = AnalyzerState::default();
    let replay = parser.parse(data, &mut state)?;

    let players = &replay.players;
    let names: Vec<&str> = username
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut target = None;
    let mut matched_by_name = false;
    if !names.is_empty() {
        target = players.iter().find(|p| {
            p.player_name.as_deref().is_some_and(|pn| {
                let pt = pn.trim();
                names.iter().any(|&n| pt.eq_ignore_ascii_case(n))
            })
        });
        matched_by_name = target.is_some();
    }

    let recorder_ps = replay.game_data.recorder_id;
    let recorder_player = recorder_ps
        .and_then(|rps| state.player_state_to_player_id.get(&rps).copied())
        .and_then(|rpid| players.iter().find(|p| p.id == Some(rpid)));

    let not_tracked = !names.is_empty() && target.is_none();
    if target.is_none() && !not_tracked {
        target = recorder_player;
    }

    let mut target_player_states: HashSet<u32> = HashSet::new();
    if let Some(tid) = target.and_then(|t| t.id) {
        for (&ps, &pid) in &state.player_state_to_player_id {
            if pid == tid {
                target_player_states.insert(ps);
            }
        }
    }
    if target_player_states.is_empty() && !matched_by_name && !not_tracked {
        if let Some(rps) = recorder_ps {
            target_player_states.insert(rps);
        }
    }

    let is_recorder_target = recorder_ps.is_some_and(|rps| target_player_states.contains(&rps));
    let target_pawns: HashSet<u32> = state
        .pawn_to_player_state
        .iter()
        .filter(|(_, ps)| target_player_states.contains(ps))
        .map(|(&pawn, _)| pawn)
        .collect();

    // Resolve orphan destroys
    state.resolve_orphans();

    let mut interactions: Vec<(f32, &'static str, String, String, bool)> = state
        .searches
        .iter()
        .filter(|s| {
            s.searcher_pawn.is_some_and(|p| target_pawns.contains(&p))
                && (!s.stale || stable_key(&s.actor_key))
        })
        .map(|s| {
            (
                s.time,
                chest_kinds::classify(&s.label),
                s.label.clone(),
                s.actor_key.clone(),
                s.destroyed,
            )
        })
        .collect();
    interactions.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut chests: Vec<ChestRecord> = Vec::new();
    let mut others: HashMap<String, i32> = HashMap::new();
    for (time, kind, label, _key, destroyed) in &interactions {
        if !chest_kinds::is_chest(kind) {
            *others.entry((*kind).to_string()).or_insert(0) += 1;
            continue;
        }
        let items: Vec<String> = state
            .tosses
            .iter()
            .filter(|t| t.time >= *time - 0.25 && t.time <= *time + 0.6)
            .map(|t| t.item.clone())
            .collect();
        let treasure_items: Vec<String> = items
            .iter()
            .filter(|i| treasure_item_re().is_match(i) && !event_item_re().is_match(i))
            .cloned()
            .collect();
        chests.push(ChestRecord {
            time_seconds: round_1_f32(*time),
            kind: (*kind).to_string(),
            blueprint: label.clone(),
            destroyed: *destroyed,
            items,
            treasure_items,
        });
    }

    // Treasures obtained by target
    let mut finds: Vec<TreasureFind> = Vec::new();
    for pz in state
        .prizes
        .iter()
        .filter(|p| p.pawn.is_some_and(|pawn| target_pawns.contains(&pawn)))
    {
        let label = clean_name(&pz.container);
        let kind = if label.is_empty() {
            "inconnu"
        } else {
            chest_kinds::classify(&label)
        };
        finds.push(TreasureFind {
            item: treasure_friendly(&pz.prize_id, lang, catalog),
            raw_item: pz.prize_id.clone(),
            prize_id: Some(pz.prize_id.clone()),
            from_golden_chest: true,
            time_exact: true,
            source_destroyed: interactions
                .iter()
                .any(|i| i.4 && i.3 == pz.container),
            obtained_at_seconds: Some(round_1_f32(pz.time)),
            source_kind: kind.to_string(),
            source_blueprint: label,
            source_is_chest: chest_kinds::is_chest(kind),
            source_certain: true,
            from_prize_handler: false,
            also_possible: Vec::new(),
            teammate: None,
        });
    }

    // Teammates
    let mut teammate_pawns: HashMap<u32, String> = HashMap::new();
    if let Some(team_idx) = target.and_then(|t| t.team_index) {
        let target_id = target.and_then(|t| t.id);
        for mate in players
            .iter()
            .filter(|p| p.team_index == Some(team_idx) && p.id != target_id)
        {
            let mate_states: HashSet<u32> = state
                .player_state_to_player_id
                .iter()
                .filter(|&(_, &pid)| Some(pid) == mate.id)
                .map(|(&ps, _)| ps)
                .collect();
            for (&pawn, &ps) in &state.pawn_to_player_state {
                if mate_states.contains(&ps) {
                    teammate_pawns.insert(pawn, mate.player_name.clone().unwrap_or_default());
                }
            }
        }
    }
    let has_team = target.and_then(|t| t.team_index).is_some_and(|ti| {
        let target_id = target.and_then(|t| t.id);
        players
            .iter()
            .any(|p| p.team_index == Some(ti) && p.id != target_id)
    });

    let mut team_chests: Vec<TeamChestCount> = Vec::new();
    if let Some(squad) = target.and_then(|t| t.team_index) {
        let target_id = target.and_then(|t| t.id);
        for mate in players
            .iter()
            .filter(|p| p.team_index == Some(squad) && p.id != target_id)
        {
            let mate_pawns: HashSet<u32> = teammate_pawns
                .keys()
                .copied()
                .filter(|k| {
                    state
                        .pawn_to_player_state
                        .get(k)
                        .and_then(|mps| state.player_state_to_player_id.get(mps))
                        .copied()
                        == mate.id
                })
                .collect();
            let mate_chests: Vec<&SearchEvent> = state
                .searches
                .iter()
                .filter(|s| {
                    s.searcher_pawn.is_some_and(|p| mate_pawns.contains(&p))
                        && (!s.stale || stable_key(&s.actor_key))
                        && chest_kinds::is_chest(chest_kinds::classify(&s.label))
                })
                .collect();
            team_chests.push(TeamChestCount {
                name: mate.player_name.clone().unwrap_or_default(),
                epic_id: mate.epic_id.clone(),
                is_bot: mate.is_bot,
                chests: mate_chests.len() as i32,
                destroyed: mate_chests.iter().filter(|s| s.destroyed).count() as i32,
            });
        }
    }

    let (vehicle_seconds, vehicle_meters, unrecorded_loot) = state.vehicle_runs(&target_pawns);

    let mut team_finds: Vec<TreasureFind> = Vec::new();
    for pz in state
        .prizes
        .iter()
        .filter(|p| p.pawn.is_some_and(|pawn| teammate_pawns.contains_key(&pawn)))
    {
        let label = clean_name(&pz.container);
        let kind = if label.is_empty() {
            "inconnu"
        } else {
            chest_kinds::classify(&label)
        };
        let mate_name = teammate_pawns
            .get(&pz.pawn.unwrap())
            .cloned()
            .unwrap_or_default();
        team_finds.push(TreasureFind {
            item: treasure_friendly(&pz.prize_id, lang, catalog),
            raw_item: pz.prize_id.clone(),
            prize_id: Some(pz.prize_id.clone()),
            from_golden_chest: true,
            time_exact: true,
            obtained_at_seconds: Some(round_1_f32(pz.time)),
            source_kind: kind.to_string(),
            source_blueprint: label,
            source_is_chest: chest_kinds::is_chest(kind),
            source_certain: true,
            source_destroyed: false,
            from_prize_handler: false,
            also_possible: Vec::new(),
            teammate: Some(mate_name),
        });
    }

    let mut equipped_by_me: Vec<&EquipEvent> = state
        .equips
        .iter()
        .filter(|e| target_pawns.contains(&e.pawn) && !event_item_re().is_match(&e.item))
        .collect();
    equipped_by_me.sort_by(|a, b| a.time.total_cmp(&b.time));

    // Prize handlers
    let mut handler_times: Vec<f32> = state
        .grants
        .iter()
        .filter(|g| target_player_states.contains(&g.player_state_actor))
        .map(|g| g.time)
        .collect();
    handler_times.sort_by(|a, b| a.total_cmp(b));
    if handler_times.is_empty() && is_recorder_target {
        if let Some(&first_h) = state
            .first_export
            .iter()
            .find(|(k, _)| k.contains("TreasureHandler"))
            .map(|(_, v)| v)
        {
            handler_times.push(first_h);
        }
    }

    for (hi, &h) in handler_times.iter().enumerate() {
        let next_handler = handler_times.get(hi + 1).copied().unwrap_or(f32::MAX);
        let closest = state
            .searches
            .iter()
            .filter(|x| {
                x.time >= h - 3.0
                    && (x.time <= h + 2.0
                        || (x.time <= h + 4.0
                            && chest_kinds::classify(&x.label) == chest_kinds::BURIED))
            })
            .min_by(|a, b| (a.time - h).abs().total_cmp(&(b.time - h).abs()));

        let mine = closest
            .and_then(|c| c.searcher_pawn)
            .is_some_and(|cp| target_pawns.contains(&cp));
        let golden = finds.iter().chain(team_finds.iter()).any(|f| {
            f.from_golden_chest
                && f.obtained_at_seconds
                    .is_some_and(|g| (g - h).abs() <= 5.0)
        });

        if mine && !golden {
            let closest = closest.unwrap();
            let kind = chest_kinds::classify(&closest.label);
            let mut item: Option<String> = state
                .first_export
                .iter()
                .find(|(k, v)| {
                    k.starts_with("WID_")
                        && !event_item_re().is_match(k)
                        && (**v - h).abs() <= 1.0
                })
                .map(|(k, _)| k.clone());

            if item.is_none() {
                let mut later_names: Vec<String> = Vec::new();
                for e in equipped_by_me
                    .iter()
                    .filter(|e| e.time >= h && e.time < next_handler)
                {
                    let fr = treasure_friendly(&e.item, lang, catalog);
                    if !fr.starts_with("B_") && !later_names.contains(&fr) {
                        later_names.push(fr);
                    }
                }
                if later_names.len() == 1 {
                    item = equipped_by_me
                        .iter()
                        .find(|e| {
                            e.time >= h
                                && e.time < next_handler
                                && treasure_friendly(&e.item, lang, catalog) == later_names[0]
                        })
                        .map(|e| e.item.clone());
                }
            }
            let item_name = item
                .as_deref()
                .map(|it| treasure_friendly(it, lang, catalog));
            let match_start = replay
                .game_data
                .utc_time_started_match
                .or(replay.info.timestamp)
                .unwrap_or_else(Utc::now);
            let local_start = match_start.with_timezone(&Local);
            let hr = round_1_f32(h);
            let unknown_key = if item.is_none() {
                let sec = hr as i32;
                Some(format!(
                    "{UNKNOWN_PREFIX}{}@{}m{:02}s",
                    local_start.format("%Y-%m-%d_%H.%M"),
                    sec / 60,
                    sec % 60
                ))
            } else {
                None
            };

            finds.push(TreasureFind {
                item: item_name.unwrap_or_else(|| UNKNOWN_FR.to_string()),
                raw_item: item.unwrap_or_else(|| "GA_TreasureHandler".to_string()),
                prize_id: unknown_key,
                obtained_at_seconds: Some(hr),
                time_exact: true,
                source_kind: kind.to_string(),
                source_blueprint: closest.label.clone(),
                source_is_chest: chest_kinds::is_chest(kind),
                source_certain: true,
                source_destroyed: closest.destroyed,
                from_golden_chest: false,
                from_prize_handler: true,
                also_possible: Vec::new(),
                teammate: None,
            });
        } else if !mine && !golden && has_team && is_recorder_target {
            let by_mate = closest
                .and_then(|c| c.searcher_pawn)
                .and_then(|sp| teammate_pawns.get(&sp))
                .cloned();
            let kind = if by_mate.is_some() {
                chest_kinds::classify(&closest.unwrap().label)
            } else {
                "inconnu"
            };
            team_finds.push(TreasureFind {
                item: UNKNOWN_FR.to_string(),
                raw_item: "GA_TreasureHandler".to_string(),
                prize_id: None,
                obtained_at_seconds: Some(round_1_f32(h)),
                time_exact: true,
                source_kind: kind.to_string(),
                source_blueprint: if by_mate.is_some() {
                    closest.unwrap().label.clone()
                } else {
                    String::new()
                },
                source_is_chest: by_mate.is_none() || chest_kinds::is_chest(kind),
                source_certain: by_mate.is_some(),
                source_destroyed: by_mate.is_some() && closest.unwrap().destroyed,
                from_golden_chest: false,
                from_prize_handler: true,
                also_possible: Vec::new(),
                teammate: Some(by_mate.unwrap_or_default()),
            });
        }
    }

    team_finds.sort_by(|a, b| {
        a.obtained_at_seconds
            .unwrap_or(f32::MAX)
            .total_cmp(&b.obtained_at_seconds.unwrap_or(f32::MAX))
    });

    // Seen prizes (other players' golden chests)
    let mut seen: Vec<SeenPrize> = Vec::new();
    for pz in state.prizes.iter().filter(|p| {
        p.pawn
            .is_some_and(|pawn| !target_pawns.contains(&pawn) && !teammate_pawns.contains_key(&pawn))
    }) {
        let who = state
            .pawn_to_player_state
            .get(&pz.pawn.unwrap())
            .and_then(|ops| state.player_state_to_player_id.get(ops))
            .and_then(|&opid| players.iter().find(|p| p.id == Some(opid)));
        let player_name = who
            .and_then(|w| w.player_name.clone())
            .unwrap_or_else(|| {
                if who.is_some_and(|w| w.is_bot) {
                    "Bot".to_string()
                } else {
                    lang.t("joueur inconnu", "unknown player", "jugador desconocido")
                        .to_string()
                }
            });
        seen.push(SeenPrize {
            player: player_name,
            prize_id: pz.prize_id.clone(),
            time_seconds: round_1_f32(pz.time),
        });
    }

    finds.sort_by(|a, b| {
        a.obtained_at_seconds
            .unwrap_or(f32::MAX)
            .total_cmp(&b.obtained_at_seconds.unwrap_or(f32::MAX))
    });

    let is_recorder = is_recorder_target;
    let placement = target
        .and_then(|t| t.placement)
        .or_else(|| {
            if is_recorder {
                replay.team_stats.as_ref().map(|ts| ts.position as i32)
            } else {
                None
            }
        });

    let session_id = replay.game_data.game_session_id.clone().unwrap_or_else(|| {
        let ts = replay
            .info
            .timestamp
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();
        format!("{ts}-{file_name}")
    });

    let ammo_boxes = others.get(chest_kinds::AMMO_BOX).copied().unwrap_or(0);
    let other_containers: i32 = others
        .iter()
        .filter(|(k, _)| k.as_str() != chest_kinds::AMMO_BOX)
        .map(|(_, &v)| v)
        .sum();
    let team_chests_opened: i32 = team_chests.iter().map(|t| t.chests).sum();

    let map = replay
        .header
        .level_names_and_times
        .iter()
        .map(|(l, _)| l)
        .find(|l| !l.is_empty())
        .cloned();

    Ok(MatchRecord {
        session_id,
        replay_file: file_name.to_string(),
        analyzed_at_utc: Utc::now(),
        match_start_utc: replay.game_data.utc_time_started_match.or(replay.info.timestamp),
        duration_seconds: Some((replay.info.length_in_ms / 1000) as i32),
        playlist: replay.game_data.current_playlist.clone(),
        game_version: if replay.header.branch.is_empty() {
            None
        } else {
            Some(replay.header.branch.clone())
        },
        player_name: target.and_then(|t| t.player_name.clone()),
        epic_id: target.and_then(|t| t.epic_id.clone()),
        tracked_by_username: matched_by_name,
        not_tracked_owner: if not_tracked {
            Some(
                recorder_player
                    .and_then(|r| r.player_name.clone())
                    .unwrap_or_else(|| "inconnu".to_string()),
            )
        } else {
            None
        },
        placement,
        total_players: replay.team_stats.as_ref().map(|ts| ts.total_players as i32),
        is_replay_owner: is_recorder,
        eliminations: if is_recorder {
            replay.stats.as_ref().map(|s| s.eliminations as i32)
        } else {
            None
        },
        chests_opened: chests.len() as i32,
        ammo_boxes_opened: ammo_boxes,
        other_containers_opened: other_containers,
        other_interactions: others,
        analyzer_version: CURRENT_VERSION,
        map,
        is_custom_match: replay.game_data.is_custom_match == Some(true),
        is_battle_royale: true,
        chests,
        treasure_finds: finds,
        team_treasure_finds: team_finds,
        seen_prizes: seen,
        team_chests,
        team_chests_opened,
        vehicle_seconds,
        vehicle_meters,
        unrecorded_vehicle_loot: unrecorded_loot,
    })
}

pub fn analyze_replay_file(
    path: &Path,
    username: Option<&str>,
    catalog: &TreasureCatalog,
    lang: &LangCtx,
) -> Result<MatchRecord, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("replay.replay");
    analyze_replay_bytes(&data, name, username, catalog, lang)
}
