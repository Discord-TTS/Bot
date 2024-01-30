use crate::Result;

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
