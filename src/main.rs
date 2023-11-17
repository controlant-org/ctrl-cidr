use anyhow::Result;
use log::trace;
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
            let config = control_aws::assume_role(role, Some(region.clone())).await;

            work.spawn(async move { controller::run(config, &app).await });
          }
        }
      }
      AuthMode::Discover(ref root_role, ref sub_role) => {
        let root_config = match root_role {
          Some(r) => control_aws::assume_role(r, None).await,
          None => aws_config::from_env().load().await,
        };
        match control_aws::org::discover_accounts(root_config).await {
          Ok(accounts) => {
            for acc in accounts {
              for region in regions.iter() {
                let app = app.clone();
                // MAYBE: support aws partition
                let role = format!("arn:aws:iam::{}:role{}", acc.id, sub_role);
                let config = control_aws::assume_role(&role, Some(region.clone())).await;

                work.spawn(async move { controller::run(config, &app).await });
              }
            }
          }
          Err(e) => {
            println!("Failed to fetch accounts: {}", e);
            sleep(Duration::from_secs(fastrand::u64(60..300))).await;
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
