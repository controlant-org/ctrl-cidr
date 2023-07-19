use anyhow::Result;
use aws_sdk_ec2::types::{Filter, IpPermission, IpRange, Tag};
use aws_sdk_eks::types::VpcConfigRequest;
use aws_types::sdk_config::SdkConfig;
use log::{debug, info, trace};
use tokio_stream::StreamExt;

use crate::cli::App;

const CONTROLLER_MARKER: &str = "manager:ctrl-cidr";

// TODO: error handling & reporting - i.e. no crash

type Sources = Vec<String>;
type PortProtocols = Vec<(i32, String)>;

pub async fn run(config: SdkConfig, app: &App) -> Result<()> {
  trace!("aws env: {:?}", &config);

  // ignore non-existing role
  let sts = aws_sdk_sts::Client::new(&config);
  if let Err(e) = sts.get_caller_identity().send().await {
    info!("ignore failed assume role: {:?}", e);
    return Ok(());
  }

  // EC2 Security Groups
  let ec2 = aws_sdk_ec2::Client::new(&config);
  let mut sg_stream = ec2
    .describe_security_groups()
    .filters(Filter::builder().name("tag-key").values(&app.ingress_sources).build())
    .filters(Filter::builder().name("tag-key").values(&app.ingress_ports).build())
    .into_paginator()
    .items()
    .send();

  while let Some(Ok(ref sg)) = sg_stream.next().await {
    let sg_id = sg.group_id.as_ref().unwrap();
    debug!("[ingress] found security group: {:?}", sg_id);

    debug!("current ingress: {:?}", sg.ip_permissions);

    let (sources, ports) = {
      let mut sources = Sources::new();
      let mut ports = PortProtocols::new();

      if let Some(ref tags) = sg.tags {
        for tag in tags {
          if let Some(ref k) = tag.key {
            if k == &app.ingress_sources {
              sources = tag
                .value
                .as_ref()
                .unwrap()
                .split(':')
                .filter_map(|s| app.cidrs.get(s))
                .flatten()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            } else if k == &app.ingress_ports {
              ports = parse_port_protocols(tag)
            }
          }
        }
      }

      (sources, ports)
    };

    let cur_ing = {
      let mut set = std::collections::HashSet::new();
      sg.ip_permissions.as_ref().unwrap().iter().for_each(|perm| {
        if let Some(ranges) = perm.ip_ranges.as_ref() {
          for range in ranges {
            set.insert((
              range.cidr_ip.as_ref().unwrap(),
              perm.from_port.unwrap_or(-1),
              perm.ip_protocol.as_ref().unwrap(),
            ));
          }
        }
      });

      set
    };

    for port in ports {
      let ranges = sources
        .iter()
        .filter(|s| !cur_ing.contains(&(s, port.0, &port.1)))
        // TODO: add cidr mapping name as part of description
        .map(|s| IpRange::builder().cidr_ip(s).description(CONTROLLER_MARKER).build())
        .collect::<Vec<_>>();

      if !ranges.is_empty() {
        info!(
          "adding ingress rules: {:?} on port {} with protocol {}",
          ranges, port.0, port.1
        );

        if app.dry_run {
          info!("dry run: not adding ingress rules");
        } else {
          let resp = ec2
            .authorize_security_group_ingress()
            .group_id(sg_id)
            .ip_permissions(
              IpPermission::builder()
                .from_port(port.0)
                .to_port(port.0)
                .ip_protocol(port.1)
                .set_ip_ranges(Some(ranges))
                .build(),
            )
            .send()
            .await?;
          info!("add ingress result: {:?}", resp);
        }
      }
    }
  }

  // EKS cluster ingress
  let eks = aws_sdk_eks::Client::new(&config);
  let mut cl_stream = eks.list_clusters().into_paginator().items().send();
  while let Some(Ok(ref cluster_name)) = cl_stream.next().await {
    debug!("working on EKS cluster: {}", cluster_name);

    if let Some(cluster) = eks.describe_cluster().name(cluster_name).send().await?.cluster {
      let mut public_access_cidrs = cluster.resources_vpc_config.unwrap().public_access_cidrs.unwrap();
      let old_len = public_access_cidrs.len();

      debug!("current cluster public access CIDRs: {:?}", &public_access_cidrs);

      if let Some(tags) = cluster.tags {
        if let Some(sources) = tags.get(&app.ingress_sources) {
          sources.split(':').for_each(|k| {
            if let Some(cidrs) = app.cidrs.get(k) {
              cidrs.iter().for_each(|cidr| public_access_cidrs.push(cidr.to_string()));
            }
          });

          public_access_cidrs.sort(); // needed for dedup
          public_access_cidrs.dedup();

          if public_access_cidrs.len() > old_len {
            info!("new cluster public access CIDRs: {:?}", &public_access_cidrs);

            if app.dry_run {
              info!("dry run: not updating cluster public access CIDRs");
            } else {
              let resp = eks
                .update_cluster_config()
                .name(cluster_name)
                .resources_vpc_config(
                  VpcConfigRequest::builder()
                    .set_public_access_cidrs(Some(public_access_cidrs))
                    .build(),
                )
                .send()
                .await?;

              info!("update cluser result: {:?}", resp);
            }
          } else {
            debug!("no updates needed for EKS cluster {}", cluster_name);
          }
        }
      }
    }
  }

  Ok(())
}

fn parse_port_protocols(tag: &Tag) -> PortProtocols {
  tag
    .value
    .as_ref()
    .unwrap()
    .split(':')
    .map(|p| {
      let mut parts = p.splitn(2, '/');
      let port = parts.next().unwrap().parse::<i32>().unwrap();
      let proto = parts.next().unwrap_or("tcp").to_string();
      (port, proto)
    })
    .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_port_protocols_single() {
    let tag = Tag::builder().value("443").build();
    let expected: PortProtocols = vec![(443, "tcp".to_string())];
    assert_eq!(parse_port_protocols(&tag), expected);
  }

  #[test]
  fn test_parse_port_protocols_multi() {
    let tag = Tag::builder().value("443:80").build();
    let expected: PortProtocols = vec![(443, "tcp".to_string()), (80, "tcp".to_string())];
    assert_eq!(parse_port_protocols(&tag), expected);
  }

  #[test]
  fn test_parse_port_protocols_combine() {
    let tag = Tag::builder().value("443/udp:80/-1").build();
    let expected: PortProtocols = vec![(443, "udp".to_string()), (80, "-1".to_string())];
    assert_eq!(parse_port_protocols(&tag), expected);

    let tag = Tag::builder().value("443/-1:80").build();
    let expected: PortProtocols = vec![(443, "-1".to_string()), (80, "tcp".to_string())];
    assert_eq!(parse_port_protocols(&tag), expected);
  }

  // TODO: test garbage tag value
}
