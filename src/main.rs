use anyhow::Result;
use chrono::{DateTime, Datelike, Days, FixedOffset, Local, NaiveDate};
use clap::Parser;
use email_address_parser::EmailAddress;
use env_logger;
use himalaya_lib::{
    AccountConfig, BackendBuilder, BackendConfig, EmailSender::Smtp, Envelope, ImapConfig,
    SenderBuilder, SmtpConfig,
};
use lettre::{
    message::{Attachment, Body, MultiPart, SinglePart},
    Message,
};
use once_cell::sync::Lazy;
use plotters::prelude::*;
use prettytable::{format, row, Table};
use serde::{Deserialize, Serialize, Serializer};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Write,
};

static CLEARLY_ERRONEOUS_DATE: Lazy<DateTime<FixedOffset>> =
    Lazy::new(|| DateTime::parse_from_rfc3339("1980-01-01T00:00:00+00:00").unwrap());

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    email: String,
    #[arg(long, default_value = "imap.gmail.com")]
    imap_host: String,
    #[arg(long, default_value = "993")]
    imap_port: u16,
    #[arg(long)]
    imap_starttls: bool,
    #[arg(long, default_value = "smtp.gmail.com")]
    smtp_host: String,
    #[arg(long, default_value = "587")]
    smtp_port: u16,
    #[arg(short, long, default_value = "14")]
    days: u64,
    #[arg(long)]
    cache: Option<String>,
    #[arg(long)]
    send_report_to_email: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let account_cfg = AccountConfig {
        email: args.email.clone(),
        email_sender: Smtp(SmtpConfig {
            host: args.smtp_host,
            port: args.smtp_port,
            ssl: Some(true),
            starttls: Some(true),
            insecure: Some(false),
            login: args.email.clone(),
            passwd_cmd: format!("pass show mailstat/{}", args.email),
        }),
        ..Default::default()
    };
    let imap_cfg = ImapConfig {
        host: args.imap_host,
        port: args.imap_port,
        starttls: Some(args.imap_starttls),
        login: args.email.clone(),
        passwd_cmd: format!("pass show mailstat/{}", args.email),
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
    // let folders = backend.list_folders()?;
    // println!("Folders: {:#?}", folders);
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
    print_counts_by_date(entries.iter().filter(|e| e.date > until));
    let table_by_domain = table_counts_by_domain(entries.iter().filter(|e| e.date > until));
    graph_counts_by_date(entries.iter());
    if args.send_report_to_email {
        let mut sender = SenderBuilder::build(&account_cfg).unwrap();
        let image_by_date = std::fs::read("var/count-by-date.png")?;
        let image_by_date_body = Body::new(image_by_date);
        let email = Message::builder()
            .from(args.email.parse().unwrap())
            .to(args.email.parse().unwrap())
            .subject("mailstat report")
            .multipart(
                MultiPart::mixed()
                    .singlepart(SinglePart::html(format!(
                        "<pre>{}</pre>",
                        table_by_domain.to_string()
                    )))
                    .singlepart(
                        Attachment::new("count-by-date.png".to_string())
                            .body(image_by_date_body, "image/png".parse().unwrap()),
                    ),
            )?;
        sender.send(&email.formatted()).unwrap();
    }
    Ok(())
}

fn count_by_date<'a>(entries: impl Iterator<Item = &'a Entry>) -> Vec<(NaiveDate, usize)> {
    let mut counts: HashMap<NaiveDate, usize> = HashMap::new();
    for entry in entries {
        if entry.date < *CLEARLY_ERRONEOUS_DATE {
            continue;
        }
        let date = NaiveDate::from_ymd_opt(entry.date.year(), entry.date.month(), entry.date.day())
            .unwrap();
        let count = counts.entry(date).or_insert(0);
        *count += 1;
    }
    let mut sorted: Vec<(NaiveDate, usize)> = counts.into_iter().collect();
    sorted.sort();
    sorted
}

fn print_counts_by_date<'a>(entries: impl Iterator<Item = &'a Entry>) {
    let counts = count_by_date(entries);
    println!("date,count");
    for (date, count) in counts.iter() {
        println!("{},{},{}", date, date.weekday(), count);
    }
}

fn graph_counts_by_date<'a>(entries: impl Iterator<Item = &'a Entry>) {
    let counts = count_by_date(entries);
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

fn count_by_domain<'a>(entries: impl Iterator<Item = &'a Entry>) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        let sender = EmailAddress::parse(&entry.from_addr, None).unwrap();
        let domain = sender.get_domain().to_string();
        let count = counts.entry(domain).or_insert(0);
        *count += 1;
    }
    counts
}

fn table_counts_by_domain<'a>(entries: impl Iterator<Item = &'a Entry>) -> Table {
    let counts = count_by_domain(entries);
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by_key(|(_, c)| *c);
    counts.reverse();
    let mut table = Table::new();
    table.set_titles(row!["domain", "count"]);
    for (domain, count) in counts.iter() {
        table.add_row(row![domain, count]);
    }
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.printstd();
    table
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
