use clap::Parser;
use ipnet::Ipv4Net;
use std::collections::HashMap;

#[derive(Parser)]
#[clap(author, version, about)]
struct Cli {
  /// CIDR maps, format is [name]=[cidr], repeat to provide multiple mappings, cidrs with the same name will be grouped together, name cannot contain colon (:)
  #[clap(long, short, parse(try_from_str = parse_key_val), required(true))]
  cidr: Vec<(String, Ipv4Net)>,
  /// AWS IAM roles to assume, repeat to list all accounts to manage. If not specified, simply loads the current environment/account.
  #[clap(long, short)]
  assume: Option<Vec<String>>,
  /// The tag "key" to use for ingress rules
  #[clap(long, short, default_value = "ingress.controlant.com")]
  ingress_key: String,
  /// Read and generate modification actions but do not actually execute them
  #[clap(long)]
  dry_run: bool,
  /// Run controller logic just once, instead of running as a service
  #[clap(long)]
  once: bool,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct App {
  pub cidrs: HashMap<String, Vec<Ipv4Net>>,
  pub assume_roles: Option<Vec<String>>,
  pub ingress_sources: String,
  pub ingress_ports: String,
  // pub egress_sources: String,
  // pub egress_ports: String,
  pub dry_run: bool,
  pub once: bool,
}

impl App {
  pub fn from_cli() -> Self {
    let cli = Cli::parse();
    let mut cidrs = HashMap::new();
    cli.cidr.iter().cloned().for_each(|(name, cidr)| {
      cidrs.entry(name).or_insert(vec![]).push(cidr);
    });
    Self {
      cidrs,
      assume_roles: cli.assume,
      ingress_sources: format!("{}/sources", cli.ingress_key),
      ingress_ports: format!("{}/ports", cli.ingress_key),
      // egress_sources: format!("{}/sources", cli.egress_key),
      // egress_ports: format!("{}/ports", cli.egress_key),
      dry_run: cli.dry_run,
      once: cli.once,
    }
  }
}

fn parse_key_val(s: &str) -> Result<(String, Ipv4Net), String> {
  let pos = s.find('=').ok_or_else(|| format!("no `=` found in `{}`", s))?;

  let key = s[..pos].to_string();
  if key.contains(':') {
    return Err(format!("colon (:) not allowed in cidr name `{}`", key));
  }

  Ok((key, s[pos + 1..].parse().map_err(|_| "invalid CIDR")?))
}
