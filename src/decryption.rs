use aes::Aes128;
use anyhow::Context;
use block_modes::block_padding::Pkcs7;
use block_modes::{BlockMode, Cbc};

type Aes128Cbc = Cbc<Aes128, Pkcs7>;

pub struct Cipher {
    key: Vec<u8>,
    iv: Vec<u8>,
}

impl Cipher {
    pub fn from_list(index: &crate::schoolism::client::SchoolismVideoList) -> Self {
        let key = index.key.clone();
        let iv = index.iv.clone();
        Self { key, iv }
    }
    
    pub fn decrypt<'a>(&'a self, blob: &'a mut[u8]) -> anyhow::Result<&'a [u8]> {
        let cipher = Aes128Cbc::new_var(&self.key, &self.iv)?;
        cipher.decrypt(blob).context("could not decrypt blob")
    }
}
