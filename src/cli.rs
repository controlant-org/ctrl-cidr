use aws_config::Region;
use clap::Parser;
use ipnet::Ipv4Net;
use std::collections::HashMap;

#[derive(Parser)]
#[clap(author, version, about)]
struct Cli {
  /// CIDR maps, format is [name]=[cidr], repeat to provide multiple mappings, cidrs with the same name will be grouped together, name cannot contain colon (:)
  // TODO: allow no cidr, thus the controller only handles cleanup
  #[clap(long, short, value_parser = parse_key_val, required(true), value_name="[name]=[cidr]")]
  cidr: Vec<(String, Ipv4Net)>,
  /// AWS IAM roles to assume, repeat to provide multiple
  #[clap(long, short, conflicts_with_all = ["root_role", "sub_role"])]
  assume: Option<Vec<String>>,
  /// AWS IAM role used to fetch all accounts from an organization
  #[clap(long, requires = "sub_role")]
  root_role: Option<String>,
  /// AWS IAM role name (with path, but not the full ARN) the controller will try to assume in all accounts under the organization, discovery controlled by `root_role` or using local credentials
  #[clap(long)]
  sub_role: Option<String>,
  /// AWS regions to manage, repeat to list all regions to manage. If not specified, simply loads the current region.
  #[clap(long, short)]
  region: Option<Vec<String>>,
  /// The tag "key" to use for ingress rules
  #[clap(long, short, default_value = "ingress.controlant.com")]
  ingress_key: String,
  /// Read and generate modification actions but do not actually execute them
  #[clap(long)]
  dry_run: bool,
  /// Run controller logic just once, instead of running as a service
  #[clap(long)]
  once: bool,
  /// Deprecate CIDRs, repeat to provide multiple
  #[clap(long, short)]
  deprecate: Option<Vec<Ipv4Net>>,
}

#[test]
fn verify_cli() {
  use clap::CommandFactory;
  Cli::command().debug_assert()
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct App {
  pub cidrs: HashMap<String, Vec<Ipv4Net>>,
  pub delete_cidrs: Vec<Ipv4Net>,
  pub auth_mode: AuthMode,
  pub regions: Option<Vec<Region>>,
  pub ingress_sources: String,
  pub ingress_ports: String,
  // pub egress_sources: String,
  // pub egress_ports: String,
  pub dry_run: bool,
  pub once: bool,
}

#[derive(Debug, Clone)]
pub enum AuthMode {
  /// Use the default credentials provider chain on local machine
  Local,
  /// Use the provided list of roles to assume
  Assume(Vec<String>),
  /// Discover accounts and roles to assume from an organization
  Discover(Option<String>, String),
}

impl App {
  pub fn from_cli() -> Self {
    let cli = Cli::parse();
    let mut cidrs = HashMap::new();
    cli.cidr.iter().cloned().for_each(|(name, cidr)| {
      cidrs.entry(name).or_insert(vec![]).push(cidr);
    });
    let auth_mode = match (cli.assume, cli.root_role, cli.sub_role) {
      (Some(roles), _, _) => AuthMode::Assume(roles),
      (None, root_role, Some(sub_role)) => AuthMode::Discover(root_role, sub_role),
      (None, None, None) => AuthMode::Local,
      _ => panic!("invalid auth mode"),
    };

    Self {
      cidrs,
      delete_cidrs: cli.deprecate.unwrap_or_default(),
      auth_mode,
      regions: cli
        .region
        .map(|rs| rs.iter().map(|x| Region::new(x.clone())).collect::<Vec<_>>()),
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
  let pos = s.find('=').ok_or("no name found for cidr mapping")?;

  let key = s[..pos].to_string();
  if key.contains(':') {
    return Err("colon (:) not allowed in cidr name".to_string());
  }

  Ok((key, s[pos + 1..].parse().map_err(|_| "invalid CIDR")?))
}
