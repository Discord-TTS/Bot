use std::collections::HashMap;

use anyhow::Ok;

use poise::serenity_prelude::small_fixed_array::FixedString;

use crate::structs::{Context, Result};

pub async fn read_files() -> Result<HashMap<FixedString<u8>, gettext::Catalog>> {
    let mut translations = HashMap::new();
    let mut reader = tokio::fs::read_dir("translations").await?;
    while let Some(entry) = reader.next_entry().await? {
        if !entry.metadata().await.is_ok_and(|e| e.is_dir()) {
            continue;
        }

        let mut reader = tokio::fs::read_dir(entry.path()).await?;
        while let Some(entry) = reader.next_entry().await? {
            if !entry.metadata().await.is_ok_and(|e| e.is_file()) {
                continue;
            }

            if !entry.path().extension().is_some_and(|e| e == "mo") {
                continue;
            }

            let os_file_path = entry.file_name();
            let file_path = os_file_path.to_str().unwrap();
            let file_name = file_path.split('.').next().unwrap();

            translations.insert(
                FixedString::from_str_trunc(file_name),
                gettext::Catalog::parse(std::fs::File::open(entry.path())?)?,
            );
        }
    }

    println!("Loaded translations");
    Ok(translations)
}

pub trait OptionGettext<'a> {
    fn gettext(self, translate: &'a str) -> &'a str;
}

impl<'a> OptionGettext<'a> for Option<&'a gettext::Catalog> {
    fn gettext(self, translate: &'a str) -> &'a str {
        self.map_or(translate, |c| c.gettext(translate))
    }
}

pub trait GetTextContextExt<'a> {
    fn gettext(self, translate: &'a str) -> &'a str;
    fn current_catalog(self) -> Option<&'a gettext::Catalog>;
}

impl<'a> GetTextContextExt<'a> for Context<'_> {
    fn gettext(self, translate: &'a str) -> &'a str {
        self.current_catalog().gettext(translate)
    }

    fn current_catalog(self) -> Option<&'a gettext::Catalog> {
        if let poise::Context::Application(ctx) = self {
            ctx.data()
                .translations
                .get(match ctx.interaction.locale.as_str() {
                    "ko" => "ko-KR",
                    "pt-BR" => "pt",
                    l => l,
                });
        };

        None
    }
}
