BIFROST_BINARY='../../target/release/bifrost-node'
# BIFROST_BINARY='docker run --rm -it bifrost-node'

if [ -z "$MNEMONIC" ]; then
  MNEMONIC=$($BIFROST_BINARY key generate -w 12 --output-type Json | jq -r '.secretPhrase')
fi

SEED=$($BIFROST_BINARY key inspect "$MNEMONIC" --output-type Json | jq -r '.secretSeed')
SR25519_SS58=$($BIFROST_BINARY key inspect --scheme sr25519 "$MNEMONIC" --output-type Json 2>&1 | jq -r '.ss58PublicKey')
ED25519_SS58=$($BIFROST_BINARY key inspect --scheme ed25519 "$MNEMONIC" --output-type Json 2>&1 | jq -r '.ss58PublicKey')
SR25519_PUB=$($BIFROST_BINARY key inspect --scheme sr25519 "$MNEMONIC" --output-type Json 2>&1 | jq -r '.publicKey')
ED25519_PUB=$($BIFROST_BINARY key inspect --scheme ed25519 "$MNEMONIC" --output-type Json 2>&1 | jq -r '.publicKey')

echo "****************** Account data ******************"
echo "secret_seed:      $SEED"
echo "mnemonic:         $MNEMONIC"
echo "sr25519 address:  $SR25519_PUB (SS58: $SR25519_SS58)"
echo "ed25519 address:  $ED25519_PUB (SS58: $ED25519_SS58)"
echo "[
    \"$SR25519_SS58\",
    \"$ED25519_SS58\",
    {
        \"grandpa\": \"$ED25519_SS58\",
        \"aura\": \"$SR25519_SS58\",
        \"imon\": \"$SR25519_SS58\",
    }
]"
echo "**************************************************"
