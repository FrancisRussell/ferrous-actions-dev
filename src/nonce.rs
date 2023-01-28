use rustup_toolchain_manifest::HashValue;

pub fn build(num_bytes: usize) -> HashValue {
    use rand::RngCore as _;

    let mut bytes = vec![0u8; num_bytes];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut bytes);
    HashValue::from_bytes(&bytes)
}
