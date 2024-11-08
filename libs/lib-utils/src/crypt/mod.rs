use config::auth_config;

use crate::b64::{self, b64u_decode, b64u_encode};

mod config;

pub fn encrypt(data: impl AsRef<[u8]>) -> Result<String, Error> {
    let config = &auth_config();
    let data =
        simple_crypt::encrypt(data.as_ref(), &config.ENCRYPT_KEY).map_err(Error::EncryptFailed)?;
    let data_str = b64u_encode(&data);

    Ok(data_str)
}

pub fn decrypt(data: &str) -> Result<String, Error> {
    let config = &auth_config();
    let data_decoded = b64u_decode(data).map_err(Error::DecodeFailed)?;
    let data = simple_crypt::decrypt(data_decoded.as_ref(), &config.ENCRYPT_KEY)
        .map_err(Error::DecryptFailed)?;
    let data_str = String::from_utf8(data).map_err(Error::StringConversionFailed)?;

    Ok(data_str)
}

#[derive(Debug)]
pub enum Error {
    EncryptFailed(anyhow::Error),
    DecryptFailed(anyhow::Error),
    DecodeFailed(b64::Error),
    StringConversionFailed(std::string::FromUtf8Error),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
