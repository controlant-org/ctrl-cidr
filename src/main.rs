use anyhow::Result;
use log::trace;

mod cli;
mod controller;

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init();
  let app = cli::App::from_cli();
  trace!("loaded app config: {:?}", &app);

  if let Some(ref assume_roles) = app.assume_roles {
    trace!("assume roles: {:?}", assume_roles);

    use aws_config::default_provider::credentials::DefaultCredentialsChain;
    use aws_config::sts::AssumeRoleProvider;
    use aws_types::region::Region;
    use std::sync::Arc;

    let tasks: Vec<_> = assume_roles
      .iter()
      .map(|role| {
        let role = role.clone();
        let app = app.clone();

        tokio::spawn(async move {
          let provider = AssumeRoleProvider::builder(role)
            .session_name("ctrl-cidr")
            .region(Region::new("eu-central-1"))
            .build(Arc::new(DefaultCredentialsChain::builder().build().await) as Arc<_>);

          let config = aws_config::from_env().credentials_provider(provider).load().await;
          controller::run(&config, &app).await.unwrap();
        })
      })
      .collect();

    for t in tasks {
      t.await?;
    }
  } else {
    let config = aws_config::load_from_env().await;
    controller::run(&config, &app).await?;
  }

  Ok(())
}
