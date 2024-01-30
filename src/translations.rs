use std::collections::HashMap;

use anyhow::Ok;

use poise::serenity_prelude::small_fixed_array::FixedString;

use crate::structs::{Context, Result};

pub fn read_files() -> Result<HashMap<FixedString<u8>, gettext::Catalog>> {
    enum EntryCheck {
        IsFile,
        IsDir,
    }

    let filter_entry = |to_check| {
        move |entry: &std::fs::DirEntry| {
            entry
                .metadata()
                .map(|m| match to_check {
                    EntryCheck::IsFile => m.is_file(),
                    EntryCheck::IsDir => m.is_dir(),
                })
                .unwrap_or(false)
        }
    };

    let translations = std::fs::read_dir("translations")?
        .map(Result::unwrap)
        .filter(filter_entry(EntryCheck::IsDir))
        .flat_map(|d| {
            std::fs::read_dir(d.path())
                .unwrap()
                .map(Result::unwrap)
                .filter(filter_entry(EntryCheck::IsFile))
                .filter(|e| e.path().extension().map_or(false, |e| e == "mo"))
                .map(|entry| {
                    let os_file_path = entry.file_name();
                    let file_path = os_file_path.to_str().unwrap();
                    let file_name = file_path.split('.').next().unwrap();

                    Ok((
                        FixedString::from_str_trunc(file_name),
                        gettext::Catalog::parse(std::fs::File::open(entry.path())?)?,
                    ))
                })
                .filter_map(Result::ok)
        })
        .collect();

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

pub trait GetTextContextExt {
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str;
    fn current_catalog(&self) -> Option<&gettext::Catalog>;
}

impl GetTextContextExt for Context<'_> {
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str {
        self.current_catalog().gettext(translate)
    }

    fn current_catalog(&self) -> Option<&gettext::Catalog> {
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
