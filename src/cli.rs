use clap::Parser;
use ipnet::Ipv4Net;
use std::collections::HashMap;

#[derive(Parser)]
#[clap(author, version, about)]
struct Cli {
  /// CIDR maps, format is [name]=[cidr], repeat to provide multiple mappings, cidrs with the same name will be grouped together, name cannot contain colon (:)
  #[clap(long, short, parse(try_from_str = parse_key_val))]
  cidr: Vec<(String, Ipv4Net)>,
  /// The tag "key" to use for ingress rules, used for all resources
  #[clap(long, short, default_value = "ingress.controlant.com")]
  ingress_key: String,
  /// The tag "key" to use for egress rules, only for Security Groups
  #[clap(long, short, default_value = "egress.controlant.com")]
  egress_key: String,
}

#[derive(Debug)]
pub struct App {
  pub cidrs: HashMap<String, Vec<Ipv4Net>>,
  pub ingress_sources: String,
  pub ingress_ports: String,
  pub egress_sources: String,
  pub egress_ports: String,
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
      ingress_sources: format!("{}/sources", cli.ingress_key),
      ingress_ports: format!("{}/ports", cli.ingress_key),
      egress_sources: format!("{}/sources", cli.egress_key),
      egress_ports: format!("{}/ports", cli.egress_key),
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
