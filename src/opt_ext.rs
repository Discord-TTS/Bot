use crate::Result;

#[cold]
fn create_err(line: u32, file: &str) -> anyhow::Error {
    anyhow::anyhow!("Unexpected None value on line {line} in {file}",)
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
                create_err(location.line(), location.file())
            }),
        }
    }
}
