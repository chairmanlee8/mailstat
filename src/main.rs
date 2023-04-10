use anyhow::Result;
use chrono::{DateTime, Datelike, Days, FixedOffset, Local, NaiveDate, NaiveTime, Utc};
use clap::Parser;
use email_address_parser::EmailAddress;
use himalaya_lib::{AccountConfig, BackendBuilder, BackendConfig, Envelope, ImapConfig};
use once_cell::sync::Lazy;
use plotters::prelude::*;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::iter::Map;

static CLEARLY_ERRONEOUS_DATE: Lazy<DateTime<FixedOffset>> =
    Lazy::new(|| DateTime::parse_from_rfc3339("1980-01-01T00:00:00+00:00").unwrap());

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    email: String,
    #[arg(short, long)]
    passwd_cmd: Option<String>,
    #[arg(long, default_value = "imap.gmail.com")]
    imap_host: String,
    #[arg(long, default_value = "993")]
    imap_port: u16,
    #[arg(short, long, default_value = "14")]
    days: u64,
    #[arg(long)]
    cache: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let passwd_cmd = args
        .passwd_cmd
        .unwrap_or_else(|| format!("pass show mailstat/{}", args.email));
    let account_cfg = AccountConfig {
        email: args.email.clone(),
        ..Default::default()
    };
    let imap_cfg = ImapConfig {
        host: args.imap_host,
        port: args.imap_port,
        login: args.email.clone(),
        passwd_cmd,
        ..Default::default()
    };
    let backend_cfg = BackendConfig::Imap(imap_cfg);
    let backend = BackendBuilder::new()
        .build(&account_cfg, &backend_cfg)
        .unwrap();
    let until = Local::now().checked_sub_days(Days::new(args.days)).unwrap();
    let mut entries: Vec<Entry> = Vec::new();
    if let Some(cache_file) = &args.cache {
        if let Ok(cache) = load_from_cache(cache_file).await {
            entries = cache;
        } else {
            eprintln!("Cache file {} not found, will create new", cache_file);
        }
    }
    let mut message_ids: HashSet<String> = entries.iter().map(|e| e.message_id.clone()).collect();
    let message_count = message_ids.len();
    println!("Messages cached: {}", message_count);
    let mut i = 0;
    'outer: loop {
        if let Some(entry) = entries.last() {
            eprintln!("Last date: {}", entry.date);
        }
        eprintln!("Loading page {}...", i);
        let page = backend.list_envelopes("INBOX", 100, i).unwrap();
        if page.is_empty() {
            break;
        }
        for envelope in page.iter() {
            if envelope.date < *CLEARLY_ERRONEOUS_DATE {
                eprintln!("Skipping clearly erroneous envelope: {:?}", envelope);
                continue;
            }
            if envelope.date < until {
                break 'outer;
            }
            if !message_ids.contains(&envelope.message_id) {
                entries.push(envelope.into());
                message_ids.insert(envelope.message_id.clone());
            }
        }
        i += 1;
    }
    eprintln!(
        "Loaded {} envelopes, {} new",
        entries.len(),
        message_ids.len() - message_count
    );
    if let Some(cache_file) = &args.cache {
        eprintln!("Saving to cache file {}...", cache_file);
        save_to_cache(cache_file, &entries).await?;
    }
    print_counts_by_date(&entries);
    print_counts_by_domain(&entries);
    graph_counts_by_date(&entries);
    Ok(())
}

fn count_by_date(entries: &[Entry]) -> HashMap<NaiveDate, usize> {
    let mut counts: HashMap<NaiveDate, usize> = HashMap::new();
    for entry in entries.iter() {
        if entry.date < *CLEARLY_ERRONEOUS_DATE {
            continue;
        }
        let date = NaiveDate::from_ymd_opt(entry.date.year(), entry.date.month(), entry.date.day())
            .unwrap();
        let count = counts.entry(date).or_insert(0);
        *count += 1;
    }
    counts
}

fn print_counts_by_date(entries: &[Entry]) {
    let counts = count_by_date(entries);
    println!("date,count");
    for (date, count) in counts.iter() {
        println!("{},{}", date, count);
    }
}

fn graph_counts_by_date(entries: &[Entry]) {
    let mut counts: Vec<(NaiveDate, usize)> = count_by_date(entries).into_iter().collect();
    counts.sort();
    let min_date = counts.first().unwrap().0;
    let max_date = counts.last().unwrap().0;
    let max_count = *counts.iter().map(|(_, c)| c).max().unwrap();
    let root = BitMapBackend::new("var/count-by-date.png", (1024, 768)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let mut chart = ChartBuilder::on(&root)
        .caption("Emails by date", ("sans-serif", 20).into_font())
        .margin(5)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d(min_date..max_date, 0..max_count)
        .unwrap();
    chart.configure_mesh().draw().unwrap();
    chart
        .draw_series(LineSeries::new(counts.iter().map(|(d, c)| (*d, *c)), &RED))
        .unwrap();
}

fn count_by_domain(entries: &[Entry]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter() {
        let sender = EmailAddress::parse(&entry.from_addr, None).unwrap();
        let domain = sender.get_domain().to_string();
        let count = counts.entry(domain).or_insert(0);
        *count += 1;
    }
    counts
}

fn print_counts_by_domain(entries: &[Entry]) {
    let counts = count_by_domain(entries);
    println!("domain,count");
    for (domain, count) in counts.iter() {
        println!("{},{}", domain, count);
    }
}

fn serialize_date<S: Serializer>(date: &DateTime<Local>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&date.to_rfc3339())
}

fn deserialize_date<'de, D: serde::Deserializer<'de>>(d: D) -> Result<DateTime<Local>, D::Error> {
    let s = String::deserialize(d)?;
    // CR: how do we get a D::Error here?
    let dt = DateTime::parse_from_rfc3339(&s).unwrap();
    Ok(dt.with_timezone(&Local))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub message_id: String,
    pub from_addr: String,
    pub subject: String,
    #[serde(
        serialize_with = "serialize_date",
        deserialize_with = "deserialize_date"
    )]
    pub date: DateTime<Local>,
}

impl From<&Envelope> for Entry {
    fn from(envelope: &Envelope) -> Self {
        Self {
            id: envelope.id.clone(),
            message_id: envelope.message_id.clone(),
            from_addr: envelope.from.addr.clone(),
            subject: envelope.subject.clone(),
            date: envelope.date.clone(),
        }
    }
}

async fn save_to_cache(cache_file: &str, entries: &Vec<Entry>) -> Result<()> {
    let mut file = File::create(cache_file)?;
    file.write_all(serde_json::to_string(entries)?.as_bytes())?;
    Ok(())
}

async fn load_from_cache(cache_file: &str) -> Result<Vec<Entry>> {
    let file = File::open(cache_file)?;
    let entries: Vec<Entry> = serde_json::from_reader(file)?;
    Ok(entries)
}
