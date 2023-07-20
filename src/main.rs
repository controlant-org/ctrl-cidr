use anyhow::Result;
use aws_config::{default_provider::credentials::DefaultCredentialsChain, sts::AssumeRoleProvider};
use aws_types::region::Region;
use log::trace;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
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
    let base_region = base_aws_config
      .region()
      .expect("failed to find the current AWS region")
      .to_owned();

    let regions = app.regions.clone().unwrap_or(vec![base_region.clone()]);

    let mut work = JoinSet::new();

    use cli::AuthMode;
    match app.auth_mode {
      AuthMode::Local => {
        for region in regions {
          let app = app.clone();

          work.spawn(async move {
            let config = aws_config::from_env().region(region).load().await;
            controller::run(config, &app).await
          });
        }
      }
      AuthMode::Assume(ref roles) => {
        for role in roles {
          for region in regions.iter() {
            let app = app.clone();
            let role = role.clone();
            let region = region.clone();

            work.spawn(async move {
              let provider = AssumeRoleProvider::builder(role)
                .session_name("ctrl-cidr")
                .region(region.clone())
                .build(Arc::new(DefaultCredentialsChain::builder().build().await) as Arc<_>);

              let config = aws_config::from_env()
                .credentials_provider(provider)
                .region(region)
                .load()
                .await;

              controller::run(config, &app).await
            });
          }
        }
      }
      AuthMode::Discover(ref root_role, ref sub_role) => {
        let accounts = discover_accounts(root_role, base_region).await?;

        for acc in accounts.iter().take(1) {
          for region in regions.iter() {
            let app = app.clone();
            // MAYBE: support aws partition
            let role = format!("arn:aws:iam::{}:role{}", acc, sub_role);
            let region = region.clone();

            work.spawn(async move {
              let provider = AssumeRoleProvider::builder(role)
                .session_name("ctrl-cidr")
                .region(region.clone())
                .build(Arc::new(DefaultCredentialsChain::builder().build().await) as Arc<_>);

              let config = aws_config::from_env()
                .credentials_provider(provider)
                .region(region)
                .load()
                .await;

              controller::run(config, &app).await
            });
          }
        }
      }
    }

    while let Some(res) = work.join_next().await {
      res.expect("join future failed").expect("controller run failed");
    }

    if app.once {
      break;
    } else {
      sleep(Duration::from_secs(5 * 60)).await;
    }
  }

  Ok(())
}

async fn discover_accounts(root_role: &Option<String>, region: Region) -> Result<Vec<String>> {
  let config = match root_role {
    Some(root_role) => {
      let provider = AssumeRoleProvider::builder(root_role)
        .session_name("ctrl-cidr")
        .region(region.clone())
        .build(Arc::new(DefaultCredentialsChain::builder().build().await) as Arc<_>);

      aws_config::from_env()
        .credentials_provider(provider)
        .region(region)
        .load()
        .await
    }
    None => aws_config::load_from_env().await,
  };

  let org = aws_sdk_organizations::Client::new(&config);

  Ok(
    org
      .list_accounts()
      .send()
      .await?
      .accounts()
      .expect("failed to list accounts")
      .into_iter()
      .map(|a| a.id().expect("failed to extract account ID").to_string())
      .collect(),
  )
}
