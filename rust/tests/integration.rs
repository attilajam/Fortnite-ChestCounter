use std::collections::HashMap;
use std::path::Path;

use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use aes::Aes256;
use chrono::{FixedOffset, TimeZone, Utc};

use chest_counter::catalog::TreasureCatalog;
use chest_counter::game_modes;
use chest_counter::lang::{LangCtx, dotnet_json_string, html_encode};
use chest_counter::models::{
    ChestRecord, Config, MatchRecord, Store, TrackerData, chest_kinds,
};
use chest_counter::overlay::build_overlay;
use chest_counter::profiles;
use chest_counter::replay::analyzer::{
    analyze_replay_bytes, decode_ability_grants_fast_array,
};
use chest_counter::replay::archive::{
    ArchiveMeta, BinaryReader, BitReader, RotatorQuantization, VectorQuantization,
    chunk_type, engine_net_ver, export_flags, net_ver, replay_ver,
};
use chest_counter::replay::crypto::decrypt_aes_ecb_pkcs7;
use chest_counter::replay::guid_cache::{
    NetFieldExportGroup, NetGuidCache, clean_path_suffix, remove_all_path_prefixes,
};
use chest_counter::replay::reader::{FILE_MAGIC, NETWORK_MAGIC, decode_player_name_data};
use chest_counter::report::{self, ReportContext};

#[test]
fn lang_formatting_and_encoding_match_dotnet() {
    let config = Config::default();
    assert_eq!(
        LangCtx::new(config.language.as_deref(), false, "auto").code(),
        "en"
    );

    let en = LangCtx::new(Some("en"), false, "12");
    assert_eq!(en.n(15531), "15,531");
    assert_eq!(en.n(-1234567), "-1,234,567");
    assert_eq!(en.format_float(49.05, 1, false), "49.0"); // banker's rounding (round ties to even)
    assert_eq!(en.format_float(49.15, 1, false), "49.2");
    assert_eq!(en.count(1, "coffre", "chest", "cofre"), "1 chest");
    assert_eq!(en.count(2, "coffre", "chest", "cofre"), "2 chests");

    let fr = LangCtx::new(Some("fr"), false, "24");
    assert_eq!(fr.n(15531), "15\u{202F}531");
    assert_eq!(fr.format_float(49.1, 1, false), "49,1");
    assert_eq!(fr.count(0, "coffre", "chest", "cofre"), "0 coffre");
    assert_eq!(fr.count(1, "coffre", "chest", "cofre"), "1 coffre");
    assert_eq!(fr.count(2, "coffre", "chest", "cofre"), "2 coffres");

    assert_eq!(html_encode("A & B <C> \"D\" 'E'"), "A &amp; B &lt;C&gt; &quot;D&quot; &#39;E&#39;");
    assert_eq!(dotnet_json_string("Vehicle & Decal \u{00b7} +"), r#""Vehicle \u0026 Decal \u00B7 \u002B""#);
}

#[test]
fn catalog_odds_and_game_modes() {
    let catalog = TreasureCatalog::load(Path::new("/nonexistent"));
    let outfit = catalog.odds.iter().find(|o| o.kind == "outfit").unwrap();
    let t = Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap();
    let times = vec![t; 4770];
    let log_n = catalog.log_never(outfit, &times, 0);
    let p = -(log_n.exp() - 1.0);
    assert_eq!(format!("{p}"), "0.003048145918170375");

    assert!(game_modes::is_battle_royale_playlist(Some("Playlist_DefaultSolo")));
    assert!(game_modes::is_battle_royale_playlist(Some("Playlist_NoBuildBR_Squad")));
    assert!(!game_modes::is_battle_royale_playlist(Some("Playlist_Figment_Solo")));
    assert!(!game_modes::is_battle_royale_playlist(Some("Playlist_BlastBerry_Squad")));
    assert!(game_modes::is_season_map(Some("/Delilah/Maps/Delilah_Terrain")));
    assert!(!game_modes::is_season_map(Some("/Game/Athena/Apollo/Maps/Apollo_Terrain")));
}

#[test]
fn bit_reader_packed_ints_strings_and_repmovement() {
    let mut meta = ArchiveMeta::default();
    meta.engine_network_version = engine_net_ver::REP_MOVE_OPTIONAL_ACCELERATION;

    // IntPacked: 300 = (300 & 0x7f) << 1 | 1 = 0x59, (300 >> 7) << 1 = 0x04
    let mut br = BitReader::from_bytes(&[0x59, 0x04], 16, meta);
    assert_eq!(br.read_int_packed(), 300);
    assert!(br.at_end());

    // Unaligned read_bits + append_bits
    let mut dst = BitReader::from_bytes(&[0b0000_0101], 3, meta);
    dst.append_bits(&[0b0000_1011], 4);
    assert_eq!(dst.bits_left(), 7);
    assert_eq!(dst.read_bits_to_u32(7), 0b1011_101);

    // RepMovement with 3 unknown trailing flag bits (ChestCounter patch for Fortnite 43 vehicles)
    let mut bits = Vec::new();
    let push_bit = |v: &mut Vec<bool>, b: bool| v.push(b);
    for _ in 0..4 { push_bit(&mut bits, false); } // flags
    for _ in 0..7 { push_bit(&mut bits, false); } // loc info = 0 (3x f32)
    for _ in 0..96 { push_bit(&mut bits, false); } // loc = (0,0,0)
    for _ in 0..3 { push_bit(&mut bits, false); } // rot = (0,0,0)
    for _ in 0..7 { push_bit(&mut bits, false); } // vel info = 0 (3x f32)
    for _ in 0..96 { push_bit(&mut bits, false); } // vel = (0,0,0)
    push_bit(&mut bits, false); // bRepAcceleration = false
    push_bit(&mut bits, true);  // trailing unknown bit 1
    push_bit(&mut bits, false); // trailing unknown bit 2
    push_bit(&mut bits, true);  // trailing unknown bit 3

    let mut bytes = vec![0u8; (bits.len() + 7) / 8];
    for (i, &b) in bits.iter().enumerate() {
        if b { bytes[i >> 3] |= 1 << (i & 7); }
    }
    let mut r = BitReader::from_bytes(&bytes, bits.len(), meta);
    let mv = r.serialize_rep_movement(
        VectorQuantization::RoundTwoDecimals,
        RotatorQuantization::ShortComponents,
        VectorQuantization::RoundTwoDecimals,
    );
    assert!(!r.is_error);
    assert!(r.at_end());
    assert_eq!(mv.location.x, 0.0);
}

#[test]
fn player_name_decoding_and_guid_cache_resolution() {
    // Encode "Attila" with inverse of PlayerNameData shift formula
    let plain = "Attila";
    let units: Vec<u16> = plain.encode_utf16().collect();
    let base = ((units.len() % 4) * 3) % 8;
    let enc_str: String = units
        .iter()
        .enumerate()
        .map(|(i, &u)| {
            let shift = ((base + 1 + i) * 3) % 8;
            char::from_u32((u.wrapping_sub(shift as u16)) as u32).unwrap()
        })
        .collect();

    let mut buf = Vec::new();
    buf.push(0); // Handle
    buf.push(0); // Unknown1
    buf.push(1); // IsPlayer = true
    let mut raw = enc_str.into_bytes();
    raw.push(0);
    buf.extend_from_slice(&(raw.len() as i32).to_le_bytes());
    buf.extend_from_slice(&raw);

    assert_eq!(
        decode_player_name_data(&buf, ArchiveMeta::default()).as_deref(),
        Some("Attila")
    );

    // NetGuidCache UAID + instance hash fallback
    let mut cache = NetGuidCache::default();
    let grp_path = "/Game/Building/ActorBlueprints/Containers/Tiered_Chest_Athena.Tiered_Chest_Athena_C".to_string();
    cache.add_export_group(grp_path.clone(), NetFieldExportGroup::new(grp_path.clone(), 1, 4));
    cache.net_guid_to_path.insert(
        42,
        remove_all_path_prefixes("Tiered_Chest_Athena47_UAID_387C1234_1632776713").to_string(),
    );
    assert_eq!(cache.resolve_group_name(42).as_deref(), Some(grp_path.as_str()));
    assert_eq!(clean_path_suffix("Tiered_Chest_Athena47"), "Tiered_Chest_Athena");
}

#[test]
fn fast_array_ability_grant_decoding() {
    let mut bit_stream: Vec<bool> = Vec::new();
    let push_bits = |dst: &mut Vec<bool>, val: u32, n: usize| {
        for k in 0..n {
            dst.push(((val >> k) & 1) != 0);
        }
    };
    let push_packed = |dst: &mut Vec<bool>, mut v: u32| {
        loop {
            let chunk = (v & 0x7f) as u8;
            v >>= 7;
            let byte = (chunk << 1) | if v > 0 { 1 } else { 0 };
            push_bits(dst, byte as u32, 8);
            if v == 0 { break; }
        }
    };

    push_bits(&mut bit_stream, 1, 1); // bSupportsFastArrayDeltaStructSerialization
    push_bits(&mut bit_stream, 1, 32); // replication key 1
    push_bits(&mut bit_stream, 2, 32); // replication key 2
    push_bits(&mut bit_stream, 0, 32); // deletes = 0
    push_bits(&mut bit_stream, 1, 32); // changed = 1
    push_bits(&mut bit_stream, 7, 32); // element_id = 7
    push_packed(&mut bit_stream, 1);   // handle = 1
    push_packed(&mut bit_stream, 8);   // num_bits = 8 (1 packed byte for guid 123)
    push_packed(&mut bit_stream, 123); // guid = 123
    push_packed(&mut bit_stream, 0);   // end of element fields

    let mut bytes = vec![0u8; (bit_stream.len() + 7) / 8];
    for (i, &b) in bit_stream.iter().enumerate() {
        if b { bytes[i >> 3] |= 1 << (i & 7); }
    }

    let mut handlers = HashMap::new();
    handlers.insert(123u32, "GA_TreasureHandler_Test_C".to_string());

    let found = decode_ability_grants_fast_array(&bytes, bit_stream.len(), &handlers);
    assert_eq!(found, vec![(7, "GA_TreasureHandler_Test_C".to_string())]);
}

#[test]
fn aes_256_ecb_pkcs7_round_trip() {
    let key = [0x42u8; 32];
    let plain = b"Hello Fortnite Replay!";
    let pad = 16 - (plain.len() % 16);
    let mut padded = plain.to_vec();
    padded.extend(std::iter::repeat_n(pad as u8, pad));

    let cipher = Aes256::new_from_slice(&key).unwrap();
    for block in padded.chunks_exact_mut(16) {
        cipher.encrypt_block(GenericArray::from_mut_slice(block));
    }

    let decrypted = decrypt_aes_ecb_pkcs7(&padded, &key).unwrap();
    assert_eq!(decrypted, plain);
}

#[test]
fn synthetic_replay_parses_header_and_events() {
    let mut buf = Vec::new();
    let write_fstring = |out: &mut Vec<u8>, s: &str| {
        if s.is_empty() {
            out.extend_from_slice(&0i32.to_le_bytes());
        } else {
            let mut b = s.as_bytes().to_vec();
            b.push(0);
            out.extend_from_slice(&(b.len() as i32).to_le_bytes());
            out.extend_from_slice(&b);
        }
    };

    // ReplayInfo
    buf.extend_from_slice(&FILE_MAGIC.to_le_bytes());
    buf.extend_from_slice(&replay_ver::HISTORY_ENCRYPTION.to_le_bytes());
    buf.extend_from_slice(&600_000u32.to_le_bytes()); // length_in_ms = 600s
    buf.extend_from_slice(&net_ver::HISTORY_HEADER_FLAGS.to_le_bytes());
    buf.extend_from_slice(&12345u32.to_le_bytes()); // changelist
    write_fstring(&mut buf, "TestReplay");
    buf.extend_from_slice(&0u32.to_le_bytes()); // is_live = false
    buf.extend_from_slice(&639360000000000000i64.to_le_bytes()); // timestamp
    buf.extend_from_slice(&0u32.to_le_bytes()); // is_compressed = false
    buf.extend_from_slice(&0u32.to_le_bytes()); // is_encrypted = false
    buf.extend_from_slice(&0u32.to_le_bytes()); // encryption_key len = 0

    // Chunk 1: Header
    let mut hdr = Vec::new();
    hdr.extend_from_slice(&NETWORK_MAGIC.to_le_bytes());
    hdr.extend_from_slice(&net_ver::HISTORY_SAVE_FULL_ENGINE_VERSION.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes()); // checksum
    hdr.extend_from_slice(&engine_net_ver::HISTORY_CLASSNETCACHE_FULLNAME.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes()); // game protocol
    hdr.extend_from_slice(&1u16.to_le_bytes()); // major
    hdr.extend_from_slice(&2u16.to_le_bytes()); // minor
    hdr.extend_from_slice(&3u16.to_le_bytes()); // patch
    hdr.extend_from_slice(&999u32.to_le_bytes()); // changelist
    write_fstring(&mut hdr, "++Fortnite+Release-43.00");
    hdr.extend_from_slice(&1u32.to_le_bytes()); // 1 level
    write_fstring(&mut hdr, "/Delilah/Maps/Delilah_Terrain");
    hdr.extend_from_slice(&0u32.to_le_bytes()); // level time
    hdr.extend_from_slice(&0u32.to_le_bytes()); // flags
    hdr.extend_from_slice(&0u32.to_le_bytes()); // game_specific_data count

    buf.extend_from_slice(&chunk_type::HEADER.to_le_bytes());
    buf.extend_from_slice(&(hdr.len() as i32).to_le_bytes());
    buf.extend_from_slice(&hdr);

    // Chunk 2: TeamStats Event
    let mut ev = Vec::new();
    write_fstring(&mut ev, "ev1");
    write_fstring(&mut ev, "AthenaReplayBrowserEvents");
    write_fstring(&mut ev, "AthenaMatchTeamStats");
    ev.extend_from_slice(&0u32.to_le_bytes());
    ev.extend_from_slice(&0u32.to_le_bytes());
    ev.extend_from_slice(&12i32.to_le_bytes()); // payload size
    ev.extend_from_slice(&0u32.to_le_bytes()); // unknown
    ev.extend_from_slice(&3u32.to_le_bytes()); // position = 3
    ev.extend_from_slice(&98u32.to_le_bytes()); // total_players = 98

    buf.extend_from_slice(&chunk_type::EVENT.to_le_bytes());
    buf.extend_from_slice(&(ev.len() as i32).to_le_bytes());
    buf.extend_from_slice(&ev);

    let catalog = TreasureCatalog::load(Path::new("/nonexistent"));
    let lang = LangCtx::new(Some("en"), true, "24");
    let rec = analyze_replay_bytes(&buf, "synthetic.replay", None, &catalog, &lang).unwrap();
    assert_eq!(rec.duration_seconds, Some(600));
    assert_eq!(rec.map.as_deref(), Some("/Delilah/Maps/Delilah_Terrain"));
    assert_eq!(rec.game_version.as_deref(), Some("++Fortnite+Release-43.00"));
    assert_eq!(rec.total_players, Some(98));
    assert_eq!(rec.replay_file, "synthetic.replay");
}

#[test]
fn report_overlay_csv_smoke_all_languages() {
    let catalog = TreasureCatalog::load(Path::new("/nonexistent"));
    let now = Utc.with_ymd_and_hms(2026, 11, 19, 20, 0, 0).unwrap();
    let mut data = TrackerData::default();
    data.matches.push(MatchRecord {
        session_id: "sess-1".to_string(),
        replay_file: "test.replay".to_string(),
        analyzed_at_utc: now,
        match_start_utc: Some(now - chrono::Duration::minutes(15)),
        duration_seconds: Some(900),
        playlist: Some("Playlist_NoBuildBR_Solo".to_string()),
        player_name: Some("Tester".to_string()),
        epic_id: Some("EPIC123".to_string()),
        placement: Some(1),
        total_players: Some(100),
        chests_opened: 12,
        map: Some("/Delilah/Maps/Delilah_Terrain".to_string()),
        is_battle_royale: true,
        chests: vec![ChestRecord {
            time_seconds: 120.5,
            kind: chest_kinds::NORMAL.to_string(),
            blueprint: "Tiered_Chest_Athena_C".to_string(),
            destroyed: false,
            items: vec![],
            treasure_items: vec![],
        }],
        ..Default::default()
    });
    let config = Config::default();

    for code in ["fr", "en", "es"] {
        let mut lang = LangCtx::new(Some(code), false, "24");
        lang.fixed_local_offset = Some(FixedOffset::east_opt(0).unwrap());
        let ctx = ReportContext {
            now_utc: now,
            session_start_utc: now - chrono::Duration::hours(1),
            progress: None,
            lang: &lang,
            catalog: &catalog,
            images_folder: None,
        };
        let view = profiles::view(&data, &config, &lang);
        let profs = profiles::list(&data, &lang);
        let html = report::report(&config, &view, &profs, &ctx);
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("Tester"));
        assert!(html.contains("\r\n"));

        let fragment = report::report_fragment(&config, &data, &ctx);
        for marker in ["class=\"report-layout\"", "id=\"editchests\"", "id=\"t-matches\""] {
            assert!(fragment.contains(marker));
        }
        for marker in ["<!doctype", "<iframe", "<script>", "<header", "http-equiv"] {
            assert!(!fragment.contains(marker));
        }

        let ov = build_overlay(&view, None, false, true, ctx.session_start_utc, &lang);
        let hud = report::overlay(&ov, "hud", &lang);
        let classic = report::overlay(&ov, "classic", &lang);
        assert!(hud.contains("12"));
        assert!(classic.contains("12"));

        let csv = report::csv(&data, &lang, &catalog);
        assert!(csv.contains("test.replay"));
    }

    // Verify BinaryReader export flags helper isn't dead
    let mut r = BinaryReader::new(&[export_flags::HAS_PATH], ArchiveMeta::default());
    assert_eq!(r.read_u8(), export_flags::HAS_PATH);
}

#[test]
fn example_data_exact_byte_parity_if_present() {
    let example = Path::new("/home/attila/Downloads/ChestCounter");
    if !example.join("data.json").exists() || !example.join("report.html").exists() {
        return;
    }

    let store = Store::new(example);
    let config = store.load_config().unwrap();
    let data = store.load_data().unwrap();
    let catalog = TreasureCatalog::load(example);

    let mut lang = LangCtx::new(config.language.as_deref(), false, "12");
    lang.fixed_local_offset = Some(FixedOffset::west_opt(8 * 3600).unwrap());

    let now = chrono::DateTime::parse_from_rfc3339("2026-11-19T17:56:14-08:00")
        .unwrap()
        .with_timezone(&Utc);
    let images = store.images_folder();
    let ctx = ReportContext {
        now_utc: now,
        session_start_utc: now,
        progress: None,
        lang: &lang,
        catalog: &catalog,
        images_folder: Some(&images),
    };

    let view = profiles::view(&data, &config, &lang);
    let profs = profiles::list(&data, &lang);

    let actual_report = report::report(&config, &view, &profs, &ctx);
    // report.html now includes hover breakdowns for all 7 rewards and uses match-start
    // timestamps for Treasure Hour boosts (intentional upgrade over the reference C# report).
    for reward in [
        "Gold Outfit",
        "Pirate Shop Access",
        "Vehicle &amp; Decal",
        "Pickaxe or Back Bling",
        "Jam Track",
        "Loading Screen",
        "Gizmo (any)",
    ] {
        assert!(
            actual_report.contains(&format!(r#"<span class="cbartip"><b>{reward}</b>"#)),
            "missing hover breakdown tooltip for {reward}"
        );
    }

    let ov = build_overlay(&view, None, false, config.show_squad_total, ctx.session_start_utc, &lang);
    let actual_overlay = report::overlay(&ov, &config.overlay_style, &lang);
    let expected_overlay = std::fs::read_to_string(example.join("overlay.html")).unwrap();
    assert_eq!(actual_overlay, expected_overlay, "overlay.html byte parity mismatch");

    let actual_csv = report::csv(&data, &lang, &catalog);
    let expected_csv = std::fs::read_to_string(example.join("matches.csv")).unwrap();
    assert_eq!(actual_csv, expected_csv, "matches.csv byte parity mismatch");
}

#[test]
fn profiles_manual_estimate_and_appearance() {
    use chest_counter::overlay::OverlayAppearance;

    assert_eq!(OverlayAppearance::clamp_size(50), 70);
    assert_eq!(OverlayAppearance::clamp_size(120), 120);
    assert_eq!(OverlayAppearance::clamp_size(200), 175);
    assert_eq!(OverlayAppearance::clamp_opacity(10), 30);
    assert_eq!(OverlayAppearance::clamp_opacity(75), 75);
    assert_eq!(OverlayAppearance::clamp_opacity(150), 100);

    let lang = LangCtx::new(Some("en"), false, "24");
    let config = Config::default();
    let mut data = TrackerData::default();
    for i in 0..10 {
        data.matches.push(MatchRecord {
            replay_file: format!("m{i}.replay"),
            player_name: Some("Tester".to_string()),
            playlist: Some("Playlist_DefaultSolo".to_string()),
            map: Some("/Delilah/Maps/Delilah_Terrain".to_string()),
            chests_opened: 10,
            ..Default::default()
        });
    }

    let view = profiles::view(&data, &config, &lang);
    let est = profiles::estimate_from_career(&view, 25).expect("estimate");
    assert_eq!(est.career_matches, 25);
    assert_eq!(est.counted_matches, 10);
    assert_eq!(est.chests, 150);
    assert!(profiles::estimate_from_career(&view, 10).is_none());

    profiles::add_manual(&mut data, &config, &lang, 50, None, None);
    let view = profiles::view(&data, &config, &lang);
    assert_eq!(view.total_chests(), 150);
    assert_eq!(view.manual_entries.len(), 1);
    let id = view.manual_entries[0].id.clone();

    assert!(profiles::remove_manual(&mut data, &config, &lang, &id));
    let view = profiles::view(&data, &config, &lang);
    assert_eq!(view.total_chests(), 100);

    profiles::set_obtained(&mut data, &config, &lang, "gold-peely", true);
    assert!(profiles::view(&data, &config, &lang).obtained_ids.contains(&"gold-peely".to_string()));
    profiles::set_obtained(&mut data, &config, &lang, "gold-peely", false);
    assert!(!profiles::view(&data, &config, &lang).obtained_ids.contains(&"gold-peely".to_string()));
}

#[test]
fn gui_server_security_and_endpoints() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;
    use chest_counter::gui_server::GuiServer;
    use serde_json::json;

    let tmp = std::env::temp_dir().join(format!("chest_counter_gui_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let report_path = tmp.join("report.html");
    let overlay_path = tmp.join("overlay.html");
    std::fs::write(&report_path, "<html><head><meta http-equiv=\"refresh\" content=\"5\"></head><body>REPORT_OK</body></html>").unwrap();
    std::fs::write(&overlay_path, "<html><body>OVERLAY_OK</body></html>").unwrap();

    let server = GuiServer::start(
        Arc::new(|| json!({"ok": true, "total": "42"})),
        Arc::new(|action, val| json!({"echo": action, "val": val})),
        Arc::new(|| Some("<div class=\"report-content\">FRAGMENT_OK</div>".into())),
        tmp.clone(),
        report_path.clone(),
        overlay_path.clone(),
    )
    .expect("start GuiServer");

    let send = |req: &str| -> String {
        let mut s = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
        s.write_all(req.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    };

    // 1a. Root without token is rejected
    let resp = send(&format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", server.port));
    assert!(!resp.starts_with("HTTP/1.1 200 OK"));

    // 1b. Valid root HTML with ?t=<token> contains embedded token
    let resp = send(&format!("GET /?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", server.token, server.port));
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
    assert!(resp.contains(&server.token));
    assert!(!resp.contains("<iframe"));

    // 2. Bad Host header rejected (DNS rebinding guard)
    let resp = send("GET / HTTP/1.1\r\nHost: evil.example.com\r\n\r\n");
    assert!(resp.starts_with("HTTP/1.1 403 Forbidden"));

    // 3. /api/state without token -> 403
    let resp = send(&format!("GET /api/state HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", server.port));
    assert!(resp.starts_with("HTTP/1.1 403 Forbidden"));

    // 4. /api/state with X-Token -> 200 JSON
    let resp = send(&format!(
        "GET /api/state HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Token: {}\r\n\r\n",
        server.port, server.token
    ));
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
    assert!(resp.contains("\"total\":\"42\""));

    // 5. /api/action with X-Token -> 200 JSON
    let body = r#"{"action":"overlayStyle","value":"classic"}"#;
    let resp = send(&format!(
        "POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Token: {}\r\nContent-Length: {}\r\n\r\n{}",
        server.port,
        server.token,
        body.len(),
        body
    ));
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
    assert!(resp.contains("\"echo\":\"overlayStyle\""));

    // 6. /report strips meta-refresh tag and requires token
    let resp = send(&format!(
        "GET /report?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        server.token, server.port
    ));
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
    assert!(resp.contains("REPORT_OK"));
    assert!(!resp.contains("http-equiv=\"refresh\""));

    let resp = send(&format!("GET /api/report HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", server.port));
    assert!(resp.starts_with("HTTP/1.1 403 Forbidden"));

    let resp = send(&format!(
        "GET /api/report HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Token: {}\r\n\r\n",
        server.port, server.token
    ));
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
    assert!(resp.contains("FRAGMENT_OK"));
    assert!(!resp.contains("REPORT_OK"));

    // 7. /overlay serves overlay.html
    let resp = send(&format!(
        "GET /overlay?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        server.token, server.port
    ));
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
    assert!(resp.contains("OVERLAY_OK"));

    drop(server);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn hover_breakdown_all_rewards_and_match_start_treasure_hours() {
    let catalog = TreasureCatalog::load(Path::new("/nonexistent"));
    let lang = LangCtx::new(Some("en"), true, "24");
    let config = Config::default();

    let mk = |name: &str, start_iso: Option<&str>, analyzed_iso: &str, chest_secs: f32| MatchRecord {
        replay_file: format!("{name}.replay"),
        player_name: Some("Tester".to_string()),
        playlist: Some("Playlist_DefaultSolo".to_string()),
        map: Some("/Delilah/Maps/Delilah_Terrain".to_string()),
        match_start_utc: start_iso.map(|s| chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)),
        analyzed_at_utc: chrono::DateTime::parse_from_rfc3339(analyzed_iso).unwrap().with_timezone(&Utc),
        chests_opened: 1,
        chests: vec![ChestRecord {
            time_seconds: chest_secs,
            kind: chest_kinds::NORMAL.to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Treasure Hour on 2026-11-14 runs 19:00:00Z .. 23:00:00Z (outfit x7.5, pirate-shop x1.5, car x10)
    let mut data = TrackerData::default();
    data.matches = vec![
        // Started 1m BEFORE Treasure Hour, chest opened 2m later (DURING Treasure Hour) -> NOT boosted
        mk("before", Some("2026-11-14T18:59:00Z"), "2026-11-14T19:10:00Z", 120.0),
        // Started AT start of Treasure Hour -> BOOSTED
        mk("start", Some("2026-11-14T19:00:00Z"), "2026-11-14T19:15:00Z", 120.0),
        // Started 1m BEFORE Treasure Hour ends, chest opened 5m later (AFTER Treasure Hour) -> BOOSTED
        mk("carryover", Some("2026-11-14T22:59:00Z"), "2026-11-14T23:10:00Z", 300.0),
        // Started AT end of Treasure Hour (half-open interval) -> NOT boosted
        mk("after", Some("2026-11-14T23:00:00Z"), "2026-11-14T23:15:00Z", 120.0),
        // No match_start_utc (only analyzed_at_utc inside window) -> NOT boosted
        mk("nostart", None, "2026-11-14T20:00:00Z", 120.0),
    ];

    let now = Utc.with_ymd_and_hms(2026, 11, 14, 23, 30, 0).unwrap();
    let ctx = ReportContext {
        now_utc: now,
        session_start_utc: now,
        progress: None,
        lang: &lang,
        catalog: &catalog,
        images_folder: None,
    };
    let view = profiles::view(&data, &config, &lang);
    let profs = profiles::list(&data, &lang);
    let html = report::report(&config, &view, &profs, &ctx);

    // Every single reward (both hero cards + weekly table + season table) has a hover breakdown tooltip
    assert_eq!(html.matches(r#"<span class="cbarwrap""#).count(), 7);
    assert_eq!(html.matches(r#"<span class="cbartip">"#).count(), 7);

    // Gold Outfit (weekly): 3 base + 2 boosted at x7.5
    assert!(html.contains(
        r#"<span class="cbartip"><b>Gold Outfit</b>"#
    ));
    assert!(html.contains("3 chests &#183; base odds (1 in 1,562,500)\r\n2 chests &#183; &#215;7.5 (\u{2248} 1 in 208,333)"));

    // Pirate Shop Access (season): 3 base + 2 boosted at x1.5
    assert!(html.contains(
        "<b>Pirate Shop Access</b>\r\n3 chests &#183; base odds (1 in 137,500)\r\n2 chests &#183; &#215;1.5 (\u{2248} 1 in 91,667)"
    ));

    // Vehicle & Decal (weekly): 3 base + 2 boosted at x10
    assert!(html.contains(
        "<b>Vehicle &amp; Decal</b>\r\n3 chests &#183; base odds (1 in 7,813)\r\n2 chests &#183; &#215;10 (\u{2248} 1 in 781)"
    ));

    // Unboosted rewards still get their base-odds breakdown on hover
    assert!(html.contains(
        "<b>Pickaxe or Back Bling</b>\r\n5 chests &#183; base odds (1 in 568)</span>"
    ));
    assert!(html.contains(
        "<b>Jam Track</b>\r\n5 chests &#183; base odds (1 in 391)</span>"
    ));
    assert!(html.contains(
        "<b>Loading Screen</b>\r\n5 chests &#183; base odds (1 in 272)</span>"
    ));
    assert!(html.contains(
        "<b>Gizmo (any)</b>\r\n5 chests &#183; base odds (1 in 127)</span>"
    ));

    // "How is this calculated?" confirms 2 boosted chests from match starts
    assert!(html.contains("Chests from matches started during one count with the boosted odds (2 of yours)."));
}

#[test]
fn charts_dataviz_and_match_filters() {
    use chest_counter::charts::nice_ceil;
    use chest_counter::models::TreasureFind;

    // 1. Axis nice_ceil rounding
    assert_eq!(nice_ceil(0.0), 1.0);
    assert_eq!(nice_ceil(3.2), 4.0);
    assert_eq!(nice_ceil(14.0), 15.0);
    assert_eq!(nice_ceil(73.0), 80.0);
    assert_eq!(nice_ceil(1524.0), 2000.0);

    // 2. Multi-match dataset across playlists, durations, Treasure Hour & treasures
    let catalog = TreasureCatalog::load(Path::new("/nonexistent"));
    let lang = LangCtx::new(Some("en"), true, "24");
    let config = Config::default();

    let mk_chests = |n: usize| -> Vec<ChestRecord> {
        (0..n)
            .map(|i| ChestRecord {
                time_seconds: 60.0 + (i as f32) * 20.0,
                kind: if i % 5 == 0 {
                    chest_kinds::RARE.to_string()
                } else {
                    chest_kinds::NORMAL.to_string()
                },
                ..Default::default()
            })
            .collect()
    };

    let mut data = TrackerData::default();
    // Match 1: Treasure Hour started at 2026-11-14T19:15:00Z, 12m, 40 chests, 1 treasure, Zero Build Squads
    data.matches.push(MatchRecord {
        replay_file: "m1.replay".to_string(),
        player_name: Some("Tester".to_string()),
        playlist: Some("Playlist_NoBuildBR_Squad".to_string()),
        map: Some("/Delilah/Maps/Delilah_Terrain".to_string()),
        match_start_utc: Some(Utc.with_ymd_and_hms(2026, 11, 14, 19, 15, 0).unwrap()),
        analyzed_at_utc: Utc.with_ymd_and_hms(2026, 11, 14, 19, 30, 0).unwrap(),
        duration_seconds: Some(720),
        chests_opened: 40,
        chests: mk_chests(40),
        treasure_finds: vec![TreasureFind {
            source_is_chest: true,
            source_certain: true,
            from_golden_chest: false,
            obtained_at_seconds: Some(240.0),
            time_exact: true,
            ..Default::default()
        }],
        ..Default::default()
    });
    // Match 2: Base odds on 2026-11-15, 10m, 20 chests, DefaultSolo
    data.matches.push(MatchRecord {
        replay_file: "m2.replay".to_string(),
        player_name: Some("Tester".to_string()),
        playlist: Some("Playlist_DefaultSolo".to_string()),
        map: Some("/Delilah/Maps/Delilah_Terrain".to_string()),
        match_start_utc: Some(Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap()),
        analyzed_at_utc: Utc.with_ymd_and_hms(2026, 11, 15, 12, 12, 0).unwrap(),
        duration_seconds: Some(600),
        chests_opened: 20,
        chests: mk_chests(20),
        ..Default::default()
    });
    // Match 3: Base odds on 2026-11-16, 15m, 60 chests, Zero Build Squads
    data.matches.push(MatchRecord {
        replay_file: "m3.replay".to_string(),
        player_name: Some("Tester".to_string()),
        playlist: Some("Playlist_NoBuildBR_Squad".to_string()),
        map: Some("/Delilah/Maps/Delilah_Terrain".to_string()),
        match_start_utc: Some(Utc.with_ymd_and_hms(2026, 11, 16, 14, 0, 0).unwrap()),
        analyzed_at_utc: Utc.with_ymd_and_hms(2026, 11, 16, 14, 16, 0).unwrap(),
        duration_seconds: Some(900),
        chests_opened: 60,
        chests: mk_chests(60),
        ..Default::default()
    });

    let now = Utc.with_ymd_and_hms(2026, 11, 16, 15, 0, 0).unwrap();
    let ctx = ReportContext {
        now_utc: now,
        session_start_utc: now,
        progress: None,
        lang: &lang,
        catalog: &catalog,
        images_folder: None,
    };
    let view = profiles::view(&data, &config, &lang);
    let profs = profiles::list(&data, &lang);
    let html = report::report(&config, &view, &profs, &ctx);

    // 3 inline SVG charts (daily combo, distribution histogram, match-phase loot rate)
    assert_eq!(html.matches(r#"<svg class="vizsvg""#).count(), 3);
    // Daily combo chart shows Treasure Hour stacked bar, 7d rolling polyline, active-day average
    assert!(html.contains(r#"class="bboost""#));
    assert!(html.contains(r#"class="gma""#));
    assert!(html.contains(r#"class="gavg""#));
    // Distribution histogram & loot rate by match phase + playlist comparison rows
    assert!(html.contains("Chests per match distribution"));
    assert!(html.contains("Loot rate by match phase"));
    assert!(html.contains("chests / hr"));
    assert_eq!(html.matches(r#"<div class="mrow">"#).count(), 2);
    // Proportional chest composition strip
    assert!(html.contains(r#"<div class="compbar""#));
    // Match table filter toolbar, Treasure Hour badge, and per-match rate
    assert!(html.contains(r#"class="mtoolbar""#));
    assert!(html.contains(r#"data-f="boost""#));
    assert!(html.contains(r#"data-f="tre""#));
    assert!(html.contains(r#"data-boost="1""#));
    assert!(html.contains(r#"data-tre="1""#));
    assert!(html.contains(r#"class="rate""#));
    // Theme toggle & keyboard-accessible chart hit rects
    assert!(html.contains(r#"id="themebtn""#));
    assert!(html.contains(r#"<rect class="svghit" tabindex="0""#));
}
