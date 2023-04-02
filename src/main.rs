use anyhow::Result;
use chrono::{Days, Local};
use clap::Parser;
use email_address_parser::EmailAddress;
use himalaya_lib::{AccountConfig, BackendBuilder, BackendConfig, Envelopes, ImapConfig};

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
    #[arg(short, long, default_value = "7")]
    days: u64,
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
    let mut envelopes = Envelopes::default();
    let mut i = 0;
    loop {
        eprintln!("Loading page {}...", i);
        let mut page = backend.list_envelopes("INBOX", 100, i).unwrap();
        if page.is_empty() {
            break;
        }
        envelopes.append(&mut page);
        if let Some(envelope) = envelopes.last() {
            eprintln!("Last date: {}", envelope.date);
            if envelope.date < until {
                break;
            }
        }
        i += 1;
    }
    eprintln!("Loaded {} envelopes", envelopes.len());
    println!("timestamp,message_id,from_domain");
    for envelope in envelopes.iter() {
        if envelope.date < until {
            continue;
        }
        let sender = EmailAddress::parse(&envelope.from.addr, None).unwrap();
        println!(
            "{},{},{}",
            envelope.date.to_rfc3339(),
            envelope.message_id,
            sender.get_domain()
        );
    }
    Ok(())
}
