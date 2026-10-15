//! Google Takeout import: `YouTube and YouTube Music/subscriptions/subscriptions.csv`.
//!
//! Columns are `Channel Id, Channel Url, Channel Title`. Header names are localised in
//! non-English exports, so columns are read by position and rows are validated by the id.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::db::Db;
use crate::model::{Channel, is_channel_id};

/// Follows every channel in the file. Returns (newly followed, channels in the file); importing
/// again is harmless.
pub fn import(db: &Db, path: &Path, now: i64) -> Result<(usize, usize)> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let channels = parse(file)?;
    if channels.is_empty() {
        bail!("{} lists no channels", path.display());
    }
    let mut added = 0;
    for ch in &channels {
        added += db.add_channel(ch, now)? as usize;
    }
    Ok((added, channels.len()))
}

pub fn parse(reader: impl Read) -> Result<Vec<Channel>> {
    let mut csv = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(reader);
    let mut out = Vec::new();
    for (i, record) in csv.records().enumerate() {
        let record = record.with_context(|| format!("subscriptions.csv row {}", i + 2))?;
        let id = record.get(0).unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        if !is_channel_id(id) {
            bail!("row {}: {id:?} is not a channel id; is this subscriptions.csv?", i + 2);
        }
        let title = record.get(2).unwrap_or_default().to_string();
        out.push(Channel { id: id.to_string(), title });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_takeout_csv() {
        let csv = "\u{feff}Channel Id,Channel Url,Channel Title\n\
                   UCsBjURrPoezykLs9EqgamOA,http://www.youtube.com/channel/UCsBjURrPoezykLs9EqgamOA,Fireship\n\
                   UCBJycsmduvYEL83R_U4JriQ,http://www.youtube.com/channel/UCBJycsmduvYEL83R_U4JriQ,\"Marques Brownlee, MKBHD\"\n\
                   \n";
        let chans = parse(csv.as_bytes()).unwrap();
        assert_eq!(chans.len(), 2);
        assert_eq!(chans[0].title, "Fireship");
        assert_eq!(chans[1].id, "UCBJycsmduvYEL83R_U4JriQ");
        assert_eq!(chans[1].title, "Marques Brownlee, MKBHD");
    }

    #[test]
    fn imports_a_file_idempotently() {
        let dir = std::env::temp_dir().join(format!("subfeed-takeout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("subscriptions.csv");
        std::fs::write(&path, "Channel Id,Channel Url,Channel Title\nUCsBjURrPoezykLs9EqgamOA,,Fireship\n").unwrap();
        let db = Db::open_in_memory().unwrap();
        assert_eq!(import(&db, &path, 0).unwrap(), (1, 1));
        assert_eq!(import(&db, &path, 0).unwrap(), (0, 1));
        std::fs::write(&path, "Channel Id,Channel Url,Channel Title\n").unwrap();
        assert!(import(&db, &path, 0).unwrap_err().to_string().contains("no channels"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_other_csvs() {
        let csv = "Video ID,Title\nabc123,Something\n";
        assert!(parse(csv.as_bytes()).is_err());
    }
}
