./target/release/bifrost-node key insert \
	--base-path ./data \
	--chain mainnet \
	--scheme Sr25519 \
	--suri 0x895d25d66d6c668417b3c6b4460eb6324da821ccfe3978fa1d01b66aca6a9c35 \
	--key-type aura

./target/release/bifrost-node key insert \
  --base-path ./data \
	--chain mainnet \
	--scheme Ed25519 \
	--suri 0x7195722f89deed235fcd92f1a86525f1da22d5191bb218a7f65f4aa6ef3be6a1 \
	--key-type gran

./target/release/bifrost-node key insert \
  --base-path ./data \
	--chain mainnet \
	--scheme Sr25519 \
	--suri 0x895d25d66d6c668417b3c6b4460eb6324da821ccfe3978fa1d01b66aca6a9c35 \
	--key-type imon

./target/release/bifrost-node \
  --base-path ./data \
  --chain mainnet \
  --port 30333 \
  --ws-port 9944 \
  --rpc-port 9933 \
  --validator \
	--rpc-methods Unsafe \
  --rpc-cors all \
  --rpc-external \
  --ws-external \
  --ethapi debug,trace,txpool \
  --name bifrost
