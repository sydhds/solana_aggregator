use anyhow::anyhow;
use solana_sdk::bs58;
use solana_sdk::signature::Signature;

/// Try to decode a string to a Solana signature
pub fn signature_decode(signature: String) -> anyhow::Result<Signature> {
    let mut tx_sig_buf = [0u8; 64];
    let decoded = bs58::decode(signature)
        .onto(&mut tx_sig_buf)
        .map_err(|e| anyhow!(e))?;

    if decoded != tx_sig_buf.len() {
        return Err(anyhow!(
            "Invalid signature, decoded {} bytes, expected {} bytes",
            decoded,
            tx_sig_buf.len()
        ));
    }

    Ok(Signature::from(tx_sig_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sig_decode() {
        assert!(signature_decode("111".to_string()).is_err());
        assert!(signature_decode("".to_string()).is_err());
        let s=  "2Jsm9rDb7xdB3guSJGWj7QMcEmG3MQEGJ93ah2ojRafvC9VcT5HDKETESThziWBbbFW3hZT525za1e2QQnhiXidT";
        assert!(signature_decode(s.to_string()).is_ok());
    }
}
