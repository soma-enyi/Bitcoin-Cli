use btc_core::address::{self, AddressReport, AddressType, PubkeyAddresses};

use super::{AppError, Context};

pub fn from_pubkey(
    ctx: &Context,
    pubkey: &str,
    address_type: Option<AddressType>,
) -> Result<PubkeyAddresses, AppError> {
    Ok(address::from_pubkey_hex(pubkey, address_type, ctx.network)?)
}

pub fn validate(ctx: &Context, input: &str) -> Result<AddressReport, AppError> {
    Ok(address::validate(input, ctx.network)?)
}
