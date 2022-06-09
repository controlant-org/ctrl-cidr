use anyhow::Result;
use aws_sdk_ec2::model::{Filter, IpPermission, IpRange};
use aws_sdk_eks::model::VpcConfigRequest;
use log::trace;
use tokio_stream::StreamExt;

// TODO: error handling & reporting

mod cli;

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init();
  let app = cli::App::from_cli();
  trace!("loaded app config: {:?}", &app);

  let config = aws_config::load_from_env().await;
  // trace!("aws env: {:?}", &config);

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
    trace!("[ingress] found security group: {:?}", sg_id);

    trace!("current ingress: {:?}", sg.ip_permissions);

    let (sources, ports) = sg
      .tags
      .as_ref()
      .unwrap()
      .iter()
      .fold((vec![], vec![]), |(mut sources, mut ports), tag| {
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
            ports = tag
              .value
              .as_ref()
              .unwrap()
              .split(':')
              .map(|p| p.parse::<i32>().unwrap())
              .collect::<Vec<_>>();
          }
        }

        (sources, ports)
      });

    let cur_ing = {
      let mut set = std::collections::HashSet::new();
      sg.ip_permissions.as_ref().unwrap().iter().for_each(|perm| {
        if perm.ip_protocol.as_ref().map_or(false, |proto| proto == "tcp") {
          if let Some(ranges) = perm.ip_ranges.as_ref() {
            for range in ranges {
              set.insert((range.cidr_ip.as_ref().unwrap(), perm.from_port.unwrap()));
            }
          }
        }
      });

      set
    };

    for port in ports {
      let ranges = sources
        .iter()
        .filter(|s| !cur_ing.contains(&(s, port)))
        .map(|s| IpRange::builder().cidr_ip(s).description("manager:ctrl-cidr").build())
        .collect::<Vec<_>>();

      if !ranges.is_empty() {
        trace!("adding ingress rules: {:?} on port {}", ranges, port);
        let resp = ec2
          .authorize_security_group_ingress()
          .group_id(sg_id)
          .ip_permissions(
            IpPermission::builder()
              .from_port(port)
              .to_port(port)
              .ip_protocol("tcp")
              .set_ip_ranges(Some(ranges))
              .build(),
          )
          .send()
          .await?;
        trace!("add ingress result: {:?}", resp);
      }
    }
  }

  // TODO: SG egress

  // EKS cluster ingress
  let eks = aws_sdk_eks::Client::new(&config);
  let mut cl_stream = eks.list_clusters().into_paginator().items().send();
  while let Some(Ok(ref cluster_name)) = cl_stream.next().await {
    trace!("working on EKS cluster: {}", cluster_name);

    if let Some(cluster) = eks.describe_cluster().name(cluster_name).send().await?.cluster {
      let mut public_access_cidrs = cluster.resources_vpc_config.unwrap().public_access_cidrs.unwrap();
      let old_len = public_access_cidrs.len();

      trace!("current cluster public access CIDRs: {:?}", &public_access_cidrs);

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
            trace!("new cluster public access CIDRs: {:?}", &public_access_cidrs);

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

            trace!("update cluser result: {:?}", resp);
          } else {
            trace!("no updates needed for  EKS cluster {}", cluster_name);
          }
        }
      }
    }
  }

  Ok(())
}
