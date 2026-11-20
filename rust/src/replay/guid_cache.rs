use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use regex::Regex;

pub fn remove_all_path_prefixes(path: &str) -> &str {
    for (i, c) in path.char_indices().rev() {
        match c {
            '.' => return &path[i + 1..],
            '/' => return path,
            _ => {}
        }
    }
    path.strip_prefix("Default__").unwrap_or(path)
}

pub fn clean_path_suffix(path: &str) -> &str {
    for (i, c) in path.char_indices().rev() {
        if !c.is_ascii_digit() && c != '_' {
            return &path[..i + c.len_utf8()];
        }
    }
    path
}

fn static_instance_suffix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(_UAID_.*|_[0-9a-f]{16}_\d+)$").unwrap())
}

#[derive(Debug, Clone)]
pub struct NetFieldExport {
    pub handle: u32,
    pub compatible_checksum: u32,
    pub name: String,
    pub incompatible: bool,
}

#[derive(Debug, Clone)]
pub struct NetFieldExportGroup {
    pub path_name: String,
    pub path_name_index: u32,
    pub exports: Vec<Option<NetFieldExport>>,
}

impl NetFieldExportGroup {
    pub fn new(path_name: String, path_name_index: u32, len: usize) -> Self {
        Self {
            path_name,
            path_name_index,
            exports: vec![None; len],
        }
    }

    pub fn len(&self) -> usize {
        self.exports.len()
    }

    pub fn is_valid_index(&self, handle: u32) -> bool {
        (handle as usize) < self.exports.len()
    }
}

#[derive(Debug, Clone)]
pub struct ExternalData {
    pub net_guid: u32,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
pub struct NetGuidCache {
    pub groups: HashMap<String, NetFieldExportGroup>,
    pub index_to_group_name: HashMap<u32, String>,
    pub net_guid_to_path: HashMap<u32, String>,
    pub external_data: HashMap<u32, ExternalData>,

    cleaned_paths: HashMap<u32, String>,
    archetype_to_group_name: HashMap<u32, String>,
    failed_paths: HashSet<String>,
}

impl NetGuidCache {
    pub fn add_export_group(&mut self, path_name: String, group: NetFieldExportGroup) {
        let idx = group.path_name_index;
        self.index_to_group_name.insert(idx, path_name.clone());
        self.groups.insert(path_name.clone(), group);
        self.cleaned_paths
            .entry(idx)
            .or_insert_with(|| remove_all_path_prefixes(&path_name).to_string());
        // ChestCounter patch: clear failed paths whenever a new export group arrives.
        self.failed_paths.clear();
    }

    pub fn get_group_mut(&mut self, path_name: &str) -> Option<&mut NetFieldExportGroup> {
        self.groups.get_mut(path_name)
    }

    pub fn get_group_by_index_mut(&mut self, idx: u32) -> Option<&mut NetFieldExportGroup> {
        let name = self.index_to_group_name.get(&idx)?.clone();
        self.groups.get_mut(&name)
    }

    pub fn get_group(&self, path_name: &str) -> Option<&NetFieldExportGroup> {
        self.groups.get(path_name)
    }

    pub fn try_get_path_name(&self, net_guid: u32) -> Option<&str> {
        self.net_guid_to_path.get(&net_guid).map(|s| s.as_str())
    }

    pub fn take_external_data(&mut self, net_guid: u32) -> Option<ExternalData> {
        self.external_data.remove(&net_guid)
    }

    pub fn try_get_class_net_cache(&self, group_path: &str, use_full_name: bool) -> Option<&NetFieldExportGroup> {
        if group_path.is_empty() {
            return None;
        }
        let cnc_path = if use_full_name {
            format!("{group_path}_ClassNetCache")
        } else {
            format!("{}_ClassNetCache", remove_all_path_prefixes(group_path))
        };
        self.groups.get(&cnc_path)
    }

    /// Resolves an actor/archetype NetGUID to its `NetFieldExportGroup` path name,
    /// matching `Unreal.Core.Models.NetGuidCache.GetNetFieldExportGroup(uint?)` + ChestCounter patches.
    pub fn resolve_group_name(&mut self, net_guid: u32) -> Option<String> {
        if let Some(cached) = self.archetype_to_group_name.get(&net_guid) {
            return Some(cached.clone());
        }
        let path = self.net_guid_to_path.get(&net_guid)?.clone();
        if self.failed_paths.contains(&path) {
            return None;
        }

        // 1) path contains groupPathFixed
        let mut matched: Option<String> = None;
        for (group_path, group) in &self.groups {
            let fixed = self
                .cleaned_paths
                .entry(group.path_name_index)
                .or_insert_with(|| remove_all_path_prefixes(group_path).to_string());
            if path.contains(fixed.as_str()) {
                matched = Some(group_path.clone());
                break;
            }
        }

        // 2) groupPathFixed contains cleanedPath
        if matched.is_none() {
            let cleaned_path = clean_path_suffix(&path);
            for (group_path, group) in &self.groups {
                if let Some(fixed) = self.cleaned_paths.get(&group.path_name_index) {
                    if fixed.contains(cleaned_path) {
                        matched = Some(group_path.clone());
                        break;
                    }
                }
            }
        }

        // 3) ChestCounter patch: strip _UAID_... / _<16hex>_<num> and digits/underscores, exact "<base>_C" match
        if matched.is_none() {
            let stripped = static_instance_suffix_re().replace(remove_all_path_prefixes(&path), "");
            let base = clean_path_suffix(&stripped);
            if !base.is_empty() {
                let class_name = format!("{base}_C");
                for (group_path, group) in &self.groups {
                    if let Some(fixed) = self.cleaned_paths.get(&group.path_name_index) {
                        if fixed == &class_name {
                            matched = Some(group_path.clone());
                            break;
                        }
                    }
                }
            }
        }

        if let Some(name) = matched {
            self.archetype_to_group_name.insert(net_guid, name.clone());
            Some(name)
        } else {
            self.failed_paths.insert(path);
            None
        }
    }
}
