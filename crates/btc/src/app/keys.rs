use btc_core::keys::{self, KeyPairInfo, KeyType};

use super::Context;

pub fn generate(ctx: &Context, key_type: KeyType) -> KeyPairInfo {
    keys::generate(ctx.network, key_type)
}
