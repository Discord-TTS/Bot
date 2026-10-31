use aformat::{ArrayString, CapStr};

const GIT_HASH: &str = env!("VERGEN_GIT_SHA");
const GIT_TREE_DIRTY: bool = match env!("VERGEN_GIT_DIRTY").as_bytes() {
    b"true" => true,
    b"false" => false,
    _ => unreachable!(),
};

#[must_use]
pub fn full() -> arrayvec::ArrayString<{ GIT_HASH.len() }> {
    let mut ret = arrayvec::ArrayString::new();
    ret.push_str(GIT_HASH);
    ret
}

#[must_use]
pub fn readable() -> ArrayString<{ 8 + 6 }> {
    let mut version_hash = ArrayString::new();
    if GIT_TREE_DIRTY {
        aformat::aformat_into!(version_hash, "{}-Dirty", CapStr::<8>(GIT_HASH));
    } else {
        aformat::aformat_into!(version_hash, "{}", CapStr::<8>(GIT_HASH));
    }
    version_hash
}
