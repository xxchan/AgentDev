use anyhow::{Context, Result};
use rand::seq::SliceRandom;

pub fn generate_random_name() -> Result<String> {
    // Generate 128 bits of entropy for a 12-word mnemonic
    let mut entropy = [0u8; 16];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut entropy);
    
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)?;
    let words: Vec<&str> = mnemonic.words().collect();
    words.choose(&mut rand::thread_rng())
        .map(|&word| word.to_string())
        .context("Failed to generate random name")
}