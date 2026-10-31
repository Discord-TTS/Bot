fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gix = vergen_gix::Gix::builder().sha(false).dirty(false).build();
    vergen_gix::Emitter::new().add_instructions(&gix)?.emit()?;
    Ok(())
}
