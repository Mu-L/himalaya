use std::str::FromStr;

use anyhow::{bail, Result};
#[cfg(feature = "account-sync")]
use dialoguer::Confirm;
use dialoguer::Input;
use email::account;
#[cfg(feature = "account-sync")]
use email::account::sync::config::SyncConfig;
use email_address::EmailAddress;

#[allow(unused)]
use crate::backend::{self, config::BackendConfig, BackendKind};
#[cfg(feature = "message-send")]
use crate::message::config::{MessageConfig, MessageSendConfig};
use crate::ui::THEME;
#[cfg(feature = "account-sync")]
use crate::wizard_prompt;

use super::TomlAccountConfig;

pub(crate) async fn configure() -> Result<Option<(String, TomlAccountConfig)>> {
    let mut config = TomlAccountConfig::default();

    config.email = Input::with_theme(&*THEME)
        .with_prompt("Email address")
        .validate_with(|email: &String| {
            if EmailAddress::is_valid(email) {
                Ok(())
            } else {
                bail!("Invalid email address: {email}")
            }
        })
        .interact()?;

    let addr = EmailAddress::from_str(&config.email).unwrap();

    let autoconfig_email = config.email.to_owned();
    let autoconfig =
        tokio::spawn(async move { account::discover::from_addr(&autoconfig_email).await.ok() });

    let account_name = Input::with_theme(&*THEME)
        .with_prompt("Account name")
        .default(addr.domain().split_once('.').unwrap().0.to_owned())
        .interact()?;

    config.display_name = Some(
        Input::with_theme(&*THEME)
            .with_prompt("Full display name")
            .default(addr.local_part().to_owned())
            .interact()?,
    );

    config.downloads_dir = Some(
        Input::with_theme(&*THEME)
            .with_prompt("Downloads directory")
            .default(String::from("~/Downloads"))
            .interact()?
            .into(),
    );

    let email = &config.email;
    let autoconfig = autoconfig.await?;
    let autoconfig = autoconfig.as_ref();

    match backend::wizard::configure(&account_name, email, autoconfig).await? {
        #[cfg(feature = "imap")]
        Some(BackendConfig::Imap(imap_config)) => {
            config.imap = Some(imap_config);
            config.backend = Some(BackendKind::Imap);
        }
        #[cfg(feature = "maildir")]
        Some(BackendConfig::Maildir(mdir_config)) => {
            config.maildir = Some(mdir_config);
            config.backend = Some(BackendKind::Maildir);
        }
        #[cfg(feature = "notmuch")]
        Some(BackendConfig::Notmuch(notmuch_config)) => {
            config.notmuch = Some(notmuch_config);
            config.backend = Some(BackendKind::Notmuch);
        }
        _ => (),
    };

    match backend::wizard::configure_sender(&account_name, email, autoconfig).await? {
        #[cfg(feature = "smtp")]
        Some(BackendConfig::Smtp(smtp_config)) => {
            config.smtp = Some(smtp_config);

            #[cfg(feature = "message-send")]
            {
                config.message = Some(MessageConfig {
                    send: Some(MessageSendConfig {
                        backend: Some(BackendKind::Smtp),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }
        }
        #[cfg(feature = "sendmail")]
        Some(BackendConfig::Sendmail(sendmail_config)) => {
            config.sendmail = Some(sendmail_config);

            #[cfg(feature = "message-send")]
            {
                config.message = Some(MessageConfig {
                    send: Some(MessageSendConfig {
                        backend: Some(BackendKind::Sendmail),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }
        }
        _ => (),
    };

    #[cfg(feature = "account-sync")]
    {
        let should_configure_sync = Confirm::new()
            .with_prompt(wizard_prompt!(
                "Do you need an offline access to your account?"
            ))
            .default(false)
            .interact_opt()?
            .unwrap_or_default();

        if should_configure_sync {
            config.sync = Some(SyncConfig {
                enable: Some(true),
                ..Default::default()
            });
        }
    }

    Ok(Some((account_name, config)))
}
