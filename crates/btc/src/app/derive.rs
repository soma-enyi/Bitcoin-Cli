use btc_core::address::AddressType;
use btc_core::derive::{self, Derivation, ExtendedKey};

use super::input::secret_arg;
use super::{AppError, Context};

pub fn derive(
    ctx: &Context,
    key: &str,
    path: &str,
    count: u32,
    address_type: Option<AddressType>,
) -> Result<Derivation, AppError> {
    let key = secret_arg(key)?;
    let key = ExtendedKey::parse(&key)?;
    let path = derive::parse_path(path)?;
    Ok(derive::derive(
        &key,
        &path,
        count,
        address_type,
        ctx.network,
    )?)
}
