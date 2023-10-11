use crate::Result;

pub trait OptionGettext<'a> {
    fn gettext(self, translate: &'a str) -> &'a str;
}

impl<'a> OptionGettext<'a> for Option<&'a gettext::Catalog> {
    fn gettext(self, translate: &'a str) -> &'a str {
        self.map_or(translate, |c| c.gettext(translate))
    }
}

pub trait OptionTryUnwrap<T> {
    fn try_unwrap(self) -> Result<T>;
}

impl<T> OptionTryUnwrap<T> for Option<T> {
    #[track_caller]
    fn try_unwrap(self) -> Result<T> {
        match self {
            Some(v) => Ok(v),
            None => Err({
                let location = std::panic::Location::caller();
                anyhow::anyhow!(
                    "Unexpected None value on line {} in {}",
                    location.line(),
                    location.file()
                )
            }),
        }
    }
}
