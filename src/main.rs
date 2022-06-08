use anyhow::Result;
use aws_sdk_ec2::model::Filter;
use aws_sdk_eks::model::VpcConfigRequest;
// use log::error;
// TODO: error handling & reporting

mod cli;

// TODO: handle pagination

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init();
  let app = cli::App::from_cli();
  dbg!(&app);

  let config = aws_config::load_from_env().await;
  let ec2 = aws_sdk_ec2::Client::new(&config);

  // update managed SGs
  let _req = ec2.describe_security_groups().filters(
    Filter::builder()
      .name("tag-key")
      .values(&app.ingress_sources)
      .values(&app.egress_sources)
      .build(),
  );
  // let resp = req.send().await?;

  // println!("SGs: {:?}", resp);

  // TODO: separate thread/task for cleanup - which requires iterating over all SGs

  let eks = aws_sdk_eks::Client::new(&config);
  for cluster_name in eks.list_clusters().send().await?.clusters.unwrap().iter() {
    if let Some(cluster) = eks.describe_cluster().name(cluster_name).send().await?.cluster {
      let mut public_access_cidrs = cluster.resources_vpc_config.unwrap().public_access_cidrs.unwrap();

      if let Some(tags) = cluster.tags {
        if let Some(sources) = tags.get(&app.ingress_sources) {
          sources.split(':').for_each(|k| {
            if let Some(cidrs) = app.cidrs.get(k) {
              cidrs.iter().for_each(|cidr| public_access_cidrs.push(cidr.to_string()));
            }
          });

          public_access_cidrs.sort(); // needed for dedup
          public_access_cidrs.dedup();

          eks
            .update_cluster_config()
            .name(cluster_name)
            .resources_vpc_config(
              VpcConfigRequest::builder()
                .set_public_access_cidrs(Some(public_access_cidrs))
                .build(),
            )
            .send()
            .await?;
        }
      }
    }
  }

  // TODO: cleanup for EKS

  Ok(())
}
