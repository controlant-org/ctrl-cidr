use anyhow::Result;
use itertools::Itertools;
use log::trace;
use std::time::Duration;
use tokio::time::sleep;

mod cli;
mod controller;

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init();
  let app = cli::App::from_cli();
  trace!("loaded app config: {:?}", &app);

  loop {
    let base_aws_config = aws_config::load_from_env().await;
    let regions = app
      .regions
      .clone()
      .unwrap_or(vec![base_aws_config.region().unwrap().clone()]);

    if let Some(ref assume_roles) = app.assume_roles {
      trace!("assume roles: {:?}", assume_roles);

      use aws_config::default_provider::credentials::DefaultCredentialsChain;
      use aws_config::sts::AssumeRoleProvider;
      use std::sync::Arc;

      let tasks: Vec<_> = assume_roles
        .iter()
        .cartesian_product(regions.iter())
        .map(|(role, region)| {
          let role = role.clone();
          let app = app.clone();
          let region = region.clone();

          tokio::spawn(async move {
            let provider = AssumeRoleProvider::builder(role)
              .session_name("ctrl-cidr")
              .region(region.clone())
              .build(Arc::new(DefaultCredentialsChain::builder().build().await) as Arc<_>);

            let config = aws_config::from_env()
              .credentials_provider(provider)
              .region(region)
              .load()
              .await;
            controller::run(&config, &app).await.unwrap();
          })
        })
        .collect();

      for t in tasks {
        t.await?;
      }
    } else {
      // TODO: make local multi-region work
      controller::run(&base_aws_config, &app).await?;
    }

    if app.once {
      break;
    } else {
      sleep(Duration::from_secs(5 * 60)).await;
    }
  }

  Ok(())
}
