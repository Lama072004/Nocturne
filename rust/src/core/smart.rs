//! Auswertung der Smart-Playlist-Regeln.
//!
//! Ablauf von [`evaluate`]:
//!  1. alle Titel der lokalen Kopie laden,
//!  2. zu jedem Titel die "Fakten" sammeln (eigene Wiedergaben, zuletzt
//!     gehört, heruntergeladen),
//!  3. Regeln prüfen, sortieren, begrenzen.
//!
//! Die eigentliche Logik ([`matches`], [`select`]) arbeitet nur auf Daten im
//! Speicher und ist deshalb ohne Datenbank testbar.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rand::seq::SliceRandom;

use crate::api::models::Track;
use crate::api::smart::{Rule, RuleField, RuleGroup, RuleOp, RuleSet, SmartPlaylist, SmartSort};
use crate::core::db;

/// Ein Titel plus alles, was die App selbst über ihn weiß.
pub struct Facts {
    pub track: Track,
    /// Wiedergaben laut eigenem Verlauf.
    pub local_plays: u32,
    /// Letzte Wiedergabe laut eigenem Verlauf (Unix-Sekunden).
    pub last_played: Option<i64>,
    pub downloaded: bool,
}

impl Facts {
    /// Der Server zählt auch Wiedergaben aus anderen Apps, der eigene
    /// Verlauf auch offline Gehörtes. Das Höhere von beiden kommt der
    /// Wahrheit am nächsten.
    fn play_count(&self) -> u32 {
        self.track.play_count.unwrap_or(0).max(self.local_plays)
    }
}

const DAY: i64 = 86_400;

fn text_matches(op: RuleOp, actual: Option<&str>, wanted: &str) -> bool {
    // Groß-/Kleinschreibung ignorieren: "rock" soll "Rock" finden.
    let actual = actual.unwrap_or("").to_lowercase();
    let wanted = wanted.trim().to_lowercase();
    match op {
        RuleOp::Is => actual == wanted,
        RuleOp::IsNot => actual != wanted,
        RuleOp::Contains => actual.contains(&wanted),
        RuleOp::NotContains => !actual.contains(&wanted),
        // Größer/Kleiner ergibt bei Text keinen Sinn -> Regel trifft nicht zu.
        _ => false,
    }
}

fn number_matches(op: RuleOp, actual: f64, wanted: &str) -> bool {
    // Komma als Dezimaltrenner akzeptieren (deutsche Tastatur).
    let Ok(wanted) = wanted.trim().replace(',', ".").parse::<f64>() else {
        return false;
    };
    match op {
        RuleOp::Is => (actual - wanted).abs() < f64::EPSILON,
        RuleOp::IsNot => (actual - wanted).abs() >= f64::EPSILON,
        RuleOp::Greater => actual > wanted,
        RuleOp::Less => actual < wanted,
        _ => false,
    }
}

fn bool_matches(op: RuleOp, actual: bool) -> bool {
    match op {
        RuleOp::IsTrue | RuleOp::Is => actual,
        RuleOp::IsFalse | RuleOp::IsNot => !actual,
        _ => false,
    }
}

fn rule_matches(rule: &Rule, f: &Facts, now: i64) -> bool {
    let t = &f.track;
    let v = rule.value.as_str();
    match rule.field {
        RuleField::Title => text_matches(rule.op, Some(&t.title), v),
        RuleField::Artist => text_matches(rule.op, t.artist.as_deref(), v),
        RuleField::Album => text_matches(rule.op, t.album.as_deref(), v),
        RuleField::Genre => text_matches(rule.op, t.genre.as_deref(), v),
        RuleField::Year => number_matches(rule.op, t.year.unwrap_or(0) as f64, v),
        RuleField::Duration => number_matches(rule.op, t.duration_secs.unwrap_or(0) as f64, v),
        RuleField::Rating => number_matches(rule.op, t.user_rating.unwrap_or(0) as f64, v),
        RuleField::PlayCount => number_matches(rule.op, f.play_count() as f64, v),
        RuleField::Starred => bool_matches(rule.op, t.starred),
        RuleField::Downloaded => bool_matches(rule.op, f.downloaded),
        RuleField::DaysSincePlayed => {
            // Nie gehört zählt als "unendlich lange her": Solche Titel
            // passen zu "länger als N Tage", aber nie zu "innerhalb von N Tagen".
            let days = f.last_played.map(|p| (now - p) as f64 / DAY as f64).unwrap_or(f64::INFINITY);
            number_matches(rule.op, days, v)
        }
        RuleField::DaysSinceAdded => {
            let days = t.added_at.map(|a| (now - a) as f64 / DAY as f64).unwrap_or(f64::INFINITY);
            number_matches(rule.op, days, v)
        }
    }
}

/// UND/ODER über eine Liste von Ergebnissen. Eine leere Liste gilt als
/// erfüllt, damit eine Gruppe ohne Regeln nichts ausschließt.
fn combine(match_all: bool, mut results: impl Iterator<Item = bool>) -> bool {
    let mut any = false;
    let mut empty = true;
    for r in results.by_ref() {
        empty = false;
        if match_all && !r {
            return false;
        }
        any |= r;
    }
    empty || match_all || any
}

fn group_matches(g: &RuleGroup, f: &Facts, now: i64) -> bool {
    combine(g.match_all, g.rules.iter().map(|r| rule_matches(r, f, now)))
}

/// Trifft das ganze Regelwerk auf diesen Titel zu?
pub fn matches(rules: &RuleSet, f: &Facts, now: i64) -> bool {
    let own = rules.rules.iter().map(|r| rule_matches(r, f, now));
    let groups = rules.groups.iter().map(|g| group_matches(g, f, now));
    combine(rules.match_all, own.chain(groups))
}

/// Filtert, sortiert und begrenzt.
pub fn select(playlist: &SmartPlaylist, facts: Vec<Facts>, now: i64) -> Vec<Track> {
    let mut hits: Vec<Facts> =
        facts.into_iter().filter(|f| matches(&playlist.rules, f, now)).collect();

    let lower = |s: &Option<String>| s.as_deref().unwrap_or("").to_lowercase();
    match playlist.sort {
        SmartSort::Random => hits.shuffle(&mut rand::thread_rng()),
        SmartSort::Title => hits.sort_by_key(|f| f.track.title.to_lowercase()),
        SmartSort::Artist => hits.sort_by_key(|f| (lower(&f.track.artist), lower(&f.track.album))),
        SmartSort::Album => hits.sort_by_key(|f| {
            (lower(&f.track.album), f.track.disc_number, f.track.track_number)
        }),
        SmartSort::YearNewest => hits.sort_by_key(|f| std::cmp::Reverse(f.track.year)),
        SmartSort::AddedNewest => hits.sort_by_key(|f| std::cmp::Reverse(f.track.added_at)),
        SmartSort::MostPlayed => hits.sort_by_key(|f| std::cmp::Reverse(f.play_count())),
        SmartSort::LeastPlayed => hits.sort_by_key(|f| f.play_count()),
        // `None` (nie gehört) sortiert vor allen Zeitpunkten -> steht bei
        // "am längsten nicht gehört" ganz oben, was genau gewünscht ist.
        SmartSort::PlayedLongestAgo => hits.sort_by_key(|f| f.last_played),
        SmartSort::PlayedRecently => hits.sort_by_key(|f| std::cmp::Reverse(f.last_played)),
        SmartSort::BestRated => hits.sort_by_key(|f| std::cmp::Reverse(f.track.user_rating)),
    }

    let max_count = playlist.limit_count.map(|n| n as usize).unwrap_or(usize::MAX);
    let max_secs = playlist.limit_minutes.map(|m| m as u64 * 60);
    let mut total = 0u64;
    let mut out = Vec::new();
    for f in hits {
        if out.len() >= max_count {
            break;
        }
        let secs = f.track.duration_secs.unwrap_or(0) as u64;
        if let Some(max) = max_secs {
            // Passt dieser Titel nicht mehr in die Zeit, wird er
            // übersprungen; ein kürzerer weiter hinten passt vielleicht noch.
            if total + secs > max {
                continue;
            }
        }
        total += secs;
        out.push(f.track);
    }
    out
}

/// Wertet eine Smart-Playlist gegen die lokale Datenbank aus.
pub fn evaluate(playlist: &SmartPlaylist) -> Result<Vec<Track>> {
    let stats: HashMap<String, (u32, i64)> = db::local_play_stats()?;
    let downloaded: HashSet<String> = db::downloads()?.into_iter().map(|t| t.id).collect();
    let facts = db::library_tracks()?
        .into_iter()
        .map(|track| {
            let s = stats.get(&track.id);
            Facts {
                local_plays: s.map(|s| s.0).unwrap_or(0),
                last_played: s.map(|s| s.1),
                downloaded: downloaded.contains(&track.id),
                track,
            }
        })
        .collect();
    Ok(select(playlist, facts, db::now_secs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000 * DAY;

    fn fact(id: &str, genre: &str, year: u32, starred: bool, last_played_days: Option<i64>) -> Facts {
        Facts {
            track: Track {
                id: id.into(),
                title: id.into(),
                genre: Some(genre.into()),
                year: Some(year),
                starred,
                duration_secs: Some(200),
                ..Default::default()
            },
            local_plays: 0,
            last_played: last_played_days.map(|d| NOW - d * DAY),
            downloaded: false,
        }
    }

    fn rule(field: RuleField, op: RuleOp, value: &str) -> Rule {
        Rule { field, op, value: value.into() }
    }

    fn ids(tracks: &[Track]) -> Vec<&str> {
        tracks.iter().map(|t| t.id.as_str()).collect()
    }

    /// Das Beispiel aus der Spezifikation:
    /// (Genre = Rock ODER Metal) UND Jahr > 1999 UND seit 30 Tagen nicht gehört.
    #[test]
    fn nested_or_group_inside_and() {
        let playlist = SmartPlaylist {
            rules: RuleSet {
                match_all: true,
                rules: vec![
                    rule(RuleField::Year, RuleOp::Greater, "1999"),
                    rule(RuleField::DaysSincePlayed, RuleOp::Greater, "30"),
                ],
                groups: vec![RuleGroup {
                    match_all: false,
                    rules: vec![
                        rule(RuleField::Genre, RuleOp::Is, "rock"),
                        rule(RuleField::Genre, RuleOp::Is, "Metal"),
                    ],
                }],
            },
            sort: SmartSort::Title,
            limit_count: None,
            ..Default::default()
        };
        let facts = vec![
            fact("a-rock-ok", "Rock", 2005, false, Some(90)),
            fact("b-metal-never-played", "Metal", 2010, false, None),
            fact("c-pop", "Pop", 2010, false, Some(90)),
            fact("d-rock-too-old", "Rock", 1990, false, Some(90)),
            fact("e-rock-played-recently", "Rock", 2005, false, Some(3)),
        ];
        assert_eq!(ids(&select(&playlist, facts, NOW)), ["a-rock-ok", "b-metal-never-played"]);
    }

    #[test]
    fn limits_by_count_and_duration() {
        let mut playlist = SmartPlaylist {
            sort: SmartSort::Title,
            limit_count: Some(2),
            ..Default::default()
        };
        let facts = || (0..5).map(|i| fact(&format!("t{i}"), "x", 2000, false, None)).collect();
        assert_eq!(select(&playlist, facts(), NOW).len(), 2);

        // 10 Minuten bei 200 s pro Titel -> genau 3 Titel passen.
        playlist.limit_count = None;
        playlist.limit_minutes = Some(10);
        assert_eq!(select(&playlist, facts(), NOW).len(), 3);
    }

    #[test]
    fn empty_rules_match_everything_and_bool_fields_work() {
        let all = SmartPlaylist { limit_count: None, ..Default::default() };
        let facts = || vec![fact("a", "x", 2000, true, None), fact("b", "x", 2000, false, None)];
        assert_eq!(select(&all, facts(), NOW).len(), 2);

        let starred = SmartPlaylist {
            rules: RuleSet {
                match_all: true,
                rules: vec![rule(RuleField::Starred, RuleOp::IsTrue, "")],
                groups: vec![],
            },
            limit_count: None,
            ..Default::default()
        };
        assert_eq!(ids(&select(&starred, facts(), NOW)), ["a"]);
    }
}
