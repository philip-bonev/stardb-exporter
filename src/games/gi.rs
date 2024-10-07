use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::mpsc,
};

use artifactarium::network::{
    gen::{command_id, proto::AchievementAllDataNotify::AchievementAllDataNotify},
    GamePacket, GameSniffer,
};
use base64::prelude::*;
use regex::Regex;

pub fn sniff(
    achievement_ids: &[u32],
    device_rx: &mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<Vec<u32>> {
    let keys = load_keys()?;
    let mut sniffer = GameSniffer::new().set_initial_keys(keys);

    let mut achievements = Vec::new();

    while let Ok(data) = device_rx.recv() {
        let Some(GamePacket::Commands(commands)) = sniffer.receive_packet(data) else {
            continue;
        };

        for command in commands {
            if command.command_id == command_id::AchievementAllDataNotify {
                if !achievements.is_empty() {
                    continue;
                }

                if let Ok(quest_data) = command.parse_proto::<AchievementAllDataNotify>() {
                    for quest in quest_data.achievement_list {
                        if achievement_ids.contains(&quest.id)
                            && (quest.status.value() == 2 || quest.status.value() == 3)
                        {
                            achievements.push(quest.id);
                        }
                    }
                }
            }
        }

        if !achievements.is_empty() {
            break;
        }
    }

    if achievements.is_empty() {
        return Err(anyhow::anyhow!("No achievements found"));
    }

    Ok(achievements)
}

fn load_keys() -> anyhow::Result<HashMap<u16, Vec<u8>>> {
    let keys: HashMap<u16, String> = serde_json::from_slice(include_bytes!("../../gi_keys.json"))?;

    let mut keys_bytes = HashMap::new();

    for (k, v) in keys {
        keys_bytes.insert(k, BASE64_STANDARD.decode(v)?);
    }

    Ok(keys_bytes)
}

pub fn pulls() -> anyhow::Result<String> {
    let mut game_path = game_path()?;

    game_path.push("webCaches");

    let re = Regex::new(r"^\d+\.\d+\.\d+\.\d+$")?;
    let mut paths: Vec<_> = game_path
        .read_dir()?
        .flat_map(|r| r.ok().map(|d| d.path()))
        .filter(|p| re.is_match(p.file_name().and_then(|o| o.to_str()).unwrap_or_default()))
        .collect();
    paths.sort();

    let mut cache_path = paths[paths.len() - 1].clone();
    cache_path.push("Cache");
    cache_path.push("Cache_Data");
    cache_path.push("data_2");

    let bytes = std::fs::read(cache_path)?;
    let data = String::from_utf8_lossy(&bytes);
    let lines: Vec<_> = data.split("1/0/").collect();

    for line in lines.iter().rev() {
        if line.starts_with("https") && line.contains("getGachaLog") {
            if let Some(url) = line.split('\0').next() {
                if ureq::get(url)
                    .call()
                    .ok()
                    .and_then(|r| r.into_json::<serde_json::Value>().ok())
                    .map(|j| j["retcode"] == 0)
                    .unwrap_or_default()
                {
                    return Ok(url.to_string());
                } else {
                    return Err(anyhow::anyhow!("Warp url outdated"));
                }
            }
        }
    }

    Err(anyhow::anyhow!("Couldn't find warp url"))
}

fn game_path() -> anyhow::Result<PathBuf> {
    let mut log_path = PathBuf::from(&std::env::var("APPDATA")?);
    log_path.pop();
    log_path.push("LocalLow");
    log_path.push("miHoYo");

    let mut log_path_cn = log_path.clone();

    log_path.push("Genshin Impact");
    log_path_cn.push("原神");

    log_path.push("output_log.txt");
    log_path_cn.push("output_log.txt");

    let log_path = match (log_path.exists(), log_path_cn.exists()) {
        (true, _) => log_path,
        (_, true) => log_path_cn,
        _ => return Err(anyhow::anyhow!("Can't find log file")),
    };

    let re = Regex::new(r".:\\.+(GenshinImpact_Data|YuanShen_Data)")?;

    for line in BufReader::new(File::open(log_path)?).lines() {
        let Ok(line) = line else {
            break;
        };

        if let Some(m) = re.find(&line) {
            return Ok(PathBuf::from(m.as_str()));
        }
    }

    Err(anyhow::anyhow!("Couldn't find game path"))
}