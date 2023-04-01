use anyhow::Result;
use clap::Parser;
use himalaya_lib::{AccountConfig, BackendBuilder, BackendConfig, ImapConfig};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    email: String,
    #[arg(short, long)]
    passwd_cmd: Option<String>,
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
        host: "imap.gmail.com".to_string(),
        port: 993,
        login: args.email.clone(),
        passwd_cmd,
        ..Default::default()
    };
    let backend_cfg = BackendConfig::Imap(imap_cfg);
    let backend = BackendBuilder::new()
        .build(&account_cfg, &backend_cfg)
        .unwrap();
    let envelopes = backend.list_envelopes("INBOX", 10, 0).unwrap();
    for envelope in envelopes.iter() {
        println!("{:?}", envelope);
    }
    Ok(())
}
